/// Simplification and downsampling helpers for route polyline geometry.
///
/// Uses Ramer-Douglas-Peucker (shared `rdp` module) for shape-preserving
/// decimation and Largest-Triangle-Three-Buckets for density reduction
/// before simplification.
use super::super::types::RouteSample;
use crate::rdp::simplify_rdp_indices;

/// Simplifies route samples using Ramer-Douglas-Peucker.
pub(crate) fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    let tuples: Vec<(f32, f32)> = points.iter().map(|p| p.point).collect();
    let indices = simplify_rdp_indices(&tuples, tolerance);
    indices.iter().map(|&i| points[i]).collect()
}

/// Reduces dense route samples with Largest-Triangle-Three-Buckets.
///
/// LTTB preserves the visible shape better than uniform sampling when
/// reducing dense route traces.
pub(crate) fn downsample_route_samples(
    points: &[RouteSample],
    target_count: usize,
) -> Vec<RouteSample> {
    if points.len() <= target_count || target_count < 3 {
        return points.to_vec();
    }

    let bucket_size = (points.len() - 2) as f64 / (target_count - 2) as f64;
    let mut sampled = Vec::with_capacity(target_count);
    let mut a = 0usize;
    sampled.push(points[a]);

    for bucket_index in 0..(target_count - 2) {
        let avg_start = ((bucket_index + 1) as f64 * bucket_size).floor() as usize + 1;
        let avg_end = ((bucket_index + 2) as f64 * bucket_size).floor() as usize + 1;
        let avg_range_end = avg_end.min(points.len());
        let avg_range_start = avg_start.min(avg_range_end.saturating_sub(1));

        let average = if avg_range_start < avg_range_end {
            let range = &points[avg_range_start..avg_range_end];
            let (sum_x, sum_y) = range.iter().fold((0.0f64, 0.0f64), |(sx, sy), sample| {
                (sx + sample.point.0 as f64, sy + sample.point.1 as f64)
            });
            let count = range.len() as f64;
            (sum_x / count, sum_y / count)
        } else {
            let fallback = points[points.len() - 1];
            (fallback.point.0 as f64, fallback.point.1 as f64)
        };

        let range_start = (bucket_index as f64 * bucket_size).floor() as usize + 1;
        let range_end = ((bucket_index + 1) as f64 * bucket_size).floor() as usize + 1;
        let candidate_start = range_start.min(points.len().saturating_sub(2));
        let candidate_end = range_end.min(points.len().saturating_sub(1));

        let point_a = points[a];
        let mut next_a = candidate_start;
        let mut max_area = -1.0f64;

        for (offset, point_b) in points[candidate_start..candidate_end.max(candidate_start + 1)]
            .iter()
            .enumerate()
        {
            let area = triangle_area(point_a.point, point_b.point, average);
            if area > max_area {
                max_area = area;
                next_a = candidate_start + offset;
            }
        }

        a = next_a;
        sampled.push(points[a]);
    }

    sampled.push(*points.last().unwrap_or(&points[0]));
    sampled
}

/// Computes triangle area used by Largest-Triangle-Three-Buckets downsampling.
fn triangle_area(point_a: (f32, f32), point_b: (f32, f32), point_c: (f64, f64)) -> f64 {
    (((point_a.0 as f64 - point_c.0) * (point_b.1 as f64 - point_a.1 as f64))
        - ((point_a.0 as f64 - point_b.0 as f64) * (point_c.1 - point_a.1 as f64)))
        .abs()
        * 0.5
}
