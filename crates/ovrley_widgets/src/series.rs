//! Numeric series: interpolation over sampled values and headings, and Ramer-Douglas-Peucker
//! line simplification. Interpolation runs per frame and allocates nothing.

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
/// Uses `partition_point` for O(log n) lookup. Empty points return `None`;
/// targets outside the point range are clamped to endpoints.
pub fn interpolate_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    interpolate_points_with(points, target_x, |left, right, ratio| {
        left + (right - left) * ratio
    })
}

/// Interpolates headings along the shortest arc, bridging gaps and clamping edges.
pub fn interpolate_heading_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    interpolate_points_with(points, target_x, |left, right, ratio| {
        let delta = (right - left + 180.0).rem_euclid(360.0) - 180.0;
        left + delta * ratio
    })
    .map(|heading| heading.rem_euclid(360.0))
}

fn interpolate_points_with(
    points: &[(f64, f64)],
    target_x: f64,
    interpolate: impl Fn(f64, f64, f64) -> f64,
) -> Option<f64> {
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
            Some(interpolate(left_y, right_y, ratio))
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

/// Perpendicular distance from `point` to the line segment `start`→`end`.
///
/// Returns Euclidean distance if the segment has zero length.
fn perpendicular_distance_impl(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let (x0, y0) = point;
    let (x1, y1) = start;
    let (x2, y2) = end;
    let dx = x2 - x1;
    let dy = y2 - y1;
    if dx.abs() <= f64::EPSILON && dy.abs() <= f64::EPSILON {
        return ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();
    }
    (dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()
}

/// Perpendicular distance from `point` to the line segment `start`→`end`.
///
/// Returns Euclidean distance if the segment has zero length.
pub fn perpendicular_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    perpendicular_distance_impl(
        (point.0 as f64, point.1 as f64),
        (start.0 as f64, start.1 as f64),
        (end.0 as f64, end.1 as f64),
    ) as f32
}

/// Simplifies a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// Returns indices into the original `points` slice. The first and last
/// points are always preserved. `tolerance` is in the same units as the
/// input coordinates (typically pixels).
fn simplify_rdp_indices_impl(points: &[(f64, f64)], tolerance: f64) -> Vec<usize> {
    if points.len() <= 2 {
        return (0..points.len()).collect();
    }

    let first = points[0];
    let last = points[points.len() - 1];

    let mut max_distance = 0.0f64;
    let mut split_index = 0usize;

    for (i, &point) in points
        .iter()
        .enumerate()
        .skip(1)
        .take(points.len().saturating_sub(2))
    {
        let distance = perpendicular_distance_impl(point, first, last);
        if distance > max_distance {
            max_distance = distance;
            split_index = i;
        }
    }

    if max_distance <= tolerance {
        return vec![0, points.len() - 1];
    }

    let mut left = simplify_rdp_indices_impl(&points[..=split_index], tolerance);
    let right = simplify_rdp_indices_impl(&points[split_index..], tolerance);

    left.pop();
    left.extend(right.iter().map(|i| i + split_index));
    left
}

/// Simplifies a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// Returns indices into the original `points` slice. The first and last
/// points are always preserved. `tolerance` is in the same units as the
/// input coordinates (typically pixels).
pub fn simplify_rdp_indices(points: &[(f32, f32)], tolerance: f32) -> Vec<usize> {
    let points_f64 = points
        .iter()
        .map(|(x, y)| (*x as f64, *y as f64))
        .collect::<Vec<_>>();
    simplify_rdp_indices_impl(&points_f64, tolerance as f64)
}

/// Simplifies a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// Returns indices into the original `points` slice. The first and last
/// points are always preserved. `tolerance` is in the same units as the
/// input coordinates (typically pixels).
pub fn simplify_rdp_indices_f64(points: &[(f64, f64)], tolerance: f64) -> Vec<usize> {
    simplify_rdp_indices_impl(points, tolerance)
}
