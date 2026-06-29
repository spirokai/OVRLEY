//! MP4 telemetry to activity-column alignment.
//!
//! This module keeps MP4-specific pre-finalizer behavior close to extraction:
//! GPS/IMU/camera streams have different cadences, continuous streams are
//! smoothed before culling, and discrete camera settings are step functions.
//! The output is [`ActivityColumns`], which the shared activity finalizer turns
//! into the canonical [`ParsedActivity`](crate::activity::schema::ParsedActivity).

use std::collections::BTreeMap;
use std::iter;

use serde_json::json;

use crate::activity::schema::{ActivityColumns, RawActivityOptions};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};
use crate::media::telemetry_math::finite_f64;

/// Builds aligned activity columns from pre-smoothed MP4 telemetry samples.
///
/// GPS timestamps anchor the output timeline when GPS data exists; otherwise
/// video FPS/duration provides a fallback timeline. IMU and camera values are
/// selected by closest timestamp. Discrete camera fields hold their last known
/// value independently because they represent step changes, not continuous
/// signals.
pub fn build_activity_columns(
    samples: &[NativeSample],
    fps: f64,
    duration_s: f64,
    file_name: Option<String>,
    camera_type: &str,
    camera_model: Option<String>,
    sync_time: Option<String>,
    telemetry_source: &str,
    timeline_kind: &str,
    series_counts: TelemetrySeriesCounts,
) -> ActivityColumns {
    let gps: Vec<&NativeSample> = samples
        .iter()
        .filter(|sample| {
            matches!(
                (
                    sample.latitude.and_then(finite_f64),
                    sample.longitude.and_then(finite_f64),
                ),
                (Some(latitude), Some(longitude)) if latitude != 0.0 || longitude != 0.0
            )
        })
        .collect();
    let imu: Vec<&NativeSample> = samples.iter().filter(|s| s.g_force.is_some()).collect();
    let cam: Vec<&NativeSample> = samples.iter().filter(|s| s.has_camera_payload()).collect();
    let has_gps = !gps.is_empty();

    let (anchor_ms, anchor_gps_idx): (Vec<f64>, Vec<Option<usize>>) = if has_gps {
        gps.iter()
            .enumerate()
            .map(|(index, sample)| (sample.timestamp_ms, Some(index)))
            .unzip()
    } else {
        let interval_ms = 1000.0 / fps.max(1.0);
        let max_t = duration_s * 1000.0;
        let count = (max_t / interval_ms).ceil() as usize;
        let timestamps: Vec<f64> = (0..count).map(|index| index as f64 * interval_ms).collect();
        (timestamps, iter::repeat(None).take(count).collect())
    };
    let n = anchor_ms.len();

    let mut timestamp = vec![None; n];
    let mut latitude = vec![None; n];
    let mut longitude = vec![None; n];
    let mut altitude = vec![None; n];
    let mut elevation = vec![None; n];
    let mut speed = vec![None; n];
    let mut heading = vec![None; n];
    let mut g_force = vec![None; n];
    let mut iso = vec![None; n];
    let mut aperture = vec![None; n];
    let mut shutter_speed = vec![None; n];
    let mut focal_length = vec![None; n];
    let mut ev = vec![None; n];
    let mut color_temperature = vec![None; n];

    let mut last_gps: Option<usize> = None;
    for (index, &gps_opt) in anchor_gps_idx.iter().enumerate() {
        if let Some(gps_index) = gps_opt {
            last_gps = Some(gps_index);
        }
        if let Some(gps_sample) = last_gps.and_then(|gps_index| gps.get(gps_index)) {
            latitude[index] = gps_sample.latitude;
            longitude[index] = gps_sample.longitude;
            altitude[index] = gps_sample.altitude;
            elevation[index] = gps_sample.altitude;
            speed[index] = gps_sample.speed;
            heading[index] = gps_sample.heading;
            timestamp[index] = gps_sample.timestamp.clone();
        }
    }

    let mut imu_idx = 0usize;
    for (index, &anchor) in anchor_ms.iter().enumerate() {
        advance_to_closest(&imu, &mut imu_idx, anchor);
        g_force[index] = imu.get(imu_idx).and_then(|sample| sample.g_force);
    }

    let mut last_iso: Option<f64> = None;
    let mut last_aperture: Option<f64> = None;
    let mut last_shutter: Option<f64> = None;
    let mut last_focal: Option<f64> = None;
    let mut last_ev: Option<f64> = None;
    let mut last_color_temp: Option<f64> = None;
    let mut cam_idx = 0usize;
    for (index, &anchor) in anchor_ms.iter().enumerate() {
        advance_to_closest(&cam, &mut cam_idx, anchor);
        if let Some(camera_sample) = cam.get(cam_idx) {
            last_iso = camera_sample.iso.or(last_iso);
            last_aperture = camera_sample.aperture.or(last_aperture);
            last_shutter = camera_sample.shutter_speed.or(last_shutter);
            last_focal = camera_sample.focal_length.or(last_focal);
            last_ev = camera_sample.ev.or(last_ev);
            last_color_temp = camera_sample.color_temperature.or(last_color_temp);
        }
        iso[index] = last_iso;
        aperture[index] = last_aperture;
        shutter_speed[index] = last_shutter;
        focal_length[index] = last_focal;
        ev[index] = last_ev;
        color_temperature[index] = last_color_temp;
    }

    let elapsed_seconds = anchor_ms
        .iter()
        .map(|timestamp_ms| Some(timestamp_ms / 1000.0))
        .collect();
    let none = || vec![None; n];
    let mut metadata = json!({
        "camera_type": camera_type,
        "camera_model": camera_model,
        "telemetry_source": telemetry_source,
        "timeline_kind": timeline_kind,
        "telemetry_sample_count": series_counts.total(),
        "gps_sample_count": series_counts.gps,
        "imu_sample_count": series_counts.imu,
        "camera_sample_count": series_counts.camera,
    });
    if let Some(sync_time) = sync_time {
        metadata["sync_time"] = json!(sync_time);
    }

    ActivityColumns {
        file_name: file_name.unwrap_or_default(),
        file_format: "mp4_telemetry".to_string(),
        metadata,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            smoothing: BTreeMap::new(),
        },
        timestamp,
        elapsed_seconds,
        latitude,
        longitude,
        elevation,
        altitude,
        speed,
        heading,
        heartrate: none(),
        cadence: none(),
        power: none(),
        temperature: none(),
        gradient: none(),
        pace: none(),
        distance: none(),
        g_force,
        vertical_speed: none(),
        torque: none(),
        stroke_rate: none(),
        stride_length: none(),
        vertical_oscillation: none(),
        ground_contact_time: none(),
        left_right_balance: none(),
        core_temperature: none(),
        air_pressure: none(),
        gear_position: none(),
        iso,
        aperture,
        shutter_speed,
        focal_length,
        ev,
        color_temperature,
        original_sample_count: samples.len(),
    }
}

fn advance_to_closest(candidates: &[&NativeSample], idx: &mut usize, target_ms: f64) {
    while *idx + 1 < candidates.len()
        && (candidates[*idx + 1].timestamp_ms - target_ms).abs()
            < (candidates[*idx].timestamp_ms - target_ms).abs()
    {
        *idx += 1;
    }
}
