//! Optional smoothing primitives for finalized activity metric series.
//!
//! The shared finalizer applies these only when extraction opts in per metric.
//! MP4 telemetry pre-treatment also reuses the zero-phase moving average
//! helpers through its existing smoothing module re-exports.

use crate::activity::schema::NumericSeries;
use crate::media::telemetry_math::{finite_f64, round_f64};

const CIRCULAR_EMA_ALPHA: f64 = 0.05;

/// A null-aware centered moving average for sparse telemetry series.
pub(crate) fn moving_average(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
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
            if let Some(value) = value.and_then(finite_f64) {
                sum += value;
                count += 1;
            }
        }

        result.push((count > 0).then_some(sum / count as f64));
    }
    result
}

/// Forward/backward (zero-phase) smoothing to avoid shifting events.
pub(crate) fn zero_phase_smooth(data: &[Option<f64>], window: usize) -> Vec<Option<f64>> {
    if window <= 1 || data.len() < 2 {
        return data.to_vec();
    }

    let forward = moving_average(data, window);
    let reversed: Vec<_> = forward.into_iter().rev().collect();
    let backward = moving_average(&reversed, window);
    backward.into_iter().rev().collect()
}

/// Converts a time horizon into a sample window using observed cadence.
pub(crate) fn smoothing_window_for_seconds(sample_timestamps_ms: &[f64], seconds: f64) -> usize {
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

/// Smooths heading as a circular EMA over unit vectors.
///
/// The smoothing factor is intentionally fixed to match the legacy frontend
/// helper. `window_seconds` is ignored by this method.
pub(crate) fn circular_ema(heading_series: &NumericSeries) -> NumericSeries {
    let mut smoothed_series = Vec::with_capacity(heading_series.len());
    let mut smoothed_x = None;
    let mut smoothed_y = None;

    for heading in heading_series {
        let Some(heading) = heading.and_then(finite_f64) else {
            smoothed_series.push(None);
            continue;
        };
        let radians = heading.to_radians();
        let next_x = radians.cos();
        let next_y = radians.sin();

        match (smoothed_x, smoothed_y) {
            (Some(x), Some(y)) => {
                smoothed_x = Some(CIRCULAR_EMA_ALPHA * next_x + (1.0 - CIRCULAR_EMA_ALPHA) * x);
                smoothed_y = Some(CIRCULAR_EMA_ALPHA * next_y + (1.0 - CIRCULAR_EMA_ALPHA) * y);
            }
            _ => {
                smoothed_x = Some(next_x);
                smoothed_y = Some(next_y);
            }
        }

        let smoothed_heading = smoothed_y.unwrap().atan2(smoothed_x.unwrap()).to_degrees();
        smoothed_series.push(round_f64((smoothed_heading + 360.0) % 360.0, 3));
    }

    smoothed_series
}
