//! Telemetry-to-activity assembly.
//!
//! Takes the smoothed [`NativeSample`] vec and video metadata, then produces a
//! [`ParsedActivity`] with a single unified timeline. GPS timestamps anchor the
//! timeline when available; video FPS is the fallback. IMU and camera values
//! are picked from the closest-in-time sample to each anchor point via a
//! two-pointer walk over the sorted domain vectors.
//!
//! The haversine formula computes cumulative track distance, and from that
//! `sample_distance_progress` (0.0–1.0) is derived — both are required by the
//! elevation and route geometry backends.
//!
//! Fields not available from MP4 telemetry (heartrate, cadence, power, …) are
//! left as empty vectors so downstream interpolation treats them as absent.
//!
//! Owns: [`build_parsed_activity`].
//! Does not own: extraction, smoothing, or the raw telemetry-parser API.

use std::collections::BTreeMap;
use std::iter;

use serde_json::{json, Value};

use crate::activity::schema::{CourseSeries, ParsedActivity, TimeSeries};
use crate::media::native_sample::{NativeSample, TelemetrySeriesCounts};
use crate::media::telemetry_math::haversine_distance;

/// Builds a [`ParsedActivity`] from smoothed MP4 telemetry samples.
///
/// The unified timeline is the GPS sample timestamp when GPS data exists,
/// otherwise evenly spaced timestamps derived from `fps` and `duration_s`.
/// IMU and camera values at each anchor point come from the closest-in-time
/// sample in their respective domain.
pub fn build_parsed_activity(
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
) -> ParsedActivity {
    // ── Separate by domain ──────────────────────────────────────────
    let gps: Vec<&NativeSample> = samples.iter().filter(|s| s.has_gps_payload()).collect();
    let imu: Vec<&NativeSample> = samples.iter().filter(|s| s.g_force.is_some()).collect();
    let cam: Vec<&NativeSample> = samples.iter().filter(|s| s.has_camera_payload()).collect();
    let has_gps = !gps.is_empty();

    // ── Anchor timeline ─────────────────────────────────────────────
    let (anchor_ms, anchor_gps_idx): (Vec<f64>, Vec<Option<usize>>) = if has_gps {
        gps.iter()
            .enumerate()
            .map(|(i, s)| (s.timestamp_ms, Some(i)))
            .unzip()
    } else {
        let interval_ms = 1000.0 / fps.max(1.0);
        let max_t = duration_s * 1000.0;
        let count = (max_t / interval_ms).ceil() as usize;
        let timestamps: Vec<f64> = (0..count).map(|i| i as f64 * interval_ms).collect();
        (timestamps, iter::repeat(None).take(count).collect())
    };
    let n = anchor_ms.len();

    // ── Allocate series buffers ─────────────────────────────────────
    let mut latitude = vec![None; n];
    let mut longitude = vec![None; n];
    let mut altitude = vec![None; n];
    let mut speed = vec![None; n];
    let mut heading = vec![None; n];
    let mut time: TimeSeries = vec![None; n];
    let mut g_force = vec![None; n];
    let mut iso = vec![None; n];
    let mut aperture = vec![None; n];
    let mut shutter_speed = vec![None; n];
    let mut focal_length = vec![None; n];
    let mut ev = vec![None; n];
    let mut color_temperature = vec![None; n];

    // ── Fill GPS fields ─────────────────────────────────────────────
    // When GPS anchors are available the anchor index matches the GPS index
    // one-to-one; when the fallback is active every anchor gets the last GPS
    // value (or None if no GPS at all).
    let mut last_gps: Option<usize> = None;
    for (i, &gps_opt) in anchor_gps_idx.iter().enumerate() {
        if let Some(gi) = gps_opt {
            last_gps = Some(gi);
        }
        if let Some(gp) = last_gps.and_then(|gi| gps.get(gi)) {
            latitude[i] = gp.latitude;
            longitude[i] = gp.longitude;
            altitude[i] = gp.altitude;
            speed[i] = gp.speed;
            heading[i] = gp.heading;
            time[i] = gp.timestamp.clone();
        }
    }

    // ── Fill IMU fields (closest-in-time, two‑pointer walk) ────────
    let mut imu_idx = 0usize;
    for (i, &a_ms) in anchor_ms.iter().enumerate() {
        advance_to_closest(&imu, &mut imu_idx, a_ms);
        g_force[i] = imu.get(imu_idx).and_then(|s| s.g_force);
    }

    // ── Fill camera fields (closest-in-time, two‑pointer walk) ──────
    // Each camera metric holds its last known value forward independently:
    // a camera sample may have ISO but lack colour temperature.
    let mut last_iso: Option<f64> = None;
    let mut last_aperture: Option<f64> = None;
    let mut last_shutter: Option<f64> = None;
    let mut last_focal: Option<f64> = None;
    let mut last_ev: Option<f64> = None;
    let mut last_color_temp: Option<f64> = None;
    let mut cam_idx = 0usize;
    for (i, &a_ms) in anchor_ms.iter().enumerate() {
        advance_to_closest(&cam, &mut cam_idx, a_ms);
        if let Some(c) = cam.get(cam_idx) {
            last_iso = c.iso.or(last_iso);
            last_aperture = c.aperture.or(last_aperture);
            last_shutter = c.shutter_speed.or(last_shutter);
            last_focal = c.focal_length.or(last_focal);
            last_ev = c.ev.or(last_ev);
            last_color_temp = c.color_temperature.or(last_color_temp);
        }
        iso[i] = last_iso;
        aperture[i] = last_aperture;
        shutter_speed[i] = last_shutter;
        focal_length[i] = last_focal;
        ev[i] = last_ev;
        color_temperature[i] = last_color_temp;
    }

    // ── Compute derived fields ──────────────────────────────────────
    let sample_elapsed_seconds: Vec<f64> = anchor_ms.iter().map(|&t| t / 1000.0).collect();

    let (distance, distance_progress) = compute_distance_progress(&latitude, &longitude);
    let course_points: CourseSeries = latitude
        .iter()
        .zip(longitude.iter())
        .map(|(&lat, &lon)| (lat, lon))
        .collect();

    let source_start_time = sync_time.or_else(|| time.iter().find_map(|t| t.clone()));
    let end_s = sample_elapsed_seconds.last().copied().unwrap_or(duration_s);

    let metadata: Value = json!({
        "camera_type": camera_type,
        "camera_model": camera_model,
        "telemetry_source": telemetry_source,
        "timeline_kind": timeline_kind,
        "telemetry_sample_count": series_counts.total(),
        "gps_sample_count": series_counts.gps,
        "imu_sample_count": series_counts.imu,
        "camera_sample_count": series_counts.camera,
        "sample_count": n,
        "duration_seconds": end_s,
    });

    // ── Assemble ParsedActivity ─────────────────────────────────────
    ParsedActivity {
        file_name,
        file_format: Some("mp4_telemetry".into()),
        metadata,
        source_start_time,
        sample_elapsed_seconds,
        sample_distance_progress: distance_progress,
        frame_elapsed_seconds: vec![],   // always []
        frame_timestamps: vec![],        // always []
        frame_distance_progress: vec![], // always []
        trim_start_seconds: 0.0,
        trim_end_seconds: end_s,
        sample_course_points: course_points.clone(),
        sample_elevations: vec![], // geometry backend falls back to elevation
        course: course_points,
        elevation: altitude.clone(),
        speed: speed,
        distance: distance,
        heartrate: vec![],
        cadence: vec![],
        power: vec![],
        temperature: vec![],
        pace: vec![],
        g_force,
        air_pressure: vec![],
        ground_contact_time: vec![],
        left_right_balance: vec![],
        stride_length: vec![],
        stroke_rate: vec![],
        torque: vec![],
        vertical_speed: vec![],
        altitude: altitude,
        iso,
        aperture,
        shutter_speed,
        focal_length,
        ev,
        color_temperature,
        gear_position: vec![],
        vertical_ratio: vec![],
        vertical_oscillation: vec![],
        core_temperature: vec![],
        gradient: vec![],
        time,
        heading,
        extra: BTreeMap::new(),
    }
}

