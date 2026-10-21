//! Local HTTP preview server with byte-range support.
//!
//! Owns: the `VideoServerHandle` / `PreviewVideoState` lifecycle, the tiny_http
//!       request workers, byte-range parsing (`parse_range`), and all
//!       preview-video and map-tile HTTP serving logic. The server binds to a
//!       random loopback port at startup and serves local preview resources.
//! Does not own: core rendering, video encoding, or activity parsing — those live
//!       in `ovrley_core`. This module is Tauri-shell infrastructure.
//!
//! Allowed dependencies: `tiny_http`, `serde`, `std` (fs, io, thread, sync).
//! Forbidden dependencies: `ovrley_core` (the core crate is not a dependency of
//!       the Tauri shell — the Tauri shell wraps core, not vice versa).
//!
//! ## Video Registry
//! Every imported preview video is registered under its own opaque import ID.
//! Registering or removing one video never invalidates another video's URL.
//!
//! ## Thread Safety
//! The preview server runs on four request-worker threads. Shared state
//! (`VideoServerHandle.inner: Arc<Mutex<VideoServerInner>>`) is locked briefly
//! on registry changes, per video request to look up its registration, and on
//! shutdown. File reads happen outside the lock. Each worker exits when the
//! shared shutdown state is set.
//!
//! ## Request Handling
//! Supports HTTP range requests (`bytes=start-end`, `bytes=start-`,
//! `bytes=-suffix`) for video scrubbing. Returns 206 Partial Content with
//! Content-Range headers for valid ranges, 416 Range Not Satisfiable for
//! out-of-bounds ranges, and 200 OK for full-file requests.

use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use uuid::Uuid;

use crate::map_tile_service::{parse_style_path, parse_tile_path, MapTileService, TilePayload};

const CHUNK_SIZE: usize = 512 * 1024;
const WORKER_COUNT: usize = 4;

/// Handle for the local HTTP preview server.
///
/// Created before the Tauri window opens and stored as managed app state.
/// The inner state is `Arc<Mutex<...>>` so the handle can be cloned and
/// shared between the Tauri command layer and the request workers.
#[derive(Clone)]
pub struct VideoServerHandle {
    inner: Arc<Mutex<VideoServerInner>>,
}

struct VideoServerInner {
    port: Option<u16>,
    videos: HashMap<String, RegisteredVideo>,
    shutdown: bool,
    started: bool,
}

#[derive(Clone)]
struct RegisteredVideo {
    path: PathBuf,
    file_size: u64,
    content_type: String,
}

/// Identity of one preview-video registration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVideoRegistration {
    pub import_id: String,
    pub preview_url: String,
}

/// Serializable diagnostic state of one registered preview video.
///
/// `preview_url` uses the random loopback port assigned at server start.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVideoState {
    pub import_id: String,
    pub preview_url: String,
    pub path: String,
    pub path_exists: bool,
    pub file_size: u64,
    pub content_type: String,
}

/// Inclusive byte range for HTTP range requests.
///
/// Both `start` and `end` are inclusive offsets (RFC 7233 semantics).
/// The server constructs Content-Range headers from these values and
/// seeks the file cursor accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    // test seam
    pub start: u64,
    pub end: u64,
}

pub enum ParsedRange {
    // test seam
    Valid(ByteRange),
    Unsatisfiable,
    Ignore,
}

impl VideoServerHandle {
    /// Creates an unstarted preview server handle with empty shared state.
    ///
    /// The handle can be cloned and registered as Tauri managed state. Calling
    /// `start` later binds the loopback socket and records the selected port.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VideoServerInner {
                port: None,
                videos: HashMap::new(),
                shutdown: false,
                started: false,
            })),
        }
    }
}

impl Default for VideoServerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoServerHandle {
    /// Starts the local HTTP server on `127.0.0.1` using a random available port.
    ///
    /// Four workers receive requests from the shared server so a long video
    /// response cannot block a later range request. Calling this more than once
    /// on the same shared handle is a no-op after the first successful start.
    pub fn start(&self) -> Result<(), String> {
        self.start_server(None)
    }

