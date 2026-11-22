//! The size-limited disk caches: their shared limit policy and usage report, and the filmstrip
//! cache.
//!
//! Both the map tile cache ([`crate::tiles`]) and the filmstrip cache keep files in the
//! application cache directory, evict the least recently used first (a read refreshes a file's
//! modified time), and take a limit in whole steps of 128 MiB between the same bounds.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

const MIB: u64 = 1024 * 1024;
/// The granularity of a cache limit.
const LIMIT_STEP: u64 = 128 * MIB;
const MIN_LIMIT: u64 = LIMIT_STEP;
const MAX_LIMIT: u64 = 40 * LIMIT_STEP;

/// A cache's size limit: a whole number of 128 MiB steps between 128 MiB and 5 GiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheLimit(u64);

impl CacheLimit {
    /// # Errors
    ///
    /// When `bytes` is not a whole number of steps inside the bounds.
    pub fn new(bytes: u64) -> Result<Self, String> {
        if !(MIN_LIMIT..=MAX_LIMIT).contains(&bytes) || !bytes.is_multiple_of(LIMIT_STEP) {
            return Err(format!(
                "Cache limit {bytes} must be a multiple of {LIMIT_STEP} between {MIN_LIMIT} and {MAX_LIMIT} bytes"
            ));
        }
        Ok(Self(bytes))
    }

    /// `steps` whole steps; only for the defaults, which lie inside the bounds.
    pub(crate) const fn steps(steps: u64) -> Self {
        Self(steps * LIMIT_STEP)
    }

    /// Any limit, for tests that fill or overfill a cache with a few bytes.
    #[cfg(test)]
    pub(crate) const fn unchecked(bytes: u64) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn bytes(self) -> u64 {
        self.0
    }

    /// The report of a cache holding `used_bytes` under this limit.
    pub(crate) fn usage(self, used_bytes: u64) -> CacheUsage {
        CacheUsage {
            used_bytes,
            limit_bytes: self.0,
            min_limit_bytes: MIN_LIMIT,
            max_limit_bytes: MAX_LIMIT,
            limit_step_bytes: LIMIT_STEP,
        }
    }
}

/// A cache's current size, its limit, and the bounds and step a new limit must respect, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheUsage {
    pub used_bytes: u64,
    pub limit_bytes: u64,
    pub min_limit_bytes: u64,
    pub max_limit_bytes: u64,
    pub limit_step_bytes: u64,
}

/// Marks a cached file as just used.
pub(crate) fn touch(path: &Path) -> std::io::Result<()> {
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()))
}

/// Removes a file; one already gone is fine.
pub(crate) fn remove_if_present(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to remove cached file {}: {error}", path.display())),
    }
}

/// Removes a cache's whole directory; one already gone is fine.
pub(crate) fn remove_dir_if_present(root: &Path) -> Result<(), String> {
    match fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to clear cache {}: {error}", root.display())),
    }
}

/// The filmstrip cache's limit when the settings hold none.
pub const DEFAULT_FILMSTRIP_CACHE_LIMIT: CacheLimit = CacheLimit::steps(2);
const FILMSTRIP_DIR: &str = "filmstrips";
const STRIP_EXTENSION: &str = "jpg";

/// The timeline filmstrips: one JPEG sprite per strip, so a video's strips survive restarts and are
/// shared by every project that uses the file. Clones share one directory and one accounting.
///
/// Strip names, which encode the source file's stamp and the strip's sampling, are
/// [`crate::video`]'s.
#[derive(Clone, Debug)]
pub struct FilmstripCache {
    root: PathBuf,
    /// Held for every write, eviction and clear, so the accounting matches the directory. Reads
    /// take no lock: a strip evicted under a read is a miss.
    accounting: Arc<Mutex<Accounting>>,
}

#[derive(Debug)]
struct Accounting {
    used_bytes: u64,
    limit: CacheLimit,
}

