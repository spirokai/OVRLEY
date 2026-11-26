//! Telemetry-parser tag maps to [`NativeSample`]s, expanding each domain's arrays at its
//! native rate.
//!
//! Every [`SampleInfo`] holds one GPMF block or video frame, whose GPS, accelerometer and camera
//! values are arrays of up to hundreds of readings:
//!
//! * **GPS**: every acquired fix row becomes a sample with its own sub-frame timestamp (GPS5
//!   rows at 18 Hz, GpsData at the camera's cadence).
//! * **IMU**: every accelerometer vector becomes a sample (200 to 400 Hz).
//! * **Camera**: every vector element (ISO, shutter, colour temperature) becomes a sample spread
//!   evenly over the frame; scalar fields (aperture, focal length, EV) apply to all of them.
//!
//! Tag values are read through typed accessors that reject mismatched wire types and non-finite
//! values. Vendors that keep camera settings outside the standard groups (Insta360 time
//! scalars; DJI, Blackmagic and RED JSON blobs) are read by their own helpers.

use super::time::{
    sub_sample_timestamp_ms, unix_millis_plus_offset_ms_to_rfc3339, unix_seconds_to_rfc3339,
};
use super::NativeSample;
use crate::import::{finite_f64, g_force_from_components, haversine_distance};
use serde_json::Value;
use std::collections::BTreeMap;
use telemetry_parser::tags_impl::{GroupId, Scalar, TagId, TagMap, TagValue};
use telemetry_parser::util::SampleInfo;

const GOPRO_GPS9_TAG: u32 = 0x4750_5339;
const IMPOSSIBLE_GPS_JUMP_MPS: f64 = 1500.0 / 3.6;

/// Converts telemetry-parser's grouped tag maps into the narrow raw-sample
/// shape consumed by the importer.
pub(crate) fn extract_native_samples(samples: &[SampleInfo]) -> Vec<NativeSample> {
    let mut result = Vec::new();
    let mut last_no_fix_coordinate = None;
    let mut acquired_gps = false;

    for (packet_index, sample) in samples.iter().enumerate() {
        let Some(tag_map) = &sample.tag_map else {
            continue;
        };

        let base = NativeSample {
            timestamp_ms: sample.timestamp_ms,
            gps_packet_index: Some(packet_index),
            ..NativeSample::default()
        };

        if !acquired_gps {
            if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
                if !gps5_fix_is_usable(gps_map) {
                    last_no_fix_coordinate =
                        last_scaled_gps5_coordinate(gps_map).or(last_no_fix_coordinate);
                }
            }
        }
        let before_gps = result.len();
        append_gps_samples(&mut result, &base, sample, tag_map);
        acquired_gps |= result[before_gps..]
            .iter()
            .any(|native| native.gps_coordinates().is_some());

        append_camera_samples(&mut result, sample, tag_map);

        if let Some(accelerometer_map) = tag_map.get(&GroupId::Accelerometer) {
            append_imu_samples(&mut result, sample, accelerometer_map);
        }
    }
    result.sort_by(|left, right| left.timestamp_ms.total_cmp(&right.timestamp_ms));
    discard_cached_gps_prefix(&mut result, last_no_fix_coordinate);
    result
}

fn last_scaled_gps5_coordinate(gps_map: &TagMap) -> Option<(f64, f64)> {
    let scales = extract_tag_i32_vec(gps_map, &TagId::Scale)?;
    if scales.len() < 2 || scales[0] == 0 || scales[1] == 0 {
        return None;
    }
    let tag = gps_map.get(&TagId::Data)?;
    let TagValue::Vec_Vec_i32(rows) = &tag.value else {
        return None;
    };
    let row = rows.get().last()?;
    Some((
        *row.first()? as f64 / scales[0] as f64,
        *row.get(1)? as f64 / scales[1] as f64,
    ))
}