/// Advances `idx` through `candidates` so it points to the sample whose
/// `timestamp_ms` is closest to `target_ms`.
///
/// `candidates` must be sorted by `timestamp_ms` ascending. The two‑pointer
/// walk ensures O(n) total time across all anchor points instead of O(n²)
/// binary-search per anchor.
fn advance_to_closest(candidates: &[&NativeSample], idx: &mut usize, target_ms: f64) {
    while *idx + 1 < candidates.len()
        && (candidates[*idx + 1].timestamp_ms - target_ms).abs()
            < (candidates[*idx].timestamp_ms - target_ms).abs()
    {
        *idx += 1;
    }
}

/// Computes cumulative Haversine distance and 0.0–1.0 progress.
fn compute_distance_progress(
    lat: &[Option<f64>],
    lon: &[Option<f64>],
) -> (Vec<Option<f64>>, Vec<f64>) {
    let mut cum = 0.0f64;
    let distance: Vec<Option<f64>> = iter::once(Some(0.0))
        .chain((1..lat.len()).map(|i| {
            if let (Some(lat1), Some(lon1), Some(lat2), Some(lon2)) =
                (lat[i - 1], lon[i - 1], lat[i], lon[i])
            {
                cum += haversine_distance(lat1, lon1, lat2, lon2);
                Some(cum)
            } else {
                Some(cum) // carry forward when coords are missing
            }
        }))
        .collect();

    let total = distance.last().copied().flatten().unwrap_or(0.0);
    let progress: Vec<f64> = if total > 0.0 {
        distance.iter().map(|d| d.unwrap_or(0.0) / total).collect()
    } else {
        (0..lat.len()).map(|_| 0.0).collect()
    };

    (distance, progress)
}
