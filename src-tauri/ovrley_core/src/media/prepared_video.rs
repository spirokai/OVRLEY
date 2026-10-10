//! Shared source preparation, independent of IPC and preview registration.
//! External probe metadata is normalized once before inspection publishes it.

use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::media::{Resolution, SourceVideoMetadata};
use crate::paths::AppPaths;

/// Session-scoped source metadata, never a preview registration or activity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectedVideoSource {
    pub source_id: String,
    /// Canonical absolute input path is metadata.path.
    pub metadata: SourceVideoMetadata,
    /// Display geometry; metadata.resolution retains the coded dimensions.
    pub display_resolution: Resolution,
    pub stamp: SourceFileStamp,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceFileStamp {
    pub size_bytes: u64,
    /// Decimal Unix nanoseconds as text, preserving precision across JS IPC.
    pub modified_at_unix_nanos: String,
}

/// Required rendering fields are normalized at the external-media boundary.
/// Missing timestamps remain explicit optional absence: synchronization owns
/// eligibility, and embedded activity is resolved only during job preparation.
pub fn normalize_inspected_metadata(
    mut metadata: SourceVideoMetadata,
    canonical_path: &str,
) -> CoreResult<(SourceVideoMetadata, Resolution)> {
    metadata.path = canonical_path.to_owned();
    let invalid = |field| CoreError::Encode(format!("Unusable video {field}: {canonical_path}"));
    metadata.duration = Some(
        metadata
            .duration
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| invalid("duration"))?,
    );
    let fps = match (metadata.fps_num, metadata.fps_den) {
        (Some(num), Some(den)) if num > 0 && den > 0 => Fps::new(num, den)?,
        _ => Fps::from_f64_metadata(metadata.fps.ok_or_else(|| invalid("frame rate"))?)?,
    };
    let (num, den) = fps.components();
    metadata.fps_num = Some(num);
    metadata.fps_den = Some(den);
    metadata.fps = Some(fps.as_f64());
    let resolution = metadata
        .resolution
        .as_ref()
        .ok_or_else(|| invalid("resolution"))?;
    if resolution.width == 0
        || resolution.height == 0
        || resolution.width > u64::from(u32::MAX)
        || resolution.height > u64::from(u32::MAX)
    {
        return Err(invalid("resolution"));
    }
    // Absent external rotation means no rotation, matching interactive import.
    let rotation = metadata.rotation_degrees.unwrap_or(0).rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) {
        return Err(invalid("rotation"));
    }
    metadata.rotation_degrees = Some(rotation);
    let display_resolution = if matches!(rotation, 90 | 270) {
        Resolution {
            width: resolution.height,
            height: resolution.width,
        }
    } else {
        resolution.clone()
    };
    Ok((metadata, display_resolution))
}

/// Practical file freshness, without hashing or copying the media payload.
pub fn source_file_stamp(path: &Path) -> CoreResult<SourceFileStamp> {
    let io_error = |source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    };
    let metadata = std::fs::metadata(path).map_err(io_error)?;
    if !metadata.is_file() {
        return Err(CoreError::Config(format!(
            "Video path is not a file: {}",
            path.display()
        )));
    }
    let modified = metadata.modified().map_err(io_error)?;
    let nanos = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos().to_string(),
        Err(error) => format!("-{}", error.duration().as_nanos()),
    };
    Ok(SourceFileStamp {
        size_bytes: metadata.len(),
        modified_at_unix_nanos: nanos,
    })
}

/// Reused immediately before a queued source's turn, independently of sessions.
pub fn check_source_freshness(source: &InspectedVideoSource) -> CoreResult<()> {
    let path = Path::new(&source.metadata.path);
    if canonical_source_path(path)? != source.metadata.path
        || source_file_stamp(path)? != source.stamp
    {
        return Err(CoreError::Config(format!(
            "Source changed; inspect it again: {}",
            source.metadata.path
        )));
    }
    Ok(())
}

