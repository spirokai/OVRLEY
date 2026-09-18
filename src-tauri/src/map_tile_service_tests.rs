//! Raster tile route integration tests.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::map_tile_service::MapTileService;
use crate::video_server::VideoServerHandle;

#[test]
fn rejects_invalid_tile_requests_before_cache_or_upstream_access() {
    let fake_upstream = FakeUpstream::start();
    let cache_root = temp_cache_root("invalid-routes");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    for path in [
        "/tiles/20/0/0.png",
        "/tiles/1/2/0.png",
        "/tiles/1/0/2.png",
        "/tiles/-1/0/0.png",
        "/tiles/1/+0/0.png",
        "/tiles/1/0/0.jpg",
        "/tiles/1/0/0.png/extra",
    ] {
        let response = request(port, "GET", path);
        assert!(
            response_headers(&response).starts_with("HTTP/1.1 400 Bad Request"),
            "expected 400 for {path}"
        );
    }

    let unsupported_method = request(port, "POST", "/tiles/1/0/0.png");
    assert!(response_headers(&unsupported_method).starts_with("HTTP/1.1 405 Method Not Allowed"));

    let unrelated = request(port, "GET", "/not-a-tile");
    assert!(response_headers(&unrelated).starts_with("HTTP/1.1 404 Not Found"));
    assert!(fake_upstream.requests().is_empty());
    assert!(!cache_root.exists());
}

#[test]
fn fetches_persists_and_reuses_a_raster_tile() {
    let tile_bytes = b"\x89PNG\r\n\x1a\nOVRLEY tile fixture";
    let fake_upstream = FakeUpstream::start();
    let cache_root = temp_cache_root("cache-miss-hit");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    let first = request(port, "GET", "/tiles/2/1/3.png");
    let first_headers = response_headers(&first);
    assert!(first_headers.starts_with("HTTP/1.1 200 OK"));
    assert!(first_headers.contains("Content-Type: image/png"));
    assert!(first_headers.contains("Cache-Control: no-store"));
    assert_eq!(response_body(&first), tile_bytes);

    let requests = fake_upstream.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /2/1/3.png\n"));
    let request_headers = requests[0].to_ascii_lowercase();
    assert!(request_headers.contains(&format!(
        "user-agent: ovrley/{} (+https://www.ovrley.cc)",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!request_headers.contains("cache-control:"));
    assert!(!request_headers.contains("pragma:"));

    let payload_path = cache_root.join("osm-standard/2/1/3.png");
    let metadata_path = cache_root.join("osm-standard/2/1/3.png.json");
    assert_eq!(
        std::fs::read(&payload_path).expect("cached payload"),
        tile_bytes
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).expect("cached metadata"))
            .expect("valid metadata JSON");
    assert_eq!(metadata["version"], 2);
    assert_eq!(metadata["contentLength"], tile_bytes.len());
    assert!(metadata["policy"].is_object());
    assert!(
        std::fs::read_dir(payload_path.parent().expect("payload directory"))
            .expect("cache directory")
            .all(|entry| !entry
                .expect("cache entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
    );

    let second = request(port, "GET", "/tiles/2/1/3.png");
    assert_eq!(response_body(&second), tile_bytes);
    assert_eq!(fake_upstream.requests().len(), 1);

    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn incomplete_and_malformed_cache_pairs_are_refetched() {
    let tile_bytes = b"\x89PNG\r\n\x1a\nOVRLEY tile fixture";
    let fake_upstream = FakeUpstream::start();
    let cache_root = temp_cache_root("invalid-pairs");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");
    let payload_path = cache_root.join("osm-standard/2/1/3.png");
    let metadata_path = cache_root.join("osm-standard/2/1/3.png.json");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );

    std::fs::remove_file(&metadata_path).expect("remove metadata");
    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );

    std::fs::remove_file(&payload_path).expect("remove payload");
    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );

    std::fs::write(&metadata_path, b"not JSON").expect("corrupt metadata");
    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );

    assert_eq!(fake_upstream.requests().len(), 4);
    assert_eq!(
        std::fs::read(payload_path).expect("repaired payload"),
        tile_bytes
    );
    serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(metadata_path).expect("repaired metadata"),
    )
    .expect("repaired metadata JSON");
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn stale_tile_is_conditionally_revalidated_and_304_refreshes_metadata() {
    let tile_bytes = b"\x89PNG\r\n\x1a\nold tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(tile_bytes, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::empty(304)
            .with_header("Cache-Control", "public, max-age=3600")
            .with_header("ETag", "\"tile-v1\""),
    ]);
    let cache_root = temp_cache_root("revalidate-304");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");
    let metadata_path = cache_root.join("osm-standard/2/1/3.png.json");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );
    let metadata_before = std::fs::read(&metadata_path).expect("initial metadata");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );
    let metadata_after = std::fs::read(&metadata_path).expect("revalidated metadata");
    assert_ne!(metadata_after, metadata_before);

    let upstream_requests = fake_upstream.requests();
    assert_eq!(upstream_requests.len(), 2);
    assert!(upstream_requests[1]
        .to_ascii_lowercase()
        .contains("if-none-match: \"tile-v1\""));

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile_bytes
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn changed_revalidation_replaces_payload_and_policy() {
    let old_tile = b"\x89PNG\r\n\x1a\nold tile";
    let new_tile = b"\x89PNG\r\n\x1a\nnew tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(old_tile, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::png(new_tile, "public, max-age=3600").with_header("ETag", "\"tile-v2\""),
    ]);
    let cache_root = temp_cache_root("revalidate-changed");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");
    let payload_path = cache_root.join("osm-standard/2/1/3.png");
    let metadata_path = cache_root.join("osm-standard/2/1/3.png.json");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        old_tile
    );
    let old_metadata = std::fs::read(&metadata_path).expect("old metadata");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        new_tile
    );
    assert_eq!(std::fs::read(&payload_path).expect("new payload"), new_tile);
    assert_ne!(
        std::fs::read(&metadata_path).expect("new metadata"),
        old_metadata
    );

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        new_tile
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn non_storable_response_is_returned_without_persistence() {
    let tile = b"\x89PNG\r\n\x1a\nprivate tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(tile, "no-store"),
        FakeResponse::png(tile, "no-store"),
    ]);
    let cache_root = temp_cache_root("no-store");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile
    );
    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    assert!(!cache_root.join("osm-standard/2/1/3.png").exists());
    assert!(!cache_root.join("osm-standard/2/1/3.png.json").exists());
}

