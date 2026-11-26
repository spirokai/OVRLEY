//! Source video metadata: the shared metadata contract, ffprobe extraction, normalization of
//! probed metadata and source file freshness.

use serde::{Deserialize, Serialize};
use crate::encode::ffmpeg::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::error::{CoreError, CoreResult};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::time::UNIX_EPOCH;
use crate::encode::timing::Fps;
use crate::paths::AppPaths;

/// Metadata of an imported source video, from ffprobe or telemetry-parser.
///
/// All fields except `path` and `has_audio` may be `None` when the probe could not report
/// them. `sync_time` is the canonical hint for aligning the video with activity telemetry;
/// `creation_time` keeps the container's own timestamp as provenance.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVideoMetadata {
    pub path: String,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub fps_num: Option<u32>,
    pub fps_den: Option<u32>,
    pub resolution: Option<Resolution>,
    pub creation_time: Option<String>,
    pub sync_time: Option<String>,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_profile: Option<String>,
    pub pix_fmt: Option<String>,
    /// ffprobe `color_range` of the video stream (`tv`, `pc`); None when untagged.
    pub color_range: Option<String>,
    /// ffprobe `color_space` (matrix) of the video stream, e.g. `bt709`; None when untagged.
    pub color_space: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub bit_rate: Option<String>,
    /// First audio stream's bitrate in bits per second, when reported by ffprobe.
    pub audio_bit_rate: Option<u64>,
    pub has_audio: bool,
    pub container_format: Option<String>,
    pub rotation_degrees: Option<i32>,
    pub camera_type: Option<String>,
    pub camera_model: Option<String>,
    pub time_source: Option<String>,
    /// Seconds the first video frame sits after the file's start, when reported
    /// by ffprobe. FFmpeg's trims count from the file's start.
    pub video_start_offset: Option<f64>,
}

impl SourceVideoMetadata {
    /// Converts a second on the video's own clock (its first frame at zero) to the
    /// file's clock, which FFmpeg's seeks and trims use. An unreported video start
    /// is the file's start.
    pub fn file_seconds(&self, video_seconds: f64) -> f64 {
        video_seconds + self.video_start_offset.unwrap_or(0.0)
    }
}

/// Width and height in pixels as reported by source video probes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resolution {
    pub width: u64,
    pub height: u64,
}

/// Below this many frames, a gap between `nb_frames` and `duration` is rounding in
/// the reported values, not frames hidden by an edit list.
const HIDDEN_FRAME_TOLERANCE: f64 = 1e-4;

/// Probes a video file with ffprobe and returns structured metadata.
///
/// Spawns `ffprobe -v quiet -print_format json -show_format -show_streams` for
/// the given file. Parses the JSON output into a [`SourceVideoMetadata`] struct.
/// `creation_time` and the ffprobe fallback `sync_time` come from
/// [`ffprobe_creation_time`]. Without a creation tag the time stays unknown:
/// the file-system modification time is usually when the file was copied, not
/// recorded.
///
/// A metadata creation time (1–3) is then refined by a time-of-day `timecode`
/// tag when one fits, see [`timecode_sync_time`].
///
/// # Steps
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
    // Resolve the ffprobe binary and spawn it.
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

    // Parse the JSON into structured metadata.
    let json: Value = serde_json::from_slice(&output.stdout)?;
    Ok(metadata_from_ffprobe_json(file_path, &json))
}

