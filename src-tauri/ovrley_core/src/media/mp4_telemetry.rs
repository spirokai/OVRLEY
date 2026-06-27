//! Source video metadata and telemetry extraction via telemetry-parser.
//!
//! Owns: `probe_video_metadata()` for telemetry-parser-first source-video
//! metadata reads. Later phases add dense GPS/camera/IMU extraction from the
//! same MP4 telemetry source.
//! Does not own: ffprobe binary discovery (see [`crate::encode::ffmpeg`]),
//! activity parsing/finalization, rendering, or output encoding.
//!
//! Allowed dependencies: `crate::error`, `crate::encode::fps`, `crate::media`,
//! `telemetry_parser`, and `std`.
//! Forbidden dependencies: `crate::commands`, `crate::render`.
//!
//! ## Thread Safety
//!
//! Single-threaded. Reads the file synchronously through telemetry-parser. No
//! shared mutable state.
//!
//! ## Performance
//!
//! Called once per imported video. `telemetry-parser` reads enough MP4 metadata
//! to identify the video track; this is not part of the render hot path.

use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::media::{Resolution, SourceVideoMetadata};
use std::fs::File;
use std::path::Path;

/// Probes source-video metadata with telemetry-parser.
///
/// This returns only fields telemetry-parser can read cheaply from the source
/// video track. Codec, audio, container, and sync timestamp fields are filled by
/// the caller using ffprobe salvage when needed.
///
/// # Errors
///
/// Returns [`CoreError::Io`] when the file cannot be opened or statted.
/// Returns [`CoreError::Encode`] when telemetry-parser cannot read video
/// metadata from the file.
#[must_use = "probe result contains source video metadata required for preview and sync"]
pub fn probe_video_metadata(file_path: &str) -> CoreResult<SourceVideoMetadata> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len() as usize;

    let mut stream = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let vm = telemetry_parser::util::get_video_metadata(&mut stream, file_size)
        .map_err(|error| CoreError::Encode(format!("telemetry-parser metadata error: {error}")))?;

    let (fps_num, fps_den) = rational_fps_parts(vm.fps);

    Ok(SourceVideoMetadata {
        path: file_path.to_string(),
        duration: positive_f64(vm.duration_s),
        fps: positive_f64(vm.fps),
        fps_num,
        fps_den,
        resolution: Some(Resolution {
            width: vm.width as u64,
            height: vm.height as u64,
        }),
        creation_time: None,
        sync_time: None,
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        bits_per_raw_sample: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: Some(vm.rotation),
    })
}

/// Converts floating FPS metadata into the shared rational representation.
///
/// This intentionally reuses [`Fps::from_f64_fallback`] instead of carrying a
/// second common-rate table in the telemetry module.
pub fn rational_fps_parts(fps: f64) -> (Option<u32>, Option<u32>) {
    match Fps::from_f64_fallback(fps) {
        Ok(fps) => (Some(fps.num), Some(fps.den)),
        Err(_) => (None, None),
    }
}

fn positive_f64(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}
