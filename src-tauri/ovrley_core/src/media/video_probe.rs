//! Source video metadata extraction via ffprobe.
//!
//! Owns: the `probe_video` function that extracts duration, FPS, resolution,
//!       codec, audio, container, rotation, and timestamp fallback metadata
//!       from an imported source video using ffprobe.
//! Does not own: ffmpeg binary discovery (see [`crate::encode::ffmpeg`]), codec
//!       availability detection (see [`crate::encode::ffmpeg::detect`]).
//!
//! Allowed dependencies: `crate::encode::ffmpeg`, `crate::error`.
//! Forbidden dependencies: `crate::commands`, `crate::render`.
//!
//! Related modules: [`crate::encode::ffmpeg`] (binary resolution).
//!
//! ## Thread Safety
//! Single-threaded. Spawns an ffprobe subprocess and waits synchronously for
//! its JSON output. No shared mutable state.
//!
//! ## Performance
//! Not a hot path — called once per imported video. Subprocess overhead
//! dominates; ffprobe typically completes in < 1 second for 1080p files.

use crate::encode::ffmpeg::binary::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::error::{CoreError, CoreResult};
use crate::media::{Resolution, SourceVideoMetadata};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Probes a video file with ffprobe and returns structured metadata.
///
/// Spawns `ffprobe -v quiet -print_format json -show_format -show_streams` for
/// the given file. Parses the JSON output into a [`SourceVideoMetadata`] struct.
/// `creation_time` and the ffprobe fallback `sync_time` come from
/// [`ffprobe_creation_time`], with the file-system modification time
/// (converted to RFC 3339) as final fallback.
///
/// A metadata creation time (1–3) is then refined by a time-of-day `timecode`
/// tag when one fits, see [`timecode_sync_time`].
///
/// # Phases
/// 1. Resolve ffprobe binary and spawn the subprocess
/// 2. Parse JSON output into structured metadata fields
/// 3. Extract video stream properties (codec, resolution, FPS, rotation)
/// 4. Build timestamp fallback fields via the creation-time priority chain
///
/// # Errors
/// Returns [`CoreError::FfmpegNotFound`] if ffmpeg/ffprobe cannot be located.
/// Returns [`CoreError::Encode`] if ffprobe exits non-zero or produces invalid JSON.
///
/// # Performance
/// Called once per imported video (not on any render hot path). Typical wall
/// time < 1 second for 1080p files.
#[must_use = "probe result contains video metadata required for rendering"]
pub fn probe_video(repo_root: &Path, file_path: &str) -> CoreResult<SourceVideoMetadata> {
    // ── PHASE 1: RESOLVE FFPROBE BINARY & SPAWN SUBPROCESS ──
    let ffmpeg_path = resolve_ffmpeg_binary(repo_root)?;
    let ffprobe_name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let ffprobe_path = ffmpeg_path.with_file_name(ffprobe_name);

    let mut command = Command::new(&ffprobe_path);
    command.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        file_path,
    ]);
    configure_ffmpeg_command(&mut command);

    let output = command
        .output()
        .map_err(|e| CoreError::Encode(format!("Failed to run ffprobe: {e}")))?;

    if !output.status.success() {
        return Err(CoreError::Ffmpeg {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // ── PHASE 2: PARSE JSON INTO STRUCTURED METADATA ──
    let json: Value = serde_json::from_slice(&output.stdout)?;
    Ok(metadata_from_ffprobe_json(file_path, &json))
}

/// Builds [`SourceVideoMetadata`] from ffprobe `-show_format -show_streams`
/// JSON output (phases 2–4 of [`probe_video`]).
pub fn metadata_from_ffprobe_json(file_path: &str, json: &Value) -> SourceVideoMetadata {
    // test seam
    let mut metadata = SourceVideoMetadata {
        path: file_path.to_string(),
        duration: None,
        fps: None,
        fps_num: None,
        fps_den: None,
        resolution: None,
        creation_time: None,
        sync_time: None,
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        color_range: None,
        color_space: None,
        bits_per_raw_sample: None,
        bit_rate: None,
        audio_bit_rate: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: None,
        camera_type: None,
        camera_model: None,
        time_source: None,
    };

    let format = json.get("format");
    let streams = json.get("streams").and_then(|v| v.as_array());

    if let Some(format_obj) = format {
        let format_duration = format_obj
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(parse_positive_f64);
        metadata.duration = format_duration;
        metadata.bit_rate = format_obj
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
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

        let audio_stream = streams
            .iter()
            .find(|stream| stream.get("codec_type").and_then(|v| v.as_str()) == Some("audio"));
        metadata.has_audio = audio_stream.is_some();
        metadata.audio_bit_rate = audio_stream
            .and_then(|stream| stream.get("bit_rate"))
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0);
    }

    // ── PHASE 3: EXTRACT VIDEO STREAM PROPERTIES ──
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
        metadata.color_range = stream
            .get("color_range")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());
        metadata.color_space = stream
            .get("color_space")
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

    // ── PHASE 4: BUILD CREATION TIME VIA PRIORITY CHAIN ──
    log::debug!("Probing video: {}", file_path);

    metadata.creation_time = ffprobe_creation_time(&json).map(str::to_string);
    if metadata.creation_time.is_some() {
        metadata.time_source = Some("ffprobe".to_string());
    }

    if metadata.creation_time.is_none() {
        log::warn!("No creation time found in video metadata. Using file system modified time as fallback.");
        if let Ok(fs_meta) = std::fs::metadata(file_path) {
            if let Ok(modified) = fs_meta.modified() {
                let dt: chrono::DateTime<chrono::Utc> = modified.into();
                let rfc3339 = dt.to_rfc3339();
                log::debug!("Fallback file modified time: {}", rfc3339);
                metadata.creation_time = Some(rfc3339);
                metadata.time_source = Some("file_mtime".to_string());
            }
        }
    }

    // 4. A time-of-day timecode refines the metadata creation time to the
    //    first frame. A non-zero resolved UTC offset proves creation_time was
    //    real UTC, so the result is absolute; a zero offset stays ambiguous.
    if metadata.time_source.as_deref() == Some("ffprobe") {
        let timecode = read_timecode(json, video_stream);
        let fps = metadata.fps_num.zip(metadata.fps_den);
        let resolved = timecode
            .zip(metadata.creation_time.as_deref())
            .and_then(|(timecode, creation_time)| timecode_sync_time(timecode, fps, creation_time));
        if let Some(resolved) = resolved {
            log::debug!(
                "Timecode {:?} resolved to {} (UTC offset {} min)",
                timecode,
                resolved.sync_time,
                resolved.utc_offset_minutes
            );
            if resolved.utc_offset_minutes != 0 {
                metadata.time_source = Some("timecode".to_string());
            }
            metadata.creation_time = Some(resolved.sync_time);
        } else if let Some(timecode) = timecode {
            log::debug!("Ignoring timecode {} that is not a time of day near creation_time", timecode);
        }
    }

    // ffprobe timestamps are a salvage sync fallback, not authoritative
    // provenance. Keep both fields until the frontend fully moves to syncTime.
    metadata.sync_time = metadata.creation_time.clone();

    log::debug!("Final selected creation time: {:?}", metadata.creation_time);

    metadata
}

