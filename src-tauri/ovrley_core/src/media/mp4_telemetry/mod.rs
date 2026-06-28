//! MP4 embedded telemetry extraction via telemetry-parser.
//!
//! Telemetry-parser reads the camera-native metadata track (GoPro GPMF, DJI
//! protobuf, Insta360 time-scalars, etc.) and exposes it as vendor-agnostic
//! tag maps in [`GroupId`]/[`TagId`] buckets. This module converts those maps
//! into a normalised columnar JSON payload that shares the same frontend
//! interpolation path as FIT/GPX/SRT imports. The full pipeline:
//!
//! 1. `extract_telemetry()` opens the file and runs telemetry-parser.
//! 2. `extract_native_samples()` converts tag maps into a [`NativeSample`] vec.
//! 3. `smooth_series()` applies zero-phase smoothing to GPS/IMU fields.
//! 4. `native_samples_to_series_json()` splits the flat vec into columnar
//!    GPS / IMU / camera series for frontend interpolation.
//!
//! Owns: `probe_video_metadata()`, `extract_telemetry()`.
//! Does not own: ffprobe binary discovery (see [`crate::encode::ffmpeg`]),
//!       activity parsing/finalization, rendering, or output encoding.
//!
//! # Sub-modules
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`extraction`] | Tag-map to [`NativeSample`] conversion, GPS/camera/IMU expansion |
//! | [`tags`] | Typed accessors for telemetry-parser `TagValue` variants |
//! | [`vendor`] | Vendor-specific camera metadata (GoPro, Insta360, DJI JSON) |
//! | [`smoothing`] | Zero-phase moving-average smoothing for continuous series |

mod extraction;
mod tags;
mod vendor;
mod smoothing;

use std::fs::File;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use telemetry_parser::tags_impl::{GroupId, TagId};
use telemetry_parser::util::SampleInfo;
use telemetry_parser::Input;

use crate::encode::fps::Fps;
use crate::error::{CoreError, CoreResult};
use crate::media::{dji_ac004, Resolution, SourceVideoMetadata};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};

use tags::extract_tag_u64;

/// GoPro GPMF stores the absolute GPS UTC timestamp under the fourcc `GPSU`.
const GOPRO_GPSU_TAG: u32 = 0x4750_5355;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Probes source-video metadata with telemetry-parser.
pub fn probe_video_metadata(file_path: &str) -> CoreResult<SourceVideoMetadata> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len() as usize;

    let mut stream = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let vm = telemetry_parser::util::get_video_metadata(&mut stream, file_size)
        .map_err(|error| CoreError::Encode(format!("telemetry-parser metadata error: {error}")))?;

    let (fps_num, fps_den) = rational_fps_parts(vm.fps);

    Ok(SourceVideoMetadata {
        path: file_path.to_string(),
        duration: positive_f64(vm.duration_s),
        fps: positive_f64(vm.fps),
        fps_num,
        fps_den,
        resolution: Some(Resolution {
            width: vm.width as u64,
            height: vm.height as u64,
        }),
        creation_time: None,
        sync_time: None,
        codec_name: None,
        codec_long_name: None,
        codec_profile: None,
        pix_fmt: None,
        bits_per_raw_sample: None,
        has_audio: false,
        container_format: None,
        rotation_degrees: Some(vm.rotation),
    })
}

/// Extracts MP4 embedded telemetry into the normalized raw-sample payload.
pub fn extract_telemetry(repo_root: &Path, file_path: &str) -> CoreResult<Option<Value>> {
    let path = Path::new(file_path);
    let file_size = std::fs::metadata(path)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len() as usize;

    let mut stream = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let input = Input::from_stream(&mut stream, file_size, path, |_| {}, cancel_flag)
        .map_err(|error| CoreError::Encode(format!("telemetry-parser parse error: {error}")))?;
    dump_raw_telemetry_parser_output(file_path, &input.samples);

    let camera_type = input.camera_type();
    let camera_model = input.camera_model().cloned();
    let samples = match input.samples {
        Some(samples) if !samples.is_empty() => samples,
        _ => return extract_dji_ac004_fallback(repo_root, path),
    };

    let has_gps_group = samples.iter().any(|s| {
        s.tag_map.as_ref().is_some_and(|m| m.contains_key(&GroupId::GPS))
    });

    let mut normalized = extraction::extract_native_samples(&samples);
    if normalized.is_empty() || (camera_type == "DJI" && !has_gps_group) {
        return extract_dji_ac004_fallback(repo_root, path);
    }
    smoothing::smooth_series(&mut normalized);

    let (series, series_counts) = native_samples_to_series_json(&normalized);

    Ok(Some(json!({
        "fileName": path.file_name().map(|name| name.to_string_lossy().to_string()),
        "fileFormat": "mp4_telemetry",
        "syncTime": extract_sync_time_from_samples(&samples),
        "metadata": {
            "camera_type": camera_type,
            "camera_model": camera_model,
            "telemetry_sample_count": series_counts.total(),
            "gps_sample_count": series_counts.gps,
            "imu_sample_count": series_counts.imu,
            "camera_sample_count": series_counts.camera,
        },
        "series": series,
    })))
}

