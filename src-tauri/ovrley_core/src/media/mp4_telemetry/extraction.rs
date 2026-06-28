//! Tag-map to [`NativeSample`] conversion with domain-specific vector expansion.
//!
//! Telemetry-parser emits one [`SampleInfo`] per GPMF block or video frame.
//! Inside each `SampleInfo`, GPS coordinates, accelerometer vectors, and
//! camera settings are stored as multi-element arrays — sometimes hundreds of
//! readings per frame. This module expands each domain's arrays into separate
//! [`NativeSample`] entries so the columnar JSON preserves the source's native
//! sample rate.
//!
//! The three domains have different expansion behaviour:
//!
//! * **GPS** — every acquired fix row becomes a sample with its own sub-frame
//!   timestamp (GPS5 rows at 18 Hz, GpsData at camera-native cadence).
//! * **IMU** — every accelerometer vector becomes a sample (200–400 Hz).
//! * **Camera** — every vector element (ISO, shutter, colour temperature)
//!   becomes a sample, distributed evenly within the frame duration. Scalar
//!   fields (aperture, focal length, EV) apply uniformly to all expanded
//!   samples from the same frame.
//!
//! Owns: [`extract_native_samples`], [`append_gps_samples`],
//!       [`append_camera_samples`], [`append_imu_samples`].
//! Does not own: tag accessors (see [`super::tags`]), vendor-specific parsing
//!       (see [`super::vendor`]), smoothing, or JSON serialization.

use std::collections::BTreeMap;

use telemetry_parser::tags_impl::{GroupId, TagId, TagMap, TagValue};
use telemetry_parser::util::SampleInfo;

use crate::media::native_sample::{sub_sample_timestamp_ms, NativeSample};
use crate::media::telemetry_math::{finite_f64, g_force_from_components};

use super::tags::{
    extract_f32_vec_all, extract_tag_f64, extract_tag_i32_vec, extract_tag_string,
    extract_tag_u32x2_rational, extract_tag_u64, extract_u16_vec_all,
};
use super::vendor::{
    extract_camera_from_json_metadata, extract_gopro_rate_hz, extract_insta360_iso,
    extract_insta360_shutter,
};

/// GoPro GPMF stores the absolute GPS UTC timestamp under the fourcc `GPSU`.
const GOPRO_GPSU_TAG: u32 = 0x4750_5355;

/// Converts telemetry-parser's grouped tag maps into the narrow raw-sample
/// shape consumed by the importer.
pub(crate) fn extract_native_samples(samples: &[SampleInfo]) -> Vec<NativeSample> {
    let mut result = Vec::new();
    let gopro_rate_hz = extract_gopro_rate_hz(samples);

    for sample in samples {
        let Some(tag_map) = &sample.tag_map else {
            continue;
        };

        let base = NativeSample {
            timestamp_ms: sample.timestamp_ms,
            ..NativeSample::default()
        };

        if let Some(gps_map) = tag_map.get(&GroupId::GPS) {
            append_gps_samples(&mut result, &base, sample, gps_map, gopro_rate_hz);
        }

        append_camera_samples(&mut result, sample, tag_map);

        if let Some(accelerometer_map) = tag_map.get(&GroupId::Accelerometer) {
            append_imu_samples(&mut result, sample, accelerometer_map);
        }
    }
    result.sort_by(|left, right| left.timestamp_ms.total_cmp(&right.timestamp_ms));
    result
}

// ---------------------------------------------------------------------------
// GPS
// ---------------------------------------------------------------------------

/// Appends GPS points at their native cadence.
fn append_gps_samples(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    gps_map: &TagMap,
    gopro_rate_hz: Option<f64>,
) {
    let Some(tag) = gps_map.get(&TagId::Data) else {
        return;
    };

    match &tag.value {
        TagValue::Vec_GpsData(gps_values) => {
            let values = gps_values.get();
            for (index, gps) in values.iter().enumerate() {
                if !gps.is_acquired {
                    continue;
                }

                let mut native = base.clone();
                native.timestamp_ms = sub_sample_timestamp_ms(sample, index, values.len());
                native.latitude = finite_f64(gps.lat);
                native.longitude = finite_f64(gps.lon);
                native.altitude = finite_f64(gps.altitude);
                native.speed = finite_f64(gps.speed / 3.6);
                native.heading = finite_f64(gps.track);
                native.timestamp = Some(unix_to_rfc3339(gps.unix_timestamp));

                if native.has_payload() {
                    result.push(native);
                }
            }
        }
        TagValue::Vec_Vec_i32(rows) => {
            append_scaled_gps_rows(result, base, sample, gps_map, rows.get(), gopro_rate_hz)
        }
        _ => {}
    }
}