/// Most a camera's `creation_time` may differ from its time-of-day timecode
/// (after removing the UTC offset) for the timecode to be trusted. GoPro
/// HERO12 stamps `creation_time` ~50 s after the first frame.
const MAX_CREATION_TIME_DRIFT_SECONDS: f64 = 300.0;

/// A timecode-derived first-frame time.
#[derive(Debug, Clone, PartialEq)]
pub struct TimecodeSync {
    /// RFC 3339 UTC timestamp (millisecond precision) of the first frame.
    pub sync_time: String,
    /// Camera UTC offset implied by timecode vs. `creation_time`.
    pub utc_offset_minutes: i32,
}

/// Interprets a `HH:MM:SS:FF` timecode as the camera's local time of day at
/// the first frame and anchors it to `creation_time` (UTC).
///
/// Accepted only when the timecode sits a real UTC offset (whole, half or
/// three-quarter hour, −12:00..+14:00) away from `creation_time`, within
/// [`MAX_CREATION_TIME_DRIFT_SECONDS`]. The result therefore never moves more
/// than that from `creation_time`. Record-run timecodes starting at
/// `HH:00:00` (e.g. `01:00:00:00`) are rejected as counters, not clocks.
/// Returns `None` for malformed input or when no offset fits.
pub fn timecode_sync_time(timecode: &str, fps: Option<(u32, u32)>, creation_time: &str) -> Option<TimecodeSync> {
    // test seam
    let parts: Vec<u32> = timecode
        .split([':', ';', '.'])
        .map(|part| part.parse::<u32>().ok())
        .collect::<Option<_>>()?;
    let [hours, minutes, seconds, frames] = parts[..] else {
        return None;
    };
    if hours >= 24 || minutes >= 60 || seconds >= 60 || (minutes == 0 && seconds == 0) {
        return None;
    }
    let frame_seconds = match fps {
        Some((num, den)) => {
            let nominal_fps = (num as f64 / den as f64).ceil();
            if frames as f64 >= nominal_fps {
                return None;
            }
            frames as f64 * den as f64 / num as f64
        }
        None => 0.0,
    };
    let timecode_of_day = (hours * 3600 + minutes * 60 + seconds) as f64 + frame_seconds;

    let creation = chrono::DateTime::parse_from_rfc3339(creation_time).ok()?.with_timezone(&chrono::Utc);
    let midnight = creation.date_naive().and_hms_opt(0, 0, 0)?.and_utc();
    let creation_of_day = (creation - midnight).num_milliseconds() as f64 / 1000.0;
    let difference = timecode_of_day - creation_of_day;

    let wrap_day = |seconds: f64| (seconds + 43_200.0).rem_euclid(86_400.0) - 43_200.0;
    let (utc_offset_minutes, drift) = (-12 * 4..=14 * 4)
        .map(|quarter| quarter * 15)
        .filter(|offset_minutes: &i32| offset_minutes.abs() % 60 != 15)
        .map(|offset_minutes| (offset_minutes, wrap_day(difference - offset_minutes as f64 * 60.0)))
        .min_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))?;
    if drift.abs() > MAX_CREATION_TIME_DRIFT_SECONDS {
        return None;
    }

    let sync = creation + chrono::Duration::milliseconds((drift * 1000.0).round() as i64);
    Some(TimecodeSync {
        sync_time: sync.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        utc_offset_minutes,
    })
}

