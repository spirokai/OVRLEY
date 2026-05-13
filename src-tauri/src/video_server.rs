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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewVideoState {
    pub import_id: String,
    pub preview_url: String,
    pub path_exists: bool,
    pub file_size: u64,
    pub content_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

enum ParsedRange {
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

        self.url_for_import(&import_id)
    }

    /// Removes the current video registration.
    ///
    /// After this succeeds, all `/video/<import_id>` requests return `404` until
    /// another video is registered.
    pub fn clear_video(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|error| error.to_string())?;
        guard.current = None;
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

    if url == "/health" {
        return respond_text(request, StatusCode(200), "ok");
    }

    if method != Method::Get && method != Method::Head {
        return respond_empty(request, StatusCode(405), common_headers(None, None));
    }

    let Some(import_id) = url.strip_prefix("/video/").filter(|id| !id.is_empty()) else {
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    let current = {
        let guard = inner.lock().map_err(|error| error.to_string())?;
        guard.current.clone()
    };

    let Some(current) = current else {
        return respond_empty(request, StatusCode(404), common_headers(None, None));
    };

    if current.import_id != import_id || !current.path.is_file() {
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
fn parse_range(header: Option<&str>, file_size: u64) -> ParsedRange {
    let Some(header) = header else {
        return ParsedRange::Ignore;
    };
    let Some(rest) = header.trim().strip_prefix("bytes=") else {
        return ParsedRange::Ignore;
    };
    let first_range = rest.split(',').next().unwrap_or("").trim();
    if first_range.is_empty() || file_size == 0 {
        return ParsedRange::Unsatisfiable;
    }

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

    let Some((start_raw, end_raw)) = first_range.split_once('-') else {
        return ParsedRange::Ignore;
    };
    let Ok(start) = start_raw.parse::<u64>() else {
        return ParsedRange::Ignore;
    };
    if start >= file_size {
        return ParsedRange::Unsatisfiable;
    }

    let end = if end_raw.trim().is_empty() {
        file_size - 1
    } else {
        let Ok(parsed_end) = end_raw.parse::<u64>() else {
            return ParsedRange::Ignore;
        };
        if parsed_end < start {
            return ParsedRange::Unsatisfiable;
        }
        parsed_end.min(file_size - 1)
    };

    ParsedRange::Valid(ByteRange { start, end })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;

    /// Verifies all byte range forms required by the Phase A plan.
    #[test]
    fn parses_required_range_forms() {
        assert_eq!(
            valid(parse_range(Some("bytes=0-"), 1000)),
            ByteRange { start: 0, end: 999 }
        );
        assert_eq!(
            valid(parse_range(Some("bytes=123-"), 1000)),
            ByteRange {
                start: 123,
                end: 999
            }
        );
        assert_eq!(
            valid(parse_range(Some("bytes=123-789"), 1000)),
            ByteRange {
                start: 123,
                end: 789
            }
        );
        assert_eq!(
            valid(parse_range(Some("bytes=-500"), 1000)),
            ByteRange {
                start: 500,
                end: 999
            }
        );
        assert_eq!(
            valid(parse_range(Some("bytes=0-99,200-299"), 1000)),
            ByteRange { start: 0, end: 99 }
        );
    }

    /// Verifies invalid and unsatisfiable ranges are rejected as `416` cases.
    #[test]
    fn rejects_unsatisfiable_ranges() {
        assert!(matches!(
            parse_range(Some("bytes=1000-"), 1000),
            ParsedRange::Unsatisfiable
        ));
        assert!(matches!(
            parse_range(Some("bytes=500-400"), 1000),
            ParsedRange::Unsatisfiable
        ));
        assert!(matches!(
            parse_range(Some("bytes=-0"), 1000),
            ParsedRange::Unsatisfiable
        ));
    }

    /// Exercises the server lifecycle and HTTP behavior with real loopback IO.
    ///
    /// The test covers startup, full/HEAD responses, range responses, suffix
    /// ranges, unsatisfiable ranges, stale import IDs, clear, and deleted source
    /// files.
    #[test]
    fn serves_full_range_stale_clear_and_deleted_file_behaviors() {
        let (path_a, data_a) = write_temp_file("a", 2048);
        let (path_b, _data_b) = write_temp_file("b", 1024);
        let server = VideoServerHandle::new();
        server.start().expect("server starts");

        let url_a = server
            .set_video(path_a.clone(), "video/mp4".to_string())
            .expect("set first video");
        let (port, id_a) = split_url(&url_a);

        let head = request(
            port,
            &format!("HEAD /video/{id_a} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        );
        assert!(headers(&head).starts_with("HTTP/1.1 200 OK"));
        assert!(headers(&head).contains("Accept-Ranges: bytes"));
        assert!(headers(&head).contains("Content-Type: video/mp4"));
        assert!(headers(&head).contains("Content-Length: 2048"));

        let range = request(
            port,
            &format!("GET /video/{id_a} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=0-999\r\n\r\n"),
        );
        assert!(headers(&range).starts_with("HTTP/1.1 206 Partial Content"));
        assert!(headers(&range).contains("Content-Range: bytes 0-999/2048"));
        assert_eq!(body(&range).len(), 1000);
        assert_eq!(&body(&range), &data_a[..1000]);

        let suffix = request(
            port,
            &format!("GET /video/{id_a} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=-500\r\n\r\n"),
        );
        assert!(headers(&suffix).starts_with("HTTP/1.1 206 Partial Content"));
        assert!(headers(&suffix).contains("Content-Length: 500"));
        assert_eq!(body(&suffix), data_a[data_a.len() - 500..]);

        let unsat = request(
            port,
            &format!(
                "GET /video/{id_a} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=999999999999999-\r\n\r\n"
            ),
        );
        assert!(headers(&unsat).starts_with("HTTP/1.1 416 Range Not Satisfiable"));
        assert!(headers(&unsat).contains("Content-Range: bytes */2048"));

        let url_b = server
            .set_video(path_b.clone(), "video/mp4".to_string())
            .expect("set second video");
        assert_ne!(url_a, url_b);
        let stale = request(
            port,
            &format!("GET /video/{id_a} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        );
        assert!(headers(&stale).starts_with("HTTP/1.1 404 Not Found"));

        let (_, id_b) = split_url(&url_b);
        server.clear_video().expect("clear video");
        let cleared = request(
            port,
            &format!("GET /video/{id_b} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        );
        assert!(headers(&cleared).starts_with("HTTP/1.1 404 Not Found"));

        let url_b = server
            .set_video(path_b.clone(), "video/mp4".to_string())
            .expect("set deleted video");
        let (_, id_b) = split_url(&url_b);
        std::fs::remove_file(&path_b).expect("delete temp video");
        let deleted = request(
            port,
            &format!("GET /video/{id_b} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        );
        assert!(headers(&deleted).starts_with("HTTP/1.1 404 Not Found"));

        let _ = std::fs::remove_file(path_a);
    }

    /// Extracts a valid range from a parsed range result for parser assertions.
    fn valid(parsed: ParsedRange) -> ByteRange {
        match parsed {
            ParsedRange::Valid(range) => range,
            _ => panic!("expected valid range"),
        }
    }

    /// Writes deterministic temporary bytes to a fake `.mp4` file.
    fn write_temp_file(label: &str, len: usize) -> (PathBuf, Vec<u8>) {
        let path = std::env::temp_dir().join(format!(
            "ovrley-video-server-{label}-{}.mp4",
            Uuid::new_v4()
        ));
        let data = (0..len).map(|idx| (idx % 251) as u8).collect::<Vec<_>>();
        std::fs::write(&path, &data).expect("write temp video");
        (path, data)
    }

    /// Splits a preview URL into its loopback port and import ID components.
    fn split_url(url: &str) -> (u16, String) {
        let without_scheme = url.strip_prefix("http://127.0.0.1:").unwrap();
        let (port, path) = without_scheme.split_once('/').unwrap();
        let id = path.strip_prefix("video/").unwrap();
        (port.parse().unwrap(), id.to_string())
    }

    /// Sends a raw HTTP request to the preview server and returns response bytes.
    ///
    /// The helper stops reading once the declared `Content-Length` has arrived
    /// so the test does not depend on the server closing the TCP connection.
    fn request(port: u16, request: &str) -> Vec<u8> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    response.extend_from_slice(&buffer[..count]);
                    if has_complete_response(&response) {
                        break;
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if response.is_empty() {
                        panic!("timed out before reading response");
                    }
                    break;
                }
                Err(error) => panic!("read response: {error}"),
            }
        }
        response
    }

    /// Returns true once the response headers and declared body length are present.
    fn has_complete_response(response: &[u8]) -> bool {
        let Some(header_end) = header_end(response) else {
            return false;
        };
        let header_text = String::from_utf8_lossy(&response[..header_end]);
        let content_length = header_text
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        response.len() >= header_end + 4 + content_length
    }

    /// Returns the HTTP header section as lossy UTF-8 for assertions.
    fn headers(response: &[u8]) -> String {
        let end = header_end(response).expect("response headers");
        String::from_utf8_lossy(&response[..end]).into_owned()
    }

    /// Returns the HTTP response body bytes after the header delimiter.
    fn body(response: &[u8]) -> Vec<u8> {
        let pos = header_end(response).expect("response body marker");
        response[pos + 4..].to_vec()
    }

    /// Finds the `\r\n\r\n` delimiter that terminates HTTP headers.
    fn header_end(bytes: &[u8]) -> Option<usize> {
        let marker = b"\r\n\r\n";
        bytes
            .windows(marker.len())
            .position(|window| window == marker)
    }

    /// Debug helper for asserting a path points at a file during test changes.
    #[allow(dead_code)]
    fn assert_is_file(path: &Path) {
        assert!(path.is_file());
    }
}