/// Builds [`SourceVideoMetadata`] from ffprobe `-show_format -show_streams`
/// JSON output.
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
        video_start_offset: None,
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

    // Extract video stream properties.
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
        metadata.video_start_offset =
            format.and_then(|format| read_video_start_offset(stream, format));
    }

    // Build creation time via priority chain.
    log::debug!("Probing video: {}", file_path);

    metadata.creation_time = ffprobe_creation_time(json).map(str::to_string);
    if metadata.creation_time.is_some() {
        metadata.time_source = Some("ffprobe".to_string());
    } else {
        log::warn!("No creation time found in video metadata.");
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

    // Without telemetry, the container timestamp is the best available sync hint.
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
fn ffprobe_creation_time(json: &Value) -> Option<&str> {
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
///
/// `nb_frames` counts the whole track while `duration` is what its edit list
/// shows. When frames are hidden the duration usually ends mid-frame and only
/// whole frames can be relied on, so it is rounded down to a whole frame.
pub fn read_video_stream_duration(stream: &Value, fps: Option<f64>) -> Option<f64> {
    // test seam
    let frames = stream
        .get("nb_frames")
        .and_then(|v| v.as_str())
        .and_then(|value| value.parse::<u64>().ok());
    let fps = fps.filter(|value| value.is_finite() && *value > 0.0);
    let duration = stream
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(parse_positive_f64);
    match (duration, frames, fps) {
        (Some(duration), Some(frames), Some(fps))
            if duration * fps < frames as f64 - HIDDEN_FRAME_TOLERANCE =>
        {
            Some(((duration * fps).floor() / fps).max(1.0 / fps))
        }
        (Some(duration), _, _) => Some(duration),
        (None, Some(frames), Some(fps)) => Some(frames as f64 / fps),
        (None, _, _) => None,
    }
}

/// How far the first video frame sits after the file's start. FFmpeg counts from
/// the earliest stream, so video that starts after its audio has a positive offset.
pub fn read_video_start_offset(stream: &Value, format: &Value) -> Option<f64> {
    let seconds = |value: &Value| {
        value
            .get("start_time")
            .and_then(Value::as_str)
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|parsed| parsed.is_finite())
    };
    Some((seconds(stream)? - seconds(format)?).max(0.0))
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
    /// Decimal Unix nanoseconds as text, so JSON readers keep their full precision.
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

/// Telemetry-first probing completed by ffprobe. This is the common owner for
/// interactive import and non-interactive inspection. ffprobe always runs: it owns
/// the timing the renderer's FFmpeg will see (see `merge_ffprobe_metadata`).
pub fn probe_video_metadata(
    paths: &AppPaths,
    file_path: &str,
) -> CoreResult<crate::media::probe::SourceVideoMetadata> {
    match crate::media::telemetry::probe_video_metadata(&paths.repo_root, file_path) {
        Ok(metadata) => match crate::media::probe::probe_video(&paths.repo_root, file_path) {
            Ok(ffprobe_metadata) => Ok(merge_ffprobe_metadata(metadata, ffprobe_metadata)),
            Err(error) => {
                log::warn!("ffprobe failed for {file_path}: {error}");
                Ok(metadata)
            }
        },
        Err(error) => {
            log::warn!(
                "telemetry-parser probe failed for {file_path}: {error}; falling back to ffprobe"
            );
            crate::media::probe::probe_video(&paths.repo_root, file_path)
        }
    }
}

fn planned_frames(metadata: &crate::media::probe::SourceVideoMetadata) -> Option<u64> {
    Fps::new(metadata.fps_num?, metadata.fps_den?)
        .ok()?
        .frame_count_for_duration(metadata.duration?)
        .ok()
}

/// Whether the clip's GPS time came from a DJI Action remote (`DJI AC00x`),
/// whose GPS clock text is model dependent, so the container's metadata time
/// outranks it.
fn defers_to_container_time(metadata: &crate::media::probe::SourceVideoMetadata) -> bool {
    metadata.camera_type.as_deref() == Some("DJI")
        && metadata
            .camera_model
            .as_deref()
            .is_some_and(|model| model.starts_with("DJI AC00"))
}

/// ffprobe owns the timing (duration, where the video starts, and whether the
/// nominal frame rate holds); every other field it only fills where
/// telemetry-parser left a gap.
fn merge_ffprobe_metadata(
    mut metadata: crate::media::probe::SourceVideoMetadata,
    ffprobe_metadata: crate::media::probe::SourceVideoMetadata,
) -> crate::media::probe::SourceVideoMetadata {
    // The renderer's FFmpeg honours edit lists and ffprobe reports what it will
    // read; telemetry-parser's duration is the whole track, edit list ignored.
    metadata.duration = ffprobe_metadata.duration.or(metadata.duration);
    // A nominal rate keeps clips of one camera joinable, but it must not plan more
    // frames than the file holds: the render cannot produce them.
    let keeps_nominal_rate = match (planned_frames(&metadata), planned_frames(&ffprobe_metadata)) {
        (Some(nominal), Some(measured)) => nominal <= measured,
        (nominal, _) => nominal.is_some(),
    };
    if !keeps_nominal_rate {
        metadata.fps = ffprobe_metadata.fps;
        metadata.fps_num = ffprobe_metadata.fps_num;
        metadata.fps_den = ffprobe_metadata.fps_den;
    }
    metadata.video_start_offset = ffprobe_metadata.video_start_offset;
    // DJI Action cameras write a true UTC container time, which outranks
    // their model-dependent GPS clock. It is reported as absolute
    // (`timecode`), never as zone-ambiguous `ffprobe` metadata.
    let container_time = matches!(
        ffprobe_metadata.time_source.as_deref(),
        Some("ffprobe" | "timecode")
    );
    if container_time && defers_to_container_time(&metadata) {
        metadata.sync_time = ffprobe_metadata.sync_time.clone();
        metadata.creation_time = ffprobe_metadata.creation_time.clone();
        metadata.time_source = Some("timecode".to_string());
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
    if metadata.color_range.is_none() {
        metadata.color_range = ffprobe_metadata.color_range;
    }
    if metadata.color_space.is_none() {
        metadata.color_space = ffprobe_metadata.color_space;
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
    if metadata.audio_bit_rate.is_none() {
        metadata.audio_bit_rate = ffprobe_metadata.audio_bit_rate;
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(duration: f64, fps_num: u32, fps_den: u32) -> SourceVideoMetadata {
        serde_json::from_value(serde_json::json!({
            "path": "clip.mp4",
            "duration": duration,
            "fps": f64::from(fps_num) / f64::from(fps_den),
            "fpsNum": fps_num,
            "fpsDen": fps_den,
            "hasAudio": false,
        }))
        .unwrap()
    }

    #[test]
    fn ffprobe_gives_the_duration_and_where_the_video_starts() {
        let mut probed = metadata(5.5, 25, 1);
        probed.video_start_offset = Some(0.04);
        let merged = merge_ffprobe_metadata(metadata(6.0, 25, 1), probed);
        assert_eq!(merged.duration, Some(5.5));
        assert_eq!(merged.file_seconds(1.0), 1.04);
    }

    #[test]
    fn a_nominal_rate_that_plans_more_frames_than_the_file_holds_gives_way() {
        let merged = merge_ffprobe_metadata(
            metadata(4325.020733, 30, 1),
            metadata(4325.020733, 1942335000, 64875311),
        );
        assert_eq!(
            (merged.fps_num, merged.fps_den),
            (Some(1942335000), Some(64875311))
        );
    }

    #[test]
    fn a_nominal_rate_that_fits_the_file_keeps_clips_of_one_camera_joinable() {
        let merged = merge_ffprobe_metadata(
            metadata(117.513333, 60000, 1001),
            metadata(117.513333, 1056900, 17627),
        );
        assert_eq!((merged.fps_num, merged.fps_den), (Some(60000), Some(1001)));
    }

    fn dji_metadata(camera_model: &str, time: &str, time_source: &str) -> SourceVideoMetadata {
        serde_json::from_value(serde_json::json!({
            "path": "clip.mp4",
            "hasAudio": false,
            "creationTime": time,
            "syncTime": time,
            "cameraType": "DJI",
            "cameraModel": camera_model,
            "timeSource": time_source,
        }))
        .unwrap()
    }

    const GPS: &str = "2026-11-08T18:33:56+00:00";
    const CONTAINER: &str = "2026-11-07T18:33:50.000Z";

    #[test]
    fn ac004_container_time_outranks_gps() {
        let merged = merge_ffprobe_metadata(
            dji_metadata("DJI AC004", GPS, "gps"),
            dji_metadata("", CONTAINER, "timecode"),
        );
        assert_eq!(merged.sync_time.as_deref(), Some(CONTAINER));
        assert_eq!(merged.creation_time.as_deref(), Some(CONTAINER));
        assert_eq!(merged.time_source.as_deref(), Some("timecode"));
    }

    #[test]
    fn ac004_gps_outranks_file_mtime() {
        let merged = merge_ffprobe_metadata(
            dji_metadata("DJI AC004", GPS, "gps"),
            dji_metadata("", CONTAINER, "file_mtime"),
        );
        assert_eq!(merged.sync_time.as_deref(), Some(GPS));
        assert_eq!(merged.time_source.as_deref(), Some("gps"));
    }

    #[test]
    fn every_ac00x_model_prefers_container_time() {
        for model in ["DJI AC003", "DJI AC004", "DJI AC006"] {
            let merged = merge_ffprobe_metadata(
                dji_metadata(model, GPS, "gps"),
                dji_metadata("", CONTAINER, "ffprobe"),
            );
            assert_eq!(merged.sync_time.as_deref(), Some(CONTAINER), "{model}");
            assert_eq!(merged.time_source.as_deref(), Some("timecode"), "{model}");
        }
    }

    #[test]
    fn other_dji_cameras_keep_gps() {
        let merged = merge_ffprobe_metadata(
            dji_metadata("Osmo 360", GPS, "gps"),
            dji_metadata("", CONTAINER, "timecode"),
        );
        assert_eq!(merged.sync_time.as_deref(), Some(GPS));
        assert_eq!(merged.time_source.as_deref(), Some("gps"));
    }
}
