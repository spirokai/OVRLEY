//! Prepared route geometry: fitting, reduction and Catmull-Rom curve reconstruction.
//! Live gestures reuse these vectors without repeating the preparation pipeline.
use super::validation::ValidatedRoutePlot;
use crate::{
    error::{WidgetError, WidgetResult},
    rdp::simplify_rdp_indices_f64,
};
use skia_safe::{Path, PathBuilder, Rect};
use std::sync::Mutex;

const ROUTE_CURVE_FLATNESS_PX: f64 = 0.35;

#[derive(Clone, Copy, Debug)]
pub struct RouteSample {
    pub point: (f64, f64),
    pub progress01: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryKey {
    width: f32,
    height: f32,
    inset: f32,
    tolerance: f32,
    density: f32,
}

impl GeometryKey {
    pub fn new(config: &ValidatedRoutePlot) -> Self {
        let line_inset = config.remaining_line_width.max(config.completed_line_width) * 0.5;
        let marker_inset = config.marker_size.max(config.marker_variant_diameter * 0.5);
        Self {
            width: config.width,
            height: config.height,
            inset: (marker_inset.max(line_inset) + 1.0).min(config.width.min(config.height) * 0.45),
            tolerance: config.simplify_tolerance_px,
            density: config.target_density,
        }
    }
}

#[derive(Debug)]
pub struct RouteGeometry {
    pub key: GeometryKey,
    pub points: Vec<(f32, f32)>,
    pub progress_values: Vec<f32>,
    pub source_point_count: usize,
    pub simplification: String,
    // Skia paths are Send, not Sync. Borrow only while preparing a static layer.
    pub path: Mutex<Path>,
    pub point_bounds: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteFrameState {
    pub progress01: f32,
    pub marker_x: f32,
    pub marker_y: f32,
    pub segment_index: usize,
}

impl RouteGeometry {
    pub fn new(config: &ValidatedRoutePlot, samples: &[RouteSample]) -> WidgetResult<Self> {
        if samples.len() < 2 {
            return Err(WidgetError::Render(
                "Route plot requires at least two valid course points".into(),
            ));
        }
        let key = GeometryKey::new(config);
        let points = samples
            .iter()
            .map(|sample| sample.point)
            .collect::<Vec<_>>();
        let fitted = fit_points(&points, &key);
        let fitted_samples = samples
            .iter()
            .zip(fitted)
            .map(|(sample, point)| RouteSample {
                point,
                progress01: sample.progress01,
            })
            .collect::<Vec<_>>();
        let target_count = (config.width * config.target_density).round().max(2.0) as usize;
        let downsampled =
            downsample_route_samples(&fitted_samples, target_count.min(fitted_samples.len()));
        let simplified =
            simplify_route_samples(&downsampled, config.simplify_tolerance_px.max(0.05));
        let curved = curve_route_samples(&simplified, ROUTE_CURVE_FLATNESS_PX);
        Self::from_points(
            key,
            curved.iter().map(|sample| (sample.point.0 as f32, sample.point.1 as f32)).collect(),
            curved.iter().map(|sample| sample.progress01).collect(),
            samples.len(),
            format!("lttb_density_{:.2}_rdp_px_{:.2}_catmull_rom_flatness_px_{ROUTE_CURVE_FLATNESS_PX:.2}", config.target_density, config.simplify_tolerance_px),
        )
    }

    /// Existing preview shape when no external route telemetry is available.
    pub fn placeholder(config: &ValidatedRoutePlot) -> WidgetResult<Self> {
        Self::from_points(
            GeometryKey::new(config),
            [
                (0.12, 0.82),
                (0.30, 0.64),
                (0.46, 0.72),
                (0.64, 0.30),
                (0.84, 0.18),
            ]
            .map(|(x, y)| (x * config.width, y * config.height))
            .to_vec(),
            vec![0.0, 0.25, 0.5, 0.75, 1.0],
            0,
            "placeholder".into(),
        )
    }

    fn from_points(
        key: GeometryKey,
        points: Vec<(f32, f32)>,
        progress_values: Vec<f32>,
        source_point_count: usize,
        simplification: String,
    ) -> WidgetResult<Self> {
        let first = points[0];
        let mut path = PathBuilder::new();
        path.move_to(first);
        let mut bounds = Rect::new(first.0, first.1, first.0, first.1);
        for point in &points {
            path.line_to(*point);
            bounds = Rect::new(
                bounds.left.min(point.0),
                bounds.top.min(point.1),
                bounds.right.max(point.0),
                bounds.bottom.max(point.1),
            );
        }
        Ok(Self {
            key,
            points,
            progress_values,
            source_point_count,
            simplification,
            path: Mutex::new(path.detach()),
            point_bounds: bounds,
        })
    }

