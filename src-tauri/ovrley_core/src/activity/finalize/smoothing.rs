//! Optional smoothing primitives for finalized activity metric series.
//!
//! The shared finalizer applies these only when extraction opts in per metric.
//! MP4 telemetry pre-treatment also reuses the zero-phase moving average
//! helpers through its existing smoothing module re-exports.

use crate::activity::schema::NumericSeries;
use crate::media::telemetry_math::{finite_f64, round_f64};

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

/// Smooths heading as a time-aware circular EMA over unit vectors.
///
/// `window_seconds` is the EMA time constant. The per-sample alpha is derived
/// from the observed elapsed time, so the same configured smoothing horizon
/// behaves consistently at different sample rates. Missing samples reset the
/// filter so it never carries a pre-gap heading into a new run.
pub(crate) fn circular_ema(
    heading_series: &NumericSeries,
    elapsed_seconds: &[f64],
    window_seconds: f64,
) -> NumericSeries {
    let mut smoothed_series = Vec::with_capacity(heading_series.len());
    let mut smoothed_x = None;
    let mut smoothed_y = None;
    let mut previous_elapsed: Option<f64> = None;

    for (index, heading) in heading_series.iter().enumerate() {
        let Some(heading) = heading.and_then(finite_f64) else {
            smoothed_series.push(None);
            smoothed_x = None;
            smoothed_y = None;
            previous_elapsed = None;
            continue;
        };
        let radians = heading.to_radians();
        let next_x = radians.cos();
        let next_y = radians.sin();
        let alpha = previous_elapsed
            .map(|previous| {
                let elapsed = elapsed_seconds[index] - previous;
                if elapsed.is_finite()
                    && elapsed > 0.0
                    && window_seconds.is_finite()
                    && window_seconds > 0.0
                {
                    1.0 - (-elapsed / window_seconds).exp()
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);

        match (smoothed_x, smoothed_y) {
            (Some(x), Some(y)) => {
                smoothed_x = Some(alpha * next_x + (1.0 - alpha) * x);
                smoothed_y = Some(alpha * next_y + (1.0 - alpha) * y);
            }
            _ => {
                smoothed_x = Some(next_x);
                smoothed_y = Some(next_y);
            }
        }

        let smoothed_heading = smoothed_y.unwrap().atan2(smoothed_x.unwrap()).to_degrees();
        smoothed_series.push(round_f64((smoothed_heading + 360.0) % 360.0, 3));
        previous_elapsed = Some(elapsed_seconds[index]);
    }

    smoothed_series
}

#[cfg(test)]
mod tests {
    use super::circular_ema;

    #[test]
    fn time_aware_circular_ema_preserves_a_short_heading_peak() {
        let result = circular_ema(
            &vec![Some(0.0), Some(30.0), Some(60.0), Some(90.0), Some(90.0)],
            &[0.0, 1.0, 2.0, 3.0, 4.0],
            0.5,
        );

        assert!(result[3].expect("heading is present") > 75.0);
    }

    #[test]
    fn circular_ema_keeps_heading_wraparound_on_the_short_arc() {
        let result = circular_ema(
            &vec![Some(350.0), Some(355.0), Some(5.0), Some(10.0)],
            &[0.0, 1.0, 2.0, 3.0],
            0.5,
        );

        assert!(result
            .iter()
            .flatten()
            .all(|heading| *heading >= 340.0 || *heading <= 20.0));
    }
}
