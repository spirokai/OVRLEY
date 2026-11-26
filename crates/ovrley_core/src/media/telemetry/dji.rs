//! DJI Action AC004 metadata: the `DJI meta`/`djmd` stream extracted with FFmpeg, and the
//! small protobuf subset holding GPS fixes and device information.
//!
//! The protobuf wire subset (varint, fixed64, fixed32 and length-delimited fields) is decoded
//! here rather than with a protobuf runtime; unknown wire types fail closed. Field numbers are
//! then read as the AC004 hierarchy (`f3.f4.f2.f1` for latitude/longitude, `f3.f1.f4` for the
//! device name). Samples without a valid, non-zero GPS fix are dropped.

use super::time::{dji_timestamp_to_rfc3339, parse_dji_timestamp, DjiGpsClock};
use crate::encode::ffmpeg::{configure_ffmpeg_command, resolve_ffmpeg_binary};
use crate::error::{CoreError, CoreResult};
use crate::import::finalize::timezone_for_coordinates;
use crate::import::g_force_from_components;
use chrono::{NaiveDateTime, TimeDelta};
use chrono_tz::Tz;
use serde_json::Value;
use std::path::Path;
use std::process::Command;





const DEFAULT_SAMPLE_RATE_HZ: f64 = 25.0;

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
    /// GPS clock time converted to UTC RFC 3339 text per the camera model's
    /// clock rule (see [`crate::media::telemetry::time::DjiGpsClock`]).
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
    let Some(stream_index) = detect_dji_meta_stream(&ffmpeg_path, file_path)? else {
        return Ok(None);
    };

    let raw_data = extract_dji_meta_raw(&ffmpeg_path, file_path, stream_index)?;
    Ok(parse_raw_metadata(&raw_data))
}

