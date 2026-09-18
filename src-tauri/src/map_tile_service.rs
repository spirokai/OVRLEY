//! On-demand raster tile fetching and persistent cache storage.
//!
//! This module is the sole owner of tile coordinate validation, upstream tile
//! requests, and the on-disk tile cache. The loopback server only delegates
//! matching requests here and turns the result into a local HTTP response.

use http_cache_semantics::{AfterResponse, BeforeRequest, CachePolicy};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

pub(crate) const OSM_STANDARD_TILE_URL: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
pub(crate) const OSM_STANDARD_CACHE_NAMESPACE: &str = "osm-standard";
const TILE_CACHE_LIMIT: u64 = 500 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct MapTileService {
    cache_root: PathBuf,
    source_url: String,
    cache_namespace: String,
    client: Client,
    in_flight: Arc<Mutex<HashMap<TileCoordinates, Arc<InFlightTile>>>>,
    maintenance: Arc<CacheMaintenance>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct TileCoordinates {
    pub(crate) zoom: u8,
    pub(crate) x: u32,
    pub(crate) y: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedTileMetadata {
    version: u8,
    cache_namespace: String,
    zoom: u8,
    x: u32,
    y: u32,
    content_length: usize,
    policy: CachePolicy,
}

struct CachedTile {
    payload: Vec<u8>,
    metadata: CachedTileMetadata,
}

struct InFlightTile {
    result: Mutex<Option<Result<Vec<u8>, String>>>,
    ready: Condvar,
}

struct CacheMaintenance {
    gate: RwLock<()>,
    accounting: Mutex<CacheAccounting>,
}

struct CacheAccounting {
    total_size: u64,
    limit: u64,
    entries: HashMap<PathBuf, u64>,
}

struct ResolveOutcome {
    payload: Vec<u8>,
    persistent_write: bool,
}

struct PersistentEntry {
    paths: CachePaths,
    size: u64,
    modified: SystemTime,
}

impl MapTileService {
    pub(crate) fn new(
        cache_root: PathBuf,
        source_url: String,
        cache_namespace: String,
    ) -> Result<Self, String> {
        Self::with_timeouts(
            cache_root,
            source_url,
            cache_namespace,
            Duration::from_secs(5),
            Duration::from_secs(15),
            TILE_CACHE_LIMIT,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_with_timeouts(
        cache_root: PathBuf,
        source_url: String,
        cache_namespace: String,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, String> {
        Self::with_timeouts(
            cache_root,
            source_url,
            cache_namespace,
            connect_timeout,
            request_timeout,
            TILE_CACHE_LIMIT,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_with_cache_limit(
        cache_root: PathBuf,
        source_url: String,
        cache_namespace: String,
        cache_limit: u64,
    ) -> Result<Self, String> {
        Self::with_timeouts(
            cache_root,
            source_url,
            cache_namespace,
            Duration::from_secs(5),
            Duration::from_secs(15),
            cache_limit,
        )
    }

    fn with_timeouts(
        cache_root: PathBuf,
        source_url: String,
        cache_namespace: String,
        connect_timeout: Duration,
        request_timeout: Duration,
        cache_limit: u64,
    ) -> Result<Self, String> {
        if !["{z}", "{x}", "{y}"]
            .iter()
            .all(|token| source_url.contains(token))
        {
            return Err("Tile source URL must contain {z}, {x}, and {y} placeholders".to_string());
        }
        if cache_namespace.is_empty()
            || !cache_namespace
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(
                "Tile cache namespace must contain only ASCII letters, digits, '-' or '_'"
                    .to_string(),
            );
        }

        let user_agent = format!(
            "OVRLEY/{} (+https://www.ovrley.cc)",
            env!("CARGO_PKG_VERSION")
        );
        let client = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .user_agent(user_agent)
            .build()
            .map_err(|error| format!("Failed to create tile HTTP client: {error}"))?;

        let service = Self {
            cache_root,
            source_url,
            cache_namespace,
            client,
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            maintenance: Arc::new(CacheMaintenance {
                gate: RwLock::new(()),
                accounting: Mutex::new(CacheAccounting {
                    total_size: 0,
                    limit: cache_limit,
                    entries: HashMap::new(),
                }),
            }),
        };
        service.initialize_cache()?;
        Ok(service)
    }

    pub(crate) fn for_application_cache(application_cache: PathBuf) -> Result<Self, String> {
        Self::new(
            application_cache.join("tiles"),
            OSM_STANDARD_TILE_URL.to_string(),
            OSM_STANDARD_CACHE_NAMESPACE.to_string(),
        )
    }

    pub(crate) fn resolve(&self, coordinates: TileCoordinates) -> Result<Vec<u8>, String> {
        let (in_flight, owns_resolution) = {
            let mut requests = self.in_flight.lock().map_err(|error| error.to_string())?;
            if let Some(in_flight) = requests.get(&coordinates) {
                (Arc::clone(in_flight), false)
            } else {
                let in_flight = Arc::new(InFlightTile {
                    result: Mutex::new(None),
                    ready: Condvar::new(),
                });
                requests.insert(coordinates, Arc::clone(&in_flight));
                (in_flight, true)
            }
        };

        if !owns_resolution {
            let mut result = in_flight.result.lock().map_err(|error| error.to_string())?;
            while result.is_none() {
                result = in_flight
                    .ready
                    .wait(result)
                    .map_err(|error| error.to_string())?;
            }
            return result.clone().expect("in-flight tile result is ready");
        }

        let result = self.resolve_owned(coordinates);
        {
            let mut shared_result = in_flight.result.lock().map_err(|error| error.to_string())?;
            *shared_result = Some(result.clone());
            in_flight.ready.notify_all();
        }
        self.in_flight
            .lock()
            .map_err(|error| error.to_string())?
            .remove(&coordinates);
        result
    }

    fn resolve_owned(&self, coordinates: TileCoordinates) -> Result<Vec<u8>, String> {
        let access = self
            .maintenance
            .gate
            .read()
            .map_err(|error| error.to_string())?;
        let outcome = self.resolve_with_access(coordinates)?;
        drop(access);
        if outcome.persistent_write {
            self.evict_if_needed()?;
        }
        Ok(outcome.payload)
    }

    fn resolve_with_access(&self, coordinates: TileCoordinates) -> Result<ResolveOutcome, String> {
        let paths = self.cache_paths(coordinates);
        let upstream_url = self
            .source_url
            .replace("{z}", &coordinates.zoom.to_string())
            .replace("{x}", &coordinates.x.to_string())
            .replace("{y}", &coordinates.y.to_string());
        let mut request = self
            .client
            .get(upstream_url)
            .build()
            .map_err(|error| format!("Failed to build tile upstream request: {error}"))?;
        let cached = self.read_cached(&paths, coordinates)?;

        if let Some(cached) = cached {
            let policy_request = cache_request(&request)?;
            match cached
                .metadata
                .policy
                .before_request(&policy_request, SystemTime::now())
            {
                BeforeRequest::Fresh(_) => {
                    touch(&paths.payload)?;
                    return Ok(ResolveOutcome {
                        payload: cached.payload,
                        persistent_write: false,
                    });
                }
                BeforeRequest::Stale {
                    request: conditional,
                    matches: true,
                } => {
                    *request.headers_mut() = conditional.headers;
                    return self.fetch(request, Some(cached), &paths, coordinates);
                }
                BeforeRequest::Stale { matches: false, .. } => {
                    self.remove_pair(&paths)?;
                }
            }
        }

        self.fetch(request, None, &paths, coordinates)
    }

    fn fetch(
        &self,
        request: reqwest::blocking::Request,
        cached: Option<CachedTile>,
        paths: &CachePaths,
        coordinates: TileCoordinates,
    ) -> Result<ResolveOutcome, String> {
        let policy_request = cache_request(&request)?;
        let response = match self.client.execute(request) {
            Ok(response) => response,
            Err(error) => {
                return self.cached_fallback(cached, paths, || {
                    format!("Tile upstream request failed: {error}")
                });
            }
        };
        let response_time = SystemTime::now();
        let policy_response = cache_response(&response)?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = cached else {
                return Err("Tile upstream returned 304 without a cached tile".to_string());
            };
            return match cached.metadata.policy.after_response(
                &policy_request,
                &policy_response,
                response_time,
            ) {
                AfterResponse::NotModified(policy, _) if policy.is_storable() => {
                    let metadata = self.metadata(coordinates, cached.payload.len(), policy);
                    self.persist(paths, &metadata, CacheWrite::MetadataOnly)?;
                    touch(&paths.payload)?;
                    Ok(ResolveOutcome {
                        payload: cached.payload,
                        persistent_write: true,
                    })
                }
                _ => {
                    touch(&paths.payload)?;
                    Ok(ResolveOutcome {
                        payload: cached.payload,
                        persistent_write: false,
                    })
                }
            };
        }

        if response.status() != reqwest::StatusCode::OK || !is_png_response(response.headers()) {
            return self.cached_fallback(cached, paths, || {
                format!(
                    "Tile upstream returned invalid HTTP response {}",
                    response.status().as_u16()
                )
            });
        }

        let policy = if let Some(cached) = cached.as_ref() {
            match cached.metadata.policy.after_response(
                &policy_request,
                &policy_response,
                response_time,
            ) {
                AfterResponse::Modified(policy, _) | AfterResponse::NotModified(policy, _) => {
                    policy
                }
            }
        } else {
            CachePolicy::new(&policy_request, &policy_response)
        };
        let payload = match response.bytes() {
            Ok(payload) => payload.to_vec(),
            Err(error) => {
                return self.cached_fallback(cached, paths, || {
                    format!("Failed to read tile upstream response: {error}")
                });
            }
        };

        let mut persistent_write = false;
        if policy.is_storable() {
            let metadata = self.metadata(coordinates, payload.len(), policy);
            self.persist(paths, &metadata, CacheWrite::PayloadAndMetadata(&payload))?;
            persistent_write = true;
        }
        Ok(ResolveOutcome {
            payload,
            persistent_write,
        })
    }

    fn cached_fallback<F>(
        &self,
        cached: Option<CachedTile>,
        paths: &CachePaths,
        error: F,
    ) -> Result<ResolveOutcome, String>
    where
        F: FnOnce() -> String,
    {
        if let Some(cached) = cached {
            touch(&paths.payload)?;
            Ok(ResolveOutcome {
                payload: cached.payload,
                persistent_write: false,
            })
        } else {
            Err(error())
        }
    }

    fn metadata(
        &self,
        coordinates: TileCoordinates,
        content_length: usize,
        policy: CachePolicy,
    ) -> CachedTileMetadata {
        CachedTileMetadata {
            version: 2,
            cache_namespace: self.cache_namespace.clone(),
            zoom: coordinates.zoom,
            x: coordinates.x,
            y: coordinates.y,
            content_length,
            policy,
        }
    }

    fn cache_paths(&self, coordinates: TileCoordinates) -> CachePaths {
        let directory = self
            .cache_root
            .join(&self.cache_namespace)
            .join(coordinates.zoom.to_string())
            .join(coordinates.x.to_string());
        let payload = directory.join(format!("{}.png", coordinates.y));
        let metadata = directory.join(format!("{}.png.json", coordinates.y));
        CachePaths {
            directory,
            payload,
            metadata,
        }
    }

    fn read_cached(
        &self,
        paths: &CachePaths,
        coordinates: TileCoordinates,
    ) -> Result<Option<CachedTile>, String> {
        let payload_exists = paths.payload.try_exists().map_err(|error| {
            format!(
                "Failed to inspect tile cache {}: {error}",
                paths.payload.display()
            )
        })?;
        let metadata_exists = paths.metadata.try_exists().map_err(|error| {
            format!(
                "Failed to inspect tile cache {}: {error}",
                paths.metadata.display()
            )
        })?;
        if !payload_exists && !metadata_exists {
            return Ok(None);
        }

        let cached = fs::read(&paths.payload)
            .ok()
            .zip(fs::read(&paths.metadata).ok())
            .and_then(|(payload, metadata)| {
                let metadata = serde_json::from_slice::<CachedTileMetadata>(&metadata).ok()?;
                self.valid_cached_payload(&payload, &metadata, coordinates)
                    .then_some(CachedTile { payload, metadata })
            });
        if cached.is_none() {
            self.remove_pair(paths)?;
        }
        Ok(cached)
    }

    fn valid_cached_payload(
        &self,
        payload: &[u8],
        metadata: &CachedTileMetadata,
        coordinates: TileCoordinates,
    ) -> bool {
        metadata.version == 2
            && metadata.cache_namespace == self.cache_namespace
            && metadata.zoom == coordinates.zoom
            && metadata.x == coordinates.x
            && metadata.y == coordinates.y
            && metadata.content_length == payload.len()
            && metadata.policy.is_storable()
    }

    fn persist(
        &self,
        paths: &CachePaths,
        metadata: &CachedTileMetadata,
        write: CacheWrite<'_>,
    ) -> Result<(), String> {
        let mut accounting = self
            .maintenance
            .accounting
            .lock()
            .map_err(|error| error.to_string())?;
        if matches!(write, CacheWrite::PayloadAndMetadata(_)) {
            fs::create_dir_all(&paths.directory).map_err(|error| {
                format!(
                    "Failed to create tile cache directory {}: {error}",
                    paths.directory.display()
                )
            })?;
        }
        let transaction_id = Uuid::new_v4();
        let temporary = CacheTemporaryPaths {
            payload: paths.directory.join(format!(".{transaction_id}.png.tmp")),
            metadata: paths.directory.join(format!(".{transaction_id}.json.tmp")),
        };
        let metadata_bytes = serde_json::to_vec(metadata)
            .map_err(|error| format!("Failed to serialize tile cache metadata: {error}"))?;
        let result = write_cache_entry(paths, &temporary, &metadata_bytes, write);
        let _ = fs::remove_file(temporary.payload);
        let _ = fs::remove_file(temporary.metadata);
        if result.is_err() {
            let _ = remove_if_present(&paths.metadata);
            let _ = remove_if_present(&paths.payload);
        }
        let current_size = if result.is_ok() {
            cache_pair_size(paths)?
        } else {
            0
        };
        replace_accounted_entry(&mut accounting, &paths.payload, current_size);
        result
    }

    fn remove_pair(&self, paths: &CachePaths) -> Result<(), String> {
        let mut accounting = self
            .maintenance
            .accounting
            .lock()
            .map_err(|error| error.to_string())?;
        remove_if_present(&paths.metadata)?;
        remove_if_present(&paths.payload)?;
        replace_accounted_entry(&mut accounting, &paths.payload, 0);
        Ok(())
    }

    fn initialize_cache(&self) -> Result<(), String> {
        self.reconcile_and_evict()
    }

    fn evict_if_needed(&self) -> Result<(), String> {
        let over_limit = {
            let accounting = self
                .maintenance
                .accounting
                .lock()
                .map_err(|error| error.to_string())?;
            accounting.total_size > accounting.limit
        };
        if over_limit {
            self.reconcile_and_evict()?;
        }
        Ok(())
    }

    fn reconcile_and_evict(&self) -> Result<(), String> {
        let _maintenance = self
            .maintenance
            .gate
            .write()
            .map_err(|error| error.to_string())?;
        let mut entries = scan_persistent_entries(&self.cache_root)?;
        entries.sort_by_key(|entry| entry.modified);

        let mut accounting = self
            .maintenance
            .accounting
            .lock()
            .map_err(|error| error.to_string())?;
        let mut total_size = entries
            .iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.size));
        let mut retained_entries = HashMap::new();
        for entry in entries {
            if total_size > accounting.limit {
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

    #[allow(dead_code)]
    pub(crate) fn clear(&self) -> Result<(), String> {
        let _maintenance = self
            .maintenance
            .gate
            .write()
            .map_err(|error| error.to_string())?;
        match fs::remove_dir_all(&self.cache_root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Failed to clear tile cache {}: {error}",
                    self.cache_root.display()
                ));
            }
        }
        let mut accounting = self
            .maintenance
            .accounting
            .lock()
            .map_err(|error| error.to_string())?;
        accounting.total_size = 0;
        accounting.entries.clear();
        Ok(())
    }
}

struct CachePaths {
    directory: PathBuf,
    payload: PathBuf,
    metadata: PathBuf,
}

struct CacheTemporaryPaths {
    payload: PathBuf,
    metadata: PathBuf,
}

#[derive(Clone, Copy)]
enum CacheWrite<'a> {
    PayloadAndMetadata(&'a [u8]),
    MetadataOnly,
}

fn write_cache_entry(
    paths: &CachePaths,
    temporary: &CacheTemporaryPaths,
    metadata: &[u8],
    write: CacheWrite<'_>,
) -> Result<(), String> {
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
    fs::rename(&temporary.metadata, &paths.metadata)
        .map_err(|error| format!("Failed to promote tile metadata into cache: {error}"))
}

fn scan_persistent_entries(cache_root: &Path) -> Result<Vec<PersistentEntry>, String> {
    if !cache_root.try_exists().map_err(|error| {
        format!(
            "Failed to inspect tile cache root {}: {error}",
            cache_root.display()
        )
    })? {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_cache_files(cache_root, &mut files)?;
    let mut valid_files = HashSet::new();
    let mut entries = Vec::new();
    for payload in files
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
    {
        if let Some(entry) = inspect_persistent_entry(cache_root, payload)? {
            valid_files.insert(entry.paths.payload.clone());
            valid_files.insert(entry.paths.metadata.clone());
            entries.push(entry);
        }
    }

    for artifact in files {
        if !valid_files.contains(&artifact) {
            remove_if_present(&artifact)?;
        }
    }
    Ok(entries)
}

fn collect_cache_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "Failed to read tile cache directory {}: {error}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Failed to inspect tile cache directory {}: {error}",
                directory.display()
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Failed to inspect tile cache artifact {}: {error}",
                entry.path().display()
            )
        })?;
        if file_type.is_dir() {
            collect_cache_files(&entry.path(), files)?;
        } else {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn inspect_persistent_entry(
    cache_root: &Path,
    payload_path: &Path,
) -> Result<Option<PersistentEntry>, String> {
    let relative = payload_path.strip_prefix(cache_root).map_err(|error| {
        format!(
            "Tile cache entry {} is outside cache root: {error}",
            payload_path.display()
        )
    })?;
    let components = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.len() != 4 {
        return Ok(None);
    }
    let namespace = components[0];
    let Some(zoom) = parse_decimal(components[1]).and_then(|value| u8::try_from(value).ok()) else {
        return Ok(None);
    };
    let Some(x) = parse_decimal(components[2]) else {
        return Ok(None);
    };
    let Some(y) = components[3].strip_suffix(".png").and_then(parse_decimal) else {
        return Ok(None);
    };
    if namespace.is_empty()
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        || zoom > 19
    {
        return Ok(None);
    }
    let coordinate_limit = 1_u32 << zoom;
    if x >= coordinate_limit || y >= coordinate_limit {
        return Ok(None);
    }

    let metadata_path = payload_path.with_extension("png.json");
    let payload_file = fs::metadata(payload_path).map_err(|error| {
        format!(
            "Failed to inspect cached tile payload {}: {error}",
            payload_path.display()
        )
    })?;
    let metadata = match fs::read(&metadata_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CachedTileMetadata>(&bytes).ok())
    {
        Some(metadata) => metadata,
        None => return Ok(None),
    };
    if metadata.version != 2
        || metadata.cache_namespace != namespace
        || metadata.zoom != zoom
        || metadata.x != x
        || metadata.y != y
        || metadata.content_length as u64 != payload_file.len()
        || !metadata.policy.is_storable()
    {
        return Ok(None);
    }

    let metadata_file = fs::metadata(&metadata_path).map_err(|error| {
        format!(
            "Failed to inspect cached tile metadata {}: {error}",
            metadata_path.display()
        )
    })?;
    Ok(Some(PersistentEntry {
        paths: CachePaths {
            directory: payload_path
                .parent()
                .expect("validated cache payload has a parent")
                .to_path_buf(),
            payload: payload_path.to_path_buf(),
            metadata: metadata_path,
        },
        size: payload_file.len() + metadata_file.len(),
        modified: payload_file.modified().map_err(|error| {
            format!(
                "Failed to read cached tile access time {}: {error}",
                payload_path.display()
            )
        })?,
    }))
}

fn cache_pair_size(paths: &CachePaths) -> Result<u64, String> {
    Ok(file_size_if_present(&paths.payload)? + file_size_if_present(&paths.metadata)?)
}

fn file_size_if_present(path: &Path) -> Result<u64, String> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!(
            "Failed to inspect tile cache file {}: {error}",
            path.display()
        )),
    }
}

