//! Disk cache of timeline filmstrips: one JPEG sprite per strip in the
//! application cache directory, so a video's strips survive restarts and are
//! shared by every project that uses the file.
//!
//! Owns: the cache directory, its size accounting and limit, eviction of the
//! least recently used strips (reading a strip refreshes its modified time),
//! and the limit's key in the shared app settings store. Does not own: strip
//! names, which encode the source file's stamp and the strip's sampling
//! (`video_filmstrip`).
//!
//! The limit follows the map tile cache: a whole number of steps between the
//! bounds, read at startup from the settings store and changed through the
//! same usage/limit/clear commands. Rust is the only writer of the key, which
//! the frontend's preferences store also opens, so the startup limit and the
//! running limit always agree.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const MIB: u64 = 1024 * 1024;
const FILMSTRIP_CACHE_LIMIT_STEP: u64 = 128 * MIB;
pub(crate) const DEFAULT_FILMSTRIP_CACHE_LIMIT: u64 = 2 * FILMSTRIP_CACHE_LIMIT_STEP;
const MIN_FILMSTRIP_CACHE_LIMIT: u64 = FILMSTRIP_CACHE_LIMIT_STEP;
const MAX_FILMSTRIP_CACHE_LIMIT: u64 = 40 * FILMSTRIP_CACHE_LIMIT_STEP;
const STRIP_EXTENSION: &str = "jpg";
const SETTINGS_STORE: &str = "ovrley-settings.json";
const FILMSTRIP_CACHE_LIMIT_KEY: &str = "filmstripCacheLimitBytes";

/// The cache's current size and its configurable limit bounds and step, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FilmstripCacheUsage {
    used_bytes: u64,
    limit_bytes: u64,
    min_limit_bytes: u64,
    max_limit_bytes: u64,
    limit_step_bytes: u64,
}

/// Accepts a user-configured cache limit only on a step inside the supported bounds.
pub(crate) fn validate_filmstrip_cache_limit(limit: u64) -> Result<u64, String> {
    if !(MIN_FILMSTRIP_CACHE_LIMIT..=MAX_FILMSTRIP_CACHE_LIMIT).contains(&limit)
        || !limit.is_multiple_of(FILMSTRIP_CACHE_LIMIT_STEP)
    {
        return Err(format!(
            "Filmstrip cache limit {limit} must be a multiple of {FILMSTRIP_CACHE_LIMIT_STEP} between {MIN_FILMSTRIP_CACHE_LIMIT} and {MAX_FILMSTRIP_CACHE_LIMIT} bytes"
        ));
    }
    Ok(limit)
}

/// The saved cache limit, or the default when none has been saved.
pub(crate) fn persisted_filmstrip_cache_limit<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<u64, String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    let Some(value) = store.get(FILMSTRIP_CACHE_LIMIT_KEY) else {
        return Ok(DEFAULT_FILMSTRIP_CACHE_LIMIT);
    };
    let limit = value.as_u64().ok_or_else(|| {
        format!("Setting {FILMSTRIP_CACHE_LIMIT_KEY} must be a whole number of bytes: {value}")
    })?;
    validate_filmstrip_cache_limit(limit)
}

/// Saves a cache limit for the next start.
pub(crate) fn persist_filmstrip_cache_limit<R: Runtime>(
    app: &AppHandle<R>,
    limit: u64,
) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE)
        .map_err(|error| error.to_string())?;
    store.set(
        FILMSTRIP_CACHE_LIMIT_KEY,
        validate_filmstrip_cache_limit(limit)?,
    );
    store.save().map_err(|error| error.to_string())
}

struct Accounting {
    used_bytes: u64,
    limit_bytes: u64,
}

/// The filmstrip disk cache. Clones share one directory and one accounting.
#[derive(Clone)]
pub(crate) struct FilmstripCache {
    root: PathBuf,
    /// Held for every write, eviction and clear, so the accounting matches the
    /// directory. Reads take no lock: a strip evicted under a read is a miss.
    accounting: Arc<Mutex<Accounting>>,
}

struct StripFile {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

impl FilmstripCache {
    /// Opens the cache at `root` (created when missing) and evicts down to `limit_bytes`.
    pub(crate) fn open(root: PathBuf, limit_bytes: u64) -> Result<Self, String> {
        let cache = Self {
            root,
            accounting: Arc::new(Mutex::new(Accounting {
                used_bytes: 0,
                limit_bytes: validate_filmstrip_cache_limit(limit_bytes)?,
            })),
        };
        cache.with_accounting(|cache, accounting| cache.reconcile_and_evict(accounting))?;
        Ok(cache)
    }

    /// Opens the cache in the application cache directory.
    pub(crate) fn for_application_cache(
        application_cache: &Path,
        limit_bytes: u64,
    ) -> Result<Self, String> {
        Self::open(application_cache.join("filmstrips"), limit_bytes)
    }

