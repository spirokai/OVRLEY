//! Noise-reduction smoothing for continuous telemetry series.
//!
//! GPS altitude, speed, heading, and IMU g-force are continuous physical
//! quantities that benefit from noise reduction. A centred moving average
//! applied forward and then backward (zero-phase) removes jitter without the
//! lag of a simple one-pole filter — important for map/course widgets where
//! lagging the rendered position behind the GPS trace is immediately visible.
//!
//! Camera settings (ISO, aperture, shutter, focal length, EV, colour
//! temperature) are excluded because they change discretely; smoothing those
//! would invent intermediate camera states that never occurred.
//!
//! Owns: [`smooth_series`], [`moving_average`], [`zero_phase_smooth`].
//! Does not own: sample extraction or JSON serialization.

use crate::media::native_sample::NativeSample;
use crate::media::telemetry_math::finite_f64;

const GPS_SPEED_SMOOTHING_SECONDS: f64 = 0.5;
const GPS_ALTITUDE_SMOOTHING_SECONDS: f64 = 1.0;
const GPS_HEADING_SMOOTHING_SECONDS: f64 = 0.5;
const G_FORCE_SMOOTHING_SECONDS: f64 = 1.0;

/// Smooths only continuous telemetry fields before JSON serialization.
///
/// GPS altitude/speed/heading and g-force benefit from noise reduction. Camera
/// settings are intentionally excluded because ISO, aperture, shutter, focal
/// length, exposure value, and color temperature can change discretely;
/// smoothing those would invent intermediate camera states.
pub(crate) fn smooth_series(samples: &mut [NativeSample]) {
    let timestamps: Vec<_> = samples.iter().map(|sample| sample.timestamp_ms).collect();
    let altitude: Vec<_> = samples.iter().map(|sample| sample.altitude).collect();
    let speed: Vec<_> = samples.iter().map(|sample| sample.speed).collect();
    let heading: Vec<_> = samples.iter().map(|sample| sample.heading).collect();
    let g_force: Vec<_> = samples.iter().map(|sample| sample.g_force).collect();

    let smoothed_altitude = zero_phase_smooth(
        &altitude,
        smoothing_window_for_seconds(&timestamps, GPS_ALTITUDE_SMOOTHING_SECONDS),
    );
    let smoothed_speed = zero_phase_smooth(
        &speed,
        smoothing_window_for_seconds(&timestamps, GPS_SPEED_SMOOTHING_SECONDS),
    );
    let smoothed_heading = zero_phase_smooth(
        &heading,
        smoothing_window_for_seconds(&timestamps, GPS_HEADING_SMOOTHING_SECONDS),
    );
    let smoothed_g_force = zero_phase_smooth(
        &g_force,
        smoothing_window_for_seconds(&timestamps, G_FORCE_SMOOTHING_SECONDS),
    );

    for index in 0..samples.len() {
        samples[index].altitude = smoothed_altitude[index];
        samples[index].speed = smoothed_speed[index];
        samples[index].heading = smoothed_heading[index];
        samples[index].g_force = smoothed_g_force[index];
    }
}

/// A null-aware centered moving average for sparse telemetry series.
fn moving_average(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window <= 1 || data.is_empty() {
        return data.to_vec();
    }

    let half = window / 2;
    let mut result = Vec::with_capacity(data.len());
    for index in 0..data.len() {
        if data[index].is_none() {
            result.push(None);
            continue;
        }

        let start = index.saturating_sub(half);
        let end = (index + half + 1).min(data.len());
        let mut sum = 0.0;
        let mut count = 0;

        for value in &data[start..end] {
            if let Some(value) = value {
                sum += value;
                count += 1;
            }
        }

        result.push((count > 0).then_some(sum / count as f64));
    }
    result
}

/// Forward/backward (zero-phase) smoothing to avoid shifting events.
fn zero_phase_smooth(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window <= 1 || data.len() < 2 {
        return data.to_vec();
    }

    let forward = moving_average(data, window);
    let reversed: Vec<_> = forward.into_iter().rev().collect();
    let backward = moving_average(&reversed, window);
    backward.into_iter().rev().collect()
}

/// Converts a time horizon into a sample window using observed cadence.
fn smoothing_window_for_seconds(sample_timestamps_ms: &[f64], seconds: f64) -> usize {
    if sample_timestamps_ms.len() < 2 || !seconds.is_finite() || seconds <= 0.0 {
        return 1;
    }

    let mut deltas_ms: Vec<_> = sample_timestamps_ms
        .windows(2)
        .filter_map(|pair| finite_f64(pair[1] - pair[0]))
        .filter(|delta| *delta > 0.0)
        .collect();
    if deltas_ms.is_empty() {
        return 1;
    }

    deltas_ms.sort_by(f64::total_cmp);
    let median_delta_ms = deltas_ms[deltas_ms.len() / 2];
    if median_delta_ms <= 0.0 {
        return 1;
    }

    ((seconds * 1000.0) / median_delta_ms).round().max(1.0) as usize
}