fn replace_accounted_entry(accounting: &mut CacheAccounting, payload: &Path, current_size: u64) {
    if let Some(previous_size) = accounting.entries.remove(payload) {
        accounting.total_size = accounting.total_size.saturating_sub(previous_size);
    }
    if current_size > 0 {
        accounting
            .entries
            .insert(payload.to_path_buf(), current_size);
        accounting.total_size = accounting.total_size.saturating_add(current_size);
    }
}

fn touch(path: &Path) -> Result<(), String> {
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()))
        .map_err(|error| {
            format!(
                "Failed to update cached tile access time {}: {error}",
                path.display()
            )
        })
}

fn is_png_response(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("image/png"))
}

fn cache_request(request: &reqwest::blocking::Request) -> Result<http::Request<()>, String> {
    let mut cache_request = http::Request::builder()
        .method(request.method().clone())
        .uri(request.url().as_str())
        .body(())
        .map_err(|error| format!("Failed to build cache-policy request: {error}"))?;
    *cache_request.headers_mut() = request.headers().clone();
    Ok(cache_request)
}

fn cache_response(response: &reqwest::blocking::Response) -> Result<http::Response<()>, String> {
    let mut cache_response = http::Response::builder()
        .status(response.status())
        .body(())
        .map_err(|error| format!("Failed to build cache-policy response: {error}"))?;
    *cache_response.headers_mut() = response.headers().clone();
    Ok(cache_response)
}

fn write_synced(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;

    let mut file = fs::File::create(path).map_err(|error| {
        format!(
            "Failed to create tile cache file {}: {error}",
            path.display()
        )
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "Failed to write tile cache file {}: {error}",
                path.display()
            )
        })
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove invalid tile cache file {}: {error}",
            path.display()
        )),
    }
}

pub(crate) fn parse_tile_path(path: &str) -> Option<TileCoordinates> {
    let route = path.strip_prefix("/tiles/")?;
    let mut segments = route.split('/');
    let zoom = parse_decimal(segments.next()?)?;
    let x = parse_decimal(segments.next()?)?;
    let y_segment = segments.next()?;
    if segments.next().is_some() {
        return None;
    }
    let y = parse_decimal(y_segment.strip_suffix(".png")?)?;

    if zoom > 19 {
        return None;
    }
    let coordinate_limit = 1_u32 << zoom;
    if x >= coordinate_limit || y >= coordinate_limit {
        return None;
    }

    Some(TileCoordinates {
        zoom: zoom as u8,
        x,
        y,
    })
}

fn parse_decimal(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}