#[test]
fn stale_tile_survives_upstream_failures_without_metadata_changes() {
    let tile = b"\x89PNG\r\n\x1a\nstale tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(tile, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::empty(500),
        FakeResponse::text("not a PNG"),
        FakeResponse::disconnect(),
        FakeResponse::png(b"\x89PNG\r\n\x1a\ntoo late", "public, max-age=3600")
            .with_delay(Duration::from_millis(400)),
    ]);
    let cache_root = temp_cache_root("stale-on-error");
    let server = tile_server_with_timeouts(&cache_root, &fake_upstream, Duration::from_millis(100));
    let port = server.port().expect("server port");
    let metadata_path = cache_root.join("osm-standard/2/1/3.png.json");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile
    );
    let original_metadata = std::fs::read(&metadata_path).expect("initial metadata");

    for _ in 0..4 {
        assert_eq!(
            response_body(&request(port, "GET", "/tiles/2/1/3.png")),
            tile
        );
        assert_eq!(
            std::fs::read(&metadata_path).expect("unchanged metadata"),
            original_metadata
        );
    }

    assert_eq!(fake_upstream.requests().len(), 5);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn uncached_upstream_failure_returns_bad_gateway() {
    let fake_upstream = FakeUpstream::start_with(vec![FakeResponse::empty(503)]);
    let cache_root = temp_cache_root("uncached-error");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    let response = request(port, "GET", "/tiles/2/1/3.png");
    assert!(response_headers(&response).starts_with("HTTP/1.1 502 Bad Gateway"));
    assert!(!cache_root.join("osm-standard/2/1/3.png").exists());
    assert!(!cache_root.join("osm-standard/2/1/3.png.json").exists());
}