struct StripFile {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

impl FilmstripCache {
    /// Opens the cache in `app_cache_dir` (created on the first write) and evicts down to `limit`.
    ///
    /// # Errors
    ///
    /// When the cache directory cannot be read or a strip over the limit cannot be removed.
    pub fn open(app_cache_dir: &Path, limit: CacheLimit) -> Result<Self, String> {
        let cache = Self {
            root: app_cache_dir.join(FILMSTRIP_DIR),
            accounting: Arc::new(Mutex::new(Accounting { used_bytes: 0, limit })),
        };
        cache.reconcile_and_evict(&mut cache.lock())?;
        Ok(cache)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Accounting> {
        self.accounting.lock().expect("filmstrip cache lock poisoned")
    }

    fn strip_path(&self, name: &str) -> Result<PathBuf, String> {
        let valid = !name.is_empty()
            && !name.starts_with('.')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !valid {
            return Err(format!("Invalid filmstrip cache name: {name}"));
        }
        Ok(self.root.join(format!("{name}.{STRIP_EXTENSION}")))
    }

    /// The cached strip named `name`, marked as just used; `None` on a miss.
    ///
    /// # Errors
    ///
    /// When `name` could leave the cache directory, or the strip exists but cannot be read.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.strip_path(name)?;
        match fs::read(&path) {
            Ok(bytes) => {
                // A strip evicted between the read and the touch stays a hit for this read.
                let _ = touch(&path);
                Ok(Some(bytes))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("Failed to read cached filmstrip {}: {error}", path.display())),
        }
    }

