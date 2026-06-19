//! Local HTTP preview server with byte-range support.
//!
//! Owns: the `VideoServerHandle` / `PreviewVideoState` lifecycle, the tiny_http
//!       accept-loop thread, byte-range parsing (`parse_range`), and all
//!       preview-video HTTP serving logic. The server binds to a random loopback
//!       port at startup and serves video files to the frontend for in-app preview.
//! Does not own: core rendering, video encoding, or activity parsing — those live
//!       in `ovrley_core`. This module is Tauri-shell infrastructure.
//!
//! Allowed dependencies: `tiny_http`, `serde`, `std` (fs, io, thread, sync).
//! Forbidden dependencies: `ovrley_core` (the core crate is not a dependency of
//!       the Tauri shell — the Tauri shell wraps core, not vice versa).
//!
//! ## Thread Safety
//! The preview server runs on a dedicated accept-loop thread. Shared state
//! (`VideoServerHandle.inner: Arc<Mutex<VideoServerInner>>`) is locked only on
//! video-path changes and shutdown — the hot request-serving path locks neither
//! a mutex nor the server handle. The accept loop exits when `shutdown_flag`
//! (AtomicBool) is set to true.
//!
//! ## Request Handling
//! Supports HTTP range requests (`bytes=start-end`, `bytes=start-`,
//! `bytes=-suffix`) for video scrubbing. Returns 206 Partial Content with
//! Content-Range headers for valid ranges, 416 Range Not Satisfiable for
//! out-of-bounds ranges, and 200 OK for full-file requests.

use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use uuid::Uuid;

const CHUNK_SIZE: usize = 512 * 1024;

/// Handle for the local HTTP preview server.
///
/// Created before the Tauri window opens and stored as managed app state.
/// The inner state is `Arc<Mutex<...>>` so the handle can be cloned and
/// shared between the Tauri command layer and the accept-loop thread.
/// Call `start()` to bind the loopback socket and spawn the server thread,
/// `stop()` to signal shutdown and join the thread.
#[derive(Clone)]
pub struct VideoServerHandle {
    inner: Arc<Mutex<VideoServerInner>>,
}

struct VideoServerInner {
    port: Option<u16>,
    current: Option<CurrentVideo>,
    shutdown: bool,
    started: bool,
}

#[derive(Clone)]
struct CurrentVideo {
    import_id: String,
    path: PathBuf,
    file_size: u64,
    content_type: String,
}

/// Serializable preview-video state returned to the frontend.
///
/// The frontend polls this after importing a video to discover the local
/// preview URL and confirm the file is accessible. `preview_url` uses a random
/// loopback port assigned at server start.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVideoState {
    pub import_id: String,
    pub preview_url: String,
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
                current: None,
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
    /// The request loop runs on a dedicated thread so Tauri setup can return
    /// promptly. Calling this more than once on the same shared handle is a
    /// no-op after the first successful start.
    pub fn start(&self) -> Result<(), String> {
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
        log::info!("Video preview server listening on 127.0.0.1:{port}");

        let inner = Arc::clone(&self.inner);
        thread::spawn(move || loop {
            match server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(request)) => handle_request(&inner, request),
                Ok(None) => {}
                Err(error) => {
                    log::warn!("Video preview server stopped receiving requests: {error}");
                    break;
                }
            }

            let should_shutdown = inner.lock().map(|guard| guard.shutdown).unwrap_or(true);
            if should_shutdown {
                break;
            }
        });

        Ok(())
    }

    /// Registers a video file as the only file the preview server may serve.
    ///
    /// This validates the path, records file size and content type, generates a
    /// fresh opaque import ID, and returns the corresponding loopback preview
    /// URL. Registering a new file immediately invalidates the previous URL.
    pub fn set_video(&self, path: PathBuf, content_type: String) -> Result<String, String> {
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("Failed to read video metadata: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("Video path is not a file: {}", path.display()));
        }

        let import_id = Uuid::new_v4().to_string();
        let file_size = metadata.len();
        let current = CurrentVideo {
            import_id: import_id.clone(),
            path,
            file_size,
            content_type,
        };

        {
            let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
            guard.current = Some(current);
        }

        log::info!(
            "Registered preview video path={} content_type={} file_size={} import_id={}",
            path.display(),
            content_type,
            file_size,
            import_id
        );

        self.url_for_import(&import_id)
    }

    /// Removes the current video registration.
    ///
    /// After this succeeds, all `/video/<import_id>` requests return `404` until
    /// another video is registered.
    pub fn clear_video(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
        guard.current = None;
        log::info!("Cleared preview video from HTTP server state");
        Ok(())
    }

    /// Returns diagnostic state for the current video registration.
    ///
    /// The state includes a fresh URL reconstruction, source-file existence, and
    /// static file metadata. It returns `None` when no video is registered or the
    /// server port is not available.
    pub fn current_state(&self) -> Option<PreviewVideoState> {
        let current = {
            let guard = self.inner.lock().ok()?;
            guard.current.clone()?
        };

        Some(PreviewVideoState {
            preview_url: self.url_for_import(&current.import_id).ok()?,
            path_exists: current.path.is_file(),
            import_id: current.import_id,
            file_size: current.file_size,
            content_type: current.content_type,
        })
    }

    /// Builds the HTTP preview URL for an import ID using the bound server port.
    fn url_for_import(&self, import_id: &str) -> Result<String, String> {
        let port = self
            .inner
            .lock()
            .map_err(|error| error.to_string())?
            .port
            .ok_or_else(|| "Video preview server is not running".to_string())?;
        Ok(format!("http://127.0.0.1:{port}/video/{import_id}"))
    }
}

