//! The tile cache against a fake upstream: persistence, revalidation, stale tiles, shared fetches,
//! eviction and clearing.

use crate::map_tile_service::{MapTileService, TileCacheStatus, TileCoordinates};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

#[test]
fn fetches_persists_and_reuses_a_vector_tile() {
    let tile_bytes = b"\x89PBF\r\n\x1a\nOVRLEY tile fixture";
    let fake_upstream = FakeUpstream::start();
    let cache_root = temp_cache_root("cache-miss-hit");
    let service = tile_service(&cache_root, &fake_upstream);

    let first = service.resolve(tile_at(3)).expect("fetched tile");
    assert!(matches!(first.cache_status, TileCacheStatus::Miss));
    assert_eq!(first.bytes, tile_bytes);

    let requests = fake_upstream.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /2/1/3.pbf\n"));
    let request_headers = requests[0].to_ascii_lowercase();
    assert!(request_headers.contains(&format!(
        "user-agent: ovrley/{} (+https://www.ovrley.cc)",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(!request_headers.contains("cache-control:"));
    assert!(!request_headers.contains("pragma:"));

    let payload_path = cache_root.join("openfreemap-planet/2/1/3.pbf");
    let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");
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

    let second = service.resolve(tile_at(3)).expect("cached tile");
    assert!(matches!(second.cache_status, TileCacheStatus::Hit));
    assert_eq!(second.bytes, tile_bytes);
    assert_eq!(fake_upstream.requests().len(), 1);

    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn incomplete_and_malformed_cache_pairs_are_refetched() {
    let tile_bytes = b"\x89PBF\r\n\x1a\nOVRLEY tile fixture";
    let fake_upstream = FakeUpstream::start();
    let cache_root = temp_cache_root("invalid-pairs");
    let service = tile_service(&cache_root, &fake_upstream);
    let payload_path = cache_root.join("openfreemap-planet/2/1/3.pbf");
    let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

    assert_eq!(
        fetch(&service, 3),
        tile_bytes
    );

    std::fs::remove_file(&metadata_path).expect("remove metadata");
    assert_eq!(
        fetch(&service, 3),
        tile_bytes
    );

    std::fs::remove_file(&payload_path).expect("remove payload");
    assert_eq!(
        fetch(&service, 3),
        tile_bytes
    );

    std::fs::write(&metadata_path, b"not JSON").expect("corrupt metadata");
    assert_eq!(
        fetch(&service, 3),
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
    let tile_bytes = b"\x89PBF\r\n\x1a\nold tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(tile_bytes, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::empty(304)
            .with_header("Cache-Control", "public, max-age=3600")
            .with_header("ETag", "\"tile-v1\""),
    ]);
    let cache_root = temp_cache_root("revalidate-304");
    let service = tile_service(&cache_root, &fake_upstream);
    let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

    assert_eq!(
        fetch(&service, 3),
        tile_bytes
    );
    let metadata_before = std::fs::read(&metadata_path).expect("initial metadata");

    assert_eq!(
        fetch(&service, 3),
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
        fetch(&service, 3),
        tile_bytes
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn changed_revalidation_replaces_payload_and_policy() {
    let old_tile = b"\x89PBF\r\n\x1a\nold tile";
    let new_tile = b"\x89PBF\r\n\x1a\nnew tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(old_tile, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::pbf(new_tile, "public, max-age=3600").with_header("ETag", "\"tile-v2\""),
    ]);
    let cache_root = temp_cache_root("revalidate-changed");
    let service = tile_service(&cache_root, &fake_upstream);
    let payload_path = cache_root.join("openfreemap-planet/2/1/3.pbf");
    let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

    assert_eq!(
        fetch(&service, 3),
        old_tile
    );
    let old_metadata = std::fs::read(&metadata_path).expect("old metadata");

    assert_eq!(
        fetch(&service, 3),
        new_tile
    );
    assert_eq!(std::fs::read(&payload_path).expect("new payload"), new_tile);
    assert_ne!(
        std::fs::read(&metadata_path).expect("new metadata"),
        old_metadata
    );

    assert_eq!(
        fetch(&service, 3),
        new_tile
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn non_storable_response_is_returned_without_persistence() {
    let tile = b"\x89PBF\r\n\x1a\nprivate tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(tile, "no-store"),
        FakeResponse::pbf(tile, "no-store"),
    ]);
    let cache_root = temp_cache_root("no-store");
    let service = tile_service(&cache_root, &fake_upstream);

    assert_eq!(
        fetch(&service, 3),
        tile
    );
    assert_eq!(
        fetch(&service, 3),
        tile
    );
    assert_eq!(fake_upstream.requests().len(), 2);
    assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
    assert!(!cache_root
        .join("openfreemap-planet/2/1/3.pbf.json")
        .exists());
}

#[test]
fn stale_tile_survives_upstream_failures_without_metadata_changes() {
    let tile = b"\x89PBF\r\n\x1a\nstale tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(tile, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
        FakeResponse::empty(500),
        FakeResponse::text("not a PBF"),
        FakeResponse::disconnect(),
        FakeResponse::pbf(b"\x89PBF\r\n\x1a\ntoo late", "public, max-age=3600")
            .with_delay(Duration::from_millis(400)),
    ]);
    let cache_root = temp_cache_root("stale-on-error");
    let service = tile_service_with_timeouts(&cache_root, &fake_upstream, Duration::from_millis(100));
    let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

    assert_eq!(
        fetch(&service, 3),
        tile
    );
    let original_metadata = std::fs::read(&metadata_path).expect("initial metadata");

    for _ in 0..4 {
        assert_eq!(
            fetch(&service, 3),
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
fn uncached_upstream_failure_is_an_error() {
    let fake_upstream = FakeUpstream::start_with(vec![FakeResponse::empty(503)]);
    let cache_root = temp_cache_root("uncached-error");
    let service = tile_service(&cache_root, &fake_upstream);

    assert!(service.resolve(tile_at(3)).is_err());
    assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
    assert!(!cache_root
        .join("openfreemap-planet/2/1/3.pbf.json")
        .exists());
}

#[test]
fn concurrent_same_tile_requests_share_one_upstream_fetch() {
    let tile = b"\x89PBF\r\n\x1a\nshared tile";
    let fake_upstream =
        FakeUpstream::start_with(vec![
            FakeResponse::pbf(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))
        ]);
    let cache_root = temp_cache_root("same-tile-dedup");
    let service = tile_service(&cache_root, &fake_upstream);

    let responses = concurrent_requests(&service, vec![3; 4]);
    assert!(responses
        .iter()
        .all(|response| response.as_deref() == Ok(&tile[..])));
    assert_eq!(fake_upstream.requests().len(), 1);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn different_tiles_fetch_concurrently() {
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::default_pbf().with_delay(Duration::from_millis(200)),
        FakeResponse::default_pbf().with_delay(Duration::from_millis(200)),
    ]);
    let cache_root = temp_cache_root("different-tile-concurrency");
    let service = tile_service(&cache_root, &fake_upstream);

    let responses = concurrent_requests(&service, vec![2, 3]);
    assert!(responses.iter().all(|response| response.is_ok()));
    assert_eq!(fake_upstream.requests().len(), 2);
    assert!(fake_upstream.max_active() >= 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn successful_write_evicts_globally_oldest_entry_and_retains_recently_served_tile() {
    let tile = b"\x89PBF\r\n\x1a\nLRU tile fixture";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(tile, "public, max-age=3600"),
        FakeResponse::pbf(tile, "public, max-age=3600"),
        FakeResponse::pbf(tile, "public, max-age=3600"),
        FakeResponse::pbf(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("global-lru");
    let obsolete_service = MapTileService::new_with_cache_limit(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf",
            fake_upstream.port
        ),
        "obsolete-openfreemap".to_string(),
        u64::MAX,
    )
    .expect("obsolete tile service");
    for y in [1, 2, 3] {
        assert_eq!(fetch(&obsolete_service, y), tile);
    }

    let first = cache_root.join("obsolete-openfreemap/2/1/1.pbf");
    let second = cache_root.join("obsolete-openfreemap/2/1/2.pbf");
    let third = cache_root.join("obsolete-openfreemap/2/1/3.pbf");
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

    let first_sidecar = first.with_extension("pbf.json");
    let first_metadata = std::fs::read(&first_sidecar).expect("first tile metadata");
    let first_metadata_modified = std::fs::metadata(&first_sidecar)
        .expect("first sidecar metadata")
        .modified()
        .expect("first sidecar modification time");
    assert_eq!(
        fetch(&obsolete_service, 1),
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
    drop(obsolete_service);

    let interrupted = cache_root.join("obsolete-openfreemap/4/5/.interrupted.pbf.tmp");
    let orphan = cache_root.join("obsolete-openfreemap/4/5/6.pbf.json");
    std::fs::create_dir_all(interrupted.parent().expect("artifact parent"))
        .expect("create artifact directory");
    std::fs::write(&interrupted, b"partial payload").expect("write interrupted payload");
    std::fs::write(&orphan, b"orphan sidecar").expect("write orphan sidecar");

    let active_service = MapTileService::new_with_cache_limit(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf",
            fake_upstream.port
        ),
        "openfreemap-planet".to_string(),
        cache_limit,
    )
    .expect("active tile service");
    assert!(!interrupted.exists());
    assert!(!orphan.exists());
    assert_eq!(
        fetch(&active_service, 0),
        tile
    );

    assert!(first.exists(), "recently served tile must be retained");
    assert!(!second.exists(), "globally oldest tile must be evicted");
    assert!(!cache_root
        .join("obsolete-openfreemap/2/1/2.pbf.json")
        .exists());
    assert!(third.exists());
    assert!(cache_root.join("openfreemap-planet/2/1/0.pbf").exists());
    assert_eq!(fake_upstream.requests().len(), 4);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn clear_removes_all_namespaces_and_a_later_request_repopulates_the_cache() {
    let tile = b"\x89PBF\r\n\x1a\nclear tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::pbf(tile, "public, max-age=3600"),
        FakeResponse::pbf(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("clear");
    let map_tile_service = MapTileService::new(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf",
            fake_upstream.port
        ),
        "openfreemap-planet".to_string(),
    )
    .expect("tile service");

    assert_eq!(
        fetch(&map_tile_service, 3),
        tile
    );
    let obsolete = cache_root.join("obsolete-source/0/0");
    std::fs::create_dir_all(&obsolete).expect("create obsolete namespace");
    std::fs::write(obsolete.join("0.pbf.tmp"), b"interrupted").expect("write artifact");

    map_tile_service.clear().expect("clear tile cache");
    assert!(!cache_root.exists());

    assert_eq!(
        fetch(&map_tile_service, 3),
        tile
    );
    assert!(cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

#[test]
fn clear_waits_for_active_tile_resolution_without_disrupting_its_response() {
    let tile = b"\x89PBF\r\n\x1a\nactive tile";
    let fake_upstream =
        FakeUpstream::start_with(vec![
            FakeResponse::pbf(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))
        ]);
    let cache_root = temp_cache_root("clear-active");
    let map_tile_service = MapTileService::new(
        cache_root.clone(),
        format!(
            "http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf",
            fake_upstream.port
        ),
        "openfreemap-planet".to_string(),
    )
    .expect("tile service");

    let resolving = map_tile_service.clone();
    let active_request = std::thread::spawn(move || fetch(&resolving, 3));
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while fake_upstream.requests().is_empty() && std::time::Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(fake_upstream.requests().len(), 1);

    map_tile_service
        .clear()
        .expect("clear waits for active request");
    let response = active_request.join().expect("active request completes");
    assert_eq!(response, tile);
    assert!(!cache_root.exists());
}

#[test]
fn failed_same_tile_request_does_not_poison_later_retry() {
    let tile = b"\x89PBF\r\n\x1a\nretry tile";
    let fake_upstream = FakeUpstream::start_with(vec![
        FakeResponse::empty(500).with_delay(Duration::from_millis(200)),
        FakeResponse::pbf(tile, "public, max-age=3600"),
    ]);
    let cache_root = temp_cache_root("failed-dedup-retry");
    let service = tile_service(&cache_root, &fake_upstream);

    let failures = concurrent_requests(&service, vec![3; 2]);
    assert!(failures.iter().all(|response| response.is_err()));
    assert_eq!(fake_upstream.requests().len(), 1);

    assert_eq!(fetch(&service, 3), tile);
    assert_eq!(fake_upstream.requests().len(), 2);
    std::fs::remove_dir_all(cache_root).expect("remove tile cache");
}

fn tile_service(cache_root: &Path, upstream: &FakeUpstream) -> MapTileService {
    MapTileService::new(
        cache_root.to_path_buf(),
        format!("http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf", upstream.port),
        "openfreemap-planet".to_string(),
    )
    .expect("tile service")
}

fn tile_service_with_timeouts(
    cache_root: &Path,
    upstream: &FakeUpstream,
    request_timeout: Duration,
) -> MapTileService {
    MapTileService::new_with_timeouts(
        cache_root.to_path_buf(),
        format!("http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf", upstream.port),
        "openfreemap-planet".to_string(),
        request_timeout,
        request_timeout,
    )
    .expect("tile service")
}

/// The tile `2/1/<y>`.
fn tile_at(y: u32) -> TileCoordinates {
    TileCoordinates { zoom: 2, x: 1, y }
}

fn fetch(service: &MapTileService, y: u32) -> Vec<u8> {
    service.resolve(tile_at(y)).expect("resolved tile").bytes
}

fn temp_cache_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ovrley-tile-cache-{label}-{}", Uuid::new_v4()))
}

fn cache_pair_size(payload: &PathBuf) -> u64 {
    let metadata = payload.with_extension("pbf.json");
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
                    .unwrap_or_else(FakeResponse::default_pbf);
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
    fn default_pbf() -> Self {
        Self::pbf(
            b"\x89PBF\r\n\x1a\nOVRLEY tile fixture",
            "public, max-age=3600",
        )
    }

    fn pbf(body: &[u8], cache_control: &'static str) -> Self {
        Self {
            status: 200,
            body: body.to_vec(),
            headers: vec![
                ("Content-Type", "application/vnd.mapbox-vector-tile"),
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

fn concurrent_requests(service: &MapTileService, rows: Vec<u32>) -> Vec<Result<Vec<u8>, String>> {
    let barrier = Arc::new(Barrier::new(rows.len() + 1));
    let handles = rows
        .into_iter()
        .map(|y| {
            let barrier = Arc::clone(&barrier);
            let service = service.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service.resolve(tile_at(y)).map(|payload| payload.bytes)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    handles
        .into_iter()
        .map(|handle| handle.join().expect("request thread"))
        .collect()
}