/// Normalizes GoPro-style GPS5 integer rows using parser-provided scale tags.
fn append_scaled_gps_rows(
    result: &mut Vec<NativeSample>,
    base: &NativeSample,
    sample: &SampleInfo,
    gps_map: &TagMap,
    rows: &[Vec<i32>],
    gopro_rate_hz: Option<f64>,
) {
    if rows.is_empty() {
        return;
    }
    let Some(scales) = extract_tag_i32_vec(gps_map, &TagId::Scale).filter(|scales| {
        scales.len() >= 5 && scales[0] != 0 && scales[1] != 0 && scales[2] != 0 && scales[3] != 0
    }) else {
        return;
    };
    let unix_ms = extract_tag_u64(gps_map, &TagId::Unknown(GOPRO_GPSU_TAG));
    let stmp_us = extract_tag_u64(gps_map, &TagId::TimestampUs);
    let rate_hz = gopro_rate_hz;

    for (index, row) in rows.iter().enumerate() {
        if row.len() < 5 {
            continue;
        }

        let latitude = row[0] as f64 / scales[0] as f64;
        let longitude = row[1] as f64 / scales[1] as f64;
        if latitude == 0.0 && longitude == 0.0 {
            continue;
        }

        let mut native = base.clone();

        native.timestamp_ms = if let (Some(stmp), Some(rate)) = (stmp_us, rate_hz) {
            stmp as f64 / 1000.0 + index as f64 / rate * 1000.0
        } else if let Some(stmp) = stmp_us {
            let stmp_ms = stmp as f64 / 1000.0;
            if index == 0 || rows.len() <= 1 {
                stmp_ms
            } else {
                stmp_ms + sample.duration_ms * index as f64 / rows.len() as f64
            }
        } else {
            sub_sample_timestamp_ms(sample, index, rows.len())
        };

        native.latitude = finite_f64(latitude);
        native.longitude = finite_f64(longitude);
        native.altitude = finite_f64(row[2] as f64 / scales[2] as f64);
        native.speed = finite_f64(row[3] as f64 / scales[3] as f64);

        if let Some(unix_ms) = unix_ms {
            if let Some(rate) = rate_hz {
                native.timestamp = Some(unix_to_rfc3339(
                    unix_ms as f64 / 1000.0 + index as f64 / rate,
                ));
            } else {
                let offset_ms = native.timestamp_ms - sample.timestamp_ms;
                native.timestamp = Some(unix_to_rfc3339(
                    unix_ms as f64 / 1000.0 + offset_ms / 1000.0,
                ));
            }
        }

        if native.has_payload() {
            result.push(native);
        }
    }
}

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

