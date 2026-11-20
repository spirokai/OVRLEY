//! GPS fix parsing for DJI AC004 metadata samples.
//!
//! The protobuf wire decoder [`super::protobuf`] produces raw field values
//! without schema meaning. This module interprets those values as GPS
//! coordinates, device metadata, velocity vectors, and timestamps by
//! navigating the known AC004 field-number hierarchy (`f3.f4.f2.f1` for
//! latitude/longitude, `f3.f1.f4` for device name, etc.). Samples without a
//! valid GPS fix or with zero coordinates are dropped so the fallback pipeline
//! never produces null-location telemetry.
//!
//! Owns: [`parse_raw_metadata`], [`parse_point_from_sample`],
//!       [`clip_clock_offset`], [`first_device_info`].
//! Does not own: wire-format decoding (see [`super::protobuf`]), inspection
//!       (see [`super::inspect`]), FFmpeg integration (see [`super`]), or the
//!       GPS clock-to-UTC rules (see [`crate::media::time`]).

use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use chrono_tz::Tz;

use super::protobuf::{get_f32, get_f64, get_string, get_submessage, get_varint};
use super::{DjiAc004Sample, DjiAc004Telemetry, DEFAULT_SAMPLE_RATE_HZ};
use crate::media::telemetry_math::g_force_from_components;
use crate::media::time::{
    advance_utc_by_ms, dji_clock_offset, dji_timestamp_to_rfc3339, parse_dji_timestamp,
};
use crate::media::timezone::timezone_for_coordinates;

/// Parses raw concatenated DJI metadata samples into structured telemetry.
///
/// FFmpeg's `data` muxer emits the metadata payloads as one raw byte stream.
/// AC004 samples are protobuf messages wrapped in top-level field `3`; the GPS
/// payload lives under `f3.f4`. Unknown fields are skipped so new camera fields
/// do not break extraction of the stable GPS subset.
///
/// `creation_time` is the video's creation timestamp, when known; it decides
/// how the GPS clock text maps to UTC (see [`dji_clock_offset`]).
pub fn parse_raw_metadata(
    raw_data: &[u8],
    creation_time: Option<DateTime<Utc>>,
) -> Option<DjiAc004Telemetry> {
    let device_info = first_device_info(raw_data);
    let sample_rate_hz = device_info
        .as_ref()
        .and_then(|info| info.sample_rate_hz)
        .filter(|rate| rate.is_finite() && *rate > 0.0);
    let sample_rate = sample_rate_hz.unwrap_or(DEFAULT_SAMPLE_RATE_HZ);

    let clock_offset = clip_clock_offset(raw_data, sample_rate, creation_time)?;
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
        while let Some((field_num, value, next_pos)) = super::protobuf::decode_field(raw_data, pos)
        {
            pos = next_pos;
            if let (3, super::protobuf::WireValue::LengthDelimited(sample_data)) =
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

/// Decides the clip's GPS clock offset once, from its first valid fix.
///
/// Returns `None` when the stream has no valid fix at all.
fn clip_clock_offset(
    raw_data: &[u8],
    sample_rate: f64,
    creation_time: Option<DateTime<Utc>>,
) -> Option<TimeDelta> {
    let (frame_index, fix) = top_level_samples(raw_data)
        .enumerate()
        .find_map(|(frame_index, sample_data)| Some((frame_index, read_gps_fix(sample_data)?)))?;
    let gps_tz = timezone_for_coordinates(fix.longitude, fix.latitude)
        .and_then(|name| name.parse::<Tz>().ok());
    let reference =
        creation_time.map(|time| advance_utc_by_ms(time, media_time_ms(frame_index, sample_rate)));
    Some(dji_clock_offset(fix.timestamp, gps_tz, reference))
}

/// The validated GPS fix subset (`f4.f2`) of one sample.
struct GpsFix {
    latitude: f64,
    longitude: f64,
    altitude: f64,
    /// Zone-less GPS clock text; see [`dji_clock_offset`].
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