fn discard_cached_gps_prefix(samples: &mut [NativeSample], cached: Option<(f64, f64)>) {
    let Some(cached) = cached else {
        return;
    };
    let gps: Vec<_> = samples
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| sample.gps_coordinates().map(|_| index))
        .collect();
    let prefix_len = gps
        .iter()
        .take_while(|&&index| samples[index].latitude.zip(samples[index].longitude) == Some(cached))
        .count();
    if prefix_len == 0 || gps.len() == prefix_len {
        return;
    }

    let stale = &samples[gps[prefix_len - 1]];
    let first_route = &samples[gps[prefix_len]];
    if gps_jump_is_possible(stale, first_route) {
        return;
    }

    // A second row in the same packet does not confirm a new fix. If the next
    // packet returns to the old location, the excursion was the bad position.
    let confirmation = gps
        .iter()
        .enumerate()
        .skip(prefix_len + 1)
        .find(|entry| samples[*entry.1].gps_packet_index != first_route.gps_packet_index);
    let (clear_start, clear_end) = match confirmation {
        Some((_, &index)) if gps_jump_is_possible(first_route, &samples[index]) => (0, prefix_len),
        Some((next, &index)) if gps_jump_is_possible(stale, &samples[index]) => (prefix_len, next),
        Some((next, _)) => (0, next),
        None => (0, gps.len()),
    };

    for &index in &gps[clear_start..clear_end] {
        let sample = &mut samples[index];
        sample.latitude = None;
        sample.longitude = None;
        sample.altitude = None;
        sample.speed = None;
        sample.heading = None;
        sample.timestamp = None;
    }
}

fn gps_jump_is_possible(from: &NativeSample, to: &NativeSample) -> bool {
    let seconds = (to.timestamp_ms - from.timestamp_ms) / 1000.0;
    seconds > 0.0
        && haversine_distance(
            from.latitude.unwrap(),
            from.longitude.unwrap(),
            to.latitude.unwrap(),
            to.longitude.unwrap(),
        ) <= seconds * IMPOSSIBLE_GPS_JUMP_MPS
}

/// Reports whether a sample contains one of the GPS groups handled below.
pub(crate) fn has_gps_source(tag_map: &BTreeMap<GroupId, TagMap>) -> bool {
    tag_map.contains_key(&GroupId::GPS) || tag_map.contains_key(&GroupId::Custom("GPS9".into()))
}

// ---------------------------------------------------------------------------
// GPS
// ---------------------------------------------------------------------------

/// Appends GPS points at their native cadence.
fn append_gps_samples(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    tag_map: &BTreeMap<GroupId, TagMap>,
) {
    if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
        if append_gps_group_samples(result, base, sample, gps_map) {
            return;
        }
    }
    if let Some(gps9_map) = tag_map.get(&GroupId::Custom("GPS9".into())) {
        append_gps_group_samples(result, base, sample, gps9_map);
    }
}

/// Appends whichever supported GPS representation is present in one group.
fn append_gps_group_samples(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    gps_map: &TagMap,
) -> bool {
    if let Some(tag) = gps_map.get(&TagId::Data) {
        match &tag.value {
            TagValue::Vec_GpsData(gps_values) => {
                let values = gps_values.get();
                for (index, gps) in values.iter().enumerate() {
                    if !gps.is_acquired {
                        result.push(NativeSample {
                            timestamp_ms: sub_sample_timestamp_ms(sample, index, values.len()),
                            gps_time_anchor: true,
                            gps_packet_index: base.gps_packet_index,
                            ..NativeSample::default()
                        });
                        continue;
                    }

                    let mut native = base.clone();
                    native.timestamp_ms = sub_sample_timestamp_ms(sample, index, values.len());
                    native.gps_time_anchor = true;
                    native.latitude = finite_f64(gps.lat);
                    native.longitude = finite_f64(gps.lon);
                    native.altitude = finite_f64(gps.altitude);
                    native.speed = finite_f64(gps.speed / 3.6);
                    native.heading = finite_f64(gps.track);
                    native.timestamp = Some(unix_seconds_to_rfc3339(gps.unix_timestamp));

                    if native.has_payload() {
                        result.push(native);
                    }
                }
                return true;
            }
            TagValue::Vec_Vec_i32(rows) => {
                let values = rows.get();
                append_scaled_gps_rows(
                    result,
                    base,
                    sample,
                    gps_map,
                    values.len(),
                    values.iter().enumerate().map(|(index, row)| {
                        (index, Some(row.iter().map(|value| *value as f64).collect()))
                    }),
                );
                return true;
            }
            _ => {}
        }
    }

    if let Some(tag) = gps_map.get(&TagId::Unknown(GOPRO_GPS9_TAG)) {
        if let TagValue::Vec_Vec_Scalar(rows) = &tag.value {
            let values = rows.get();
            append_scaled_gps_rows(
                result,
                base,
                sample,
                gps_map,
                values.len(),
                values
                    .iter()
                    .enumerate()
                    .map(|(index, row)| (index, row.iter().map(scalar_to_f64).collect())),
            );
            return true;
        }
    }

    false
}

