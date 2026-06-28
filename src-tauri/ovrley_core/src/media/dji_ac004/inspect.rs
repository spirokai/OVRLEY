//! Bounded field-tree inspection for DJI AC004 metadata.
//!
//! When the AC004 fallback does not produce expected telemetry, the raw
//! protobuf field tree contains clues about schema changes or new camera
//! firmware. This module walks the full message hierarchy up to a configurable
//! depth and surfaces every known and unknown field number so engineers can
//! correlate field values with video time without needing to update the parser
//! first. The output includes both a standard-parsed view (via
//! [`super::parser`]) and a complete field tree with inferred wire types.
//!
//! Owns: [`inspect_raw_metadata`], [`inspect_sample`], [`inspect_message`].
//! Does not own: standard parsing (see [`super::parser`]), wire-format decoding
//!       (see [`super::protobuf`]), or FFmpeg integration (see [`super`]).

use serde_json::{json, Value};

use super::protobuf::{get_f32, get_f64, get_string, get_submessage, get_varint, iter_fields, WireValue};

pub fn inspect_raw_metadata(raw_data: &[u8], stream_index: usize, max_samples: usize) -> Value {
    let parsed = super::parser::parse_raw_metadata(raw_data);
    let mut samples = Vec::new();
    let mut pos = 0;
    let mut frame_index = 0usize;
    let mut top_level_field_count = 0usize;

    while pos < raw_data.len() {
        let Some((field_num, value, next_pos)) = super::protobuf::decode_field(raw_data, pos) else {
            break;
        };
        pos = next_pos;
        top_level_field_count += 1;

        if field_num != 3 {
            continue;
        }
        let WireValue::LengthDelimited(sample_data) = value else {
            continue;
        };

        if samples.len() < max_samples {
            samples.push(inspect_sample(sample_data, frame_index));
        }
        frame_index += 1;
    }

    json!({
        "streamIndex": stream_index,
        "rawByteCount": raw_data.len(),
        "topLevelFieldCount": top_level_field_count,
        "sampleEnvelopeCount": frame_index,
        "parsedTelemetry": parsed.as_ref().map(|telemetry| json!({
            "deviceName": telemetry.device_name,
            "sampleRateHz": telemetry.sample_rate_hz,
            "syncTime": telemetry.sync_time,
            "validGpsSampleCount": telemetry.samples.len(),
            "firstSamples": telemetry.samples.iter().take(max_samples).map(|sample| json!({
                "frameIndex": sample.frame_index,
                "timestampMs": sample.timestamp_ms,
                "timestamp": sample.timestamp,
                "latitude": sample.latitude,
                "longitude": sample.longitude,
                "altitude": sample.altitude,
                "speed": sample.speed,
                "heading": sample.heading,
                "gForce": sample.g_force,
            })).collect::<Vec<_>>(),
        })),
        "sampleFieldTrees": samples,
    })
}

fn inspect_sample(sample_data: &[u8], frame_index: usize) -> Value {
    let gps_msg = get_submessage(sample_data, 4);
    let device_msg = gps_msg.and_then(|message| get_submessage(message, 1));
    let fix_msg = gps_msg.and_then(|message| get_submessage(message, 2));
    let coords_msg = fix_msg.and_then(|message| get_submessage(message, 1));
    let velocity_msg = gps_msg.and_then(|message| get_submessage(message, 3));
    let timestamp_msg = fix_msg.and_then(|message| get_submessage(message, 6));

    json!({
        "frameIndex": frame_index,
        "schemaView": {
            "device": device_msg.map(|message| json!({
                "deviceName_f4": get_string(message, 4),
                "sampleRateHz_f5": get_f32(message, 5),
            })),
            "fix": fix_msg.map(|message| json!({
                "fixType_f1_f1": coords_msg.and_then(|coords| get_varint(coords, 1)),
                "latitude_f1_f2": coords_msg.and_then(|coords| get_f64(coords, 2)),
                "longitude_f1_f3": coords_msg.and_then(|coords| get_f64(coords, 3)),
                "altitudeMm_f2": get_varint(message, 2),
                "timestamp_f6_f1": timestamp_msg.and_then(|timestamp| get_string(timestamp, 1)),
            })),
            "velocity": velocity_msg.map(|message| json!({
                "vx_f1": get_f32(message, 1),
                "vy_f2": get_f32(message, 2),
            })),
            "unknownFutureMetrics": {
                "field_2_3_1": get_submessage(sample_data, 2)
                    .and_then(|message| get_submessage(message, 3))
                    .and_then(|message| get_f32(message, 1)),
                "field_2_6_1": get_submessage(sample_data, 2)
                    .and_then(|message| get_submessage(message, 6))
                    .and_then(|message| get_varint(message, 1)),
                "field_2_13_2": get_submessage(sample_data, 2)
                    .and_then(|message| get_submessage(message, 13))
                    .and_then(|message| get_varint(message, 2)),
                "field_2_13_3": get_submessage(sample_data, 2)
                    .and_then(|message| get_submessage(message, 13))
                    .and_then(|message| get_varint(message, 3)),
            },
            "accelerationCandidate_2_10": get_submessage(sample_data, 2)
                .and_then(|message| get_submessage(message, 10))
                .map(|message| json!({
                    "x_f2": get_f32(message, 2),
                    "y_f3": get_f32(message, 3),
                    "z_f4": get_f32(message, 4),
                })),
        },
        "fieldTree": inspect_message(sample_data, 0, 4),
    })
}

fn inspect_message(data: &[u8], depth: usize, max_depth: usize) -> Vec<Value> {
    iter_fields(data)
        .map(|(field_num, value)| {
            let wire_type = match value {
                WireValue::Varint(_) => "varint",
                WireValue::Fixed64(_) => "fixed64",
                WireValue::LengthDelimited(_) => "lengthDelimited",
                WireValue::Fixed32(_) => "fixed32",
            };

            let value_json = match value {
                WireValue::Varint(value) => json!({ "u64": value }),
                WireValue::Fixed64(bytes) => {
                    json!({ "f64": bytes.try_into().ok().map(f64::from_le_bytes) })
                }
                WireValue::Fixed32(bytes) => {
                    json!({ "f32": bytes.try_into().ok().map(f32::from_le_bytes) })
                }
                WireValue::LengthDelimited(bytes) => {
                    let utf8 = std::str::from_utf8(bytes).ok();
                    let nested = if depth < max_depth {
                        let children = inspect_message(bytes, depth + 1, max_depth);
                        (!children.is_empty()).then_some(children)
                    } else {
                        None
                    };
                    json!({
                        "byteLen": bytes.len(),
                        "utf8": utf8,
                        "nested": nested,
                    })
                }
            };

            json!({
                "field": field_num,
                "wire": wire_type,
                "value": value_json,
            })
        })
        .collect()
}