/// Appends camera samples with sub-frame vector expansion.
pub(crate) fn append_camera_samples(
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

    let mut shutter_speed =
        if let Some(exposure) = tag_map.get(&GroupId::Exposure) {
            extract_tag_f64(exposure, &TagId::ShutterSpeed)
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
    };

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
fn append_imu_samples(
    result: &mut Vec<NativeSample>,
    sample: &SampleInfo,
    accel_map: &TagMap,
) {
    let Some(tag) = accel_map.get(&TagId::Data) else {
        return;
    };
    let stmp_us = extract_tag_u64(accel_map, &TagId::TimestampUs);

    match &tag.value {
        TagValue::Vec_Vector3_i16(values) => {
            let vectors = values.get();
            let Some(scale) = extract_tag_f64(accel_map, &TagId::Scale).filter(|s| *s != 0.0)
            else {
                return;
            };
            for (index, vec) in vectors.iter().enumerate() {
                let ts = imu_sample_timestamp_ms(sample, index, vectors.len(), stmp_us);
                let g = compute_g_force_components(
                    vec.x as f64 / scale,
                    vec.y as f64 / scale,
                    vec.z as f64 / scale,
                    accel_map,
                );
                if let Some(g) = g {
                    result.push(NativeSample {
                        timestamp_ms: ts,
                        g_force: Some(g),
                        ..NativeSample::default()
                    });
                }
            }
        }
        TagValue::Vec_Vector3_f32(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                let ts = imu_sample_timestamp_ms(sample, index, vectors.len(), stmp_us);
                let g = compute_g_force_components(vec.x as f64, vec.y as f64, vec.z as f64, accel_map);
                if let Some(g) = g {
                    result.push(NativeSample {
                        timestamp_ms: ts,
                        g_force: Some(g),
                        ..NativeSample::default()
                    });
                }
            }
        }
        TagValue::Vec_Vector3_f64(values) => {
            let vectors = values.get();
            for (index, vec) in vectors.iter().enumerate() {
                let ts = imu_sample_timestamp_ms(sample, index, vectors.len(), stmp_us);
                let g = compute_g_force_components(vec.x, vec.y, vec.z, accel_map);
                if let Some(g) = g {
                    result.push(NativeSample {
                        timestamp_ms: ts,
                        g_force: Some(g),
                        ..NativeSample::default()
                    });
                }
            }
        }
        _ => {
            if let Some(g) = extract_g_force(accel_map) {
                result.push(NativeSample {
                    timestamp_ms: sample.timestamp_ms,
                    g_force: Some(g),
                    ..NativeSample::default()
                });
            }
        }
    }
}

/// Fallback IMU extractor: takes the last vector for unknown accelerator types.
fn extract_g_force(map: &TagMap) -> Option<f64> {
    let tag = map.get(&TagId::Data)?;
    match &tag.value {
        TagValue::Vec_Vector3_i16(values) => {
            let value = values.get().last()?;
            let scale = extract_tag_f64(map, &TagId::Scale).filter(|scale| *scale != 0.0)?;
            compute_g_force_components(
                value.x as f64 / scale,
                value.y as f64 / scale,
                value.z as f64 / scale,
                map,
            )
        }
        TagValue::Vec_Vector3_f32(values) => values.get().last().and_then(|value| {
            compute_g_force_components(value.x as f64, value.y as f64, value.z as f64, map)
        }),
        TagValue::Vec_Vector3_f64(values) => values
            .get()
            .last()
            .and_then(|value| compute_g_force_components(value.x, value.y, value.z, map)),
        TagValue::Vec_TimeVector3_f32(values) => values.get().last().and_then(|value| {
            compute_g_force_components(value.x as f64, value.y as f64, value.z as f64, map)
        }),
        TagValue::Vec_TimeVector3_f64(values) => values
            .get()
            .last()
            .and_then(|value| compute_g_force_components(value.x, value.y, value.z, map)),
        _ => None,
    }
}

/// Converts acceleration vectors into dynamic load relative to resting gravity.
fn compute_g_force_components(x: f64, y: f64, z: f64, map: &TagMap) -> Option<f64> {
    let unit_factor = match extract_tag_string(map, &TagId::Unit).as_deref() {
        Some("m/s\u{00b2}") | Some("m/s^2") | Some("m/s2") => 1.0 / 9.80665,
        _ => 1.0,
    };
    g_force_from_components(x * unit_factor, y * unit_factor, z * unit_factor)
}

/// Computes a per-vector IMU sample timestamp using the accelerometer's STMP.
fn imu_sample_timestamp_ms(sample: &SampleInfo, index: usize, count: usize, stmp_us: Option<u64>) -> f64 {
    if let Some(stmp) = stmp_us {
        let base_ms = stmp as f64 / 1000.0;
        if index == 0 || count <= 1 {
            base_ms
        } else {
            base_ms + sample.duration_ms * index as f64 / count as f64
        }
    } else {
        sub_sample_timestamp_ms(sample, index, count)
    }
}

use super::unix_to_rfc3339;
