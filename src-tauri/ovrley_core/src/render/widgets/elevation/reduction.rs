/// Elevation point reduction: smoothing, downsampling, projection, and
/// Ramer-Douglas-Peucker simplification.
///
/// Smooths using a Savitzky-Golay kernel, downsamples via even spacing, then
/// applies RDP in screen space to produce geometry that preserves the visual
/// profile shape with fewer points.
use crate::rdp::simplify_rdp_indices;

/// Elevation sample selected for geometry reduction.
#[derive(Clone, Copy)]
pub(crate) struct ReducedElevationPoint {
    pub(crate) progress01: f32,
    pub(crate) elevation: f64,
    pub(crate) preserve: bool,
}

/// Reduces dense elevation samples before screen-space projection.
pub(crate) fn downsample_elevation_points(
    points: &[(f32, f64)],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count || target_count < 3 {
        return points
            .iter()
            .enumerate()
            .map(|(index, (progress01, elevation))| ReducedElevationPoint {
                progress01: *progress01,
                elevation: *elevation,
                preserve: index == 0 || index + 1 == points.len(),
            })
            .collect();
    }

    let smoothed = smooth_elevation_points(points);
    select_evenly_spaced_elevation_points(&smoothed, target_count)
}

/// Smooths elevation samples with a fixed Savitzky-Golay kernel.
///
/// Savitzky-Golay coefficients smooth noisy GPS elevation while preserving
/// overall profile shape better than a simple moving average.
fn smooth_elevation_points(points: &[(f32, f64)]) -> Vec<ReducedElevationPoint> {
    const COEFFICIENTS: [f64; 11] = [
        -36.0, 9.0, 44.0, 69.0, 84.0, 89.0, 84.0, 69.0, 44.0, 9.0, -36.0,
    ];
    let radius = COEFFICIENTS.len() / 2;

    points
        .iter()
        .enumerate()
        .map(|(index, (progress01, elevation))| {
            let mut total = 0.0f64;
            let mut coefficient_total = 0.0f64;

            for (offset, coefficient) in COEFFICIENTS.iter().enumerate() {
                let neighbor_index = index as isize + offset as isize - radius as isize;
                if neighbor_index < 0 || neighbor_index >= points.len() as isize {
                    continue;
                }
                let neighbor_value = points[neighbor_index as usize].1;
                if !neighbor_value.is_finite() {
                    continue;
                }
                total += neighbor_value * coefficient;
                coefficient_total += coefficient;
            }

            let smoothed_elevation = if coefficient_total.abs() <= f64::EPSILON {
                *elevation
            } else {
                total / coefficient_total
            };

            ReducedElevationPoint {
                progress01: *progress01,
                elevation: smoothed_elevation,
                preserve: index == 0 || index + 1 == points.len(),
            }
        })
        .collect()
}

/// Selects a stable set of evenly spaced smoothed elevation samples.
fn select_evenly_spaced_elevation_points(
    points: &[ReducedElevationPoint],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count {
        return points.to_vec();
    }

    let mut selected = Vec::with_capacity(target_count);
    let last_index = points.len() - 1;
    for sample_index in 0..target_count {
        let source_index = ((sample_index as f64 * last_index as f64)
            / (target_count.saturating_sub(1).max(1) as f64))
            .round() as usize;
        if let Some(point) = points.get(source_index.min(last_index)).copied() {
            if selected
                .last()
                .map(|last: &ReducedElevationPoint| {
                    (last.progress01 - point.progress01).abs() <= f32::EPSILON
                })
                .unwrap_or(false)
            {
                continue;
            }
            selected.push(point);
        }
    }

    if selected.len() == 1 && points.len() > 1 {
        selected.push(*points.last().unwrap());
    }

    selected
}

/// Projected elevation sample used during screen-space simplification.
#[derive(Clone, Copy)]
pub(crate) struct ElevationSample {
    pub(crate) point: (f32, f32),
    pub(crate) progress01: f32,
    pub(crate) preserve: bool,
}

/// Simplifies projected elevation samples while preserving protected endpoints.
pub(crate) fn simplify_elevation_samples(
    points: &[ElevationSample],
    tolerance: f32,
) -> Vec<ElevationSample> {
    if points.len() <= 2 || tolerance <= 0.0 {
        return points.to_vec();
    }

    let preserved_indexes = points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| point.preserve.then_some(index))
        .collect::<Vec<_>>();
    if preserved_indexes.len() >= 2 {
        let mut result = Vec::new();
        for window in preserved_indexes.windows(2) {
            let start = window[0];
            let end = window[1];
            let simplified_segment =
                simplify_elevation_samples_segment(&points[start..=end], tolerance);
            if result.is_empty() {
                result.extend(simplified_segment);
            } else {
                result.extend(simplified_segment.into_iter().skip(1));
            }
        }
        return result;
    }

    simplify_elevation_samples_segment(points, tolerance)
}

/// Runs RDP simplification over one continuous elevation segment.
pub(crate) fn simplify_elevation_samples_segment(
    points: &[ElevationSample],
    tolerance: f32,
) -> Vec<ElevationSample> {
    let tuples: Vec<(f32, f32)> = points.iter().map(|p| p.point).collect();
    let indices = simplify_rdp_indices(&tuples, tolerance);
    indices.iter().map(|&i| points[i]).collect()
}

/// Projects reduced elevation samples into widget-space points.
///
/// Normalizes elevation into the plot's vertical range, then allows y_scale
/// to exaggerate or compress the profile around its centerline.
pub(crate) fn project_elevation_points(
    points: &[ReducedElevationPoint],
    width: f32,
    height: f32,
    margin: f32,
    y_scale: f32,
) -> Vec<(f32, f32)> {
    let min_elevation = points
        .iter()
        .map(|point| point.elevation)
        .fold(f64::INFINITY, f64::min);
    let max_elevation = points
        .iter()
        .map(|point| point.elevation)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max_elevation - min_elevation).max(1e-9);
    let inner_width = (width * (1.0 - 2.0 * margin)).max(1.0);
    let inner_height = (height * (1.0 - 2.0 * margin)).max(1.0);

    points
        .iter()
        .map(|point| {
            let progress01 = point.progress01.clamp(0.0, 1.0);
            let normalized = ((point.elevation - min_elevation) / span) as f32;
            let centered = ((normalized - 0.5) * y_scale + 0.5).clamp(0.0, 1.0);
            let point_x = width * margin + inner_width * progress01;
            let point_y = height - (height * margin + inner_height * centered);
            (point_x, point_y)
        })
        .collect()
}