    pub(crate) fn start_with_map_tile_service(
        &self,
        map_tile_service: MapTileService,
    ) -> Result<(), String> {
        self.start_server(Some(Arc::new(map_tile_service)))
    }

    fn start_server(&self, map_tile_service: Option<Arc<MapTileService>>) -> Result<(), String> {
        {
            let guard = self.inner.lock().map_err(|error| error.to_string())?;
            if guard.started {
                return Ok(());
            }
        }

        let server = Server::http("127.0.0.1:0").map_err(|error| error.to_string())?;
        let port = match server.server_addr().to_ip() {
            Some(addr) => addr.port(),
            None => return Err("Video server did not bind to an IP socket".to_string()),
        };

        {
            let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
            guard.port = Some(port);
            guard.shutdown = false;
            guard.started = true;
        }

        let server = Arc::new(server);
        for _ in 0..WORKER_COUNT {
            let server = Arc::clone(&server);
            let inner = Arc::clone(&self.inner);
            let map_tile_service = map_tile_service.clone();
            thread::spawn(move || loop {
                match server.recv_timeout(Duration::from_millis(200)) {
                    Ok(Some(request)) => {
                        handle_request(&inner, map_tile_service.as_deref(), request)
                    }
                    Ok(None) => {}
                    Err(error) => {
                        log::warn!(
                            "Video preview server worker stopped receiving requests: {error}"
                        );
                        break;
                    }
                }

                let should_shutdown = inner.lock().map(|guard| guard.shutdown).unwrap_or(true);
                if should_shutdown {
                    break;
                }
            });
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn port(&self) -> Option<u16> {
        self.inner.lock().ok()?.port
    }

    /// Registers a video file under a fresh opaque import ID.
    ///
    /// This validates the path, records file size and content type, and returns
    /// the import ID with its loopback preview URL. Existing registrations stay
    /// valid; registering the same path twice yields two independent IDs.
    pub fn register_video(
        &self,
        path: PathBuf,
        content_type: String,
    ) -> Result<PreviewVideoRegistration, String> {
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("Failed to read video metadata: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("Video path is not a file: {}", path.display()));
        }

        let import_id = Uuid::new_v4().to_string();
        let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
        let preview_url = video_url(guard.port, &import_id)?;
        guard.videos.insert(
            import_id.clone(),
            RegisteredVideo {
                path,
                file_size: metadata.len(),
                content_type,
            },
        );

        Ok(PreviewVideoRegistration {
            import_id,
            preview_url,
        })
    }

    /// Removes one video registration and returns the path it served.
    ///
    /// Afterwards `/video/<import_id>` returns `404`; other registrations are
    /// unaffected. Unknown IDs are an error so callers cannot silently lose track
    /// of registrations they own.
    pub fn remove_video(&self, import_id: &str) -> Result<PathBuf, String> {
        let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
        guard
            .videos
            .remove(import_id)
            .map(|video| video.path)
            .ok_or_else(|| format!("Unknown preview video import ID: {import_id}"))
    }

    /// Removes every video registration.
    pub fn clear_videos(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
        guard.videos.clear();
        Ok(())
    }

    /// Returns diagnostic state for every video registration, ordered by path
    /// and then import ID.
    ///
    /// Each entry includes a fresh URL reconstruction, source-file existence, and
    /// static file metadata.
    pub fn video_states(&self) -> Result<Vec<PreviewVideoState>, String> {
        let (port, videos) = {
            let guard = self.inner.lock().map_err(|error| error.to_string())?;
            (guard.port, guard.videos.clone())
        };

        let mut states = videos
            .into_iter()
            .map(|(import_id, video)| {
                Ok(PreviewVideoState {
                    preview_url: video_url(port, &import_id)?,
                    path_exists: video.path.is_file(),
                    path: video.path.display().to_string(),
                    import_id,
                    file_size: video.file_size,
                    content_type: video.content_type,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        states.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.import_id.cmp(&right.import_id))
        });
        Ok(states)
    }

    /// Builds the MapLibre URL template for cached OpenFreeMap styles.
    pub fn map_style_url_template(&self) -> Result<String, String> {
        let port = self
            .inner
            .lock()
            .map_err(|error| error.to_string())?
            .port
            .ok_or_else(|| "Video preview server is not running".to_string())?;
        Ok(format!("http://127.0.0.1:{port}/styles/{{style}}"))
    }

    /// Builds the MapLibre URL template for vector tiles served by this process.
    pub fn tile_url_template(&self) -> Result<String, String> {
        let port = self
            .inner
            .lock()
            .map_err(|error| error.to_string())?
            .port
            .ok_or_else(|| "Video preview server is not running".to_string())?;
        Ok(format!(
            "http://127.0.0.1:{port}/tiles/{{z}}/{{x}}/{{y}}.pbf"
        ))
    }
}

/// Builds the HTTP preview URL for an import ID using the bound server port.
fn video_url(port: Option<u16>, import_id: &str) -> Result<String, String> {
    let port = port.ok_or_else(|| "Video preview server is not running".to_string())?;
    Ok(format!("http://127.0.0.1:{port}/video/{import_id}"))
}

/// Handles one tiny_http request and logs any response construction failure.
///
/// Request-specific errors are not propagated to the server loop because a
/// failed client request should not terminate the preview server thread.
fn handle_request(
    inner: &Arc<Mutex<VideoServerInner>>,
    map_tile_service: Option<&MapTileService>,
    request: Request,
) {
    if let Err(error) = respond_to_request(inner, map_tile_service, request) {
        log::warn!("Video preview request failed: {error}");
    }
}

/// Routes and validates an incoming HTTP request.
///
/// Supports `/health`, `/styles/<style>`, `/tiles/<z>/<x>/<y>.pbf`, and `/video/<import_id>`.
/// Video requests must match a registered import ID whose source file still
/// exists; unknown or removed IDs and missing files return `404`.
fn respond_to_request(
    inner: &Arc<Mutex<VideoServerInner>>,
    map_tile_service: Option<&MapTileService>,
    request: Request,
) -> Result<(), String> {
    let method = request.method().clone();
    let url = request.url().to_string();

    if url == "/health" {
        return respond_text(request, StatusCode(200), "ok");
    }

    if url.starts_with("/styles/") {
        if method != Method::Get {
            return respond_empty(request, StatusCode(405), common_headers(None, None));
        }
        let Some(style) = parse_style_path(&url) else {
            return respond_empty(request, StatusCode(400), common_headers(None, None));
        };
        let Some(map_tile_service) = map_tile_service else {
            return respond_empty(request, StatusCode(404), common_headers(None, None));
        };
        let tile_url_template = {
            let port = inner
                .lock()
                .map_err(|error| error.to_string())?
                .port
                .ok_or_else(|| "Video preview server is not running".to_string())?;
            format!("http://127.0.0.1:{port}/tiles/{{z}}/{{x}}/{{y}}.pbf")
        };
        return match map_tile_service.resolve_style(style, &tile_url_template) {
            Ok(payload) => respond_map_resource(request, payload, "application/json"),
            Err(error) => {
                log::warn!("Map style request failed: {error}");
                respond_empty(request, StatusCode(502), common_headers(None, None))
            }
        };
    }

    if url.starts_with("/tiles/") {
        if method != Method::Get {
            return respond_empty(request, StatusCode(405), common_headers(None, None));
        }
        let Some(coordinates) = parse_tile_path(&url) else {
            return respond_empty(request, StatusCode(400), common_headers(None, None));
        };
        let Some(map_tile_service) = map_tile_service else {
            return respond_empty(request, StatusCode(404), common_headers(None, None));
        };
        return match map_tile_service.resolve(coordinates) {
            Ok(payload) => {
                respond_map_resource(request, payload, "application/vnd.mapbox-vector-tile")
            }
            Err(error) => {
                log::warn!("Tile request failed: {error}");
                respond_empty(request, StatusCode(502), common_headers(None, None))
            }
        };
    }

    if method != Method::Get && method != Method::Head {
        return respond_empty(request, StatusCode(405), common_headers(None, None));
    }

    let Some(import_id) = url.strip_prefix("/video/").filter(|id| !id.is_empty()) else {
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    let video = {
        let guard = inner.lock().map_err(|error| error.to_string())?;
        guard.videos.get(import_id).cloned()
    };

    let Some(video) = video else {
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    if !video.path.is_file() {
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    }

    let range_header = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.as_str().to_string());
    let is_head = method == Method::Head;

    match parse_range(range_header.as_deref(), video.file_size) {
        ParsedRange::Valid(range) => respond_video_range(request, video, range, is_head),
        ParsedRange::Unsatisfiable => {
            let headers = common_headers(None, Some(format!("bytes */{}", video.file_size)));
            respond_empty(request, StatusCode(416), headers)
        }
        ParsedRange::Ignore => respond_video_full(request, video, is_head),
    }
}

fn respond_map_resource(
    request: Request,
    payload: TilePayload,
    content_type: &str,
) -> Result<(), String> {
    let content_length = payload.bytes.len();
    let mut headers = common_headers(Some(content_type.to_string()), None);
    headers.push(header("Access-Control-Allow-Origin", "*")?);
    headers.push(header("Content-Length", content_length.to_string())?);
    headers.push(header(
        "X-OVRLEY-Cache",
        payload.cache_status.as_header_value(),
    )?);
    request
        .respond(
            Response::new(
                StatusCode(200),
                headers,
                std::io::Cursor::new(payload.bytes),
                Some(content_length),
                None,
            )
            .with_chunked_threshold(usize::MAX),
        )
        .map_err(|error| error.to_string())
}

/// Streams the entire registered video file as a `200 OK` response.
///
/// `HEAD` requests receive identical headers without a body. File IO is started
/// after the shared state has already been cloned out of the mutex.
fn respond_video_full(
    request: Request,
    video: RegisteredVideo,
    is_head: bool,
) -> Result<(), String> {
    let mut headers = common_headers(Some(video.content_type), None);
    headers.push(header("Content-Length", video.file_size.to_string())?);

    if is_head {
        return respond_empty(request, StatusCode(200), headers);
    }

    let file = File::open(&video.path).map_err(|error| {
        format!(
            "Failed to open video file {}: {error}",
            video.path.display()
        )
    })?;
    let reader = ChunkedRead::new(file.take(video.file_size));
    let response = Response::new(
        StatusCode(200),
        headers,
        reader,
        Some(video.file_size as usize),
        None,
    )
    .with_chunked_threshold(usize::MAX);
    request.respond(response).map_err(|error| error.to_string())
}

/// Streams a single byte range from the registered video file as `206`.
///
/// The response includes `Content-Range` and `Content-Length` headers suitable
/// for native browser/WebView video seeking. `HEAD` requests only receive the
/// headers.
fn respond_video_range(
    request: Request,
    video: RegisteredVideo,
    range: ByteRange,
    is_head: bool,
) -> Result<(), String> {
    let range_len = range.end - range.start + 1;
    let mut headers = common_headers(
        Some(video.content_type),
        Some(format!(
            "bytes {}-{}/{}",
            range.start, range.end, video.file_size
        )),
    );
    headers.push(header("Content-Length", range_len.to_string())?);

    if is_head {
        return respond_empty(request, StatusCode(206), headers);
    }

    let mut file = File::open(&video.path).map_err(|error| {
        format!(
            "Failed to open video file {}: {error}",
            video.path.display()
        )
    })?;
    file.seek(SeekFrom::Start(range.start))
        .map_err(|error| format!("Failed to seek video file: {error}"))?;

    let reader = ChunkedRead::new(file.take(range_len));
    let response = Response::new(
        StatusCode(206),
        headers,
        reader,
        Some(range_len as usize),
        None,
    )
    .with_chunked_threshold(usize::MAX);
    request.respond(response).map_err(|error| error.to_string())
}

/// Sends a plain-text response with common cache/range headers.
fn respond_text(request: Request, status: StatusCode, body: &str) -> Result<(), String> {
    let mut headers = common_headers(Some("text/plain".to_string()), None);
    headers.push(header("Content-Length", body.len().to_string())?);
    request
        .respond(Response::new(
            status,
            headers,
            body.as_bytes(),
            Some(body.len()),
            None,
        ))
        .map_err(|error| error.to_string())
}

/// Sends an empty response with the supplied status and headers.
fn respond_empty(request: Request, status: StatusCode, headers: Vec<Header>) -> Result<(), String> {
    request
        .respond(
            Response::new(status, headers, std::io::empty(), Some(0), None)
                .with_chunked_threshold(usize::MAX),
        )
        .map_err(|error| error.to_string())
}

/// Builds headers shared by full, partial, and error video responses.
///
/// `Accept-Ranges: bytes` advertises seek support, and `Cache-Control: no-store`
/// prevents stale preview data from being reused across imports.
fn common_headers(content_type: Option<String>, content_range: Option<String>) -> Vec<Header> {
    let mut headers = vec![
        header("Accept-Ranges", "bytes").expect("static header is valid"),
        header("Cache-Control", "no-store").expect("static header is valid"),
    ];

    if let Some(content_type) = content_type {
        headers.push(header("Content-Type", content_type).expect("content type header is valid"));
    }

    if let Some(content_range) = content_range {
        headers
            .push(header("Content-Range", content_range).expect("content range header is valid"));
    }

    headers
}

/// Creates a tiny_http header from string components.
fn header(name: &str, value: impl Into<String>) -> Result<Header, String> {
    Header::from_bytes(name.as_bytes(), value.into().as_bytes())
        .map_err(|_| format!("Invalid HTTP header: {name}"))
}

/// Parses a practical single byte range from an HTTP `Range` header.
///
/// The parser supports open-ended, bounded, suffix, and comma-separated range
/// headers. Multipart requests use the first range only, which matches the
/// preview server's single-client/browser-focused requirements.
pub fn parse_range(header: Option<&str>, file_size: u64) -> ParsedRange {
    // test seam
    let Some(header) = header else {
        return ParsedRange::Ignore;
    };
    let Some(rest) = header.trim().strip_prefix("bytes=") else {
        return ParsedRange::Ignore;
    };
    // Per RFC 7233, multi-range headers list comma-separated ranges. We only
    // serve single ranges, so pick the first one and ignore the rest.
    let first_range = rest.split(',').next().unwrap_or("").trim();
    if first_range.is_empty() || file_size == 0 {
        return ParsedRange::Unsatisfiable;
    }

    // Suffix range: `bytes=-N` → last N bytes of the file.
    if let Some(suffix) = first_range.strip_prefix('-') {
        let Ok(length) = suffix.parse::<u64>() else {
            return ParsedRange::Ignore;
        };
        if length == 0 {
            return ParsedRange::Unsatisfiable;
        }
        let start = file_size.saturating_sub(length);
        return ParsedRange::Valid(ByteRange {
            start,
            end: file_size - 1,
        });
    }

    // Open-ended or bounded range: `bytes=START-` or `bytes=START-END`.
    let Some((start_raw, end_raw)) = first_range.split_once('-') else {
        return ParsedRange::Ignore;
    };
    let Ok(start) = start_raw.parse::<u64>() else {
        return ParsedRange::Ignore;
    };
    if start >= file_size {
        return ParsedRange::Unsatisfiable;
    }

    // Empty end_raw means an open-ended request (`bytes=START-`), which
    // extends to the last byte of the file.
    let end = if end_raw.trim().is_empty() {
        file_size - 1
    } else {
        let Ok(parsed_end) = end_raw.parse::<u64>() else {
            return ParsedRange::Ignore;
        };
        if parsed_end < start {
            return ParsedRange::Unsatisfiable;
        }
        // Clamp `end` to the last valid byte index so the client can request
        // an oversized end without getting a 416.
        parsed_end.min(file_size - 1)
    };

    ParsedRange::Valid(ByteRange { start, end })
}

/// Adapts a reader so each `read` call returns at most `CHUNK_SIZE` bytes.
///
/// This prevents the tiny_http response serializer from allocating a buffer
/// large enough to hold an entire video file at once, which would cause
/// OOM at 4K resolutions.
struct ChunkedRead<R> {
    inner: R,
}

impl<R> ChunkedRead<R> {
    /// Wraps a reader so each read call is capped at the server chunk size.
    fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: Read> Read for ChunkedRead<R> {
    /// Reads at most `CHUNK_SIZE` bytes from the wrapped reader.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = buf.len().min(CHUNK_SIZE);
        self.inner.read(&mut buf[..len])
    }
}