/// Normalizes scaled GoPro GPS rows regardless of their parser scalar type.
fn append_scaled_gps_rows(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    gps_map: &TagMap,
    row_count: usize,
    rows: impl Iterator<Item = (usize, Option<Vec<f64>>)>,
) {
    if row_count == 0 {
        return;
    }
    if !gps5_fix_is_usable(gps_map) {
        for index in 0..row_count {
            result.push(NativeSample {
                timestamp_ms: sub_sample_timestamp_ms(sample, index, row_count),
                gps_time_anchor: true,
                gps_packet_index: base.gps_packet_index,
                ..NativeSample::default()
            });
        }
        return;
    }
    let Some(scales) = extract_tag_i32_vec(gps_map, &TagId::Scale).filter(|scales| {
        scales.len() >= 5 && scales[0] != 0 && scales[1] != 0 && scales[2] != 0 && scales[3] != 0
    }) else {
        return;
    };
    let unix_ms = extract_tag_u64(gps_map, &TagId::Unknown(GOPRO_GPSU_TAG));

    for (index, numeric_row) in rows {
        let Some(numeric_row) = numeric_row else {
            continue;
        };
        append_scaled_gps_row(
            result,
            base,
            sample,
            &numeric_row,
            scales,
            index,
            row_count,
            unix_ms,
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "the row, its scales and its position within the packet")]
fn append_scaled_gps_row(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    row: &[f64],
    scales: &[i32],
    index: usize,
    row_count: usize,
    unix_ms: Option<u64>,
) {
    if row.len() < 5 {
        return;
    }

    let latitude = row[0] / scales[0] as f64;
    let longitude = row[1] / scales[1] as f64;
    if latitude == 0.0 && longitude == 0.0 {
        return;
    }

    let mut native = base.clone();
    native.timestamp_ms = sub_sample_timestamp_ms(sample, index, row_count);
    native.gps_time_anchor = true;
    native.latitude = finite_f64(latitude);
    native.longitude = finite_f64(longitude);
    native.altitude = finite_f64(row[2] / scales[2] as f64);
    native.speed = finite_f64(row[3] / scales[3] as f64);

    if let Some(unix_ms) = unix_ms {
        let offset_ms = native.timestamp_ms - sample.timestamp_ms;
        native.timestamp = Some(unix_millis_plus_offset_ms_to_rfc3339(unix_ms, offset_ms));
    }

    if native.has_payload() {
        result.push(native);
    }
}

fn scalar_to_f64(value: &Scalar) -> Option<f64> {
    match value {
        Scalar::u8(value) => finite_f64(*value as f64),
        Scalar::i8(value) => finite_f64(*value as f64),
        Scalar::u16(value) => finite_f64(*value as f64),
        Scalar::i16(value) => finite_f64(*value as f64),
        Scalar::u32(value) => finite_f64(*value as f64),
        Scalar::i32(value) => finite_f64(*value as f64),
        Scalar::u64(value) => finite_f64(*value as f64),
        Scalar::i64(value) => finite_f64(*value as f64),
        Scalar::f32(value) => finite_f64(*value as f64),
        Scalar::f64(value) => finite_f64(*value),
        Scalar::String(_) | Scalar::bool(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

/// Appends camera samples with sub-frame vector expansion.
fn append_camera_samples(
    result: &mut Vec<NativeSample>,
    sample: &SampleInfo,
    tag_map: &BTreeMap<GroupId, TagMap>,
) {
    let mut iso = if let Some(exposure) = tag_map.get(&GroupId::Exposure) {
        extract_tag_f64(exposure, &TagId::ISOValue).map(|v| vec![v])
    } else {
        None
    }
    .or_else(|| {
        tag_map
            .get(&GroupId::Default)
            .and_then(|m| extract_tag_f64(m, &TagId::ISOValue))
            .map(|v| vec![v])
    })
    .or_else(|| {
        tag_map
            .get(&GroupId::Custom("SensorISO".into()))
            .and_then(|m| extract_u16_vec_all(m, &TagId::Data))
    })
    .or_else(|| extract_insta360_iso(tag_map));

    let mut shutter_speed = if let Some(exposure) = tag_map.get(&GroupId::Exposure) {
        extract_tag_f64(exposure, &TagId::ShutterSpeed)
            .or_else(|| extract_tag_u32x2_fraction(exposure, &TagId::ShutterSpeed))
            .or_else(|| extract_tag_f64(exposure, &TagId::ExposureTime))
            .map(|v| vec![v])
    } else {
        None
    }
    .or_else(|| {
        tag_map
            .get(&GroupId::Exposure)
            .and_then(|m| extract_f32_vec_all(m, &TagId::Data))
    })
    .or_else(|| {
        tag_map
            .get(&GroupId::Exposure)
            .and_then(|m| extract_tag_u32x2_rational(m, &TagId::Custom("ShutterSpeed2".into())))
            .map(|v| vec![v])
    })
    .or_else(|| {
        tag_map
            .get(&GroupId::Default)
            .and_then(|m| extract_tag_f64(m, &TagId::ExposureTime))
            .map(|v| vec![v])
    })
    .or_else(|| {
        tag_map
            .get(&GroupId::Imager)
            .and_then(|m| extract_tag_f64(m, &TagId::ExposureTime))
            .map(|v| vec![v])
    })
    .or_else(|| extract_insta360_shutter(tag_map));

    let ev = if let Some(exposure) = tag_map.get(&GroupId::Exposure) {
        extract_tag_f64(exposure, &TagId::Custom("EV".into()))
            .or_else(|| extract_tag_f64(exposure, &TagId::Custom("ExposureValue".into())))
            .map(|v| vec![v])
    } else {
        None
    }
    .or_else(|| extract_insta360_ev(tag_map));

    let mut aperture = tag_map
        .get(&GroupId::Lens)
        .and_then(|m| extract_tag_f64(m, &TagId::IrisFStop))
        .map(|v| vec![v]);

    let mut focal_length = tag_map
        .get(&GroupId::Lens)
        .and_then(|m| extract_tag_f64(m, &TagId::FocalLength))
        .map(|v| vec![v]);

    let mut color_temperature = tag_map
        .get(&GroupId::Colors)
        .and_then(|m| extract_tag_f64(m, &TagId::WhiteBalance))
        .map(|v| vec![v])
        .or_else(|| {
            tag_map
                .get(&GroupId::Custom("WhiteBalanceTemperature".into()))
                .and_then(|m| extract_u16_vec_all(m, &TagId::Data))
        });

    extract_camera_from_json_metadata(
        tag_map,
        &mut iso,
        &mut shutter_speed,
        &mut color_temperature,
        &mut aperture,
        &mut focal_length,
    );

    let max_count = [
        iso.as_ref().map(|v| v.len()),
        shutter_speed.as_ref().map(|v| v.len()),
        color_temperature.as_ref().map(|v| v.len()),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(1);

    for i in 0..max_count {
        let timestamp_ms = sub_sample_timestamp_ms(sample, i, max_count);
        let ns = NativeSample {
            timestamp_ms,
            iso: iso.as_ref().and_then(|v| v.get(i).copied()),
            shutter_speed: shutter_speed.as_ref().and_then(|v| v.get(i).copied()),
            ev: ev.as_ref().and_then(|v| v.get(i).copied()),
            aperture: aperture.as_ref().and_then(|v| v.get(i).copied()),
            focal_length: focal_length.as_ref().and_then(|v| v.get(i).copied()),
            color_temperature: color_temperature.as_ref().and_then(|v| v.get(i).copied()),
            ..NativeSample::default()
        };
        if ns.has_camera_payload() {
            result.push(ns);
        }
    }
}

// ---------------------------------------------------------------------------
// IMU
// ---------------------------------------------------------------------------

/// Appends one IMU (g-force) sample per accelerometer vector.
fn append_imu_samples(result: &mut Vec<NativeSample>, sample: &SampleInfo, accel_map: &TagMap) {
    let Some(tag) = accel_map.get(&TagId::Data) else {
        return;
    };
    match &tag.value {
        TagValue::Vec_Vector3_i16(values) => {
            let vectors = values.get();
            let Some(scale) = extract_tag_f64(accel_map, &TagId::Scale).filter(|s| *s != 0.0)
            else {
                return;
            };
            for (index, vec) in vectors.iter().enumerate() {
                append_imu_vector_sample(
                    result,
                    sample,
                    index,
                    vectors.len(),
                    [vec.x as f64 / scale, vec.y as f64 / scale, vec.z as f64 / scale],
                    accel_map,
                );
            }
        }
        TagValue::Vec_Vector3_f32(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                append_imu_vector_sample(
                    result,
                    sample,
                    index,
                    vectors.len(),
                    [vec.x as f64, vec.y as f64, vec.z as f64],
                    accel_map,
                );
            }
        }
        TagValue::Vec_Vector3_f64(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                append_imu_vector_sample(
                    result,
                    sample,
                    index,
                    vectors.len(),
                    [vec.x, vec.y, vec.z],
                    accel_map,
                );
            }
        }
        TagValue::Vec_TimeVector3_f32(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                append_imu_vector_sample(
                    result,
                    sample,
                    index,
                    vectors.len(),
                    [vec.x as f64, vec.y as f64, vec.z as f64],
                    accel_map,
                );
            }
        }
        TagValue::Vec_TimeVector3_f64(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                append_imu_vector_sample(
                    result,
                    sample,
                    index,
                    vectors.len(),
                    [vec.x, vec.y, vec.z],
                    accel_map,
                );
            }
        }
        _ => {
            if let Some((x, y, z)) = extract_last_acceleration_components(accel_map) {
                append_imu_vector_sample(result, sample, 0, 1, [x, y, z], accel_map);
            }
        }
    }
}

/// Appends one converted IMU vector, retaining both scalar and axis-specific
/// values. Axis values remain available even if scalar magnitude calculation
/// rejects an overflowed result.
fn append_imu_vector_sample(
    result: &mut Vec<NativeSample>,
    sample: &SampleInfo,
    index: usize,
    count: usize,
    [x, y, z]: [f64; 3],
    accel_map: &TagMap,
) {
    let Some((x, y, z)) = acceleration_components_to_g(x, y, z, accel_map) else {
        return;
    };

    result.push(NativeSample {
        timestamp_ms: sub_sample_timestamp_ms(sample, index, count),
        g_force: g_force_from_components(x, y, z),
        g_force_x: Some(x),
        g_force_y: Some(y),
        g_force_z: Some(z),
        ..NativeSample::default()
    });
}

/// Fallback IMU extractor: takes the last vector for unknown accelerator types.
fn extract_last_acceleration_components(map: &TagMap) -> Option<(f64, f64, f64)> {
    let tag = map.get(&TagId::Data)?;
    match &tag.value {
        TagValue::Vec_Vector3_i16(values) => {
            let value = values.get().last()?;
            let scale = extract_tag_f64(map, &TagId::Scale).filter(|scale| *scale != 0.0)?;
            Some((
                value.x as f64 / scale,
                value.y as f64 / scale,
                value.z as f64 / scale,
            ))
        }
        TagValue::Vec_Vector3_f32(values) => values
            .get()
            .last()
            .map(|value| (value.x as f64, value.y as f64, value.z as f64)),
        TagValue::Vec_Vector3_f64(values) => {
            values.get().last().map(|value| (value.x, value.y, value.z))
        }
        TagValue::Vec_TimeVector3_f32(values) => values
            .get()
            .last()
            .map(|value| (value.x as f64, value.y as f64, value.z as f64)),
        TagValue::Vec_TimeVector3_f64(values) => {
            values.get().last().map(|value| (value.x, value.y, value.z))
        }
        _ => None,
    }
}

/// Converts acceleration vectors into g units while retaining each axis.
fn acceleration_components_to_g(x: f64, y: f64, z: f64, map: &TagMap) -> Option<(f64, f64, f64)> {
    let unit_factor = match extract_tag_string(map, &TagId::Unit).as_deref() {
        Some("m/s\u{00b2}") | Some("m/s^2") | Some("m/s2") => 1.0 / 9.80665,
        _ => 1.0,
    };
    Some((
        finite_f64(x * unit_factor)?,
        finite_f64(y * unit_factor)?,
        finite_f64(z * unit_factor)?,
    ))
}

/// GoPro GPMF stores the absolute GPS UTC timestamp under the fourcc `GPSU`.
pub(crate) const GOPRO_GPSU_TAG: u32 = 0x4750_5355;
/// GoPro GPMF stores GPS fix quality under the fourcc `GPSF`.
const GOPRO_GPSF_TAG: u32 = 0x4750_5346;
/// GoPro GPMF stores GPS precision under the fourcc `GPSP`.
const GOPRO_GPSP_TAG: u32 = 0x4750_5350;

/// Reads numeric telemetry tags through one conversion path.
fn extract_tag_f64(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u8(value) => finite_f64(*value.get() as f64),
        TagValue::i8(value) => finite_f64(*value.get() as f64),
        TagValue::u16(value) => finite_f64(*value.get() as f64),
        TagValue::i16(value) => finite_f64(*value.get() as f64),
        TagValue::u32(value) => finite_f64(*value.get() as f64),
        TagValue::i32(value) => finite_f64(*value.get() as f64),
        TagValue::u64(value) => finite_f64(*value.get() as f64),
        TagValue::i64(value) => finite_f64(*value.get() as f64),
        TagValue::f32(value) => finite_f64(*value.get() as f64),
        TagValue::f64(value) => finite_f64(*value.get()),
        _ => None,
    }
}

