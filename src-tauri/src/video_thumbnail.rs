//! Still frames of source videos: the preview stored in a project archive and the
//! cards of the video panel. Card thumbnails are made on import and project load and
//! are never stored.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Largest thumbnail accepted, generated or read from a project archive.
pub(crate) const MAX_THUMBNAIL_SIZE: u64 = 2 * 1024 * 1024;
const THUMBNAIL_FILTER: &str =
    "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2:color=black@0";

/// Renders the first frame of a video as a 320×180 PNG, letterboxed with
/// transparency. `None` when FFmpeg cannot produce the frame.
pub(crate) fn generate_thumbnail(ffmpeg: &Path, video_path: &Path) -> Option<Vec<u8>> {
    let mut command = Command::new(ffmpeg);
    ovrley_core::encode::ffmpeg::binary::configure_ffmpeg_command(&mut command);
    let output = command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(video_path)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg(THUMBNAIL_FILTER)
        .arg("-f")
        .arg("image2pipe")
        .arg("-vcodec")
        .arg("png")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success()
        || output.stdout.is_empty()
        || output.stdout.len() as u64 > MAX_THUMBNAIL_SIZE
    {
        return None;
    }
    Some(output.stdout)
}

/// Encodes PNG bytes as a `data:` URL the webview can show directly.
pub(crate) fn png_data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", BASE64_STANDARD.encode(bytes))
}

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