    #[cfg(test)]
    fn open_unchecked(root: PathBuf, limit_bytes: u64) -> Self {
        let cache = Self {
            root,
            accounting: Arc::new(Mutex::new(Accounting {
                used_bytes: 0,
                limit_bytes,
            })),
        };
        cache
            .with_accounting(|cache, accounting| cache.reconcile_and_evict(accounting))
            .unwrap();
        cache
    }

    fn with_accounting<T>(
        &self,
        action: impl FnOnce(&Self, &mut Accounting) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut accounting = self.accounting.lock().map_err(|error| error.to_string())?;
        action(self, &mut accounting)
    }

    fn strip_path(&self, name: &str) -> Result<PathBuf, String> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !name.starts_with('.');
        if !valid {
            return Err(format!("Invalid filmstrip cache name: {name}"));
        }
        Ok(self.root.join(format!("{name}.{STRIP_EXTENSION}")))
    }

    /// The cached strip named `name`, marked as just used; `None` on a miss.
    pub(crate) fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.strip_path(name)?;
        match fs::read(&path) {
            Ok(bytes) => {
                // A strip evicted between the read and the touch stays a hit for this read.
                let _ = touch(&path);
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!(
                "Failed to read cached filmstrip {}: {error}",
                path.display()
            )),
        }
    }

    /// Stores a strip under `name`, replacing any earlier one, then evicts the
    /// least recently used strips while the cache is over its limit.
    pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.strip_path(name)?;
        self.with_accounting(|cache, accounting| {
            fs::create_dir_all(&cache.root).map_err(|error| {
                format!(
                    "Failed to create filmstrip cache {}: {error}",
                    cache.root.display()
                )
            })?;
            let replaced = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            let temporary = cache
                .root
                .join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
            fs::write(&temporary, bytes)
                .and_then(|()| fs::rename(&temporary, &path))
                .map_err(|error| {
                    let _ = fs::remove_file(&temporary);
                    format!(
                        "Failed to write cached filmstrip {}: {error}",
                        path.display()
                    )
                })?;
            accounting.used_bytes = accounting
                .used_bytes
                .saturating_sub(replaced)
                .saturating_add(bytes.len() as u64);
            if accounting.used_bytes > accounting.limit_bytes {
                cache.reconcile_and_evict(accounting)?;
            }
            Ok(())
        })
    }

    pub(crate) fn usage(&self) -> Result<FilmstripCacheUsage, String> {
        self.with_accounting(|_, accounting| Ok(usage_of(accounting)))
    }

    /// Applies a new limit and evicts least recently used strips down to it.
    pub(crate) fn set_limit(&self, limit_bytes: u64) -> Result<FilmstripCacheUsage, String> {
        let limit_bytes = validate_filmstrip_cache_limit(limit_bytes)?;
        self.with_accounting(|cache, accounting| {
            accounting.limit_bytes = limit_bytes;
            cache.reconcile_and_evict(accounting)?;
            Ok(usage_of(accounting))
        })
    }

    /// Deletes every cached strip.
    pub(crate) fn clear(&self) -> Result<FilmstripCacheUsage, String> {
        self.with_accounting(|cache, accounting| {
            match fs::remove_dir_all(&cache.root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Failed to clear filmstrip cache {}: {error}",
                        cache.root.display()
                    ))
                }
            }
            accounting.used_bytes = 0;
            Ok(usage_of(accounting))
        })
    }

    /// Recounts the directory, deletes leftovers of interrupted writes, and
    /// deletes the least recently used strips until the cache fits its limit.
    fn reconcile_and_evict(&self, accounting: &mut Accounting) -> Result<(), String> {
        let mut strips = self.scan()?;
        strips.sort_by_key(|strip| strip.modified);
        let mut used = strips
            .iter()
            .fold(0_u64, |total, strip| total.saturating_add(strip.size));
        for strip in &strips {
            if used <= accounting.limit_bytes {
                break;
            }
            remove_if_present(&strip.path)?;
            used = used.saturating_sub(strip.size);
        }
        accounting.used_bytes = used;
        Ok(())
    }

    fn scan(&self) -> Result<Vec<StripFile>, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "Failed to read filmstrip cache {}: {error}",
                    self.root.display()
                ))
            }
        };
        let mut strips = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let meta = entry.metadata().map_err(|error| error.to_string())?;
            let is_strip = meta.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == STRIP_EXTENSION);
            if !is_strip {
                // Leftovers of an interrupted write, or anything else that is not a strip.
                if meta.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    remove_if_present(&path)?;
                }
                continue;
            }
            strips.push(StripFile {
                path,
                size: meta.len(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
        Ok(strips)
    }
}

fn usage_of(accounting: &Accounting) -> FilmstripCacheUsage {
    FilmstripCacheUsage {
        used_bytes: accounting.used_bytes,
        limit_bytes: accounting.limit_bytes,
        min_limit_bytes: MIN_FILMSTRIP_CACHE_LIMIT,
        max_limit_bytes: MAX_FILMSTRIP_CACHE_LIMIT,
        limit_step_bytes: FILMSTRIP_CACHE_LIMIT_STEP,
    }
}

fn touch(path: &Path) -> std::io::Result<()> {
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()))
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove cached filmstrip {}: {error}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ovrley-filmstrip-cache-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn set_age(cache: &FilmstripCache, name: &str, seconds_ago: u64) {
        let path = cache.strip_path(name).unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(seconds_ago))
            .unwrap();
    }

    #[test]
    fn limits_must_be_whole_steps_within_bounds() {
        assert!(validate_filmstrip_cache_limit(DEFAULT_FILMSTRIP_CACHE_LIMIT).is_ok());
        assert!(validate_filmstrip_cache_limit(MIN_FILMSTRIP_CACHE_LIMIT).is_ok());
        assert!(validate_filmstrip_cache_limit(MAX_FILMSTRIP_CACHE_LIMIT).is_ok());
        assert!(validate_filmstrip_cache_limit(0).is_err());
        assert!(validate_filmstrip_cache_limit(MIN_FILMSTRIP_CACHE_LIMIT + 1).is_err());
        assert!(validate_filmstrip_cache_limit(
            MAX_FILMSTRIP_CACHE_LIMIT + FILMSTRIP_CACHE_LIMIT_STEP
        )
        .is_err());
    }

    #[test]
    fn stores_and_reads_strips_and_counts_their_size() {
        let root = temp_root();
        let cache = FilmstripCache::open(root.clone(), DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();

        assert_eq!(cache.read("a").unwrap(), None);
        cache.write("a", &[1; 10]).unwrap();
        cache.write("b", &[2; 5]).unwrap();
        // Replacing a strip counts only the new one.
        cache.write("a", &[3; 4]).unwrap();

        assert_eq!(cache.read("a").unwrap(), Some(vec![3; 4]));
        let usage = cache.usage().unwrap();
        assert_eq!(usage.used_bytes, 9);
        assert_eq!(usage.limit_bytes, DEFAULT_FILMSTRIP_CACHE_LIMIT);
        // A reopened cache recounts the same strips.
        let reopened = FilmstripCache::open(root.clone(), DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();
        assert_eq!(reopened.usage().unwrap().used_bytes, 9);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn evicts_the_least_recently_used_strips_past_the_limit() {
        let root = temp_root();
        let cache = FilmstripCache::open_unchecked(root.clone(), 30);
        for name in ["old", "used", "new"] {
            cache.write(name, &[0; 10]).unwrap();
        }
        set_age(&cache, "old", 300);
        set_age(&cache, "used", 200);
        set_age(&cache, "new", 100);
        // Reading the second oldest strip makes it the most recently used.
        cache.read("used").unwrap();

        cache.write("newest", &[0; 10]).unwrap();

        assert_eq!(cache.read("old").unwrap(), None);
        assert!(cache.read("used").unwrap().is_some());
        assert!(cache.read("new").unwrap().is_some());
        assert!(cache.read("newest").unwrap().is_some());
        assert_eq!(cache.usage().unwrap().used_bytes, 30);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_lower_limit_evicts_down_to_it_and_clearing_empties_the_cache() {
        let root = temp_root();
        let cache = FilmstripCache::open(root.clone(), DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();
        cache.write("a", &[0; 10]).unwrap();
        assert!(cache.set_limit(MIN_FILMSTRIP_CACHE_LIMIT + 1).is_err());

        let usage = cache.set_limit(MIN_FILMSTRIP_CACHE_LIMIT).unwrap();
        assert_eq!(
            (usage.used_bytes, usage.limit_bytes),
            (10, MIN_FILMSTRIP_CACHE_LIMIT)
        );

        let usage = cache.clear().unwrap();
        assert_eq!(usage.used_bytes, 0);
        assert_eq!(cache.read("a").unwrap(), None);
        // The cache keeps working after a clear.
        cache.write("b", &[0; 3]).unwrap();
        assert_eq!(cache.usage().unwrap().used_bytes, 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opening_removes_leftovers_of_interrupted_writes() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".abc.tmp"), [0; 50]).unwrap();
        fs::write(root.join("kept.jpg"), [0; 7]).unwrap();

        let cache = FilmstripCache::open(root.clone(), DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();

        assert!(!root.join(".abc.tmp").exists());
        assert_eq!(cache.usage().unwrap().used_bytes, 7);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn names_cannot_leave_the_cache_directory() {
        let cache = FilmstripCache::open_unchecked(temp_root(), 10);
        assert!(cache.read("../escape").is_err());
        assert!(cache.read(".hidden").is_err());
        assert!(cache.write("a/b", &[0]).is_err());
        assert!(cache.read("").is_err());
    }
}