/// Extracts a rational value from a `u32x2` (num, den) pair.
///
/// Canon stores shutter speed as `Custom("ShutterSpeed2")` with a u32x2 value
/// where the pair is `(den, num)` and the exposure time = `num / den` seconds.
fn extract_tag_u32x2_rational(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u32x2(value) => {
            let (a, b) = *value.get();
            if a != 0 {
                let v = b as f64 / a as f64;
                if v.is_finite() && v > 0.0 {
                    return Some(v);
                }
            }
            if b != 0 {
                let v = a as f64 / b as f64;
                if v.is_finite() && v > 0.0 {
                    return Some(v);
                }
            }
            None
        }
        _ => None,
    }
}

/// Reads a `u32x2` (numerator, denominator) fraction, the shape of
/// telemetry-parser's `ShutterSpeed` in seconds.
fn extract_tag_u32x2_fraction(map: &TagMap, id: &TagId) -> Option<f64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u32x2(value) => {
            let (numerator, denominator) = *value.get();
            (denominator != 0).then(|| numerator as f64 / denominator as f64)
        }
        _ => None,
    }
}

/// Reads integer vector companion tags such as GPMF `SCAL`.
fn extract_tag_i32_vec<'a>(map: &'a TagMap, id: &TagId) -> Option<&'a [i32]> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_i32(value) => Some(value.get()),
        _ => None,
    }
}

