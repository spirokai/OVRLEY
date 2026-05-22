//! Video metadata extraction via ffprobe.
//!
//! Owns: video probe types (`VideoMetadata`, `Resolution`) and the `probe_video`
//!       function that extracts duration, FPS, resolution, codec, and creation-time
//!       metadata from an MP4/MOV file using ffprobe.
//! Does not own: ffmpeg binary discovery (see [`crate::encode::ffmpeg`]), codec
//!       availability detection (see [`crate::encode::codec_detect`]).
//!
//! Allowed dependencies: `crate::encode::ffmpeg`, `crate::error`.
//! Forbidden dependencies: `crate::commands`, `crate::render`.
//!
//! Related modules: [`crate::encode::ffmpeg`] (binary resolution),
//!       [`crate::encode::codec_detect`] (encoder capability probing).
//!
//! ## Thread Safety
//! Single-threaded. Spawns an ffprobe subprocess and waits synchronously for
//! its JSON output. No shared mutable state.
//!
//! ## Performance
//! Not a hot path — called once per imported video. Subprocess overhead
//! dominates; ffprobe typically completes in < 1 second for 1080p files.

use crate::encode::ffmpeg::{resolve_ffmpeg_binary, suppress_child_console};
use crate::error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Extracted video metadata returned to the frontend after import.
///
/// All fields except `path` and `has_audio` may be `None` if ffprobe did not
/// report them. `creation_time` uses a priority chain: stream-level
/// `creation_time` tag first, then container-level `creation_time`, then
/// the file-system modification time as a final fallback.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
}

/// Width and height in pixels as reported by ffprobe.
#[derive(Debug, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}

