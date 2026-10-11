//! Shared interpolation utilities.
//!
//! Owns: linear interpolation over numeric series, optional series,
//!        and general point-list interpolation.
//! Does not own: activity densification (that stays in `activity/interpolate`).
//!
//! Allowed dependencies: std.
//! Forbidden dependencies: config, activity, render, encode.
//!
//! ## Performance
//! The `interpolate_points` function uses `partition_point` (O(log n))
//! and is called per-frame during progress lookup. Avoid allocation
//! inside the interpolation function itself — callers should batch.

/// Collects (x, y) pairs where y is present.
pub fn collect_valid_numeric_points(x_values: &[f64], y_values: &[Option<f64>]) -> Vec<(f64, f64)> {
    x_values
        .iter()
        .copied()
        .zip(y_values.iter().copied())
        .filter_map(|(x, y)| y.map(|value| (x, value)))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingSamplePolicy {
    Bridge,
    Preserve,
}

/// Linearly interpolates y at `target_x` between two nearest points.
///
/// Uses `partition_point` for O(log n) lookup. Returns `None` if
/// `target_x` is outside the point range (clamped to endpoints).
pub fn interpolate_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    match points.len() {
        0 => None,
        1 => Some(points[0].1),
        _ => {
            if target_x <= points[0].0 {
                return Some(points[0].1);
            }
            let last = points.len() - 1;
            if target_x >= points[last].0 {
                return Some(points[last].1);
            }
            let right_index = points.partition_point(|(x, _)| *x < target_x);
            if right_index < points.len() && (points[right_index].0 - target_x).abs() <= 1e-9 {
                return Some(points[right_index].1);
            }
            let left_index = right_index.saturating_sub(1);
            let (left_x, left_y) = points[left_index];
            let (right_x, right_y) = points[right_index];
            if (right_x - left_x).abs() <= f64::EPSILON {
                return Some(right_y);
            }
            let ratio = (target_x - left_x) / (right_x - left_x);
            Some(left_y + (right_y - left_y) * ratio)
        }
    }
}

/// Linearly interpolates an optional numeric series at `target_x`.
///
/// `Bridge` filters out `None` values and delegates to [`interpolate_points`].
/// `Preserve` treats missing samples as zero endpoints without bridging
/// directly between the surrounding valid samples.
pub fn interpolate_numeric_series_value(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
    missing_sample_policy: MissingSamplePolicy,
) -> Option<f64> {
    if missing_sample_policy == MissingSamplePolicy::Preserve {
        if y_values.is_empty() {
            return None;
        }
        assert_eq!(
            x_values.len(),
            y_values.len(),
            "numeric series must align with its timestamp series"
        );
        let right = x_values.partition_point(|x| *x < target_x);
        if right < x_values.len() && (x_values[right] - target_x).abs() <= 1e-9 {
            return y_values[right];
        }
        if right == 0 {
            return y_values[0];
        }
        if right >= x_values.len() {
            return y_values[y_values.len() - 1];
        }
        let left = right - 1;
        let left_value = y_values[left].unwrap_or(0.0);
        let right_value = y_values[right].unwrap_or(0.0);
        return interpolate_points(
            &[(x_values[left], left_value), (x_values[right], right_value)],
            target_x,
        );
    }

    let points = collect_valid_numeric_points(x_values, y_values);
    interpolate_points(&points, target_x)
}
