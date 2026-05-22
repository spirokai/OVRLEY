use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use uuid::Uuid;

use app_lib::video_server::{parse_range, ByteRange, ParsedRange, VideoServerHandle};

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
