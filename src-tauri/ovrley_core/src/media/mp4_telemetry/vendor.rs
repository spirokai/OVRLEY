//! Vendor-specific camera metadata extraction from non-standard tag layouts.
//!
//! Camera vendors do not place equivalent fields under a universal tag group.
//! GoPro stores shutter speed as a flat `Vec_f32` in the Exposure group,
//! Insta360 uses time-scalar vectors with embedded timestamps, and DJI /
//! Blackmagic / RED embed camera settings in opaque JSON blobs. Isolating the
//! vendor-specific traversal logic here keeps
//! [`extraction::append_camera_samples`] focused on fallback priority and
//! vector expansion rather than tag-map archaeology.
//!
//! Owns: GoPro GPMF rate parsing, Insta360 time-scalar unpacking, DJI /
//!       Blackmagic / RED JSON metadata path extraction.
//! Does not own: generic tag accessors (see [`super::tags`]), sample extraction,
//!       or serialization.

use std::collections::BTreeMap;

use serde_json::Value;
use telemetry_parser::tags_impl::{GroupId, TagId, TagMap, TagValue};
use telemetry_parser::util::SampleInfo;

// ---------------------------------------------------------------------------
// GoPro
// ---------------------------------------------------------------------------

/// Scans all samples for the GoPro `RATE` tag (e.g. "2_1SEC" → 2 Hz).
pub(crate) fn extract_gopro_rate_hz(samples: &[SampleInfo]) -> Option<f64> {
    for sample in samples {
        let Some(tag_map) = &sample.tag_map else {
            continue;
        };
        if let Some(default_map) = tag_map.get(&GroupId::Default) {
            if let Some(tag) = default_map.get(&TagId::Unknown(0x5241_5445)) {
                if let TagValue::String(s) = &tag.value {
                    if let Some(rate) = parse_gopro_rate(s.get()) {
                        return Some(rate);
                    }
                }
            }
        }
    }
    None
}

fn parse_gopro_rate(s: &str) -> Option<f64> {
    s.split('_')
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .filter(|h| *h > 0.0)
}

// ---------------------------------------------------------------------------
// Insta360
// ---------------------------------------------------------------------------

/// Insta360 shutter from `Exposure` / `TagId::Data` as `Vec_TimeScalar_f64`.
pub(crate) fn extract_insta360_shutter(tag_map: &BTreeMap<GroupId, TagMap>) -> Option<Vec<f64>> {
    let exposure_map = tag_map.get(&GroupId::Exposure)?;
    let tag = exposure_map.get(&TagId::Data)?;
    match &tag.value {
        TagValue::Vec_TimeScalar_f64(values) => {
            let v: Vec<f64> = values.get().iter().map(|ts| ts.v).filter(|x| *x > 0.0).collect();
            if v.is_empty() { None } else { Some(v) }
        }
        _ => None,
    }
}