/// Probes a video file with ffprobe and returns structured metadata.
///
/// Spawns `ffprobe -v quiet -print_format json -show_format -show_streams` for
/// the given file. Parses the JSON output into a [`VideoMetadata`] struct.
/// Creation time is resolved with this priority:
/// 1. Stream-level `creation_time` tag from the first video stream
/// 2. Container-level `creation_time` from the format tags
/// 3. File-system modification time (converted to RFC 3339)
///
/// # Errors
/// Returns [`CoreError::FfmpegNotFound`] if ffmpeg/ffprobe cannot be located.
/// Returns [`CoreError::Encode`] if ffprobe exits non-zero or produces invalid JSON.
///
/// # Performance
/// Called once per imported video (not on any render hot path). Typical wall
/// time < 1 second for 1080p files.
#[must_use = "probe result contains video metadata required for rendering"]
pub fn probe_video(repo_root: &Path, file_path: &str) -> CoreResult<VideoMetadata> {
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let ffprobe_name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let ffprobe_path = ffmpeg_path.with_file_name(ffprobe_name);

    let mut command = Command::new(ffprobe_path);
    command.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        file_path,
    ]);
    suppress_child_console(&mut command);

    let output = command
        .output()
        .map_err(|e| CoreError::Encode(format!("Failed to run ffprobe: {e}")))?;

    if !output.status.success() {
        return Err(CoreError::Ffmpeg {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let json: Value = serde_json::from_slice(&output.stdout)?;

    let mut metadata = VideoMetadata {
        path: file_path.to_string(),
        duration: None,
        fps: None,
        fps_num: None,
        fps_den: None,
        resolution: None,
        creation_time: None,
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        bits_per_raw_sample: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: None,
    };

    let format = json.get("format");
    let streams = json.get("streams").and_then(|v| v.as_array());

    if let Some(format_obj) = format {
        let format_duration = format_obj
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(parse_positive_f64);
        metadata.duration = format_duration;
        metadata.container_format = format_obj
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
    }

    let mut video_stream = None;
    if let Some(streams) = streams {
        for stream in streams {
            if stream.get("codec_type").and_then(|v| v.as_str()) == Some("video") {
                video_stream = Some(stream);
                break;
            }
        }

        metadata.has_audio = streams
            .iter()
            .any(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("audio"));
    }

    if let Some(stream) = video_stream {
        metadata.codec_name = stream
            .get("codec_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.codec_long_name = stream
            .get("codec_long_name")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.codec_profile = stream
            .get("profile")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.pix_fmt = stream
            .get("pix_fmt")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.bits_per_raw_sample = stream
            .get("bits_per_raw_sample")
            .and_then(|v| v.as_str())
            .and_then(|value| value.parse::<u32>().ok());
        metadata.rotation_degrees = read_rotation_degrees(stream);

        if let (Some(w), Some(h)) = (
            stream.get("width").and_then(|v| v.as_u64()),
            stream.get("height").and_then(|v| v.as_u64()),
        ) {
            metadata.resolution = Some(Resolution {
                width: w,
                height: h,
            });
        }

        if let Some((num, den)) = read_rational_rate(stream, "avg_frame_rate")
            .or_else(|| read_rational_rate(stream, "r_frame_rate"))
        {
            metadata.fps = Some(num as f64 / den as f64);
            metadata.fps_num = Some(num);
            metadata.fps_den = Some(den);
        }

        metadata.duration = read_video_stream_duration(stream, metadata.fps).or(metadata.duration);
    }

    log::debug!("Probing video: {}", file_path);

    // Priority order:
    // 1. format.tags.creation_time
    let format_creation_time = format
        .and_then(|f| f.get("tags"))
        .and_then(|t| t.get("creation_time"))
        .and_then(|v| v.as_str());
    if let Some(t) = format_creation_time {
        log::debug!("Found format.tags.creation_time: {}", t);
    }

    // 2. streams[0].tags.creation_time
    let stream_creation_time = video_stream
        .and_then(|s| s.get("tags"))
        .and_then(|t| t.get("creation_time"))
        .and_then(|v| v.as_str());
    if let Some(t) = stream_creation_time {
        log::debug!("Found streams[0].tags.creation_time: {}", t);
    }

    // 3. format.tags.com.apple.quicktime.creationdate
    let apple_creation_date = format
        .and_then(|f| f.get("tags"))
        .and_then(|t| t.get("com.apple.quicktime.creationdate"))
        .and_then(|v| v.as_str());
    if let Some(t) = apple_creation_date {
        log::debug!("Found format.tags.com.apple.quicktime.creationdate: {}", t);
    }

    metadata.creation_time = format_creation_time
        .or(stream_creation_time)
        .or(apple_creation_date)
        .map(|s| s.to_string());

    if metadata.creation_time.is_none() {
        log::warn!("No creation time found in video metadata. Using file system modified time as fallback.");
        if let Ok(fs_meta) = std::fs::metadata(file_path) {
            if let Ok(modified) = fs_meta.modified() {
                let dt: chrono::DateTime<chrono::Utc> = modified.into();
                let rfc3339 = dt.to_rfc3339();
                log::debug!("Fallback file modified time: {}", rfc3339);
                metadata.creation_time = Some(rfc3339);
            }
        }
    }

    log::debug!("Final selected creation time: {:?}", metadata.creation_time);

    Ok(metadata)
}

fn read_rational_rate(stream: &Value, key: &str) -> Option<(u32, u32)> {
    let value = stream.get(key).and_then(|v| v.as_str())?;
    let (num, den) = value.split_once('/')?;
    let num = num.parse::<u32>().ok()?;
    let den = den.parse::<u32>().ok()?;
    (num > 0 && den > 0).then_some((num, den))
}

/// Reads video stream duration from an ffprobe JSON stream object.
///
/// Prefers the explicit `duration` field. Falls back to computing duration from
/// `nb_frames / fps` when the duration field is absent or unparseable. The FPS
/// value is expected to have been validated (finite, positive) before this call.
/// Returns `None` when neither source is available.
pub fn read_video_stream_duration(stream: &Value, fps: Option<f64>) -> Option<f64> {
    // test seam
    stream
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(parse_positive_f64)
        .or_else(|| {
            let frames = stream
                .get("nb_frames")
                .and_then(|v| v.as_str())
                .and_then(|value| value.parse::<u64>().ok())?;
            let fps = fps.filter(|value| value.is_finite() && *value > 0.0)?;
            Some(frames as f64 / fps)
        })
}

fn parse_positive_f64(value: &str) -> Option<f64> {
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

fn read_rotation_degrees(stream: &Value) -> Option<i32> {
    stream
        .get("tags")
        .and_then(|tags| tags.get("rotate"))
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<i32>().ok())
        .or_else(|| {
            stream
                .get("side_data_list")
                .and_then(|value| value.as_array())
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        item.get("rotation")
                            .and_then(|value| value.as_f64())
                            .map(|value| value.round() as i32)
                    })
                })
        })
}