/// Reads the `timecode` tag from the video stream, then any other stream
/// (e.g. a `tmcd` data track), then the container.
fn read_timecode<'a>(json: &'a Value, video_stream: Option<&'a Value>) -> Option<&'a str> {
    let tag = |value: &'a Value| value.get("tags")?.get("timecode")?.as_str();
    video_stream
        .and_then(tag)
        .or_else(|| json.get("streams")?.as_array()?.iter().find_map(tag))
        .or_else(|| json.get("format").and_then(tag))
}

/// Selects a video's creation timestamp from ffprobe JSON
/// (`-show_format -show_streams`), in priority order:
/// 1. Container-level `creation_time` from the format tags
/// 2. Stream-level `creation_time` from the first video stream
/// 3. QuickTime-specific `com.apple.quicktime.creationdate` format tag
pub(crate) fn ffprobe_creation_time(json: &Value) -> Option<&str> {
    fn string_tag<'a>(tags: Option<&'a Value>, key: &str) -> Option<&'a str> {
        tags?.get(key)?.as_str()
    }

    let format_tags = json.get("format").and_then(|format| format.get("tags"));
    let video_stream_tags = json
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        })
        .and_then(|stream| stream.get("tags"));

    string_tag(format_tags, "creation_time")
        .or_else(|| string_tag(video_stream_tags, "creation_time"))
        .or_else(|| string_tag(format_tags, "com.apple.quicktime.creationdate"))
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