#[test]
fn concurrent_same_tile_requests_share_one_upstream_fetch() {
    let tile = b"\x89PNG\r\n\x1a\nshared tile";
    let fake_upstream =
        FakeUpstream::start_with(vec![
            FakeResponse::png(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))
        ]);
    let cache_root = temp_cache_root("same-tile-dedup");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    let responses = concurrent_requests(port, vec!["/tiles/2/1/3.png"; 4]);
    assert!(responses
        .iter()
        .all(|response| response_body(response) == tile));
    assert_eq!(fake_upstream.requests().len(), 1);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn different_tiles_fetch_concurrently() {
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::default_png().with_delay(Duration::from_millis(200)),
        FakeResponse::default_png().with_delay(Duration::from_millis(200)),
    ]);
    let cache_root = temp_cache_root("different-tile-concurrency");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    let responses = concurrent_requests(port, vec!["/tiles/2/1/2.png", "/tiles/2/1/3.png"]);
    assert!(responses
        .iter()
        .all(|response| response_headers(response).starts_with("HTTP/1.1 200 OK")));
    assert_eq!(fake_upstream.requests().len(), 2);
    assert!(fake_upstream.max_active() >= 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn successful_write_evicts_globally_oldest_entry_and_retains_recently_served_tile() {
    let tile = b"\x89PNG\r\n\x1a\nLRU tile fixture";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(tile, "public, max-age=3600"),
        FakeResponse::png(tile, "public, max-age=3600"),
        FakeResponse::png(tile, "public, max-age=3600"),
        FakeResponse::png(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("global-lru");
    let obsolete_service = MapTileService::new_with_cache_limit(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png",
            fake_upstream.port
        ),
        "obsolete-source".to_string(),
        u64::MAX,
    )
    .expect("obsolete tile service");
    let obsolete_server = VideoServerHandle::new();
    obsolete_server
        .start_with_map_tile_service(obsolete_service)
        .expect("obsolete server starts");
    let obsolete_port = obsolete_server.port().expect("obsolete server port");

    for path in ["/tiles/2/1/1.png", "/tiles/2/1/2.png", "/tiles/2/1/3.png"] {
        assert_eq!(response_body(&request(obsolete_port, "GET", path)), tile);
    }

    let first = cache_root.join("obsolete-source/2/1/1.png");
    let second = cache_root.join("obsolete-source/2/1/2.png");
    let third = cache_root.join("obsolete-source/2/1/3.png");
    std::fs::File::options()
        .write(true)
        .open(&first)
        .expect("open first tile")
        .set_modified(SystemTime::now() - Duration::from_secs(300))
        .expect("date first tile");
    std::fs::File::options()
        .write(true)
        .open(&second)
        .expect("open second tile")
        .set_modified(SystemTime::now() - Duration::from_secs(200))
        .expect("date second tile");
    std::fs::File::options()
        .write(true)
        .open(&third)
        .expect("open third tile")
        .set_modified(SystemTime::now() - Duration::from_secs(100))
        .expect("date third tile");

    let first_sidecar = first.with_extension("png.json");
    let first_metadata = std::fs::read(&first_sidecar).expect("first tile metadata");
    let first_metadata_modified = std::fs::metadata(&first_sidecar)
        .expect("first sidecar metadata")
        .modified()
        .expect("first sidecar modification time");
    assert_eq!(
        response_body(&request(obsolete_port, "GET", "/tiles/2/1/1.png")),
        tile
    );
    assert_eq!(
        std::fs::read(&first_sidecar).expect("untouched first tile metadata"),
        first_metadata
    );
    assert_eq!(
        std::fs::metadata(&first_sidecar)
            .expect("untouched first sidecar metadata")
            .modified()
            .expect("untouched first sidecar modification time"),
        first_metadata_modified
    );
    let cache_limit = cache_pair_size(&first) + cache_pair_size(&second) + cache_pair_size(&third);
    drop(obsolete_server);

    let interrupted = cache_root.join("obsolete-source/4/5/.interrupted.png.tmp");
    let orphan = cache_root.join("obsolete-source/4/5/6.png.json");
    std::fs::create_dir_all(interrupted.parent().expect("artifact parent"))
        .expect("create artifact directory");
    std::fs::write(&interrupted, b"partial payload").expect("write interrupted payload");
    std::fs::write(&orphan, b"orphan sidecar").expect("write orphan sidecar");

    let active_service = MapTileService::new_with_cache_limit(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png",
            fake_upstream.port
        ),
        "osm-standard".to_string(),
        cache_limit,
    )
    .expect("active tile service");
    assert!(!interrupted.exists());
    assert!(!orphan.exists());
    let active_server = VideoServerHandle::new();
    active_server
        .start_with_map_tile_service(active_service)
        .expect("active server starts");
    let active_port = active_server.port().expect("active server port");

    assert_eq!(
        response_body(&request(active_port, "GET", "/tiles/2/1/0.png")),
        tile
    );

    assert!(first.exists(), "recently served tile must be retained");
    assert!(!second.exists(), "globally oldest tile must be evicted");
    assert!(!cache_root.join("obsolete-source/2/1/2.png.json").exists());
    assert!(third.exists());
    assert!(cache_root.join("osm-standard/2/1/0.png").exists());
    assert_eq!(fake_upstream.requests().len(), 4);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn clear_removes_all_namespaces_and_a_later_request_repopulates_the_cache() {
    let tile = b"\x89PNG\r\n\x1a\nclear tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::png(tile, "public, max-age=3600"),
        FakeResponse::png(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("clear");
    let map_tile_service = MapTileService::new(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png",
            fake_upstream.port
        ),
        "osm-standard".to_string(),
    )
    .expect("tile service");
    let server = VideoServerHandle::new();
    server
        .start_with_map_tile_service(map_tile_service.clone())
        .expect("server starts");
    let port = server.port().expect("server port");

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile
    );
    let obsolete = cache_root.join("obsolete-source/0/0");
    std::fs::create_dir_all(&obsolete).expect("create obsolete namespace");
    std::fs::write(obsolete.join("0.png.tmp"), b"interrupted").expect("write artifact");

    map_tile_service.clear().expect("clear tile cache");
    assert!(!cache_root.exists());

    assert_eq!(
        response_body(&request(port, "GET", "/tiles/2/1/3.png")),
        tile
    );
    assert!(cache_root.join("osm-standard/2/1/3.png").exists());
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn clear_waits_for_active_tile_resolution_without_disrupting_its_response() {
    let tile = b"\x89PNG\r\n\x1a\nactive tile";
    let fake_upstream =
        FakeUpstream::start_with(vec![
            FakeResponse::png(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))
        ]);
    let cache_root = temp_cache_root("clear-active");
    let map_tile_service = MapTileService::new(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png",
            fake_upstream.port
        ),
        "osm-standard".to_string(),
    )
    .expect("tile service");
    let server = VideoServerHandle::new();
    server
        .start_with_map_tile_service(map_tile_service.clone())
        .expect("server starts");
    let port = server.port().expect("server port");

    let active_request = std::thread::spawn(move || request(port, "GET", "/tiles/2/1/3.png"));
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while fake_upstream.requests().is_empty() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(fake_upstream.requests().len(), 1);

    map_tile_service
        .clear()
        .expect("clear waits for active request");
    let response = active_request.join().expect("active request completes");
    assert!(response_headers(&response).starts_with("HTTP/1.1 200 OK"));
    assert_eq!(response_body(&response), tile);
    assert!(!cache_root.exists());
}