/// Falls back to the repo-owned DJI AC004 decoder.
fn extract_dji_ac004_fallback(repo_root: &Path, path: &Path) -> CoreResult<Option<Value>> {
    let Some(telemetry) = dji_ac004::extract_from_video(repo_root, path)? else {
        return Ok(None);
    };

    let mut normalized: Vec<_> = telemetry
        .samples
        .iter()
        .map(|sample| NativeSample {
            timestamp_ms: sample.timestamp_ms,
            timestamp: Some(sample.timestamp.clone()),
            latitude: Some(sample.latitude),
            longitude: Some(sample.longitude),
            altitude: Some(sample.altitude),
            speed: Some(sample.speed),
            heading: sample.heading,
            g_force: sample.g_force,
            ..NativeSample::default()
        })
        .collect();

    if normalized.is_empty() {
        return Ok(None);
    }
    smoothing::smooth_series(&mut normalized);
    let (series, series_counts) = native_samples_to_series_json(&normalized);

    Ok(Some(json!({
        "fileName": path.file_name().map(|name| name.to_string_lossy().to_string()),
        "fileFormat": "mp4_telemetry",
        "syncTime": telemetry.sync_time,
        "metadata": {
            "camera_type": "DJI",
            "camera_model": telemetry.device_name,
            "telemetry_sample_count": series_counts.total(),
            "gps_sample_count": series_counts.gps,
            "imu_sample_count": series_counts.imu,
            "camera_sample_count": series_counts.camera,
            "dji_ac004_sample_rate_hz": telemetry.sample_rate_hz,
        },
        "series": series,
    })))
}

/// Converts floating FPS metadata into the shared rational representation.
pub fn rational_fps_parts(fps: f64) -> (Option<u32>, Option<u32>) {
    match Fps::from_f64_fallback(fps) {
        Ok(fps) => (Some(fps.num), Some(fps.den)),
        Err(_) => (None, None),
    }
}

// ---------------------------------------------------------------------------
// Columnar JSON serialization
// ---------------------------------------------------------------------------

/// Serializes normalized native samples into columnar cadence-domain series.
fn native_samples_to_series_json(samples: &[NativeSample]) -> (Value, TelemetrySeriesCounts) {
    let mut gps_time_ms = Vec::new();
    let mut gps_timestamp = Vec::new();
    let mut latitude = Vec::new();
    let mut longitude = Vec::new();
    let mut altitude = Vec::new();
    let mut elevation = Vec::new();
    let mut speed = Vec::new();
    let mut heading = Vec::new();

    let mut imu_time_ms = Vec::new();
    let mut g_force = Vec::new();

    let mut camera_time_ms = Vec::new();
    let mut iso = Vec::new();
    let mut aperture = Vec::new();
    let mut shutter_speed = Vec::new();
    let mut focal_length = Vec::new();
    let mut ev = Vec::new();
    let mut color_temperature = Vec::new();

    for sample in samples {
        if sample.has_gps_payload() {
            gps_time_ms.push(sample.timestamp_ms);
            gps_timestamp.push(sample.timestamp.clone());
            latitude.push(sample.latitude);
            longitude.push(sample.longitude);
            altitude.push(sample.altitude);
            elevation.push(sample.altitude);
            speed.push(sample.speed);
            heading.push(sample.heading);
        }

        if sample.g_force.is_some() {
            imu_time_ms.push(sample.timestamp_ms);
            g_force.push(sample.g_force);
        }

        if sample.has_camera_payload() {
            camera_time_ms.push(sample.timestamp_ms);
            iso.push(sample.iso);
            aperture.push(sample.aperture);
            shutter_speed.push(sample.shutter_speed);
            focal_length.push(sample.focal_length);
            ev.push(sample.ev);
            color_temperature.push(sample.color_temperature);
        }
    }

    let counts = TelemetrySeriesCounts {
        gps: gps_time_ms.len(),
        imu: imu_time_ms.len(),
        camera: camera_time_ms.len(),
    };

    (
        json!({
            "gps": {
                "timeMs": gps_time_ms,
                "timestamp": gps_timestamp,
                "latitude": latitude,
                "longitude": longitude,
                "altitude": altitude,
                "elevation": elevation,
                "speed": speed,
                "heading": heading,
            },
            "imu": {
                "timeMs": imu_time_ms,
                "gForce": g_force,
            },
            "camera": {
                "timeMs": camera_time_ms,
                "iso": iso,
                "aperture": aperture,
                "shutterSpeed": shutter_speed,
                "focalLength": focal_length,
                "ev": ev,
                "colorTemperature": color_temperature,
            },
        }),
        counts,
    )
}