/// Insta360 ISO from `Default` / `Custom("AAAData")` as `Vec_TimeScalar_Json`.
pub(crate) fn extract_insta360_iso(tag_map: &BTreeMap<GroupId, TagMap>) -> Option<Vec<f64>> {
    let default_map = tag_map.get(&GroupId::Default)?;
    let tag = default_map.get(&TagId::Custom("AAAData".into()))?;
    match &tag.value {
        TagValue::Vec_TimeScalar_Json(values) => {
            let v: Vec<f64> = values.get().iter()
                .filter_map(|ts| ts.v.get("iso_value").and_then(|v| v.as_u64()).map(|iso| iso as f64))
                .collect();
            if v.is_empty() { None } else { Some(v) }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// JSON metadata (DJI WM169 / WA530 / OQ101, Blackmagic, RED)
// ---------------------------------------------------------------------------

/// Scans `TagId::Metadata` JSON blobs for camera settings.
pub(crate) fn extract_camera_from_json_metadata(
    tag_map: &BTreeMap<GroupId, TagMap>,
    iso: &mut Option<Vec<f64>>,
    shutter_speed: &mut Option<Vec<f64>>,
    color_temperature: &mut Option<Vec<f64>>,
    aperture: &mut Option<Vec<f64>>,
    focal_length: &mut Option<Vec<f64>>,
) {
    let Some(default_map) = tag_map.get(&GroupId::Default) else {
        return;
    };
    let Some(tag) = default_map.get(&TagId::Metadata) else {
        return;
    };
    let TagValue::Json(meta) = &tag.value else {
        return;
    };
    let meta: &Value = meta.get();

    // DJI WM169 / WA530 / OQ101 paths (nested protobuf flattened as JSON)
    try_extract_json_path(meta, "/iso/iso", iso, |v| v.as_f64());
    try_extract_json_path(meta, "/exposureTime/exposureTime", shutter_speed, json_rational_to_f64);
    try_extract_json_path(meta, "/whiteBalanceCct/whiteBalanceCct", color_temperature, |v| v.as_f64());
    try_extract_json_path(meta, "/fNumber/fNumber", aperture, json_rational_to_f64);
    try_extract_json_path(meta, "/focalLength/focalLength", focal_length, json_rational_to_f64);

    // Blackmagic paths (flat per-frame metadata)
    try_extract_json_path(meta, "/iso", iso, |v| v.as_f64());
    try_extract_json_path(meta, "/white_balance_kelvin", color_temperature, |v| v.as_f64());
    try_extract_json_path(meta, "/exposure", shutter_speed, |v| v.as_f64());
    try_extract_json_string_parsed(meta, "/shutter_value", shutter_speed, parse_rational_string);
    try_extract_json_string_parsed(meta, "/aperture", aperture, parse_fstop_string);
    try_extract_json_string_parsed(meta, "/focal_length", focal_length, parse_mm_string);

    // RED paths (flat per-frame metadata from R3D + RMD sidecar)
    try_extract_json_path(meta, "/iso", iso, |v| v.as_f64());
    try_extract_json_path(meta, "/white_balance_kelvin", color_temperature, |v| v.as_f64());
    try_extract_json_path(meta, "/exposure_time", shutter_speed, |v| v.as_f64());
    try_extract_json_path(meta, "/focal_length", focal_length, |v| v.as_f64());
    try_extract_json_string_parsed(meta, "/aperture", aperture, |s| s.parse::<f64>().ok());
}

fn try_extract_json_path<F>(
    json: &Value,
    pointer: &str,
    target: &mut Option<Vec<f64>>,
    extract: F,
) where
    F: Fn(&Value) -> Option<f64>,
{
    if target.is_some() {
        return;
    }
    if let Some(value) = json.pointer(pointer).and_then(&extract) {
        *target = Some(vec![value]);
    }
}

fn try_extract_json_string_parsed<F>(
    json: &Value,
    pointer: &str,
    target: &mut Option<Vec<f64>>,
    parse: F,
) where
    F: Fn(&str) -> Option<f64>,
{
    if target.is_some() {
        return;
    }
    if let Some(value) = json.pointer(pointer).and_then(|v| v.as_str()).and_then(&parse) {
        *target = Some(vec![value]);
    }
}

fn parse_rational_string(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().ok()?;
        let den = parts[1].parse::<f64>().ok()?;
        (den != 0.0).then_some(num / den)
    } else {
        None
    }
}

fn parse_fstop_string(s: &str) -> Option<f64> {
    s.trim_start_matches("f/").parse::<f64>().ok()
}

fn parse_mm_string(s: &str) -> Option<f64> {
    s.trim_end_matches("mm").parse::<f64>().ok()
}

fn json_rational_to_f64(v: &Value) -> Option<f64> {
    v.as_array().and_then(|arr| {
        if arr.len() >= 2 {
            let num = arr[0].as_f64()?;
            let den = arr[1].as_f64()?;
            (den != 0.0).then_some(num / den)
        } else {
            None
        }
    })
}