    pub fn scale_to(&self, config: &ValidatedRoutePlot) -> [f32; 2] {
        [
            config.width / self.key.width,
            config.height / self.key.height,
        ]
    }

    pub fn state(&self, progress01: f32) -> RouteFrameState {
        let target = progress01.clamp(0.0, 1.0);
        let index = self
            .progress_values
            .partition_point(|value| *value < target)
            .clamp(1, self.points.len() - 1);
        let left = self.progress_values[index - 1];
        let right = self.progress_values[index];
        let mix = if right > left {
            ((target - left) / (right - left)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let a = self.points[index - 1];
        let b = self.points[index];
        RouteFrameState {
            progress01: target,
            marker_x: a.0 + (b.0 - a.0) * mix,
            marker_y: a.1 + (b.1 - a.1) * mix,
            segment_index: index,
        }
    }

    pub fn completed_path(&self, state: &RouteFrameState) -> Path {
        let mut path = PathBuilder::new();
        path.move_to(self.points[0]);
        for point in &self.points[..state.segment_index] {
            path.line_to(*point);
        }
        path.line_to((state.marker_x, state.marker_y));
        path.detach()
    }
}

fn fit_points(points: &[(f64, f64)], key: &GeometryKey) -> Vec<(f64, f64)> {
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let (width, height, inset) = (key.width as f64, key.height as f64, key.inset as f64);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    let scale =
        ((width - inset * 2.0).max(1.0) / span_x).min((height - inset * 2.0).max(1.0) / span_y);
    let offset_x = (width - span_x * scale) / 2.0;
    let offset_y = (height - span_y * scale) / 2.0;
    points
        .iter()
        .map(|(x, y)| {
            (
                (x - min_x) * scale + offset_x,
                height - ((y - min_y) * scale + offset_y),
            )
        })
        .collect()
}

/// Simplifies route samples using Ramer-Douglas-Peucker.
pub(super) fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    let tuples: Vec<(f64, f64)> = points.iter().map(|p| p.point).collect();
    let indices = simplify_rdp_indices_f64(&tuples, tolerance as f64);
    indices.iter().map(|&i| points[i]).collect()
}

/// Reduces dense route samples with Largest-Triangle-Three-Buckets.
///
/// LTTB preserves the visible shape better than uniform sampling when
/// reducing dense route traces.
fn downsample_route_samples(points: &[RouteSample], target_count: usize) -> Vec<RouteSample> {
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
                (sx + sample.point.0, sy + sample.point.1)
            });
            let count = range.len() as f64;
            (sum_x / count, sum_y / count)
        } else {
            let fallback = points[points.len() - 1];
            fallback.point
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
fn triangle_area(point_a: (f64, f64), point_b: (f64, f64), point_c: (f64, f64)) -> f64 {
    (((point_a.0 - point_c.0) * (point_b.1 - point_a.1))
        - ((point_a.0 - point_b.0) * (point_c.1 - point_a.1)))
        .abs()
        * 0.5
}

const MAX_SUBDIVISION_DEPTH: u8 = 12;
const MIN_KNOT_INTERVAL: f64 = 1e-6;

#[derive(Clone, Copy)]
struct CubicSegment {
    start: (f64, f64),
    control1: (f64, f64),
    control2: (f64, f64),
    end: (f64, f64),
    start_fraction: f64,
    end_fraction: f64,
}

/// Reconstructs a smooth route while preserving every source sample and its
/// progress value.
fn curve_route_samples(samples: &[RouteSample], flatness_tolerance_px: f64) -> Vec<RouteSample> {
    if samples.len() < 3 {
        return samples.to_vec();
    }

    let mut curved = Vec::with_capacity(samples.len());
    curved.push(samples[0]);

    for segment_index in 0..samples.len() - 1 {
        let start = samples[segment_index];
        let end = samples[segment_index + 1];
        if start.point == end.point {
            curved.push(end);
            continue;
        }

        let previous = if segment_index == 0 {
            extrapolate_before(start.point, end.point)
        } else {
            samples[segment_index - 1].point
        };
        let following = if segment_index + 2 < samples.len() {
            samples[segment_index + 2].point
        } else {
            extrapolate_after(start.point, end.point)
        };
        let (control1, control2) =
            catmull_rom_controls(previous, start.point, end.point, following);
        let mut flattened = Vec::new();
        flatten_cubic(
            CubicSegment {
                start: start.point,
                control1,
                control2,
                end: end.point,
                start_fraction: 0.0,
                end_fraction: 1.0,
            },
            flatness_tolerance_px,
            0,
            &mut flattened,
        );
        curved.extend(
            flattened
                .into_iter()
                .map(|(point, segment_fraction)| RouteSample {
                    point,
                    progress01: start.progress01
                        + (end.progress01 - start.progress01) * segment_fraction as f32,
                }),
        );
    }

    curved
}

fn catmull_rom_controls(
    point0: (f64, f64),
    point1: (f64, f64),
    point2: (f64, f64),
    point3: (f64, f64),
) -> ((f64, f64), (f64, f64)) {
    let interval01 = knot_interval(point0, point1);
    let interval12 = knot_interval(point1, point2);
    let interval23 = knot_interval(point2, point3);
    let tangent1 = scale(
        add(
            subtract(
                divide(subtract(point1, point0), interval01),
                divide(subtract(point2, point0), interval01 + interval12),
            ),
            divide(subtract(point2, point1), interval12),
        ),
        interval12,
    );
    let tangent2 = scale(
        add(
            subtract(
                divide(subtract(point2, point1), interval12),
                divide(subtract(point3, point1), interval12 + interval23),
            ),
            divide(subtract(point3, point2), interval23),
        ),
        interval12,
    );

    (
        add(point1, scale(tangent1, 1.0 / 3.0)),
        subtract(point2, scale(tangent2, 1.0 / 3.0)),
    )
}

fn flatten_cubic(
    segment: CubicSegment,
    flatness_tolerance_px: f64,
    depth: u8,
    output: &mut Vec<((f64, f64), f64)>,
) {
    if depth == MAX_SUBDIVISION_DEPTH
        || cubic_is_flat_enough(
            segment.start,
            segment.control1,
            segment.control2,
            segment.end,
            flatness_tolerance_px,
        )
    {
        output.push((segment.end, segment.end_fraction));
        return;
    }

    let start_control_midpoint = midpoint(segment.start, segment.control1);
    let control_midpoint = midpoint(segment.control1, segment.control2);
    let control_end_midpoint = midpoint(segment.control2, segment.end);
    let left_control_midpoint = midpoint(start_control_midpoint, control_midpoint);
    let right_control_midpoint = midpoint(control_midpoint, control_end_midpoint);
    let curve_midpoint = midpoint(left_control_midpoint, right_control_midpoint);
    let middle_fraction = (segment.start_fraction + segment.end_fraction) * 0.5;

    flatten_cubic(
        CubicSegment {
            start: segment.start,
            control1: start_control_midpoint,
            control2: left_control_midpoint,
            end: curve_midpoint,
            start_fraction: segment.start_fraction,
            end_fraction: middle_fraction,
        },
        flatness_tolerance_px,
        depth + 1,
        output,
    );
    flatten_cubic(
        CubicSegment {
            start: curve_midpoint,
            control1: right_control_midpoint,
            control2: control_end_midpoint,
            end: segment.end,
            start_fraction: middle_fraction,
            end_fraction: segment.end_fraction,
        },
        flatness_tolerance_px,
        depth + 1,
        output,
    );
}

fn cubic_is_flat_enough(
    start: (f64, f64),
    control1: (f64, f64),
    control2: (f64, f64),
    end: (f64, f64),
    tolerance: f64,
) -> bool {
    point_to_line_distance(control1, start, end).max(point_to_line_distance(control2, start, end))
        <= tolerance
}

fn point_to_line_distance(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let line = subtract(end, start);
    let line_length = distance(start, end);
    if line_length == 0.0 {
        return distance(point, start);
    }
    ((point.0 - start.0) * line.1 - (point.1 - start.1) * line.0).abs() / line_length
}

fn knot_interval(start: (f64, f64), end: (f64, f64)) -> f64 {
    distance(start, end).sqrt().max(MIN_KNOT_INTERVAL)
}

fn distance(start: (f64, f64), end: (f64, f64)) -> f64 {
    let delta = subtract(end, start);
    (delta.0 * delta.0 + delta.1 * delta.1).sqrt()
}

fn extrapolate_before(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    (start.0 * 2.0 - end.0, start.1 * 2.0 - end.1)
}

fn extrapolate_after(start: (f64, f64), end: (f64, f64)) -> (f64, f64) {
    (end.0 * 2.0 - start.0, end.1 * 2.0 - start.1)
}

fn midpoint(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    ((left.0 + right.0) * 0.5, (left.1 + right.1) * 0.5)
}

fn add(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 + right.0, left.1 + right.1)
}

fn subtract(left: (f64, f64), right: (f64, f64)) -> (f64, f64) {
    (left.0 - right.0, left.1 - right.1)
}

fn scale(point: (f64, f64), factor: f64) -> (f64, f64) {
    (point.0 * factor, point.1 * factor)
}

fn divide(point: (f64, f64), divisor: f64) -> (f64, f64) {
    (point.0 / divisor, point.1 / divisor)
}
