//! MP4 telemetry to activity-column alignment.
//!
//! This module keeps MP4-specific pre-finalizer behavior close to extraction:
//! GPS/IMU/camera streams have different cadences, continuous streams are
//! smoothed before culling, and discrete camera settings are step functions.
//! The output is [`ActivityColumns`], which the shared activity finalizer turns
//! into the canonical [`ParsedActivity`](crate::activity::schema::ParsedActivity).

use std::iter;

use serde_json::json;

use crate::activity::schema::{ActivityColumns, RawActivityOptions, SmoothingOption};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};

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
        .filter(|sample| sample.gps_coordinates().is_some())
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
    let metadata = json!({
        "camera_type": camera_type,
        "camera_model": camera_model,
        "telemetry_source": telemetry_source,
        "timeline_kind": timeline_kind,
        "telemetry_sample_count": series_counts.total(),
        "gps_sample_count": series_counts.gps,
        "imu_sample_count": series_counts.imu,
        "camera_sample_count": series_counts.camera,
    });
    ActivityColumns {
        file_name: file_name.unwrap_or_default(),
        file_format: "mp4_telemetry".to_string(),
        metadata,
        sync_time,
        options: RawActivityOptions {
            skip_idle_gap_fill: true,
            smoothing: [(
                "heading".to_string(),
                SmoothingOption {
                    enabled: true,
                    method: "circular_ema".to_string(),
                    window_seconds: 0.0,
                },
            )]
            .into(),
        },
        preserve_direct_metric_gaps: Default::default(),
        timestamp,
        elapsed_seconds,
        latitude,
        longitude,
        elevation,
        barometric_altitude: none(),
        speed,
        heading,
        heartrate: none(),
        cadence: none(),
        power: none(),
        engine_power: none(),
        temperature: none(),
        calories: none(),
        gradient: none(),
        pace: none(),
        distance: none(),
        distance_to_home: none(),
        g_force,
        g_force_x: none(),
        g_force_y: none(),
        g_force_z: none(),
        rpm: none(),
        throttle_position: none(),
        brake_position: none(),
        lean_angle: none(),
        vertical_speed: none(),
        torque: none(),
        stroke_rate: none(),
        stride_length: none(),
        vertical_oscillation: none(),
        ground_contact_time: none(),
        left_right_balance: none(),
        core_temperature: none(),
        air_pressure: none(),
        gear_position: vec![None; n],
        iso,
        aperture,
        shutter_speed,
        focal_length,
        ev,
        color_temperature,
        original_sample_count: samples.len(),
        include_original_sample_count_metadata: true,
        lap_number: vec![None; n],
        lap_markers: crate::activity::schema::LapMarkers::None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::finalize::finalize_activity_columns;

    #[test]
    fn mp4_columns_keep_existing_telemetry_and_leave_csv_metrics_absent() {
        let samples = vec![
            NativeSample {
                timestamp_ms: 0.0,
                latitude: Some(47.0),
                longitude: Some(8.0),
                speed: Some(5.0),
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 1000.0,
                latitude: Some(47.0001),
                longitude: Some(8.0001),
                speed: Some(6.0),
                ..NativeSample::default()
            },
        ];
        let columns = build_activity_columns(
            &samples,
            30.0,
            1.0,
            Some("telemetry.mp4".to_string()),
            "GoPro",
            Some("HERO".to_string()),
            None,
            "telemetry_parser",
            "gps_anchored",
            TelemetrySeriesCounts {
                gps: 2,
                imu: 0,
                camera: 0,
            },
        );

        let activity = finalize_activity_columns(&columns, None)
            .unwrap()
            .parsed_activity;

        assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0]);
        assert_eq!(activity.speed, vec![Some(5.0), Some(6.0)]);
        assert_eq!(
            activity.course,
            vec![(Some(47.0), Some(8.0)), (Some(47.0001), Some(8.0001))]
        );
        for series in [
            &activity.g_force_x,
            &activity.g_force_y,
            &activity.g_force_z,
            &activity.rpm,
            &activity.throttle_position,
            &activity.brake_position,
            &activity.lean_angle,
        ] {
            assert!(series.is_empty());
        }
    }
}
