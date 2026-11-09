//! DJI AC004 metadata fallback parser.
//!
//! Owns: extracting the `DJI meta`/`djmd` stream with FFmpeg and decoding the
//! small protobuf subset used by DJI Action cameras with the AC004 GPS remote.
//! Does not own: generic MP4 telemetry parsing, frontend activity finalization,
//! or any GPX/export format.
//!
//! # Sub-modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`protobuf`] | Wire-format decoding, field iteration, typed accessors |
//! | [`parser`] | GPS fix parsing, device-info extraction |

mod parser;
mod protobuf;

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::error::{CoreError, CoreResult};

pub use parser::{parse_raw_metadata, parse_raw_metadata_with_reference};

pub(crate) const DEFAULT_SAMPLE_RATE_HZ: f64 = 25.0;

/// Parsed telemetry from one DJI AC004 metadata stream.
#[derive(Debug, Clone, PartialEq)]
pub struct DjiAc004Telemetry {
    /// Device name reported by the remote, usually `DJI AC004`.
    pub device_name: Option<String>,
    /// Sample cadence reported by the device metadata.
    pub sample_rate_hz: Option<f64>,
    /// First usable point timestamp, formatted for the importer.
    pub sync_time: Option<String>,
    /// GPS points decoded from valid fix samples.
    pub samples: Vec<DjiAc004Sample>,
}

/// One valid GPS point decoded from a DJI AC004 metadata sample.
#[derive(Debug, Clone, PartialEq)]
pub struct DjiAc004Sample {
    /// Zero-based metadata sample index from the raw stream.
    pub frame_index: usize,
    /// Video-relative time derived from sample index and device sample rate.
    pub timestamp_ms: f64,
    /// Camera-local timestamp promoted to UTC-formatted text for compatibility
    /// with the existing importer contract.
    pub timestamp: String,
    /// Latitude in decimal degrees.
    pub latitude: f64,
    /// Longitude in decimal degrees.
    pub longitude: f64,
    /// Altitude in meters, converted from DJI's millimeter integer.
    pub altitude: f64,
    /// 2D velocity magnitude in meters per second.
    pub speed: f64,
    /// Velocity-derived heading in degrees, where 0 is north and 90 is east.
    pub heading: Option<f64>,
    /// Dynamic acceleration magnitude relative to 1g.
    pub g_force: Option<f64>,
}

/// Extracts and parses AC004 telemetry from a video file.
///
/// The function returns `Ok(None)` when the file has no DJI metadata stream or
/// the stream does not contain valid AC004 GPS fixes. FFmpeg failures after a
/// DJI stream is detected are returned as errors because the file appears to be
/// supported but the external extraction step failed.
pub fn extract_from_video(
    repo_root: &Path,
    file_path: &Path,
) -> CoreResult<Option<DjiAc004Telemetry>> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let Some(stream_info) = detect_dji_meta_stream(&ffmpeg_path, file_path)? else {
        return Ok(None);
    };

    let raw_data = extract_dji_meta_raw(&ffmpeg_path, file_path, stream_info.index)?;
    Ok(parser::parse_raw_metadata_with_reference(
        &raw_data,
        stream_info.creation_time,
    ))
}

struct DjiMetaStreamInfo {
    index: usize,
    creation_time: Option<DateTime<Utc>>,
}

/// Locates the metadata track that FFmpeg should extract for AC004 parsing.
fn detect_dji_meta_stream(
    ffmpeg_path: &Path,
    file_path: &Path,
) -> CoreResult<Option<DjiMetaStreamInfo>> {
    let ffprobe_path = ffmpeg_path.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });

    let mut command = Command::new(&ffprobe_path);
    command
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(file_path);
    configure_ffmpeg_command(&mut command);

    let output = command
        .output()
        .map_err(|error| CoreError::Encode(format!("Failed to run ffprobe: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Ffmpeg {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let json: Value = serde_json::from_slice(&output.stdout)?;
    let Some(streams) = json.get("streams").and_then(Value::as_array) else {
        return Ok(None);
    };

    let format_creation_time = json
        .get("format")
        .and_then(|format| format.get("tags"))
        .and_then(|tags| tags.get("creation_time"))
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_utc);

    for stream in streams {
        let codec_tag = stream.get("codec_tag_string").and_then(Value::as_str);
        let tags = stream.get("tags");
        let handler_name = tags
            .and_then(|tags| tags.get("handler_name"))
            .and_then(Value::as_str);

        if codec_tag == Some("djmd") || handler_name == Some("DJI meta") {
            if let Some(index) = stream.get("index").and_then(Value::as_u64) {
                let stream_creation_time = tags
                    .and_then(|tags| tags.get("creation_time"))
                    .and_then(Value::as_str)
                    .and_then(parse_rfc3339_utc);
                return Ok(Some(DjiMetaStreamInfo {
                    index: index as usize,
                    creation_time: format_creation_time.or(stream_creation_time),
                }));
            }
        }
    }

    Ok(None)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Copies the DJI data track out of the container without transcoding.
fn extract_dji_meta_raw(
    ffmpeg_path: &Path,
    file_path: &Path,
    stream_index: usize,
) -> CoreResult<Vec<u8>> {
    let map_arg = format!("0:{stream_index}");
    let mut command = Command::new(ffmpeg_path);
    command
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(file_path)
        .arg("-map")
        .arg(map_arg)
        .arg("-c")
        .arg("copy")
        .arg("-f")
        .arg("data")
        .arg("pipe:1");
    configure_ffmpeg_command(&mut command);

    let output = command
        .output()
        .map_err(|error| CoreError::Encode(format!("Failed to run ffmpeg: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Ffmpeg {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(output.stdout)
}
