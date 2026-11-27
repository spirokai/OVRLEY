//! Settings → the disk caches: one row each for the map tile cache and the filmstrip cache, with
//! its usage, a size limit slider and Clear cache. The caches themselves belong to the map
//! services and the video tools; this module runs their commands on workers and keeps the rows'
//! state.

use crate::store::services::{
    MapServices, Services, VideoTools, FILMSTRIP_CACHE_LIMIT_KEY, MAP_TILE_CACHE_LIMIT_KEY,
};
use crate::store::Store;
use ovrley_services::cache::{CacheLimit, CacheUsage, LimitedCache};
use serde_json::Value;
use std::collections::VecDeque;

/// A disk cache the settings show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskCache {
    MapTiles,
    Filmstrips,
}

impl DiskCache {
    const ALL: [Self; 2] = [Self::MapTiles, Self::Filmstrips];

    /// The settings key its limit is saved under.
    fn limit_key(self) -> &'static str {
        match self {
            Self::MapTiles => MAP_TILE_CACHE_LIMIT_KEY,
            Self::Filmstrips => FILMSTRIP_CACHE_LIMIT_KEY,
        }
    }
}

/// The rows' state.
#[derive(Debug, Default)]
pub struct DiskCachesState {
    map_tiles: DiskCacheState,
    filmstrips: DiskCacheState,
}

impl DiskCachesState {
    pub fn get(&self, cache: DiskCache) -> &DiskCacheState {
        match cache {
            DiskCache::MapTiles => &self.map_tiles,
            DiskCache::Filmstrips => &self.filmstrips,
        }
    }

    fn get_mut(&mut self, cache: DiskCache) -> &mut DiskCacheState {
        match cache {
            DiskCache::MapTiles => &mut self.map_tiles,
            DiskCache::Filmstrips => &mut self.filmstrips,
        }
    }
}

/// A cache row's state.
#[derive(Debug, Default)]
pub struct DiskCacheState {
    /// The cache's usage, once loaded.
    pub usage: Option<CacheUsage>,
    /// The slider's value while it drags, and until the command it committed answers.
    pub draft_limit: Option<u64>,
    /// Whether a cache command is running; it disables Clear.
    pub pending: bool,
    /// The commands given while one runs, since the slider stays live meanwhile. They run one after
    /// another, so the newest is also the last to reach the cache and the settings file.
    queued: VecDeque<Command>,
}

impl DiskCacheState {
    /// The slider's limit: the draft while dragging, else the cache's.
    pub fn limit(&self) -> Option<u64> {
        self.draft_limit
            .or(self.usage.map(|usage| usage.limit_bytes))
    }
}

#[derive(Clone, Copy, Debug)]
enum Command {
    Usage,
    SetLimit(u64),
    Clear,
}

/// Runs `command` on `cache`, which the map services or the video tools own. Blocking.
fn run(
    maps: &dyn MapServices,
    video: &dyn VideoTools,
    cache: DiskCache,
    command: Command,
) -> Result<CacheUsage, String> {
    let cache: &dyn LimitedCache = match cache {
        DiskCache::MapTiles => maps.tile_cache(),
        DiskCache::Filmstrips => video.filmstrip_cache(),
    };
    match command {
        Command::Usage => Ok(cache.usage()),
        Command::SetLimit(limit) => cache.set_limit(CacheLimit::new(limit)?),
        Command::Clear => cache.clear(),
    }
}

/// The settings showed: every cache's usage loads on a worker. A failure goes to the error alert.
pub fn load_disk_caches(_: &mut Store, services: &mut Services) {
    for cache in DiskCache::ALL {
        let (maps, video) = (services.maps.clone(), services.video_tools.clone());
        services.workers.spawn("disk-cache-usage", move || {
            let usage = run(maps.as_ref(), video.as_ref(), cache, Command::Usage);
            Box::new(move |store, _| match usage {
                Ok(usage) => store.disk_caches.get_mut(cache).usage = Some(usage),
                Err(error) => store.media.set_error_message(error),
            })
        });
    }
}

