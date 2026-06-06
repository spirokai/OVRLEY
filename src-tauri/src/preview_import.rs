//! Preview import helpers for the Tauri application shell.
//!
//! Owns: MIME-type detection for preview video paths, and conservative
//!       user-facing warnings derived from ffprobe metadata (HEVC, high
//!       bit depth, 4:2:2/4:4:4 chroma).
//! Does not own: the HTTP preview server, video probing, or import
//!       orchestration — those live in `video_server`, `ovrley_core`, and
//!       `tauri_commands` respectively.
//!
//! Allowed dependencies: `serde_json`, `std`.
//! Forbidden dependencies: `ovrley_core` (these are thin format/string helpers
//!       that don't need domain types).

use std::path::PathBuf;

/// Builds conservative user-facing preview warnings from ffprobe metadata.
///
/// These warnings never block import. They only flag formats that native WebView
/// media decoders commonly struggle with, such as HEVC, high bit depth, or
/// 4:2:2/4:4:4 chroma formats.
pub(crate) fn preview_warnings_for_metadata(metadata: &serde_json::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let pix_fmt = metadata
        .get("pixFmt")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bits_per_raw_sample = metadata
        .get("bitsPerRawSample")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if bits_per_raw_sample > 8 || pix_fmt.contains("10") || pix_fmt.contains("12") {
        warnings.push(
            "10-bit or higher-bit-depth footage may not play reliably in the native preview."
                .to_string(),
        );
    }

    if pix_fmt.contains("422") || pix_fmt.contains("444") {
        warnings.push(
            "4:2:2 or 4:4:4 footage may decode slowly or fail in the native preview.".to_string(),
        );
    }

    warnings
}

/// Maps a source path extension to the MIME type sent by the preview server.
///
/// Unknown extensions fall back to `application/octet-stream` so the preview
/// server can still attempt playback without claiming an incorrect video type.
pub(crate) fn content_type_for_path(path: &str) -> String {
    match PathBuf::from(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
    .to_string()
}