#[test]
fn failed_same_tile_request_does_not_poison_later_retry() {
    let tile = b"\x89PNG\r\n\x1a\nretry tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::empty(500).with_delay(Duration::from_millis(200)),
        FakeResponse::png(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("failed-dedup-retry");
    let server = tile_server(&cache_root, &fake_upstream);
    let port = server.port().expect("server port");

    let failures = concurrent_requests(port, vec!["/tiles/2/1/3.png"; 2]);
    assert!(failures
        .iter()
        .all(|response| response_headers(response).starts_with("HTTP/1.1 502 Bad Gateway")));
    assert_eq!(fake_upstream.requests().len(), 1);

    let retry = request(port, "GET", "/tiles/2/1/3.png");
    assert_eq!(response_body(&retry), tile);
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

fn tile_server(cache_root: &PathBuf, upstream: &FakeUpstream) -> VideoServerHandle {
    let map_tile_service = MapTileService::new(
        cache_root.clone(),
        format!("http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png", upstream.port),
        "osm-standard".to_string(),
    )
    .expect("tile service");
    let server = VideoServerHandle::new();
    server
        .start_with_map_tile_service(map_tile_service)
        .expect("server starts");
    server
}

fn tile_server_with_timeouts(
    cache_root: &PathBuf,
    upstream: &FakeUpstream,
    request_timeout: Duration,
) -> VideoServerHandle {
    let map_tile_service = MapTileService::new_with_timeouts(
        cache_root.clone(),
        format!("http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.png", upstream.port),
        "osm-standard".to_string(),
        request_timeout,
        request_timeout,
    )
    .expect("tile service");
    let server = VideoServerHandle::new();
    server
        .start_with_map_tile_service(map_tile_service)
        .expect("server starts");
    server
}

fn temp_cache_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ovrley-tile-cache-{label}-{}", Uuid::new_v4()))
}

fn cache_pair_size(payload: &PathBuf) -> u64 {
    let metadata = payload.with_extension("png.json");
    std::fs::metadata(payload).expect("payload metadata").len()
        + std::fs::metadata(metadata).expect("sidecar metadata").len()
}

struct FakeUpstream {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    max_active: Arc<AtomicUsize>,
}

impl FakeUpstream {
    fn start() -> Self {
        Self::start_with(Vec::new())
    }

    fn start_with(responses: Vec<FakeResponse>) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").expect("fake upstream binds");
        let port = server
            .server_addr()
            .to_ip()
            .expect("upstream IP address")
            .port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = Arc::clone(&requests);
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let response_active = Arc::clone(&active);
        let response_max_active = Arc::clone(&max_active);
        std::thread::spawn(move || {
            while let Ok(request) = server.recv() {
                let mut record = format!("{} {}\n", request.method(), request.url());
                for header in request.headers() {
                    record.push_str(&format!("{}: {}\n", header.field, header.value));
                }
                recorded_requests
                    .lock()
                    .expect("request records")
                    .push(record);
                let response = responses
                    .lock()
                    .expect("fake responses")
                    .pop_front()
                    .unwrap_or_else(FakeResponse::default_png);
                let active = Arc::clone(&response_active);
                let max_active = Arc::clone(&response_max_active);
                std::thread::spawn(move || response.respond(request, &active, &max_active));
            }
        });
        Self {
            port,
            requests,
            max_active,
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("request records").clone()
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
struct FakeResponse {
    status: u16,
    body: Vec<u8>,
    headers: Vec<(&'static str, &'static str)>,
    delay: Option<Duration>,
    disconnect: bool,
}

impl FakeResponse {
    fn default_png() -> Self {
        Self::png(
            b"\x89PNG\r\n\x1a\nOVRLEY tile fixture",
            "public, max-age=3600",
        )
    }

    fn png(body: &[u8], cache_control: &'static str) -> Self {
        Self {
            status: 200,
            body: body.to_vec(),
            headers: vec![
                ("Content-Type", "image/png"),
                ("Cache-Control", cache_control),
            ],
            delay: None,
            disconnect: false,
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
            headers: Vec::new(),
            delay: None,
            disconnect: false,
        }
    }

    fn text(body: &str) -> Self {
        Self {
            status: 200,
            body: body.as_bytes().to_vec(),
            headers: vec![("Content-Type", "text/plain")],
            delay: None,
            disconnect: false,
        }
    }

    fn disconnect() -> Self {
        Self {
            status: 0,
            body: Vec::new(),
            headers: Vec::new(),
            delay: None,
            disconnect: true,
        }
    }

    fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers.push((name, value));
        self
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    fn respond(self, request: tiny_http::Request, active: &AtomicUsize, max_active: &AtomicUsize) {
        let active_now = active.fetch_add(1, Ordering::SeqCst) + 1;
        max_active.fetch_max(active_now, Ordering::SeqCst);
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        if self.disconnect {
            active.fetch_sub(1, Ordering::SeqCst);
            return;
        }
        let mut response = tiny_http::Response::from_data(self.body)
            .with_status_code(tiny_http::StatusCode(self.status));
        for (name, value) in self.headers {
            response.add_header(
                tiny_http::Header::from_bytes(name, value).expect("fake response header"),
            );
        }
        let _ = request.respond(response);
        active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn concurrent_requests(port: u16, paths: Vec<&'static str>) -> Vec<Vec<u8>> {
    let barrier = Arc::new(Barrier::new(paths.len() + 1));
    let handles = paths
        .into_iter()
        .map(|path| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                request(port, "GET", path)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    handles
        .into_iter()
        .map(|handle| handle.join().expect("request thread"))
        .collect()
}

fn request(port: u16, method: &str, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect server");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
    )
    .expect("write request");

    let mut response = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&buffer[..count]);
                if response_is_complete(&response) {
                    break;
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                panic!("timed out reading response")
            }
            Err(error) => panic!("read response: {error}"),
        }
    }
    response
}

fn response_is_complete(response: &[u8]) -> bool {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    response.len() >= header_end + 4 + content_length
}

fn response_headers(response: &[u8]) -> String {
    let marker = b"\r\n\r\n";
    let end = response
        .windows(marker.len())
        .position(|window| window == marker)
        .expect("response headers");
    String::from_utf8_lossy(&response[..end]).into_owned()
}

fn response_body(response: &[u8]) -> &[u8] {
    let marker = b"\r\n\r\n";
    let end = response
        .windows(marker.len())
        .position(|window| window == marker)
        .expect("response body");
    &response[end + marker.len()..]
}