/// The slider moves.
pub fn draft_disk_cache_limit(store: &mut Store, cache: DiskCache, limit: u64) {
    store.disk_caches.get_mut(cache).draft_limit = Some(limit);
}

/// The slider is released at `limit`.
pub fn commit_disk_cache_limit(
    store: &mut Store,
    services: &mut Services,
    cache: DiskCache,
    limit: u64,
) {
    queue_command(store, services, cache, Command::SetLimit(limit));
}

/// Clear cache.
pub fn clear_disk_cache(store: &mut Store, services: &mut Services, cache: DiskCache) {
    queue_command(store, services, cache, Command::Clear);
}

/// Runs a cache command on a worker, after the ones given before it. Only the newest command's
/// answer shows; a failure goes to the error alert. A limit that applied is saved.
fn queue_command(store: &mut Store, services: &mut Services, cache: DiskCache, command: Command) {
    let row = store.disk_caches.get_mut(cache);
    if row.pending {
        row.queued.push_back(command);
        return;
    }
    row.pending = true;
    start_command(services, cache, command);
}

fn start_command(services: &mut Services, cache: DiskCache, command: Command) {
    let (maps, video) = (services.maps.clone(), services.video_tools.clone());
    services.workers.spawn("disk-cache", move || {
        let result = run(maps.as_ref(), video.as_ref(), cache, command);
        Box::new(move |store, services| {
            if let (Ok(usage), Command::SetLimit(_)) = (&result, command) {
                services
                    .preferences
                    .set(cache.limit_key(), Value::from(usage.limit_bytes));
            }
            let row = store.disk_caches.get_mut(cache);
            let next = row.queued.pop_front();
            match result {
                Ok(usage) if next.is_none() => row.usage = Some(usage),
                Ok(_) => {}
                Err(error) => store.media.set_error_message(error),
            }
            let row = store.disk_caches.get_mut(cache);
            if let Some(next) = next {
                start_command(services, cache, next);
            } else {
                row.draft_limit = None;
                row.pending = false;
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;
    use serde_json::json;
    use std::time::Duration;

    fn settle(dispatcher: &Dispatcher, fakes: &Fakes) {
        for _ in 0..2000 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(|store| {
                !store.disk_caches.get(DiskCache::MapTiles).pending
                    && store.disk_caches.get(DiskCache::MapTiles).usage.is_some()
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_cache_row_applies_only_the_newest_command_and_saves_the_limit() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        let row = |store: &Store| store.disk_caches.get(DiskCache::MapTiles).limit();
        dispatcher.dispatch(load_disk_caches);
        settle(&dispatcher, &fakes);
        assert_eq!(dispatcher.with_store(row), Some(512 << 20));
        dispatcher.dispatch(|store, _| draft_disk_cache_limit(store, DiskCache::MapTiles, 1 << 30));
        assert_eq!(
            dispatcher.with_store(row),
            Some(1 << 30),
            "the slider drafts"
        );
        dispatcher.dispatch(|store, services| {
            commit_disk_cache_limit(store, services, DiskCache::MapTiles, 1 << 30)
        });
        dispatcher.dispatch(|store, services| {
            commit_disk_cache_limit(store, services, DiskCache::MapTiles, 768 << 20)
        });
        assert!(dispatcher.with_store(|store| store.disk_caches.get(DiskCache::MapTiles).pending));
        settle(&dispatcher, &fakes);
        dispatcher.with_store(|store| {
            assert_eq!(
                row(store),
                Some(768 << 20),
                "only the newest command applies"
            );
            assert_eq!(store.disk_caches.get(DiskCache::MapTiles).draft_limit, None);
        });
        let saved = fakes
            .preferences
            .saves
            .borrow()
            .last()
            .and_then(|values| values.get(MAP_TILE_CACHE_LIMIT_KEY).cloned());
        assert_eq!(saved, Some(json!(768u64 << 20)));
        dispatcher
            .dispatch(|store, services| clear_disk_cache(store, services, DiskCache::MapTiles));
        settle(&dispatcher, &fakes);
        assert!(fakes
            .maps
            .tile_cache
            .calls
            .lock()
            .unwrap()
            .contains(&"clear".to_string()));
    }
}