/// Reads integer timestamp fields (e.g. GoPro `GPSU`).
pub(crate) fn extract_tag_u64(map: &TagMap, id: &TagId) -> Option<u64> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::u64(value) => Some(*value.get()),
        _ => None,
    }
}

/// Rejects GPS5 packets that explicitly report no fix or unusable precision.
pub(crate) fn gps5_fix_is_usable(gps_map: &TagMap) -> bool {
    // Conservative choice for now: if GoPro marks GPS5 as no-fix or sentinel
    // precision, do not turn those rows into a route or GPS-derived timeline.
    // This needs proper field testing across real cameras/conditions. It may
    // be better UX to import these coordinates with a warning and let the user
    // decide whether the track is usable.
    let fix = extract_tag_f64(gps_map, &TagId::Unknown(GOPRO_GPSF_TAG));
    let precision = extract_tag_f64(gps_map, &TagId::Unknown(GOPRO_GPSP_TAG));

    fix.is_none_or(|fix| fix >= 2.0)
        && precision.is_none_or(|precision| precision.is_finite() && precision < 9999.0)
}

/// Reads string tags only when the parser exposed them as structured strings.
fn extract_tag_string(map: &TagMap, id: &TagId) -> Option<String> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::String(value) => Some(value.get().clone()),
        _ => None,
    }
}

