//! Tauri command wrapper for `ovrley_services::video_thumbnail`.

use ovrley_services::video_thumbnail::{generate_thumbnail, png_data_url};
use std::path::PathBuf;

/// Renders the card thumbnail of a source video. `None` when FFmpeg cannot decode
/// a frame of it; an FFmpeg that cannot be found is an error.
#[tauri::command]
pub(crate) async fn backend_video_thumbnail(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<String>, String> {
    let video_path = PathBuf::from(path);
    if !video_path.is_absolute() {
        return Err("Video thumbnail path must be absolute".into());
    }
    let paths = crate::runtime_paths::app_paths(&app)?;
    let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root)
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        generate_thumbnail(&ffmpeg, &video_path).map(|bytes| png_data_url(&bytes))
    })
    .await
    .map_err(|error| format!("Video thumbnail task failed: {error}"))
}
