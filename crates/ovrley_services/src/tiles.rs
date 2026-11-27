//! OpenFreeMap vector tiles, fetched on demand and kept in a size-limited disk cache that follows
//! the server's HTTP caching rules. The styles that read them are installed with the build: see
//! [`crate::maps`].
//!
//! Each tile is a payload file and a JSON metadata file beside it, which holds the cache policy and
//! what the payload must be. A pair that doesn't match its metadata is refetched; a tile that
//! cannot be revalidated is served stale; concurrent requests for one tile share one fetch.

use crate::cache::{remove_dir_if_present, remove_if_present, touch, CacheLimit, CacheUsage, LimitedCache};
use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use reqwest::blocking::{Client, Request, Response};
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

const OPENFREEMAP_TILE_URL: &str = "https://tiles.openfreemap.org/planet/latest/{z}/{x}/{y}.pbf";
const OPENFREEMAP_CACHE_NAMESPACE: &str = "openfreemap-planet";
const TILE_DIR: &str = "tiles";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// The metadata's format; a pair of another version is refetched.
const METADATA_VERSION: u8 = 2;
/// The deepest zoom vector tiles exist at.
const MAX_ZOOM: u8 = 14;

/// The tile cache's limit when the settings hold none.
pub const DEFAULT_TILE_CACHE_LIMIT: CacheLimit = CacheLimit::steps(4);