/// Extracts all elements from a `Vec_f32` tag value.
fn extract_f32_vec_all(map: &TagMap, id: &TagId) -> Option<Vec<f64>> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_f32(values) => Some(values.get().iter().map(|x| *x as f64).collect()),
        _ => None,
    }
}

/// Extracts all elements from a `Vec_u16` tag value.
fn extract_u16_vec_all(map: &TagMap, id: &TagId) -> Option<Vec<f64>> {
    let tag = map.get(id)?;
    match &tag.value {
        TagValue::Vec_u16(values) => Some(values.get().iter().map(|x| *x as f64).collect()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Insta360
// ---------------------------------------------------------------------------

/// Insta360 shutter from `Exposure` / `TagId::Data` as `Vec_TimeScalar_f64`.
fn extract_insta360_shutter(tag_map: &BTreeMap<GroupId, TagMap>) -> Option<Vec<f64>> {
    let exposure_map = tag_map.get(&GroupId::Exposure)?;
    let tag = exposure_map.get(&TagId::Data)?;
    match &tag.value {
        TagValue::Vec_TimeScalar_f64(values) => {
            let v: Vec<f64> = values
                .get()
                .iter()
                .map(|ts| ts.v)
                .filter(|x| *x > 0.0)
                .collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => None,
    }
}

/// Insta360 ISO from `Default` / `Custom("AAAData")` as `Vec_TimeScalar_Json`.
fn extract_insta360_iso(tag_map: &BTreeMap<GroupId, TagMap>) -> Option<Vec<f64>> {
    let default_map = tag_map.get(&GroupId::Default)?;
    let tag = default_map.get(&TagId::Custom("AAAData".into()))?;
    match &tag.value {
        TagValue::Vec_TimeScalar_Json(values) => {
            let v: Vec<f64> = values
                .get()
                .iter()
                .filter_map(|ts| {
                    ts.v.get("iso_value")
                        .and_then(|v| v.as_u64())
                        .map(|iso| iso as f64)
                })
                .collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => None,
    }
}

/// Insta360 EV from `Default` / `Custom("AAAData")` as `Vec_TimeScalar_Json`.
fn extract_insta360_ev(tag_map: &BTreeMap<GroupId, TagMap>) -> Option<Vec<f64>> {
    let default_map = tag_map.get(&GroupId::Default)?;
    let tag = default_map.get(&TagId::Custom("AAAData".into()))?;
    match &tag.value {
        TagValue::Vec_TimeScalar_Json(values) => {
            let v: Vec<f64> = values
                .get()
                .iter()
                .filter_map(|ts| {
                    ts.v.get("ev_target")
                        .and_then(|v| v.as_f64())
                        .filter(|x| x.is_finite())
                })
                .collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// JSON metadata (DJI WM169 / WA530 / OQ101, Blackmagic, RED)
// ---------------------------------------------------------------------------

/// Scans `TagId::Metadata` JSON blobs for camera settings.
fn extract_camera_from_json_metadata(
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
    try_extract_json_path(
        meta,
        "/exposureTime/exposureTime",
        shutter_speed,
        json_rational_to_f64,
    );
    try_extract_json_path(
        meta,
        "/whiteBalanceCct/whiteBalanceCct",
        color_temperature,
        |v| v.as_f64(),
    );
    try_extract_json_path(meta, "/fNumber/fNumber", aperture, json_rational_to_f64);
    try_extract_json_path(
        meta,
        "/focalLength/focalLength",
        focal_length,
        json_rational_to_f64,
    );

    // Blackmagic paths (flat per-frame metadata)
    try_extract_json_path(meta, "/iso", iso, |v| v.as_f64());
    try_extract_json_path(meta, "/white_balance_kelvin", color_temperature, |v| {
        v.as_f64()
    });
    try_extract_json_path(meta, "/exposure", shutter_speed, |v| v.as_f64());
    try_extract_json_string_parsed(meta, "/shutter_value", shutter_speed, parse_rational_string);
    try_extract_json_string_parsed(meta, "/aperture", aperture, parse_fstop_string);
    try_extract_json_string_parsed(meta, "/focal_length", focal_length, parse_mm_string);

    // RED paths (flat per-frame metadata from R3D + RMD sidecar)
    try_extract_json_path(meta, "/iso", iso, |v| v.as_f64());
    try_extract_json_path(meta, "/white_balance_kelvin", color_temperature, |v| {
        v.as_f64()
    });
    try_extract_json_path(meta, "/exposure_time", shutter_speed, |v| v.as_f64());
    try_extract_json_path(meta, "/focal_length", focal_length, |v| v.as_f64());
    try_extract_json_string_parsed(meta, "/aperture", aperture, |s| s.parse::<f64>().ok());
}

fn try_extract_json_path<F>(json: &Value, pointer: &str, target: &mut Option<Vec<f64>>, extract: F)
where
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
    if let Some(value) = json
        .pointer(pointer)
        .and_then(|v| v.as_str())
        .and_then(&parse)
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gps_sample(
        timestamp_ms: f64,
        latitude: f64,
        longitude: f64,
        packet_index: usize,
    ) -> NativeSample {
        NativeSample {
            timestamp_ms,
            gps_time_anchor: true,
            gps_packet_index: Some(packet_index),
            latitude: Some(latitude),
            longitude: Some(longitude),
            speed: Some(0.0),
            ..NativeSample::default()
        }
    }

    #[test]
    fn discards_cached_fix_after_confirmed_impossible_jump() {
        let cached = (12.7158364, 77.2275231);
        let mut samples = vec![
            NativeSample {
                timestamp_ms: 0.0,
                gps_time_anchor: true,
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 500.0,
                g_force: Some(1.2),
                ..NativeSample::default()
            },
            gps_sample(24_960.0, cached.0, cached.1, 24),
            gps_sample(25_900.0, cached.0, cached.1, 24),
            gps_sample(26_000.0, 12.4198418, 76.6687983, 25),
            gps_sample(26_100.0, 12.4198420, 76.6687990, 25),
            gps_sample(27_040.0, 12.4198500, 76.6688000, 26),
        ];

        discard_cached_gps_prefix(&mut samples, Some(cached));

        assert_eq!(samples.len(), 7);
        assert!(samples[2..4]
            .iter()
            .all(|sample| sample.gps_time_anchor && !sample.has_gps_payload()));
        assert_eq!(samples[1].g_force, Some(1.2));
        assert_eq!(samples[4].latitude, Some(12.4198418));
    }

    #[test]
    fn keeps_stationary_fix_when_later_movement_is_plausible() {
        let cached = (12.7158364, 77.2275231);
        let mut samples = vec![
            gps_sample(1000.0, cached.0, cached.1, 1),
            gps_sample(2000.0, cached.0, cached.1, 2),
            gps_sample(3000.0, 12.7158400, 77.2275300, 3),
            gps_sample(3100.0, 12.7158410, 77.2275310, 3),
        ];

        discard_cached_gps_prefix(&mut samples, Some(cached));

        assert!(samples.iter().all(NativeSample::has_gps_payload));
    }
}