pub fn canonical_source_path(path: &Path) -> CoreResult<String> {
    let canonical = std::fs::canonicalize(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    canonical
        .into_os_string()
        .into_string()
        .map_err(|_| CoreError::Config("Source path cannot be represented as UTF-8".into()))
}

/// Telemetry-first probing with the existing ffprobe salvage policy. This is
/// the common owner for interactive import and non-interactive inspection.
pub fn probe_video_metadata(
    paths: &AppPaths,
    file_path: &str,
) -> CoreResult<crate::media::SourceVideoMetadata> {
    match crate::media::mp4_telemetry::probe_video_metadata(&paths.repo_root, file_path) {
        Ok(metadata) => {
            if needs_ffprobe_salvage(&metadata) {
                match crate::media::video_probe::probe_video(&paths.repo_root, file_path) {
                    Ok(ffprobe_metadata) => Ok(merge_ffprobe_metadata(metadata, ffprobe_metadata)),
                    Err(error) => {
                        log::warn!("ffprobe fallback failed for {file_path}: {error}");
                        Ok(metadata)
                    }
                }
            } else {
                Ok(metadata)
            }
        }
        Err(error) => {
            log::warn!(
                "telemetry-parser probe failed for {file_path}: {error}; falling back to ffprobe"
            );
            crate::media::video_probe::probe_video(&paths.repo_root, file_path)
        }
    }
}

fn needs_ffprobe_salvage(metadata: &crate::media::SourceVideoMetadata) -> bool {
    metadata.duration.is_none()
        || metadata.fps.is_none()
        || metadata.fps_num.is_none()
        || metadata.fps_den.is_none()
        || metadata.sync_time.is_none()
        || metadata.creation_time.is_none()
        || metadata.codec_name.is_none()
        || metadata.codec_long_name.is_none()
        || metadata.codec_profile.is_none()
        || metadata.pix_fmt.is_none()
        || metadata.bits_per_raw_sample.is_none()
        || metadata.resolution.is_none()
        || metadata
            .rotation_degrees
            .map(|degrees| degrees.rem_euclid(360) == 0)
            .unwrap_or(true)
        || metadata.container_format.is_none()
        || metadata.bit_rate.is_none()
        || !metadata.has_audio
}

fn merge_ffprobe_metadata(
    mut metadata: crate::media::SourceVideoMetadata,
    ffprobe_metadata: crate::media::SourceVideoMetadata,
) -> crate::media::SourceVideoMetadata {
    if metadata.duration.is_none() {
        metadata.duration = ffprobe_metadata.duration;
    }
    if metadata.fps.is_none() {
        metadata.fps = ffprobe_metadata.fps;
    }
    if metadata.fps_num.is_none() {
        metadata.fps_num = ffprobe_metadata.fps_num;
    }
    if metadata.fps_den.is_none() {
        metadata.fps_den = ffprobe_metadata.fps_den;
    }
    if metadata.sync_time.is_none() {
        metadata.sync_time = ffprobe_metadata
            .sync_time
            .clone()
            .or_else(|| ffprobe_metadata.creation_time.clone());
    }
    if metadata.creation_time.is_none() {
        metadata.creation_time = ffprobe_metadata.creation_time;
        metadata.time_source = ffprobe_metadata.time_source.clone();
    }
    if metadata.codec_name.is_none() {
        metadata.codec_name = ffprobe_metadata.codec_name;
    }
    if metadata.codec_long_name.is_none() {
        metadata.codec_long_name = ffprobe_metadata.codec_long_name;
    }
    if metadata.codec_profile.is_none() {
        metadata.codec_profile = ffprobe_metadata.codec_profile;
    }
    if metadata.pix_fmt.is_none() {
        metadata.pix_fmt = ffprobe_metadata.pix_fmt;
    }
    if metadata.bits_per_raw_sample.is_none() {
        metadata.bits_per_raw_sample = ffprobe_metadata.bits_per_raw_sample;
    }
    if metadata.resolution.is_none() {
        metadata.resolution = ffprobe_metadata.resolution.clone();
    }
    if metadata
        .rotation_degrees
        .map(|degrees| degrees.rem_euclid(360) == 0)
        .unwrap_or(true)
        && ffprobe_metadata.rotation_degrees.is_some()
    {
        metadata.rotation_degrees = ffprobe_metadata.rotation_degrees;
    }
    metadata.has_audio = metadata.has_audio || ffprobe_metadata.has_audio;
    if metadata.container_format.is_none() {
        metadata.container_format = ffprobe_metadata.container_format;
    }
    if metadata.bit_rate.is_none() {
        metadata.bit_rate = ffprobe_metadata.bit_rate;
    }
    metadata
}
