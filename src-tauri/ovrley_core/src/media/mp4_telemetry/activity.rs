//! MP4 telemetry to activity-column alignment.
//!
//! This module keeps MP4-specific pre-finalizer behavior close to extraction:
//! GPS/IMU/camera streams have different cadences, continuous streams are
//! smoothed before culling, and discrete camera settings are step functions.
//! The output is [`ActivityColumns`], which the shared activity finalizer turns
//! into the canonical [`ParsedActivity`](crate::activity::schema::ParsedActivity).

use std::iter;

use serde_json::json;

use crate::activity::schema::{
    ActivityColumns, DirectMetricGapPolicy, RawActivityOptions, SmoothingOption,
};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};

/// Builds aligned activity columns from pre-smoothed MP4 telemetry samples.
///
/// GPS packet timestamps anchor the output timeline when GPS data exists,
/// including packets whose position was rejected. Video cadence covers any
/// prefix before GPS packets and the video end. IMU uses the latest reading at
/// or before each anchor; discrete camera settings hold their last known value.
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
    let imu: Vec<&NativeSample> = samples.iter().filter(|s| s.has_imu_payload()).collect();
    let cam: Vec<&NativeSample> = samples.iter().filter(|s| s.has_camera_payload()).collect();
    let has_gps = !gps.is_empty();
    let elapsed_origin_ms = samples
        .first()
        .map(|sample| sample.timestamp_ms)
        .unwrap_or(0.0);
    let interval_ms = 1000.0 / fps.max(1.0);
    let media_end_ms = if duration_s > 0.0 {
        elapsed_origin_ms + duration_s * 1000.0
    } else {
        samples
            .last()
            .map(|sample| sample.timestamp_ms)
            .unwrap_or(elapsed_origin_ms)
    };

    let (mut anchor_ms, mut anchor_gps_idx): (Vec<f64>, Vec<Option<usize>>) = if has_gps {
        let first_packet_ms = samples
            .iter()
            .find(|sample| sample.gps_time_anchor || sample.gps_coordinates().is_some())
            .map(|sample| sample.timestamp_ms.min(media_end_ms))
            .unwrap_or(media_end_ms);
        let prefix_count = ((first_packet_ms - elapsed_origin_ms) / interval_ms).ceil() as usize;
        let prefix =
            (0..prefix_count).map(|index| (elapsed_origin_ms + index as f64 * interval_ms, None));
        let mut gps_index = 0;
        prefix
            .chain(
                samples
                    .iter()
                    .filter(|sample| {
                        (sample.gps_time_anchor || sample.gps_coordinates().is_some())
                            && sample.timestamp_ms <= media_end_ms
                    })
                    .map(|sample| {
                        let index = sample.gps_coordinates().map(|_| {
                            let index = gps_index;
                            gps_index += 1;
                            index
                        });
                        (sample.timestamp_ms, index)
                    }),
            )
            .unzip()
    } else {
        let count = ((media_end_ms - elapsed_origin_ms) / interval_ms).ceil() as usize;
        let timestamps: Vec<f64> = (0..count)
            .map(|index| elapsed_origin_ms + index as f64 * interval_ms)
            .collect();
        (timestamps, iter::repeat(None).take(count).collect())
    };
    if anchor_ms.last().is_none_or(|last| *last < media_end_ms) {
        anchor_ms.push(media_end_ms);
        anchor_gps_idx.push(None);
    }
    let n = anchor_ms.len();

    let mut timestamp = vec![None; n];
    let mut latitude = vec![None; n];
    let mut longitude = vec![None; n];
    let mut elevation = vec![None; n];
    let mut speed = vec![None; n];
    let mut heading = vec![None; n];
    let mut g_force = vec![None; n];
    let mut g_force_x = vec![None; n];
    let mut g_force_y = vec![None; n];
    let mut g_force_z = vec![None; n];
    let mut iso = vec![None; n];
    let mut aperture = vec![None; n];
    let mut shutter_speed = vec![None; n];
    let mut focal_length = vec![None; n];
    let mut ev = vec![None; n];
    let mut color_temperature = vec![None; n];

    for (index, &gps_opt) in anchor_gps_idx.iter().enumerate() {
        if let Some(gps_sample) = gps_opt.and_then(|gps_index| gps.get(gps_index)) {
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
        while imu_idx < imu.len() && imu[imu_idx].timestamp_ms <= anchor {
            imu_idx += 1;
        }
        if let Some(sample) = imu_idx.checked_sub(1).and_then(|index| imu.get(index)) {
            g_force[index] = sample.g_force;
            g_force_x[index] = sample.g_force_x;
            g_force_y[index] = sample.g_force_y;
            g_force_z[index] = sample.g_force_z;
        }
    }

    let mut last_iso: Option<f64> = None;
    let mut last_aperture: Option<f64> = None;
    let mut last_shutter: Option<f64> = None;
    let mut last_focal: Option<f64> = None;
    let mut last_ev: Option<f64> = None;
    let mut last_color_temp: Option<f64> = None;
    let mut cam_idx = 0usize;
    for (index, &anchor) in anchor_ms.iter().enumerate() {
        while cam_idx < cam.len() && cam[cam_idx].timestamp_ms <= anchor {
            let camera_sample = cam[cam_idx];
            last_iso = camera_sample.iso.or(last_iso);
            last_aperture = camera_sample.aperture.or(last_aperture);
            last_shutter = camera_sample.shutter_speed.or(last_shutter);
            last_focal = camera_sample.focal_length.or(last_focal);
            last_ev = camera_sample.ev.or(last_ev);
            last_color_temp = camera_sample.color_temperature.or(last_color_temp);
            cam_idx += 1;
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
        .map(|timestamp_ms| Some((timestamp_ms - elapsed_origin_ms) / 1000.0))
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
            smoothing: [
                (
                    "pace".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "zero_phase_ma".to_string(),
                        window_seconds: 5.0,
                    },
                ),
                (
                    "heading".to_string(),
                    SmoothingOption {
                        enabled: true,
                        method: "circular_ema".to_string(),
                        window_seconds: 1.0,
                    },
                ),
            ]
            .into(),
        },
        preserve_direct_metric_gaps: DirectMetricGapPolicy {
            speed: true,
            heading: true,
        },
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
        engine_load: none(),
        temperature: none(),
        calories: none(),
        gradient: none(),
        pace: none(),
        distance: none(),
        distance_to_home: none(),
        g_force,
        g_force_x,
        g_force_y,
        g_force_z,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::finalize::finalize_activity_columns;

    #[test]
    fn invalid_gps_lead_in_keeps_time_and_independent_metrics() {
        let samples = vec![
            NativeSample {
                timestamp_ms: 1000.0,
                gps_time_anchor: true,
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 1000.0,
                g_force: Some(1.2),
                shutter_speed: Some(0.005),
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 2000.0,
                gps_time_anchor: true,
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 26_000.0,
                gps_time_anchor: true,
                latitude: Some(12.4198418),
                longitude: Some(76.6687983),
                speed: Some(5.823),
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 27_000.0,
                gps_time_anchor: true,
                latitude: Some(12.4198500),
                longitude: Some(76.6688000),
                speed: Some(5.9),
                ..NativeSample::default()
            },
        ];
        let columns = build_activity_columns(
            &samples,
            30.0,
            26.0,
            None,
            "GoPro",
            None,
            None,
            "telemetry_parser",
            "gps_anchored",
            TelemetrySeriesCounts {
                gps: 2,
                imu: 1,
                camera: 1,
            },
        );
        let activity = finalize_activity_columns(&columns, None)
            .unwrap()
            .parsed_activity;

        assert_eq!(activity.sample_elapsed_seconds, vec![0.0, 1.0, 25.0, 26.0]);
        assert_eq!(activity.metadata["duration_seconds"], 26.0);
        assert_eq!(activity.course[0], (None, None));
        assert_eq!(activity.g_force[0], Some(1.2));
        assert_eq!(activity.shutter_speed[0], Some(0.005));
        assert_eq!(activity.course[2], (Some(12.4198418), Some(76.6687983)));
        assert_eq!(
            crate::activity::interpolate::interpolate_course_value(
                &activity.sample_elapsed_seconds,
                &activity.course,
                0.0,
            ),
            (Some(12.4198418), Some(76.6687983)),
        );
    }

    #[test]
    fn mp4_columns_keep_existing_telemetry_and_leave_csv_metrics_absent() {
        let samples = vec![
            NativeSample {
                timestamp_ms: 80.0,
                latitude: Some(47.0),
                longitude: Some(8.0),
                speed: Some(5.0),
                ..NativeSample::default()
            },
            NativeSample {
                timestamp_ms: 1080.0,
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
