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
/// Filters out `None` values, then delegates to [`interpolate_points`].
pub fn interpolate_numeric_series_value(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    let points = collect_valid_numeric_points(x_values, y_values);
    interpolate_points(&points, target_x)
}

/// Linearly interpolates an optional numeric series at `target_x`.
///
/// Alias for [`interpolate_numeric_series_value`] — provided for
/// callers that use the "optional" naming convention.
pub fn interpolate_optional_numeric_series(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    interpolate_numeric_series_value(x_values, y_values, target_x)
}