/// Locates the metadata track that FFmpeg should extract for AC004 parsing.
fn detect_dji_meta_stream(ffmpeg_path: &Path, file_path: &Path) -> CoreResult<Option<usize>> {
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

    for stream in streams {
        let codec_tag = stream.get("codec_tag_string").and_then(Value::as_str);
        let handler_name = stream
            .get("tags")
            .and_then(|tags| tags.get("handler_name"))
            .and_then(Value::as_str);

        if codec_tag == Some("djmd") || handler_name == Some("DJI meta") {
            if let Some(index) = stream.get("index").and_then(Value::as_u64) {
                return Ok(Some(index as usize));
            }
        }
    }

    Ok(None)
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

/// Parses raw concatenated DJI metadata samples into structured telemetry.
///
/// FFmpeg's `data` muxer emits the metadata payloads as one raw byte stream.
/// AC004 samples are protobuf messages wrapped in top-level field `3`; the GPS
/// payload lives under `f3.f4`. Unknown fields are skipped so new camera fields
/// do not break extraction of the stable GPS subset.
pub fn parse_raw_metadata(raw_data: &[u8]) -> Option<DjiAc004Telemetry> {
    let device_info = first_device_info(raw_data);
    let sample_rate_hz = device_info
        .as_ref()
        .and_then(|info| info.sample_rate_hz)
        .filter(|rate| rate.is_finite() && *rate > 0.0);
    let sample_rate = sample_rate_hz.unwrap_or(DEFAULT_SAMPLE_RATE_HZ);
    let gps_clock = DjiGpsClock::for_device(
        device_info
            .as_ref()
            .and_then(|info| info.device_name.as_deref()),
    );

    let clock_offset = clip_clock_offset(raw_data, gps_clock)?;
    let points: Vec<_> = top_level_samples(raw_data)
        .enumerate()
        .filter_map(|(frame_index, sample_data)| {
            parse_point_from_sample(
                sample_data,
                frame_index,
                media_time_ms(frame_index, sample_rate),
                clock_offset,
            )
        })
        .collect();

    if points.is_empty() {
        return None;
    }

    Some(DjiAc004Telemetry {
        device_name: device_info.and_then(|info| info.device_name),
        sample_rate_hz,
        sync_time: points.first().map(|point| point.timestamp.clone()),
        samples: points,
    })
}

/// Yields the top-level sample payloads (field `3`) in stream order; a
/// payload's position is its frame index.
fn top_level_samples(raw_data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut pos = 0;
    std::iter::from_fn(move || {
        while let Some((field_num, value, next_pos)) = decode_field(raw_data, pos)
        {
            pos = next_pos;
            if let (3, WireValue::LengthDelimited(sample_data)) =
                (field_num, value)
            {
                return Some(sample_data);
            }
        }
        None
    })
}

fn media_time_ms(frame_index: usize, sample_rate: f64) -> f64 {
    frame_index as f64 * 1000.0 / sample_rate
}

/// Decides the clip's GPS clock offset once, from its first valid fix and the
/// timezone at that fix's coordinates.
///
/// Returns `None` when the stream has no valid fix at all.
fn clip_clock_offset(raw_data: &[u8], gps_clock: DjiGpsClock) -> Option<TimeDelta> {
    let fix = top_level_samples(raw_data).find_map(read_gps_fix)?;
    let gps_tz = timezone_for_coordinates(fix.longitude, fix.latitude)
        .and_then(|name| name.parse::<Tz>().ok());
    Some(gps_clock.offset(fix.timestamp, gps_tz))
}

/// The validated GPS fix subset (`f4.f2`) of one sample.
struct GpsFix {
    latitude: f64,
    longitude: f64,
    altitude: f64,
    /// Zone-less GPS clock text; see [`DjiGpsClock`].
    timestamp: NaiveDateTime,
}

/// Reads a sample's GPS fix, rejecting samples without a fix, with zero
/// coordinates, or with unparseable clock text.
fn read_gps_fix(sample_data: &[u8]) -> Option<GpsFix> {
    let gps_msg = get_submessage(sample_data, 4)?;
    let fix_msg = get_submessage(gps_msg, 2)?;
    let coords_msg = get_submessage(fix_msg, 1)?;

    let fix_type = get_varint(coords_msg, 1)?;
    if fix_type == 0 {
        return None;
    }

    let latitude = get_f64(coords_msg, 2)?;
    let longitude = get_f64(coords_msg, 3)?;
    if latitude == 0.0 && longitude == 0.0 {
        return None;
    }

    let altitude = get_varint(fix_msg, 2).map_or(0.0, |value| value as f64 / 1000.0);
    let timestamp_msg = get_submessage(fix_msg, 6)?;
    let timestamp = parse_dji_timestamp(&get_string(timestamp_msg, 1)?)?;

    Some(GpsFix {
        latitude,
        longitude,
        altitude,
        timestamp,
    })
}

/// Reads the AC004 GPS subset from one top-level sample message.
fn parse_point_from_sample(
    sample_data: &[u8],
    frame_index: usize,
    timestamp_ms: f64,
    clock_offset: TimeDelta,
) -> Option<DjiAc004Sample> {
    let fix = read_gps_fix(sample_data)?;

    let (vx, vy) = get_submessage(sample_data, 4)
        .and_then(|gps_msg| get_submessage(gps_msg, 3))
        .map_or((0.0, 0.0), |velocity_msg| {
            (
                get_f32(velocity_msg, 1).unwrap_or(0.0) as f64,
                get_f32(velocity_msg, 2).unwrap_or(0.0) as f64,
            )
        });
    let heading = velocity_heading_degrees(vx, vy);
    let g_force = extract_acceleration_g_force(sample_data);

    Some(DjiAc004Sample {
        frame_index,
        timestamp_ms,
        timestamp: dji_timestamp_to_rfc3339(fix.timestamp, clock_offset),
        latitude: fix.latitude,
        longitude: fix.longitude,
        altitude: fix.altitude,
        speed: (vx * vx + vy * vy).sqrt(),
        heading,
        g_force,
    })
}

fn velocity_heading_degrees(vx: f64, vy: f64) -> Option<f64> {
    if !vx.is_finite() || !vy.is_finite() || (vx == 0.0 && vy == 0.0) {
        return None;
    }

    Some((vx.atan2(vy).to_degrees() + 360.0) % 360.0)
}

fn extract_acceleration_g_force(sample_data: &[u8]) -> Option<f64> {
    let sensor_msg = get_submessage(sample_data, 2)?;
    let acceleration_msg = get_submessage(sensor_msg, 10)?;
    let x = get_f32(acceleration_msg, 2)? as f64;
    let y = get_f32(acceleration_msg, 3)? as f64;
    let z = get_f32(acceleration_msg, 4)? as f64;
    g_force_from_components(x, y, z)
}

#[derive(Debug, Clone, PartialEq)]
struct DeviceInfo {
    device_name: Option<String>,
    sample_rate_hz: Option<f64>,
}

/// Reads stream-level AC004 device facts from the first available sample.
fn first_device_info(raw_data: &[u8]) -> Option<DeviceInfo> {
    let sample_data = top_level_samples(raw_data).next()?;
    let gps_msg = get_submessage(sample_data, 4)?;
    let device_msg = get_submessage(gps_msg, 1)?;
    Some(DeviceInfo {
        device_name: get_string(device_msg, 4),
        sample_rate_hz: get_f32(device_msg, 5).map(|value| value as f64),
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WireValue<'a> {
    Varint(u64),
    Fixed64(&'a [u8]),
    LengthDelimited(&'a [u8]),
    Fixed32(&'a [u8]),
}

/// Decodes one protobuf field from a bounded byte slice.
///
/// The fallback only implements wire types used by the observed AC004 metadata:
/// varint, fixed64, length-delimited, and fixed32. Unsupported wire types return
/// `None` so higher-level parsing stops instead of trying to resynchronize from
/// an ambiguous byte position.
pub fn decode_field(data: &[u8], pos: usize) -> Option<(u64, WireValue<'_>, usize)> {
    if pos >= data.len() {
        return None;
    }

    let (tag, mut pos) = decode_varint(data, pos)?;
    let field_num = tag >> 3;
    let wire_type = tag & 0x07;

    match wire_type {
        0 => {
            let (value, next_pos) = decode_varint(data, pos)?;
            Some((field_num, WireValue::Varint(value), next_pos))
        }
        1 => {
            let end = pos.checked_add(8)?;
            if end <= data.len() {
                Some((field_num, WireValue::Fixed64(&data[pos..end]), end))
            } else {
                None
            }
        }
        2 => {
            let (length, next_pos) = decode_varint(data, pos)?;
            pos = next_pos;
            let end = pos.checked_add(length as usize)?;
            if end <= data.len() {
                Some((field_num, WireValue::LengthDelimited(&data[pos..end]), end))
            } else {
                None
            }
        }
        5 => {
            let end = pos.checked_add(4)?;
            if end <= data.len() {
                Some((field_num, WireValue::Fixed32(&data[pos..end]), end))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Decodes a protobuf base-128 varint and returns the next unread byte offset.
///
/// The ten-byte limit mirrors protobuf's 64-bit integer bound. Treating longer
/// runs as malformed prevents an unterminated varint from scanning arbitrary
/// data while looking for a continuation bit that never arrives.
pub fn decode_varint(data: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;

    while pos < data.len() && shift < 70 {
        let byte = data[pos];
        result |= u64::from(byte & 0x7f) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            return Some((result, pos));
        }
        shift += 7;
    }

    None
}

/// Creates a forward-only protobuf field iterator over a message slice.
pub fn iter_fields(data: &[u8]) -> FieldIter<'_> {
    FieldIter { data, pos: 0 }
}

pub struct FieldIter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for FieldIter<'a> {
    type Item = (u64, WireValue<'a>);

    /// Advances by exactly one protobuf field and stops on malformed input.
    fn next(&mut self) -> Option<Self::Item> {
        let (field_num, value, next_pos) = decode_field(self.data, self.pos)?;
        self.pos = next_pos;
        Some((field_num, value))
    }
}

/// Returns the first nested message stored in the requested protobuf field.
pub fn get_submessage(data: &[u8], target_field: u64) -> Option<&[u8]> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::LengthDelimited(bytes) = value {
                return Some(bytes);
            }
        }
        None
    })
}

/// Returns the first unsigned integer field matching the requested number.
pub fn get_varint(data: &[u8], target_field: u64) -> Option<u64> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Varint(value) = value {
                return Some(value);
            }
        }
        None
    })
}

/// Returns the first little-endian double field matching the requested number.
pub fn get_f64(data: &[u8], target_field: u64) -> Option<f64> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Fixed64(bytes) = value {
                return bytes.try_into().ok().map(f64::from_le_bytes);
            }
        }
        None
    })
}

/// Returns the first little-endian float field matching the requested number.
pub fn get_f32(data: &[u8], target_field: u64) -> Option<f32> {
    iter_fields(data).find_map(|(field_num, value)| {
        if field_num == target_field {
            if let WireValue::Fixed32(bytes) = value {
                return bytes.try_into().ok().map(f32::from_le_bytes);
            }
        }
        None
    })
}

/// Decodes a length-delimited field as UTF-8 when the schema expects text.
pub fn get_string(data: &[u8], target_field: u64) -> Option<String> {
    let bytes = get_submessage(data, target_field)?;
    std::str::from_utf8(bytes).ok().map(str::to_string)
}
