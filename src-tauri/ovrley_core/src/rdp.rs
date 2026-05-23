//! Shared Ramer-Douglas-Peucker line simplification.
//!
//! Owns: the RDP algorithm and perpendicular distance calculation.
//! Does not own: route or elevation domain types.
//!
//! Allowed dependencies: std.
//! Forbidden dependencies: config, activity, render, encode, commands.
//!
//! ## Performance
//! Called once per widget build (not per-frame). O(n log n) worst case
//! with tolerance-based early termination.

/// Perpendicular distance from `point` to the line segment `start`→`end`.
///
/// Returns Euclidean distance if the segment has zero length.
pub fn perpendicular_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    let (x0, y0) = point;
    let (x1, y1) = start;
    let (x2, y2) = end;
    let dx = x2 - x1;
    let dy = y2 - y1;
    if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
        return ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();
    }
    (dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()
}

/// Simplifies a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// Returns indices into the original `points` slice. The first and last
/// points are always preserved. `tolerance` is in the same units as the
/// input coordinates (typically pixels).
pub fn simplify_rdp_indices(points: &[(f32, f32)], tolerance: f32) -> Vec<usize> {
    if points.len() <= 2 {
        return (0..points.len()).collect();
    }

    let first = points[0];
    let last = points[points.len() - 1];

    let mut max_distance = 0.0f32;
    let mut split_index = 0usize;

    for (i, &point) in points
        .iter()
        .enumerate()
        .skip(1)
        .take(points.len().saturating_sub(2))
    {
        let distance = perpendicular_distance(point, first, last);
        if distance > max_distance {
            max_distance = distance;
            split_index = i;
        }
    }

    if max_distance <= tolerance {
        return vec![0, points.len() - 1];
    }

    let mut left = simplify_rdp_indices(&points[..=split_index], tolerance);
    let right = simplify_rdp_indices(&points[split_index..], tolerance);

    left.pop();
    left.extend(right.iter().map(|i| i + split_index));
    left
}