/// The tile source and its disk cache. Clones share the cache, its accounting and its fetches.
#[derive(Clone)]
pub struct TileCache {
    cache_root: PathBuf,
    source_url: String,
    cache_namespace: String,
    client: Client,
    in_flight: Arc<Mutex<HashMap<TileCoordinates, Arc<InFlightTile>>>>,
    maintenance: Arc<CacheMaintenance>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TileCoordinates {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedTileMetadata {
    version: u8,
    cache_namespace: String,
    zoom: u8,
    x: u32,
    y: u32,
    content_length: u64,
    policy: CachePolicy,
}

impl CachedTileMetadata {
    /// Whether this metadata belongs to a payload of `payload_len` bytes at `coordinates`.
    fn describes(&self, namespace: &str, coordinates: TileCoordinates, payload_len: u64) -> bool {
        self.version == METADATA_VERSION
            && self.cache_namespace == namespace
            && (self.zoom, self.x, self.y) == (coordinates.zoom, coordinates.x, coordinates.y)
            && self.content_length == payload_len
            && self.policy.is_storable()
    }
}

struct CachedTile {
    payload: Vec<u8>,
    metadata: CachedTileMetadata,
}

struct InFlightTile {
    result: Mutex<Option<Result<TilePayload, String>>>,
    ready: Condvar,
}

#[derive(Clone)]
pub(crate) struct TilePayload {
    pub bytes: Vec<u8>,
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests check where a tile came from"))]
    pub cache_status: TileCacheStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileCacheStatus {
    Hit,
    Miss,
    Revalidated,
    Refreshed,
    Stale,
}

struct CacheMaintenance {
    /// Fetches hold it shared; a full rescan or a clear holds it exclusively.
    gate: RwLock<()>,
    accounting: Mutex<CacheAccounting>,
}

struct CacheAccounting {
    total_size: u64,
    limit: CacheLimit,
    entries: HashMap<PathBuf, u64>,
}

struct ResolveOutcome {
    payload: Vec<u8>,
    persistent_write: bool,
    cache_status: TileCacheStatus,
}

struct PersistentEntry {
    paths: CachePaths,
    size: u64,
    modified: SystemTime,
}

struct CachePaths {
    directory: PathBuf,
    payload: PathBuf,
    metadata: PathBuf,
}

#[derive(Clone, Copy)]
enum CacheWrite<'a> {
    PayloadAndMetadata(&'a [u8]),
    MetadataOnly,
}

fn is_valid_namespace(namespace: &str) -> bool {
    !namespace.is_empty() && namespace.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

impl TileCache {
    /// Opens the OpenFreeMap tile cache in `app_cache_dir` and evicts down to `limit`.
    ///
    /// # Errors
    ///
    /// When the HTTP client cannot be built, or the cache cannot be scanned.
    pub fn open(app_cache_dir: &Path, limit: CacheLimit) -> Result<Self, String> {
        Self::with_source(
            app_cache_dir.join(TILE_DIR),
            OPENFREEMAP_TILE_URL.into(),
            OPENFREEMAP_CACHE_NAMESPACE.into(),
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
            limit,
        )
    }

    fn with_source(
        cache_root: PathBuf,
        source_url: String,
        cache_namespace: String,
        connect_timeout: Duration,
        request_timeout: Duration,
        limit: CacheLimit,
    ) -> Result<Self, String> {
        debug_assert!(["{z}", "{x}", "{y}"].iter().all(|token| source_url.contains(token)), "{source_url} names z, x and y");
        debug_assert!(is_valid_namespace(&cache_namespace), "{cache_namespace} is a valid namespace");
        let client = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .user_agent(format!("OVRLEY/{} (+https://www.ovrley.cc)", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("Failed to create tile HTTP client: {error}"))?;
        let cache = Self {
            cache_root,
            source_url,
            cache_namespace,
            client,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            maintenance: Arc::new(CacheMaintenance {
                gate: RwLock::new(()),
                accounting: Mutex::new(CacheAccounting { total_size: 0, limit, entries: HashMap::new() }),
            }),
        };
        cache.reconcile_and_evict()?;
        Ok(cache)
    }

    fn accounting(&self) -> MutexGuard<'_, CacheAccounting> {
        self.maintenance.accounting.lock().expect("tile cache accounting lock poisoned")
    }

    /// The tile at `coordinates`, from the cache or the network. A request for a tile being fetched
    /// waits for that fetch.
    pub(crate) fn resolve(&self, coordinates: TileCoordinates) -> Result<TilePayload, String> {
        let (in_flight, owns_resolution) = {
            let mut requests = self.in_flight.lock().expect("in-flight tiles lock poisoned");
            match requests.get(&coordinates) {
                Some(in_flight) => (Arc::clone(in_flight), false),
                None => {
                    let in_flight = Arc::new(InFlightTile { result: Mutex::new(None), ready: Condvar::new() });
                    requests.insert(coordinates, Arc::clone(&in_flight));
                    (in_flight, true)
                }
            }
        };

        if !owns_resolution {
            let result = in_flight.result.lock().expect("in-flight tile lock poisoned");
            let result = in_flight.ready.wait_while(result, |result| result.is_none()).expect("in-flight tile lock poisoned");
            return result.clone().expect("the waited-for result is set");
        }

        let result = self.resolve_owned(coordinates);
        *in_flight.result.lock().expect("in-flight tile lock poisoned") = Some(result.clone());
        in_flight.ready.notify_all();
        self.in_flight.lock().expect("in-flight tiles lock poisoned").remove(&coordinates);
        result
    }

    fn resolve_owned(&self, coordinates: TileCoordinates) -> Result<TilePayload, String> {
        let outcome = {
            let _access = self.maintenance.gate.read().expect("tile cache gate poisoned");
            self.resolve_with_access(coordinates)?
        };
        if outcome.persistent_write {
            self.evict_if_needed()?;
        }
        Ok(TilePayload { bytes: outcome.payload, cache_status: outcome.cache_status })
    }

    fn resolve_with_access(&self, coordinates: TileCoordinates) -> Result<ResolveOutcome, String> {
        let paths = self.cache_paths(coordinates);
        let upstream_url = self
            .source_url
            .replace("{z}", &coordinates.zoom.to_string())
            .replace("{x}", &coordinates.x.to_string())
            .replace("{y}", &coordinates.y.to_string());
        let mut request =
            self.client.get(upstream_url).build().map_err(|error| format!("Failed to build tile upstream request: {error}"))?;

        if let Some(cached) = self.read_cached(&paths, coordinates)? {
            match cached.metadata.policy.before_request(&cache_request(&request)?, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    touch_tile(&paths.payload)?;
                    return Ok(ResolveOutcome { payload: cached.payload, persistent_write: false, cache_status: TileCacheStatus::Hit });
                }
                BeforeRequest::Stale { request: conditional, matches: true } => {
                    *request.headers_mut() = conditional.headers;
                    return self.fetch(request, Some(cached), &paths, coordinates);
                }
                BeforeRequest::Stale { matches: false, .. } => self.remove_pair(&paths)?,
            }
        }
        self.fetch(request, None, &paths, coordinates)
    }

    fn fetch(
        &self,
        request: Request,
        cached: Option<CachedTile>,
        paths: &CachePaths,
        coordinates: TileCoordinates,
    ) -> Result<ResolveOutcome, String> {
        let policy_request = cache_request(&request)?;
        let response = match self.client.execute(request) {
            Ok(response) => response,
            Err(error) => return cached_fallback(cached, paths, || format!("Tile upstream request failed: {error}")),
        };
        let response_time = SystemTime::now();
        let policy_response = cache_response(&response)?;

        if response.status() == StatusCode::NOT_MODIFIED {
            let Some(cached) = cached else {
                return Err("Tile upstream returned 304 without a cached tile".into());
            };
            touch_tile(&paths.payload)?;
            let persistent_write = match cached.metadata.policy.after_response(&policy_request, &policy_response, response_time) {
                AfterResponse::NotModified(policy, _) if policy.is_storable() => {
                    self.persist(paths, &self.metadata(coordinates, &cached.payload, policy), CacheWrite::MetadataOnly)?;
                    true
                }
                _ => false,
            };
            return Ok(ResolveOutcome { payload: cached.payload, persistent_write, cache_status: TileCacheStatus::Revalidated });
        }

        if response.status() != StatusCode::OK || !is_vector_tile_response(response.headers()) {
            let status = response.status().as_u16();
            return cached_fallback(cached, paths, || format!("Tile upstream returned invalid HTTP response {status}"));
        }

        let (cache_status, policy) = match cached.as_ref() {
            Some(cached) => match cached.metadata.policy.after_response(&policy_request, &policy_response, response_time) {
                AfterResponse::Modified(policy, _) | AfterResponse::NotModified(policy, _) => (TileCacheStatus::Refreshed, policy),
            },
            None => (TileCacheStatus::Miss, CachePolicy::new(&policy_request, &policy_response)),
        };
        let payload = match response.bytes() {
            Ok(payload) => payload.to_vec(),
            Err(error) => return cached_fallback(cached, paths, || format!("Failed to read tile upstream response: {error}")),
        };

        let persistent_write = policy.is_storable();
        if persistent_write {
            self.persist(paths, &self.metadata(coordinates, &payload, policy), CacheWrite::PayloadAndMetadata(&payload))?;
        }
        Ok(ResolveOutcome { payload, persistent_write, cache_status })
    }

    fn metadata(&self, coordinates: TileCoordinates, payload: &[u8], policy: CachePolicy) -> CachedTileMetadata {
        CachedTileMetadata {
            version: METADATA_VERSION,
            cache_namespace: self.cache_namespace.clone(),
            zoom: coordinates.zoom,
            x: coordinates.x,
            y: coordinates.y,
            content_length: payload.len() as u64,
            policy,
        }
    }

    fn cache_paths(&self, coordinates: TileCoordinates) -> CachePaths {
        let directory = self
            .cache_root
            .join(&self.cache_namespace)
            .join(coordinates.zoom.to_string())
            .join(coordinates.x.to_string());
        CachePaths {
            payload: directory.join(format!("{}.pbf", coordinates.y)),
            metadata: directory.join(format!("{}.pbf.json", coordinates.y)),
            directory,
        }
    }

    /// The cached pair at `paths`; a pair that is incomplete or doesn't match its metadata is
    /// removed and reads as a miss.
    fn read_cached(&self, paths: &CachePaths, coordinates: TileCoordinates) -> Result<Option<CachedTile>, String> {
        let exists = |path: &Path| {
            path.try_exists().map_err(|error| format!("Failed to inspect tile cache {}: {error}", path.display()))
        };
        if !exists(&paths.payload)? && !exists(&paths.metadata)? {
            return Ok(None);
        }
        let cached = fs::read(&paths.payload).ok().zip(fs::read(&paths.metadata).ok()).and_then(|(payload, metadata)| {
            let metadata = serde_json::from_slice::<CachedTileMetadata>(&metadata).ok()?;
            metadata
                .describes(&self.cache_namespace, coordinates, payload.len() as u64)
                .then_some(CachedTile { payload, metadata })
        });
        if cached.is_none() {
            self.remove_pair(paths)?;
        }
        Ok(cached)
    }

    fn persist(&self, paths: &CachePaths, metadata: &CachedTileMetadata, write: CacheWrite<'_>) -> Result<(), String> {
        let mut accounting = self.accounting();
        if matches!(write, CacheWrite::PayloadAndMetadata(_)) {
            fs::create_dir_all(&paths.directory).map_err(|error| {
                format!("Failed to create tile cache directory {}: {error}", paths.directory.display())
            })?;
        }
        let transaction = Uuid::new_v4();
        let temporary = CachePaths {
            directory: paths.directory.clone(),
            payload: paths.directory.join(format!(".{transaction}.pbf.tmp")),
            metadata: paths.directory.join(format!(".{transaction}.json.tmp")),
        };
        let metadata_bytes =
            serde_json::to_vec(metadata).map_err(|error| format!("Failed to serialize tile cache metadata: {error}"))?;
        let result = write_cache_entry(paths, &temporary, &metadata_bytes, write);
        let _ = fs::remove_file(&temporary.payload);
        let _ = fs::remove_file(&temporary.metadata);
        let current_size = match &result {
            Ok(()) => pair_size(paths)?,
            Err(_) => {
                let _ = remove_if_present(&paths.metadata);
                let _ = remove_if_present(&paths.payload);
                0
            }
        };
        replace_accounted_entry(&mut accounting, &paths.payload, current_size);
        result
    }

    fn remove_pair(&self, paths: &CachePaths) -> Result<(), String> {
        let mut accounting = self.accounting();
        remove_if_present(&paths.metadata)?;
        remove_if_present(&paths.payload)?;
        replace_accounted_entry(&mut accounting, &paths.payload, 0);
        Ok(())
    }

    fn evict_if_needed(&self) -> Result<(), String> {
        let over_limit = {
            let accounting = self.accounting();
            accounting.total_size > accounting.limit.bytes()
        };
        if over_limit {
            self.reconcile_and_evict()?;
        }
        Ok(())
    }

    /// Recounts the cache, deletes everything that is not a valid pair, and deletes the least
    /// recently used pairs until the cache fits its limit.
    fn reconcile_and_evict(&self) -> Result<(), String> {
        let _maintenance = self.maintenance.gate.write().expect("tile cache gate poisoned");
        let mut entries = scan_persistent_entries(&self.cache_root)?;
        entries.sort_by_key(|entry| entry.modified);

        let mut accounting = self.accounting();
        let mut total_size = entries.iter().fold(0_u64, |total, entry| total.saturating_add(entry.size));
        let mut retained_entries = HashMap::new();
        for entry in entries {
            if total_size > accounting.limit.bytes() {
                remove_if_present(&entry.paths.metadata)?;
                remove_if_present(&entry.paths.payload)?;
                total_size = total_size.saturating_sub(entry.size);
            } else {
                retained_entries.insert(entry.paths.payload, entry.size);
            }
        }
        accounting.total_size = total_size;
        accounting.entries = retained_entries;
        Ok(())
    }
}

impl LimitedCache for TileCache {
    fn usage(&self) -> CacheUsage {
        let accounting = self.accounting();
        accounting.limit.usage(accounting.total_size)
    }

    fn set_limit(&self, limit: CacheLimit) -> Result<CacheUsage, String> {
        self.accounting().limit = limit;
        self.reconcile_and_evict()?;
        Ok(self.usage())
    }

    // Waits for running fetches first.
    fn clear(&self) -> Result<CacheUsage, String> {
        let _maintenance = self.maintenance.gate.write().expect("tile cache gate poisoned");
        remove_dir_if_present(&self.cache_root)?;
        let mut accounting = self.accounting();
        accounting.total_size = 0;
        accounting.entries.clear();
        Ok(accounting.limit.usage(0))
    }
}

fn touch_tile(path: &Path) -> Result<(), String> {
    touch(path).map_err(|error| format!("Failed to update cached tile access time {}: {error}", path.display()))
}

/// The cached tile, stale, when the upstream failed; else the upstream's `error`.
fn cached_fallback(
    cached: Option<CachedTile>,
    paths: &CachePaths,
    error: impl FnOnce() -> String,
) -> Result<ResolveOutcome, String> {
    let cached = cached.ok_or_else(error)?;
    touch_tile(&paths.payload)?;
    Ok(ResolveOutcome { payload: cached.payload, persistent_write: false, cache_status: TileCacheStatus::Stale })
}

fn write_cache_entry(paths: &CachePaths, temporary: &CachePaths, metadata: &[u8], write: CacheWrite<'_>) -> Result<(), String> {
    if let CacheWrite::PayloadAndMetadata(payload) = write {
        write_synced(&temporary.payload, payload)?;
    }
    write_synced(&temporary.metadata, metadata)?;
    remove_if_present(&paths.metadata)?;
    if matches!(write, CacheWrite::PayloadAndMetadata(_)) {
        remove_if_present(&paths.payload)?;
        fs::rename(&temporary.payload, &paths.payload)
            .map_err(|error| format!("Failed to promote tile payload into cache: {error}"))?;
    }
    fs::rename(&temporary.metadata, &paths.metadata).map_err(|error| format!("Failed to promote tile metadata into cache: {error}"))
}

fn scan_persistent_entries(cache_root: &Path) -> Result<Vec<PersistentEntry>, String> {
    let exists = cache_root
        .try_exists()
        .map_err(|error| format!("Failed to inspect tile cache root {}: {error}", cache_root.display()))?;
    if !exists {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_cache_files(cache_root, &mut files)?;
    let mut valid_files = HashSet::new();
    let mut entries = Vec::new();
    for payload in files.iter().filter(|path| path.extension().is_some_and(|extension| extension == "pbf")) {
        if let Some(entry) = inspect_persistent_entry(cache_root, payload)? {
            valid_files.insert(entry.paths.payload.clone());
            valid_files.insert(entry.paths.metadata.clone());
            entries.push(entry);
        }
    }

    // Everything else goes, whatever an earlier version kept here included.
    for artifact in files {
        if !valid_files.contains(&artifact) {
            remove_if_present(&artifact)?;
        }
    }
    Ok(entries)
}

fn collect_cache_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Failed to read tile cache directory {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("Failed to inspect tile cache directory {}: {error}", directory.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect tile cache artifact {}: {error}", entry.path().display()))?;
        if file_type.is_dir() {
            collect_cache_files(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

/// The pair whose payload is `payload_path`, when its place names a tile and its metadata
/// describes it.
fn inspect_persistent_entry(cache_root: &Path, payload_path: &Path) -> Result<Option<PersistentEntry>, String> {
    let relative = payload_path
        .strip_prefix(cache_root)
        .map_err(|error| format!("Tile cache entry {} is outside cache root: {error}", payload_path.display()))?;
    let components: Vec<&str> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect();
    let [namespace, zoom, x, y] = components[..] else {
        return Ok(None);
    };
    let (Some(zoom), Some(x), Some(y)) = (
        parse_decimal(zoom).and_then(|zoom| u8::try_from(zoom).ok()).filter(|zoom| *zoom <= MAX_ZOOM),
        parse_decimal(x),
        y.strip_suffix(".pbf").and_then(parse_decimal),
    ) else {
        return Ok(None);
    };
    let coordinate_limit = 1_u32 << zoom;
    if !is_valid_namespace(namespace) || x >= coordinate_limit || y >= coordinate_limit {
        return Ok(None);
    }

    let metadata_path = payload_path.with_extension("pbf.json");
    let payload_file = fs::metadata(payload_path)
        .map_err(|error| format!("Failed to inspect cached tile payload {}: {error}", payload_path.display()))?;
    let Some(metadata) = fs::read(&metadata_path).ok().and_then(|bytes| serde_json::from_slice::<CachedTileMetadata>(&bytes).ok())
    else {
        return Ok(None);
    };
    if !metadata.describes(namespace, TileCoordinates { zoom, x, y }, payload_file.len()) {
        return Ok(None);
    }

    let metadata_file = fs::metadata(&metadata_path)
        .map_err(|error| format!("Failed to inspect cached tile metadata {}: {error}", metadata_path.display()))?;
    Ok(Some(PersistentEntry {
        paths: CachePaths {
            directory: payload_path.parent().expect("a payload under the cache root has a parent").to_path_buf(),
            payload: payload_path.to_path_buf(),
            metadata: metadata_path,
        },
        size: payload_file.len() + metadata_file.len(),
        modified: payload_file
            .modified()
            .map_err(|error| format!("Failed to read cached tile access time {}: {error}", payload_path.display()))?,
    }))
}

fn pair_size(paths: &CachePaths) -> Result<u64, String> {
    let size = |path: &Path| match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!("Failed to inspect tile cache file {}: {error}", path.display())),
    };
    Ok(size(&paths.payload)? + size(&paths.metadata)?)
}

fn replace_accounted_entry(accounting: &mut CacheAccounting, payload: &Path, current_size: u64) {
    if let Some(previous_size) = accounting.entries.remove(payload) {
        accounting.total_size = accounting.total_size.saturating_sub(previous_size);
    }
    if current_size > 0 {
        accounting.entries.insert(payload.to_path_buf(), current_size);
        accounting.total_size = accounting.total_size.saturating_add(current_size);
    }
}

fn is_vector_tile_response(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| {
            matches!(
                media_type.trim().to_ascii_lowercase().as_str(),
                "application/x-protobuf" | "application/vnd.mapbox-vector-tile" | "application/octet-stream"
            )
        })
}

/// The request as the cache policy reads it.
fn cache_request(request: &Request) -> Result<http::Request<()>, String> {
    let mut cache_request = http::Request::builder()
        .method(request.method().clone())
        .uri(request.url().as_str())
        .body(())
        .map_err(|error| format!("Failed to build cache-policy request: {error}"))?;
    *cache_request.headers_mut() = request.headers().clone();
    Ok(cache_request)
}

/// The response as the cache policy reads it.
fn cache_response(response: &Response) -> Result<http::Response<()>, String> {
    let mut cache_response = http::Response::builder()
        .status(response.status())
        .body(())
        .map_err(|error| format!("Failed to build cache-policy response: {error}"))?;
    *cache_response.headers_mut() = response.headers().clone();
    Ok(cache_response)
}

fn write_synced(path: &Path, contents: &[u8]) -> Result<(), String> {
    let mut file = fs::File::create(path).map_err(|error| format!("Failed to create tile cache file {}: {error}", path.display()))?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Failed to write tile cache file {}: {error}", path.display()))
}

fn parse_decimal(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then(|| value.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    const TILE: &[u8] = b"\x89PBF\r\n\x1a\nOVRLEY tile fixture";

    #[test]
    fn fetches_persists_and_reuses_a_vector_tile() {
        let fake_upstream = FakeUpstream::start();
        let cache_root = temp_cache_root("cache-miss-hit");
        let service = tile_service(&cache_root, &fake_upstream);

        let first = service.resolve(tile_at(3)).expect("fetched tile");
        assert_eq!(first.cache_status, TileCacheStatus::Miss);
        assert_eq!(first.bytes, TILE);

        let requests = fake_upstream.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /2/1/3.pbf\n"));
        let request_headers = requests[0].to_ascii_lowercase();
        assert!(request_headers.contains(&format!("user-agent: ovrley/{} (+https://www.ovrley.cc)", env!("CARGO_PKG_VERSION"))));
        assert!(!request_headers.contains("cache-control:"));
        assert!(!request_headers.contains("pragma:"));

        let payload_path = cache_root.join("openfreemap-planet/2/1/3.pbf");
        let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");
        assert_eq!(fs::read(&payload_path).expect("cached payload"), TILE);
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&metadata_path).expect("cached metadata")).expect("valid metadata JSON");
        assert_eq!(metadata["version"], 2);
        assert_eq!(metadata["contentLength"], TILE.len());
        assert!(metadata["policy"].is_object());
        assert!(fs::read_dir(payload_path.parent().expect("payload directory"))
            .expect("cache directory")
            .all(|entry| !entry.expect("cache entry").file_name().to_string_lossy().ends_with(".tmp")));

        let second = service.resolve(tile_at(3)).expect("cached tile");
        assert_eq!(second.cache_status, TileCacheStatus::Hit);
        assert_eq!(second.bytes, TILE);
        assert_eq!(fake_upstream.requests().len(), 1);

        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn incomplete_and_malformed_cache_pairs_are_refetched() {
        let fake_upstream = FakeUpstream::start();
        let cache_root = temp_cache_root("invalid-pairs");
        let service = tile_service(&cache_root, &fake_upstream);
        let payload_path = cache_root.join("openfreemap-planet/2/1/3.pbf");
        let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

        assert_eq!(fetch(&service, 3), TILE);
        fs::remove_file(&metadata_path).expect("remove metadata");
        assert_eq!(fetch(&service, 3), TILE);
        fs::remove_file(&payload_path).expect("remove payload");
        assert_eq!(fetch(&service, 3), TILE);
        fs::write(&metadata_path, b"not JSON").expect("corrupt metadata");
        assert_eq!(fetch(&service, 3), TILE);

        assert_eq!(fake_upstream.requests().len(), 4);
        assert_eq!(fs::read(payload_path).expect("repaired payload"), TILE);
        serde_json::from_slice::<serde_json::Value>(&fs::read(metadata_path).expect("repaired metadata"))
            .expect("repaired metadata JSON");
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn stale_tile_is_conditionally_revalidated_and_304_refreshes_metadata() {
        let tile_bytes = b"\x89PBF\r\n\x1a\nold tile";
        let fake_upstream = FakeUpstream::start_with(vec![
            FakeResponse::pbf(tile_bytes, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
            FakeResponse::empty(304).with_header("Cache-Control", "public, max-age=3600").with_header("ETag", "\"tile-v1\""),
        ]);
        let cache_root = temp_cache_root("revalidate-304");
        let service = tile_service(&cache_root, &fake_upstream);
        let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

        assert_eq!(fetch(&service, 3), tile_bytes);
        let metadata_before = fs::read(&metadata_path).expect("initial metadata");

        assert_eq!(fetch(&service, 3), tile_bytes);
        let metadata_after = fs::read(&metadata_path).expect("revalidated metadata");
        assert_ne!(metadata_after, metadata_before);

        let upstream_requests = fake_upstream.requests();
        assert_eq!(upstream_requests.len(), 2);
        assert!(upstream_requests[1].to_ascii_lowercase().contains("if-none-match: \"tile-v1\""));

        assert_eq!(fetch(&service, 3), tile_bytes);
        assert_eq!(fake_upstream.requests().len(), 2);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
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

        assert_eq!(fetch(&service, 3), old_tile);
        let old_metadata = fs::read(&metadata_path).expect("old metadata");

        assert_eq!(fetch(&service, 3), new_tile);
        assert_eq!(fs::read(&payload_path).expect("new payload"), new_tile);
        assert_ne!(fs::read(&metadata_path).expect("new metadata"), old_metadata);

        assert_eq!(fetch(&service, 3), new_tile);
        assert_eq!(fake_upstream.requests().len(), 2);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn non_storable_response_is_returned_without_persistence() {
        let tile = b"\x89PBF\r\n\x1a\nprivate tile";
        let fake_upstream =
            FakeUpstream::start_with(vec![FakeResponse::pbf(tile, "no-store"), FakeResponse::pbf(tile, "no-store")]);
        let cache_root = temp_cache_root("no-store");
        let service = tile_service(&cache_root, &fake_upstream);

        assert_eq!(fetch(&service, 3), tile);
        assert_eq!(fetch(&service, 3), tile);
        assert_eq!(fake_upstream.requests().len(), 2);
        assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
        assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf.json").exists());
    }

    #[test]
    fn stale_tile_survives_upstream_failures_without_metadata_changes() {
        let tile = b"\x89PBF\r\n\x1a\nstale tile";
        let fake_upstream = FakeUpstream::start_with(vec![
            FakeResponse::pbf(tile, "public, max-age=0").with_header("ETag", "\"tile-v1\""),
            FakeResponse::empty(500),
            FakeResponse::text("not a PBF"),
            FakeResponse::disconnect(),
            FakeResponse::pbf(b"\x89PBF\r\n\x1a\ntoo late", "public, max-age=3600").with_delay(Duration::from_millis(400)),
        ]);
        let cache_root = temp_cache_root("stale-on-error");
        let service = tile_service_with(&cache_root, &fake_upstream, "openfreemap-planet", Duration::from_millis(100), DEFAULT_TILE_CACHE_LIMIT);
        let metadata_path = cache_root.join("openfreemap-planet/2/1/3.pbf.json");

        assert_eq!(fetch(&service, 3), tile);
        let original_metadata = fs::read(&metadata_path).expect("initial metadata");

        for _ in 0..4 {
            assert_eq!(fetch(&service, 3), tile);
            assert_eq!(fs::read(&metadata_path).expect("unchanged metadata"), original_metadata);
        }

        assert_eq!(fake_upstream.requests().len(), 5);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn uncached_upstream_failure_is_an_error() {
        let fake_upstream = FakeUpstream::start_with(vec![FakeResponse::empty(503)]);
        let cache_root = temp_cache_root("uncached-error");
        let service = tile_service(&cache_root, &fake_upstream);

        assert!(service.resolve(tile_at(3)).is_err());
        assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
        assert!(!cache_root.join("openfreemap-planet/2/1/3.pbf.json").exists());
    }

    #[test]
    fn concurrent_same_tile_requests_share_one_upstream_fetch() {
        let tile = b"\x89PBF\r\n\x1a\nshared tile";
        let fake_upstream =
            FakeUpstream::start_with(vec![FakeResponse::pbf(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))]);
        let cache_root = temp_cache_root("same-tile-dedup");
        let service = tile_service(&cache_root, &fake_upstream);

        let responses = concurrent_requests(&service, vec![3; 4]);
        assert!(responses.iter().all(|response| response.as_deref() == Ok(&tile[..])));
        assert_eq!(fake_upstream.requests().len(), 1);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
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
        assert!(responses.iter().all(Result::is_ok));
        assert_eq!(fake_upstream.requests().len(), 2);
        assert!(fake_upstream.max_active() >= 2);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn successful_write_evicts_globally_oldest_entry_and_retains_recently_served_tile() {
        let tile = b"\x89PBF\r\n\x1a\nLRU tile fixture";
        let fake_upstream = FakeUpstream::start_with(vec![FakeResponse::pbf(tile, "public, max-age=3600"); 4]);
        let cache_root = temp_cache_root("global-lru");
        let obsolete_service =
            tile_service_with(&cache_root, &fake_upstream, "obsolete-openfreemap", REQUEST_TIMEOUT, CacheLimit::unchecked(u64::MAX));
        for y in [1, 2, 3] {
            assert_eq!(fetch(&obsolete_service, y), tile);
        }

        let first = cache_root.join("obsolete-openfreemap/2/1/1.pbf");
        let second = cache_root.join("obsolete-openfreemap/2/1/2.pbf");
        let third = cache_root.join("obsolete-openfreemap/2/1/3.pbf");
        for (path, seconds_ago) in [(&first, 300), (&second, 200), (&third, 100)] {
            fs::File::options()
                .write(true)
                .open(path)
                .expect("open tile")
                .set_modified(SystemTime::now() - Duration::from_secs(seconds_ago))
                .expect("date tile");
        }

        let first_sidecar = first.with_extension("pbf.json");
        let first_metadata = fs::read(&first_sidecar).expect("first tile metadata");
        let first_metadata_modified = fs::metadata(&first_sidecar).expect("first sidecar").modified().expect("modified time");
        assert_eq!(fetch(&obsolete_service, 1), tile);
        assert_eq!(fs::read(&first_sidecar).expect("untouched first tile metadata"), first_metadata);
        assert_eq!(
            fs::metadata(&first_sidecar).expect("untouched first sidecar").modified().expect("modified time"),
            first_metadata_modified
        );
        let cache_limit = test_pair_size(&first) + test_pair_size(&second) + test_pair_size(&third);
        drop(obsolete_service);

        let interrupted = cache_root.join("obsolete-openfreemap/4/5/.interrupted.pbf.tmp");
        let orphan = cache_root.join("obsolete-openfreemap/4/5/6.pbf.json");
        fs::create_dir_all(interrupted.parent().expect("artifact parent")).expect("create artifact directory");
        fs::write(&interrupted, b"partial payload").expect("write interrupted payload");
        fs::write(&orphan, b"orphan sidecar").expect("write orphan sidecar");

        let active_service =
            tile_service_with(&cache_root, &fake_upstream, "openfreemap-planet", REQUEST_TIMEOUT, CacheLimit::unchecked(cache_limit));
        assert!(!interrupted.exists());
        assert!(!orphan.exists());
        assert_eq!(fetch(&active_service, 0), tile);

        assert!(first.exists(), "recently served tile must be retained");
        assert!(!second.exists(), "globally oldest tile must be evicted");
        assert!(!cache_root.join("obsolete-openfreemap/2/1/2.pbf.json").exists());
        assert!(third.exists());
        assert!(cache_root.join("openfreemap-planet/2/1/0.pbf").exists());
        assert_eq!(fake_upstream.requests().len(), 4);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn clear_removes_all_namespaces_and_a_later_request_repopulates_the_cache() {
        let tile = b"\x89PBF\r\n\x1a\nclear tile";
        let fake_upstream = FakeUpstream::start_with(vec![FakeResponse::pbf(tile, "public, max-age=3600"); 2]);
        let cache_root = temp_cache_root("clear");
        let service = tile_service(&cache_root, &fake_upstream);

        assert_eq!(fetch(&service, 3), tile);
        let obsolete = cache_root.join("obsolete-source/0/0");
        fs::create_dir_all(&obsolete).expect("create obsolete namespace");
        fs::write(obsolete.join("0.pbf.tmp"), b"interrupted").expect("write artifact");

        assert_eq!(service.clear().expect("clear tile cache").used_bytes, 0);
        assert!(!cache_root.exists());

        assert_eq!(fetch(&service, 3), tile);
        assert!(cache_root.join("openfreemap-planet/2/1/3.pbf").exists());
        assert_eq!(fake_upstream.requests().len(), 2);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    #[test]
    fn clear_waits_for_active_tile_resolution_without_disrupting_its_response() {
        let tile = b"\x89PBF\r\n\x1a\nactive tile";
        let fake_upstream =
            FakeUpstream::start_with(vec![FakeResponse::pbf(tile, "public, max-age=3600").with_delay(Duration::from_millis(200))]);
        let cache_root = temp_cache_root("clear-active");
        let service = tile_service(&cache_root, &fake_upstream);

        let resolving = service.clone();
        let active_request = std::thread::spawn(move || fetch(&resolving, 3));
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while fake_upstream.requests().is_empty() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(fake_upstream.requests().len(), 1);

        service.clear().expect("clear waits for active request");
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
        assert!(failures.iter().all(Result::is_err));
        assert_eq!(fake_upstream.requests().len(), 1);

        assert_eq!(fetch(&service, 3), tile);
        assert_eq!(fake_upstream.requests().len(), 2);
        fs::remove_dir_all(cache_root).expect("remove tile cache");
    }

    fn tile_service(cache_root: &Path, upstream: &FakeUpstream) -> TileCache {
        tile_service_with(cache_root, upstream, "openfreemap-planet", REQUEST_TIMEOUT, DEFAULT_TILE_CACHE_LIMIT)
    }

    fn tile_service_with(
        cache_root: &Path,
        upstream: &FakeUpstream,
        namespace: &str,
        timeout: Duration,
        limit: CacheLimit,
    ) -> TileCache {
        TileCache::with_source(
            cache_root.to_path_buf(),
            format!("http://127.0.0.1:{}/{{z}}/{{x}}/{{y}}.pbf", upstream.port),
            namespace.into(),
            timeout.min(CONNECT_TIMEOUT),
            timeout,
            limit,
        )
        .expect("tile service")
    }

    /// The tile `2/1/<y>`.
    fn tile_at(y: u32) -> TileCoordinates {
        TileCoordinates { zoom: 2, x: 1, y }
    }

    fn fetch(service: &TileCache, y: u32) -> Vec<u8> {
        service.resolve(tile_at(y)).expect("resolved tile").bytes
    }

    fn temp_cache_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ovrley-tile-cache-{label}-{}", Uuid::new_v4()))
    }

    fn test_pair_size(payload: &Path) -> u64 {
        fs::metadata(payload).expect("payload metadata").len()
            + fs::metadata(payload.with_extension("pbf.json")).expect("sidecar metadata").len()
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
            let port = server.server_addr().to_ip().expect("upstream IP address").port();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let recorded_requests = Arc::clone(&requests);
            let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let response_max_active = Arc::clone(&max_active);
            std::thread::spawn(move || {
                while let Ok(request) = server.recv() {
                    let mut record = format!("{} {}\n", request.method(), request.url());
                    for header in request.headers() {
                        record.push_str(&format!("{}: {}\n", header.field, header.value));
                    }
                    recorded_requests.lock().expect("request records").push(record);
                    let response = responses.lock().expect("fake responses").pop_front().unwrap_or_else(FakeResponse::default_pbf);
                    let active = Arc::clone(&active);
                    let max_active = Arc::clone(&response_max_active);
                    std::thread::spawn(move || response.respond(request, &active, &max_active));
                }
            });
            Self { port, requests, max_active }
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
            Self::pbf(TILE, "public, max-age=3600")
        }

        fn pbf(body: &[u8], cache_control: &'static str) -> Self {
            Self {
                headers: vec![("Content-Type", "application/vnd.mapbox-vector-tile"), ("Cache-Control", cache_control)],
                ..Self::with_body(200, body)
            }
        }

        fn empty(status: u16) -> Self {
            Self::with_body(status, &[])
        }

        fn text(body: &str) -> Self {
            Self { headers: vec![("Content-Type", "text/plain")], ..Self::with_body(200, body.as_bytes()) }
        }

        fn disconnect() -> Self {
            Self { disconnect: true, ..Self::with_body(0, &[]) }
        }

        fn with_body(status: u16, body: &[u8]) -> Self {
            Self { status, body: body.to_vec(), headers: Vec::new(), delay: None, disconnect: false }
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
            if !self.disconnect {
                let mut response = tiny_http::Response::from_data(self.body).with_status_code(tiny_http::StatusCode(self.status));
                for (name, value) in self.headers {
                    response.add_header(tiny_http::Header::from_bytes(name, value).expect("fake response header"));
                }
                let _ = request.respond(response);
            }
            active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn concurrent_requests(service: &TileCache, rows: Vec<u32>) -> Vec<Result<Vec<u8>, String>> {
        let barrier = Arc::new(Barrier::new(rows.len() + 1));
        let handles: Vec<_> = rows
            .into_iter()
            .map(|y| {
                let barrier = Arc::clone(&barrier);
                let service = service.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    service.resolve(tile_at(y)).map(|payload| payload.bytes)
                })
            })
            .collect();
        barrier.wait();
        handles.into_iter().map(|handle| handle.join().expect("request thread")).collect()
    }
}