    /// Stores a strip under `name`, replacing any earlier one, then evicts the least recently used
    /// strips while the cache is over its limit.
    ///
    /// # Errors
    ///
    /// When `name` could leave the cache directory, or the strip cannot be written.
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.strip_path(name)?;
        let mut accounting = self.lock();
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("Failed to create filmstrip cache {}: {error}", self.root.display()))?;
        let replaced = fs::metadata(&path).map_or(0, |meta| meta.len());
        let temporary = self.root.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
        fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, &path)).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            format!("Failed to write cached filmstrip {}: {error}", path.display())
        })?;
        accounting.used_bytes = accounting.used_bytes.saturating_sub(replaced).saturating_add(bytes.len() as u64);
        if accounting.used_bytes > accounting.limit.bytes() {
            self.reconcile_and_evict(&mut accounting)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn usage(&self) -> CacheUsage {
        let accounting = self.lock();
        accounting.limit.usage(accounting.used_bytes)
    }

    /// Applies a new limit and evicts the least recently used strips down to it.
    ///
    /// # Errors
    ///
    /// When the directory cannot be read or a strip cannot be removed.
    pub fn set_limit(&self, limit: CacheLimit) -> Result<CacheUsage, String> {
        let mut accounting = self.lock();
        accounting.limit = limit;
        self.reconcile_and_evict(&mut accounting)?;
        Ok(limit.usage(accounting.used_bytes))
    }

    /// Deletes every cached strip.
    ///
    /// # Errors
    ///
    /// When the cache directory cannot be removed.
    pub fn clear(&self) -> Result<CacheUsage, String> {
        let mut accounting = self.lock();
        remove_dir_if_present(&self.root)?;
        accounting.used_bytes = 0;
        Ok(accounting.limit.usage(0))
    }

    /// Recounts the directory, deletes leftovers of interrupted writes, and deletes the least
    /// recently used strips until the cache fits its limit.
    fn reconcile_and_evict(&self, accounting: &mut Accounting) -> Result<(), String> {
        let mut strips = self.scan()?;
        strips.sort_by_key(|strip| strip.modified);
        let mut used = strips.iter().fold(0_u64, |total, strip| total.saturating_add(strip.size));
        for strip in &strips {
            if used <= accounting.limit.bytes() {
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
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("Failed to read filmstrip cache {}: {error}", self.root.display())),
        };
        let mut strips = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let meta = entry.metadata().map_err(|error| error.to_string())?;
            let is_strip = meta.is_file() && path.extension().is_some_and(|extension| extension == STRIP_EXTENSION);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("ovrley-filmstrip-cache-test-{}", uuid::Uuid::new_v4()))
    }

    fn set_age(cache: &FilmstripCache, name: &str, seconds_ago: u64) {
        fs::File::options()
            .write(true)
            .open(cache.strip_path(name).unwrap())
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(seconds_ago))
            .unwrap();
    }

    #[test]
    fn limits_must_be_whole_steps_within_bounds() {
        assert!(CacheLimit::new(MIN_LIMIT).is_ok());
        assert!(CacheLimit::new(MAX_LIMIT).is_ok());
        assert!(CacheLimit::new(0).is_err());
        assert!(CacheLimit::new(MIN_LIMIT + 1).is_err());
        assert!(CacheLimit::new(MAX_LIMIT + LIMIT_STEP).is_err());
        assert_eq!(CacheLimit::new(DEFAULT_FILMSTRIP_CACHE_LIMIT.bytes()), Ok(DEFAULT_FILMSTRIP_CACHE_LIMIT));
    }

    #[test]
    fn stores_and_reads_strips_and_counts_their_size() {
        let dir = temp_dir();
        let cache = FilmstripCache::open(&dir, DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();

        assert_eq!(cache.read("a").unwrap(), None);
        cache.write("a", &[1; 10]).unwrap();
        cache.write("b", &[2; 5]).unwrap();
        // Replacing a strip counts only the new one.
        cache.write("a", &[3; 4]).unwrap();

        assert_eq!(cache.read("a").unwrap(), Some(vec![3; 4]));
        let usage = cache.usage();
        assert_eq!(usage.used_bytes, 9);
        assert_eq!(usage.limit_bytes, DEFAULT_FILMSTRIP_CACHE_LIMIT.bytes());
        // A reopened cache recounts the same strips.
        let reopened = FilmstripCache::open(&dir, DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();
        assert_eq!(reopened.usage().used_bytes, 9);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn evicts_the_least_recently_used_strips_past_the_limit() {
        let dir = temp_dir();
        let cache = FilmstripCache::open(&dir, CacheLimit::unchecked(30)).unwrap();
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
        assert_eq!(cache.usage().used_bytes, 30);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_lower_limit_evicts_down_to_it_and_clearing_empties_the_cache() {
        let dir = temp_dir();
        let cache = FilmstripCache::open(&dir, DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();
        cache.write("a", &[0; 10]).unwrap();

        let usage = cache.set_limit(CacheLimit::steps(1)).unwrap();
        assert_eq!((usage.used_bytes, usage.limit_bytes), (10, MIN_LIMIT));

        let usage = cache.clear().unwrap();
        assert_eq!(usage.used_bytes, 0);
        assert_eq!(cache.read("a").unwrap(), None);
        // The cache keeps working after a clear.
        cache.write("b", &[0; 3]).unwrap();
        assert_eq!(cache.usage().used_bytes, 3);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn opening_removes_leftovers_of_interrupted_writes() {
        let dir = temp_dir();
        let root = dir.join(FILMSTRIP_DIR);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".abc.tmp"), [0; 50]).unwrap();
        fs::write(root.join("kept.jpg"), [0; 7]).unwrap();

        let cache = FilmstripCache::open(&dir, DEFAULT_FILMSTRIP_CACHE_LIMIT).unwrap();

        assert!(!root.join(".abc.tmp").exists());
        assert_eq!(cache.usage().used_bytes, 7);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn names_cannot_leave_the_cache_directory() {
        let cache = FilmstripCache::open(&temp_dir(), CacheLimit::unchecked(10)).unwrap();
        assert!(cache.read("../escape").is_err());
        assert!(cache.read(".hidden").is_err());
        assert!(cache.write("a/b", &[0]).is_err());
        assert!(cache.read("").is_err());
    }
}
