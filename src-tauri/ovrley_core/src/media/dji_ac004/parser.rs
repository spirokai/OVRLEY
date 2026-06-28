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
//!       [`first_device_info`].
//! Does not own: wire-format decoding (see [`super::protobuf`]), inspection
//!       (see [`super::inspect`]), or FFmpeg integration (see [`super`]).

use chrono::{DateTime, NaiveDateTime, Utc};

use super::protobuf::{get_f32, get_f64, get_string, get_submessage, get_varint};
use super::{DjiAc004Sample, DjiAc004Telemetry, DEFAULT_SAMPLE_RATE_HZ};
use crate::media::telemetry_math::g_force_from_components;

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

    let mut points = Vec::new();
    let mut pos = 0;
    let mut frame_index = 0usize;

    while pos < raw_data.len() {
        let Some((field_num, value, next_pos)) = super::protobuf::decode_field(raw_data, pos) else {
            break;
        };
        pos = next_pos;

        if field_num == 3 {
            if let super::protobuf::WireValue::LengthDelimited(sample_data) = value {
                if let Some(mut point) = parse_point_from_sample(sample_data, frame_index) {
                    point.timestamp_ms = frame_index as f64 * 1000.0 / sample_rate;
                    points.push(point);
                }
                frame_index += 1;
            }
        }
    }

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

/// Reads the AC004 GPS subset from one top-level sample message.
fn parse_point_from_sample(sample_data: &[u8], frame_index: usize) -> Option<DjiAc004Sample> {
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
    let timestamp = {
        let timestamp_text = get_string(timestamp_msg, 1)?;
        let naive = NaiveDateTime::parse_from_str(&timestamp_text, crate::media::dji_ac004::DJI_TIMESTAMP_FORMAT).ok()?;
        let datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(naive, Utc);
        datetime.to_rfc3339()
    };

    let (vx, vy) = get_submessage(gps_msg, 3).map_or((0.0, 0.0), |velocity_msg| {
        (
            get_f32(velocity_msg, 1).unwrap_or(0.0) as f64,
            get_f32(velocity_msg, 2).unwrap_or(0.0) as f64,
        )
    });
    let heading = velocity_heading_degrees(vx, vy);
    let g_force = extract_acceleration_g_force(sample_data);

    Some(DjiAc004Sample {
        frame_index,
        timestamp_ms: 0.0,
        timestamp,
        latitude,
        longitude,
        altitude,
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
    let mut pos = 0;
    while pos < raw_data.len() {
        let Some((field_num, value, next_pos)) = super::protobuf::decode_field(raw_data, pos) else {
            break;
        };
        pos = next_pos;

        if field_num != 3 {
            continue;
        }
        let super::protobuf::WireValue::LengthDelimited(sample_data) = value else {
            continue;
        };
        let gps_msg = get_submessage(sample_data, 4)?;
        let device_msg = get_submessage(gps_msg, 1)?;
        return Some(DeviceInfo {
            device_name: get_string(device_msg, 4),
            sample_rate_hz: get_f32(device_msg, 5).map(|value| value as f64),
        });
    }
    None
}
