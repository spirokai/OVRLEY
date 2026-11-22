//! Still frames of source videos: the preview stored in a project archive and the
//! cards of the video panel. Card thumbnails are made on import and project load and
//! are never stored.

use std::path::Path;
use std::process::{Command, Stdio};

/// Largest thumbnail accepted, generated or read from a project archive.
pub const MAX_THUMBNAIL_SIZE: u64 = 2 * 1024 * 1024;
const THUMBNAIL_FILTER: &str =
    "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2:color=black@0";

/// Renders the first frame of a video as a 320×180 PNG, letterboxed with
/// transparency. `None` when FFmpeg cannot produce the frame.
pub fn generate_thumbnail(ffmpeg: &Path, video_path: &Path) -> Option<Vec<u8>> {
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