// ---------------------------------------------------------------------------
// Sync time
// ---------------------------------------------------------------------------

/// Infers video start time from the first acquired GPS timestamp.
fn extract_sync_time_from_samples(samples: &[SampleInfo]) -> Option<String> {
    for sample in samples {
        let Some(tag_map) = &sample.tag_map else {
            continue;
        };
        let Some(gps_map) = tag_map.get(&GroupId::GPS) else {
            continue;
        };
        let Some(tag) = gps_map.get(&TagId::Data) else {
            continue;
        };

        match &tag.value {
            telemetry_parser::tags_impl::TagValue::Vec_GpsData(gps_values) => {
                let Some(gps) = gps_values.get().first().filter(|gps| gps.is_acquired) else {
                    continue;
                };
                return Some(unix_to_rfc3339(
                    gps.unix_timestamp - sample.timestamp_ms / 1000.0,
                ));
            }
            telemetry_parser::tags_impl::TagValue::Vec_Vec_i32(rows) if !rows.get().is_empty() => {
                if let Some(unix_ms) = extract_tag_u64(gps_map, &TagId::Unknown(GOPRO_GPSU_TAG)) {
                    if sample.timestamp_ms == 0.0 {
                        return Some(unix_millis_to_rfc3339(unix_ms));
                    }
                    return Some(unix_to_rfc3339(
                        unix_ms as f64 / 1000.0 - sample.timestamp_ms / 1000.0,
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Timestamp formatting
// ---------------------------------------------------------------------------

/// Converts GPS epoch timestamps into RFC 3339 format.
pub(crate) fn unix_to_rfc3339(unix_ts: f64) -> String {
    if !unix_ts.is_finite() {
        return String::new();
    }

    let secs = unix_ts.floor() as i64;
    let nanos = ((unix_ts - secs as f64) * 1_000_000_000.0)
        .round()
        .clamp(0.0, 999_999_999.0) as u32;

    Utc.timestamp_opt(secs, nanos)
        .single()
        .map(|datetime| datetime.to_rfc3339())
        .unwrap_or_default()
}

/// Converts millisecond epoch timestamps without floating-point.
fn unix_millis_to_rfc3339(unix_ms: u64) -> String {
    Utc.timestamp_millis_opt(unix_ms as i64)
        .single()
        .map(|datetime| datetime.to_rfc3339())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Debug output
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
fn dump_raw_telemetry_parser_output(file_path: &str, samples: &Option<Vec<SampleInfo>>) {
    let debug_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join("debug")
        .join("mp4telemetry");

    if let Err(error) = std::fs::create_dir_all(&debug_dir) {
        log::warn!("failed to create MP4 telemetry debug directory: {error}");
        return;
    }

    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let filename = format!("{stem}-telemetry-parser-raw.txt");
    let output_path = debug_dir.join(filename);

    if let Err(error) = std::fs::write(output_path, format!("{samples:#?}")) {
        log::warn!("failed to write MP4 telemetry parser debug output: {error}");
    }
}

#[cfg(not(debug_assertions))]
fn dump_raw_telemetry_parser_output(_file_path: &str, _samples: &Option<Vec<SampleInfo>>) {}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Treats non-positive probe values as absent.
fn positive_f64(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}