/// Handles one tiny_http request and logs any response construction failure.
///
/// Request-specific errors are not propagated to the server loop because a
/// failed client request should not terminate the preview server thread.
fn handle_request(inner: &Arc<Mutex<VideoServerInner>>, request: Request) {
    if let Err(error) = respond_to_request(inner, request) {
        log::warn!("Video preview request failed: {error}");
    }
}

/// Routes and validates an incoming HTTP request.
///
/// Only `/health` and `/video/<import_id>` are supported. Video requests must
/// match the current import ID and source file; stale IDs, missing files, and
/// absent registrations all return `404`.
fn respond_to_request(
    inner: &Arc<Mutex<VideoServerInner>>,
    request: Request,
) -> Result<(), String> {
    let method = request.method().clone();
    let url = request.url().to_string();
    log::info!("Video preview request method={method:?} url={url}");

    if url == "/health" {
        return respond_text(request, StatusCode(200), "ok");
    }

    if method != Method::Get && method != Method::Head {
        return respond_empty(request, StatusCode(405), common_headers(None, None));
    }

    let Some(import_id) = url.strip_prefix("/video/").filter(|id| !id.is_empty()) else {
        log::warn!("Rejecting preview request with unknown route url={url}");
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    let current = {
        let guard = inner.lock().map_err(|error| error.to_string())?;
        guard.current.clone()
    };

    let Some(current) = current else {
        log::warn!("Rejecting preview request url={url} because no video is registered");
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    if current.import_id != import_id || !current.path.is_file() {
        log::warn!(
            "Rejecting preview request url={} expected_import_id={} request_import_id={} path_exists={}",
            url,
            current.import_id,
            import_id,
            current.path.is_file()
        );
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    }

    let range_header = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.as_str().to_string());
    let is_head = method == Method::Head;

    match parse_range(range_header.as_deref(), current.file_size) {
        ParsedRange::Valid(range) => respond_video_range(request, current, range, is_head),
        ParsedRange::Unsatisfiable => {
            log::warn!(
                "Responding 416 for preview request url={} file_size={} range_header={:?}",
                url,
                current.file_size,
                range_header
            );
            let headers = common_headers(None, Some(format!("bytes */{}", current.file_size)));
            respond_empty(request, StatusCode(416), headers)
        }
        ParsedRange::Ignore => respond_video_full(request, current, is_head),
    }
}

/// Streams the entire registered video file as a `200 OK` response.
///
/// `HEAD` requests receive identical headers without a body. File IO is started
/// after the shared state has already been cloned out of the mutex.
fn respond_video_full(
    request: Request,
    current: CurrentVideo,
    is_head: bool,
) -> Result<(), String> {
    log::info!(
        "Responding preview full import_id={} status=200 head={} path={} file_size={}",
        current.import_id,
        is_head,
        current.path.display(),
        current.file_size
    );
    let mut headers = common_headers(Some(current.content_type), None);
    headers.push(header("Content-Length", current.file_size.to_string())?);

    if is_head {
        return respond_empty(request, StatusCode(200), headers);
    }

    let file = File::open(&current.path).map_err(|error| {
        format!(
            "Failed to open video file {}: {error}",
            current.path.display()
        )
    })?;
    let reader = ChunkedRead::new(file.take(current.file_size));
    let response = Response::new(
        StatusCode(200),
        headers,
        reader,
        Some(current.file_size as usize),
        None,
    );
    request.respond(response).map_err(|error| error.to_string())
}

/// Streams a single byte range from the registered video file as `206`.
///
/// The response includes `Content-Range` and `Content-Length` headers suitable
/// for native browser/WebView video seeking. `HEAD` requests only receive the
/// headers.
fn respond_video_range(
    request: Request,
    current: CurrentVideo,
    range: ByteRange,
    is_head: bool,
) -> Result<(), String> {
    let range_len = range.end - range.start + 1;
    log::info!(
        "Responding preview range import_id={} status=206 head={} path={} start={} end={} len={}",
        current.import_id,
        is_head,
        current.path.display(),
        range.start,
        range.end,
        range_len
    );
    let mut headers = common_headers(
        Some(current.content_type),
        Some(format!(
            "bytes {}-{}/{}",
            range.start, range.end, current.file_size
        )),
    );
    headers.push(header("Content-Length", range_len.to_string())?);

    if is_head {
        return respond_empty(request, StatusCode(206), headers);
    }

    let mut file = File::open(&current.path).map_err(|error| {
        format!(
            "Failed to open video file {}: {error}",
            current.path.display()
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
    );
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
        .respond(Response::new(
            status,
            headers,
            std::io::empty(),
            Some(0),
            None,
        ))
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
