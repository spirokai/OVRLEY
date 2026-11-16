//! Persisted filmstrip cache settings.
//!
//! Owns: the cache-limit key in the shared app settings store, which the
//! frontend's preferences store also opens. Rust is the only writer of this key
//! so the startup limit and the running limit always agree.

use ovrley_services::filmstrip_cache::{
    validate_filmstrip_cache_limit, DEFAULT_FILMSTRIP_CACHE_LIMIT,
};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const SETTINGS_STORE: &str = "ovrley-settings.json";
const FILMSTRIP_CACHE_LIMIT_KEY: &str = "filmstripCacheLimitBytes";

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
