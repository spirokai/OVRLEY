//! Tauri command wrappers for `ovrley_services::video_filmstrip` and the filmstrip cache.

use ovrley_services::filmstrip_cache::{FilmstripCache, FilmstripCacheUsage};
use ovrley_services::video_filmstrip::{
    cached_filmstrip, filmstrip, Filmstrip, FilmstripHwaccel, FilmstripRequest, FilmstripWindow,
};
use std::path::PathBuf;

/// The cached strip `window` of a source video, without decoding anything:
/// `None` on a cache miss. Cheap enough to run outside the decode queue.
#[tauri::command]
pub(crate) async fn backend_cached_video_filmstrip(
    cache: tauri::State<'_, FilmstripCache>,
    path: String,
    duration_seconds: f64,
    window: FilmstripWindow,
    frame_width: u32,
    frame_height: u32,
) -> Result<Option<Filmstrip>, String> {
    let request = FilmstripRequest::new(
        PathBuf::from(path),
        duration_seconds,
        window,
        frame_width,
        frame_height,
    )?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cached_filmstrip(&cache, &request))
        .await
        .map_err(|error| format!("Video filmstrip task failed: {error}"))?
}

/// The strip `window` of a source video, from the disk cache or decoded with
/// `hwaccel` (or in software) and cached. `None` when FFmpeg cannot decode a
/// frame of it; an FFmpeg that cannot be found is an error.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn backend_video_filmstrip(
    app: tauri::AppHandle,
    cache: tauri::State<'_, FilmstripCache>,
    path: String,
    duration_seconds: f64,
    window: FilmstripWindow,
    frame_width: u32,
    frame_height: u32,
    hwaccel: Option<FilmstripHwaccel>,
) -> Result<Option<Filmstrip>, String> {
    let request = FilmstripRequest::new(
        PathBuf::from(path),
        duration_seconds,
        window,
        frame_width,
        frame_height,
    )?;
    let paths = crate::runtime_paths::app_paths(&app)?;
    let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root)
        .map_err(|error| error.to_string())?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || filmstrip(&cache, &ffmpeg, &request, hwaccel))
        .await
        .map_err(|error| format!("Video filmstrip task failed: {error}"))?
}

/// The filmstrip cache's current size and limit.
#[tauri::command]
pub(crate) async fn backend_get_filmstrip_cache(
    cache: tauri::State<'_, FilmstripCache>,
) -> Result<FilmstripCacheUsage, String> {
    cache.usage()
}

/// Saves and applies a new filmstrip cache limit, evicting down to it.
#[tauri::command]
pub(crate) async fn backend_set_filmstrip_cache_limit(
    app: tauri::AppHandle,
    cache: tauri::State<'_, FilmstripCache>,
    limit_bytes: u64,
) -> Result<FilmstripCacheUsage, String> {
    let cache = cache.inner().clone();
    let usage = tauri::async_runtime::spawn_blocking(move || cache.set_limit(limit_bytes))
        .await
        .map_err(|error| error.to_string())??;
    crate::filmstrip_cache_settings::persist_filmstrip_cache_limit(&app, limit_bytes)?;
    Ok(usage)
}

/// Deletes every cached filmstrip.
#[tauri::command]
pub(crate) async fn backend_clear_filmstrip_cache(
    cache: tauri::State<'_, FilmstripCache>,
) -> Result<FilmstripCacheUsage, String> {
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cache.clear())
        .await
        .map_err(|error| error.to_string())?
}
