//! Elevation profile geometry: sample reduction, simplification and prepared paths.
use crate::series::simplify_rdp_indices;
use crate::widgets::elevation::ValidatedElevationPlot;
use crate::{WidgetError, WidgetResult};
use skia_safe::{Path, PathBuilder, Rect};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryKey {
    pub width: f32,
    pub height: f32,
    y_scale: f32,
    tolerance: f32,
    density: f32,
}

impl GeometryKey {
    pub fn new(config: &ValidatedElevationPlot, scale: f32) -> Self {
        Self {
            width: config.width * scale,
            height: config.height * scale,
            y_scale: config.y_scale,
            tolerance: config.simplify_tolerance_px,
            density: config.target_density,
        }
    }
}

#[derive(Debug)]
pub struct ElevationGeometry {
    pub key: GeometryKey,
    pub points: Vec<(f32, f32)>,
    pub progress_values: Vec<f32>,
    pub elapsed_fractions: Vec<f32>,
    pub elevation_data_range: Option<(f64, f64)>,
    pub source_point_count: usize,
    pub simplification: String,
    pub point_bounds: Rect,
    pub paths: Mutex<(Path, Path)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElevationFrameState {
    pub progress01: f32,
    pub marker_x: f32,
    pub marker_y: f32,
    pub elevation_m: Option<f64>,
    pub frame_elapsed_fraction: f32,
}

impl ElevationGeometry {
    pub fn new(
        config: &ValidatedElevationPlot,
        scale: f32,
        samples: &[(f32, f64, f32)],
    ) -> WidgetResult<Self> {
        if samples.is_empty() {
            return Err(WidgetError::Render(
                "Elevation plot requires elevation samples".into(),
            ));
        }
        let key = GeometryKey::new(config, scale);
        let count = (key.width * config.target_density).round().max(2.0) as usize;
        let reduced = downsample_elevation_points(samples, count.min(samples.len()));
        let min = samples.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max = samples
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let projected = project_elevation_points(
            &reduced,
            min,
            max,
            key.width,
            key.height,
            0.0,
            config.y_scale,
        );
        let simplified = simplify_elevation_samples(&projected, config.simplify_tolerance_px);
        Ok(Self::from_points(
            key,
            simplified.iter().map(|p| p.point).collect(),
            simplified.iter().map(|p| p.progress01).collect(),
            simplified.iter().map(|p| p.elapsed_fraction).collect(),
            Some((min, max)),
            samples.len(),
            format!(
                "sg11_density_{:.2}_rdp_px_{:.2}",
                config.target_density, config.simplify_tolerance_px
            ),
        ))
    }

    pub fn placeholder(config: &ValidatedElevationPlot, scale: f32) -> Self {
        let key = GeometryKey::new(config, scale);
        let (w, h) = (key.width, key.height);
        Self::from_points(
            key,
            vec![
                (8.0 * scale, h - 8.0 * scale),
                (w * 0.32, h * 0.55),
                (w * 0.62, h * 0.36),
                (w - 8.0 * scale, h * 0.48),
            ],
            vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0],
            None,
            0,
            "placeholder".into(),
        )
    }

    fn from_points(
        key: GeometryKey,
        points: Vec<(f32, f32)>,
        progress_values: Vec<f32>,
        elapsed_fractions: Vec<f32>,
        elevation_data_range: Option<(f64, f64)>,
        source_point_count: usize,
        simplification: String,
    ) -> Self {
        let first = points[0];
        let mut bounds = Rect::new(first.0, first.1, first.0, first.1);
        for &(x, y) in &points {
            bounds = Rect::new(
                bounds.left.min(x),
                bounds.top.min(y),
                bounds.right.max(x),
                bounds.bottom.max(y),
            );
        }
        let paths = Mutex::new((
            profile_path(&points, None),
            profile_path(&points, Some(key.height)),
        ));
        Self {
            key,
            points,
            progress_values,
            elapsed_fractions,
            elevation_data_range,
            source_point_count,
            simplification,
            point_bounds: bounds,
            paths,
        }
    }

    pub fn state(
        &self,
        progress01: f32,
        elevation_m: Option<f64>,
        frame_elapsed_fraction: f32,
    ) -> ElevationFrameState {
        let (_, (marker_x, geometry_y)) =
            point_at_progress(&self.points, &self.progress_values, progress01);
        let marker_y = self.elevation_data_range.zip(elevation_m).map_or(
            geometry_y,
            |((min, max), elevation)| {
                project_single_elevation_y(
                    elevation,
                    min,
                    max,
                    self.key.height,
                    0.0,
                    self.key.y_scale,
                )
            },
        );
        ElevationFrameState {
            progress01,
            marker_x,
            marker_y,
            elevation_m,
            frame_elapsed_fraction,
        }
    }

    pub fn completed_paths(&self, state: &ElevationFrameState) -> (Path, Path) {
        let points = build_elevation_completed_points(
            &self.points,
            &self.progress_values,
            &self.elapsed_fractions,
            state.progress01,
            state.frame_elapsed_fraction,
        );
        (
            profile_path(&points, None),
            profile_path(&points, Some(self.key.height)),
        )
    }
}

fn profile_path(points: &[(f32, f32)], baseline: Option<f32>) -> Path {
    let mut path = PathBuilder::new();
    if points.len() < 2 {
        return path.detach();
    }
    let first = points[0];
    path.move_to(baseline.map_or(first, |y| (first.0, y)));
    let start = if baseline.is_none() || baseline == Some(first.1) {
        1
    } else {
        0
    };
    for point in &points[start..] {
        path.line_to(*point);
    }
    if let Some(y) = baseline {
        path.line_to((points.last().expect("profile endpoint").0, y));
        path.close();
    }
    path.detach()
}

/// Binary lookup supports backward seeks and duplicate distance-progress runs.
fn point_at_progress(points: &[(f32, f32)], progress: &[f32], target: f32) -> (usize, (f32, f32)) {
    let last = points.len() - 1;
    if target <= progress[0] {
        return (1.min(last), points[0]);
    }
    if target >= progress[last] {
        return (last, points[last]);
    }
    let right = progress
        .partition_point(|value| *value < target)
        .clamp(1, last);
    let span = progress[right] - progress[right - 1];
    let mix = if span > 0.0 {
        (target - progress[right - 1]) / span
    } else {
        0.0
    };
    let (a, b) = (points[right - 1], points[right]);
    (right, (a.0 + (b.0 - a.0) * mix, a.1 + (b.1 - a.1) * mix))
}

/// A stationary climb fills by elapsed time while its x coordinate stays fixed.
pub fn build_elevation_completed_points(
    points: &[(f32, f32)],
    progress: &[f32],
    elapsed: &[f32],
    progress01: f32,
    fraction: f32,
) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let (index, hit) = point_at_progress(points, progress, progress01);
    let equal = |value: f32| (value - progress01).abs() <= 1e-6;
    let (mut start, mut end) = (index, index);
    if equal(progress[index]) {
        while start > 0 && equal(progress[start - 1]) {
            start -= 1;
        }
        while end + 1 < progress.len() && equal(progress[end + 1]) {
            end += 1;
        }
    }
    let mut completed = Vec::new();
    let endpoint = if end > start {
        completed.extend_from_slice(&points[..start]);
        for i in start..=end {
            if elapsed[i] < fraction {
                completed.push(points[i]);
            }
        }
        point_at_progress(&points[start..=end], &elapsed[start..=end], fraction).1
    } else {
        completed.extend_from_slice(&points[..index]);
        hit
    };
    if completed.is_empty() {
        completed.push(points[0]);
    }
    let last = completed.last().expect("completed profile start");
    if (last.0 - endpoint.0).hypot(last.1 - endpoint.1) > 1e-3 {
        completed.push(endpoint);
    }
    completed
}

// Elevation point reduction: smoothing, downsampling, projection, and
// Ramer-Douglas-Peucker simplification.
//
// Smooths using a Savitzky-Golay kernel, downsamples via even spacing, then
// applies RDP in screen space to produce geometry that preserves the visual
// profile shape with fewer points.

/// Elevation sample selected for geometry reduction.
#[derive(Clone, Copy)]
pub struct ReducedElevationPoint {
    pub(crate) progress01: f32,
    pub(crate) elevation: f64,
    pub(crate) elapsed_fraction: f32,
    pub(crate) preserve: bool,
}

/// Reduces dense elevation samples before screen-space projection.
pub fn downsample_elevation_points(
    points: &[(f32, f64, f32)],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count || target_count < 3 {
        return points
            .iter()
            .enumerate()
            .map(
                |(index, (progress01, elevation, elapsed_fraction))| ReducedElevationPoint {
                    progress01: *progress01,
                    elevation: *elevation,
                    elapsed_fraction: *elapsed_fraction,
                    preserve: index == 0 || index + 1 == points.len(),
                },
            )
            .collect();
    }

    let mut smoothed = smooth_elevation_points(points);
    mark_vertical_segments(&mut smoothed);
    select_evenly_spaced_elevation_points(&smoothed, target_count)
}

/// Marks preserve on consecutive same-progress runs with meaningful elevation span.
///
/// For each maximal run of points sharing the same `progress01` (within
/// `f32::EPSILON`), if `max_elevation - min_elevation >= 0.5m`, every point
/// in the run is marked `preserve = true`. This prevents the even-spacing
/// pass from collapsing real vertical flight segments (e.g., drone hover-climb)
/// into a single point.
const VERTICAL_THRESHOLD_M: f64 = 0.5;

fn mark_vertical_segments(points: &mut [ReducedElevationPoint]) {
    if points.is_empty() {
        return;
    }

    let mut run_start = 0;
    for i in 1..=points.len() {
        let same_progress = i < points.len()
            && (points[i].progress01 - points[run_start].progress01).abs() <= f32::EPSILON;

        if !same_progress {
            let run_end = i - 1;
            if run_end > run_start {
                let min_elev = points[run_start..=run_end]
                    .iter()
                    .map(|p| p.elevation)
                    .fold(f64::INFINITY, f64::min);
                let max_elev = points[run_start..=run_end]
                    .iter()
                    .map(|p| p.elevation)
                    .fold(f64::NEG_INFINITY, f64::max);
                if max_elev - min_elev >= VERTICAL_THRESHOLD_M {
                    for pt in &mut points[run_start..=run_end] {
                        pt.preserve = true;
                    }
                }
            }
            run_start = i;
        }
    }
}

/// Smooths elevation samples with a fixed Savitzky-Golay kernel.
///
/// Savitzky-Golay coefficients smooth noisy GPS elevation while preserving
/// overall profile shape better than a simple moving average.
fn smooth_elevation_points(points: &[(f32, f64, f32)]) -> Vec<ReducedElevationPoint> {
    const COEFFICIENTS: [f64; 11] = [
        -36.0, 9.0, 44.0, 69.0, 84.0, 89.0, 84.0, 69.0, 44.0, 9.0, -36.0,
    ];
    let radius = COEFFICIENTS.len() / 2;

    points
        .iter()
        .enumerate()
        .map(|(index, (progress01, elevation, elapsed_fraction))| {
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
                elapsed_fraction: *elapsed_fraction,
                preserve: index == 0 || index + 1 == points.len(),
            }
        })
        .collect()
}

/// Selects a stable set of evenly spaced smoothed elevation samples.
///
/// Preserved points (vertical segments) are never skipped — they are always
/// admitted even if their progress duplicates the previous selection.
fn select_evenly_spaced_elevation_points(
    points: &[ReducedElevationPoint],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count {
        return points.to_vec();
    }

    let mut selected_indexes = Vec::with_capacity(target_count);
    let last_index = points.len() - 1;
    for sample_index in 0..target_count {
        let source_index = ((sample_index as f64 * last_index as f64)
            / (target_count.saturating_sub(1).max(1) as f64))
            .round() as usize;
        selected_indexes.push(source_index.min(last_index));
    }

    selected_indexes.extend(
        points
            .iter()
            .enumerate()
            .filter_map(|(index, point)| point.preserve.then_some(index)),
    );
    selected_indexes.sort_unstable();
    selected_indexes.dedup();

    let mut selected = Vec::with_capacity(selected_indexes.len());
    for source_index in selected_indexes {
        if let Some(point) = points.get(source_index).copied() {
            if !point.preserve
                && selected
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
pub struct ElevationSample {
    pub point: (f32, f32),
    pub progress01: f32,
    pub elapsed_fraction: f32,
    pub preserve: bool,
}

/// Simplifies projected elevation samples while preserving protected endpoints.
pub fn simplify_elevation_samples(
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
pub fn simplify_elevation_samples_segment(
    points: &[ElevationSample],
    tolerance: f32,
) -> Vec<ElevationSample> {
    let tuples: Vec<(f32, f32)> = points.iter().map(|p| p.point).collect();
    let indices = simplify_rdp_indices(&tuples, tolerance);
    indices.iter().map(|&i| points[i]).collect()
}

/// Projects a single elevation value into widget-space y coordinate.
///
/// Uses the same formula as `project_elevation_points` but for a single
/// value — used by frame state to compute marker_y from elapsed-time
/// elevation data.
pub(crate) fn project_single_elevation_y(
    elevation_m: f64,
    min: f64,
    max: f64,
    height: f32,
    margin: f32,
    y_scale: f32,
) -> f32 {
    let span = (max - min).max(1e-9);
    let inner_height = (height * (1.0 - 2.0 * margin)).max(1.0);
    let normalized = ((elevation_m - min) / span) as f32;
    let centered = ((normalized - 0.5) * y_scale + 0.5).clamp(0.0, 1.0);
    height - (height * margin + inner_height * centered)
}

/// Projects reduced elevation samples into widget-space points.
///
/// Normalizes elevation into the plot's vertical range, then allows y_scale
/// to exaggerate or compress the profile around its centerline.
pub(crate) fn project_elevation_points(
    points: &[ReducedElevationPoint],
    min_elevation: f64,
    max_elevation: f64,
    width: f32,
    height: f32,
    margin: f32,
    y_scale: f32,
) -> Vec<ElevationSample> {
    let inner_width = (width * (1.0 - 2.0 * margin)).max(1.0);

    points
        .iter()
        .map(|point| {
            let point_x = width * margin + inner_width * point.progress01.clamp(0.0, 1.0);
            let point_y = project_single_elevation_y(
                point.elevation,
                min_elevation,
                max_elevation,
                height,
                margin,
                y_scale,
            );
            ElevationSample {
                point: (point_x, point_y),
                progress01: point.progress01,
                elapsed_fraction: point.elapsed_fraction,
                preserve: point.preserve,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        downsample_elevation_points, simplify_elevation_samples,
        simplify_elevation_samples_segment, ElevationGeometry, ElevationSample,
    };
    use crate::widgets::elevation::ValidatedElevationPlot;

    /// Builds a minimal `NormalizedElevationPlot` for testing.
    ///
    /// All dimensions and style values are arbitrary — the test exercises
    /// elapsed-fraction propagation, not rendering.
    fn minimal_plot() -> ValidatedElevationPlot {
        ValidatedElevationPlot {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 200.0,
            rotation: 0.0,
            y_scale: 1.0,
            simplify_tolerance_px: 0.0,
            target_density: 2.0,
            remaining_line_width: 1.0,
            remaining_line_color: "#ffffff".into(),
            remaining_line_opacity: 1.0,
            show_full_activity: true,
            starting_altitude_m: None,
            completed_line_width: 2.0,
            completed_line_color: "#00ff00".into(),
            completed_line_opacity: 1.0,
            area_remaining_color: "#000000".into(),
            area_remaining_opacity: 0.3,
            area_completed_color: "#00ff00".into(),
            area_completed_opacity: 0.5,
            area_gradient: false,
            marker_variant: crate::widgets::plot_marker::MarkerVariant::Single,
            marker_variant_diameter: 0.0,
            marker_size: 8.0,
            marker_color: "#ff0000".into(),
            marker_opacity: 1.0,
            show_elevation_metric: false,
            show_elevation_imperial: false,
            metric_label_offset_x: 0.0,
            metric_label_offset_y: 0.0,
            imperial_label_offset_x: 0.0,
            imperial_label_offset_y: 0.0,
            label_font: None,
            label_font_size: 14.0,
            label_color: "#ffffff".into(),
        }
    }

    #[test]
    fn geometry_has_elapsed_fractions_parallel_to_progress() {
        let plot = minimal_plot();
        // 10 points: linear progress 0..1, linear elevation 100..200,
        // elapsed fractions 0.0, 0.1, ..., 0.9
        let raw_points: Vec<(f32, f64, f32)> = (0..10)
            .map(|i| {
                let progress = i as f32 / 9.0;
                let elevation = 100.0 + i as f64 * 10.0;
                let elapsed = i as f32 / 9.0;
                (progress, elevation, elapsed)
            })
            .collect();

        let geometry = ElevationGeometry::new(&plot, 1.0, &raw_points).unwrap();

        assert_eq!(
            geometry.elapsed_fractions.len(),
            geometry.points.len(),
            "elapsed_fractions must be parallel to points"
        );
        assert_eq!(
            geometry.elapsed_fractions.len(),
            geometry.progress_values.len(),
            "elapsed_fractions must be parallel to progress_values"
        );
        // Elapsed fractions should be monotonically non-decreasing
        for window in geometry.elapsed_fractions.windows(2) {
            assert!(
                window[0] <= window[1] + f32::EPSILON,
                "elapsed_fractions must be monotonically non-decreasing, got {:?}",
                geometry.elapsed_fractions
            );
        }
    }

    #[test]
    fn geometry_elevation_data_range_matches_source() {
        let plot = minimal_plot();
        // Elevation ranges from 50.0 to 150.0
        let raw_points: Vec<(f32, f64, f32)> = (0..20)
            .map(|i| {
                let progress = i as f32 / 19.0;
                let elevation = 50.0 + i as f64 * (100.0 / 19.0);
                let elapsed = i as f32 / 19.0;
                (progress, elevation, elapsed)
            })
            .collect();

        let geometry = ElevationGeometry::new(&plot, 1.0, &raw_points).unwrap();

        let (min_elev, max_elev) = geometry.elevation_data_range.unwrap();
        assert!(
            (min_elev - 50.0).abs() < 1e-6,
            "min elevation should be 50.0, got {min_elev}"
        );
        assert!(
            (max_elev - 150.0).abs() < 1e-6,
            "max elevation should be 150.0, got {max_elev}"
        );
    }

    #[test]
    fn geometry_elapsed_fractions_bounded_0_1() {
        let plot = minimal_plot();
        // Elapsed fractions that span a full 0..1 range
        let raw_points: Vec<(f32, f64, f32)> = (0..15)
            .map(|i| {
                let progress = i as f32 / 14.0;
                let elevation = 1000.0 + i as f64;
                let elapsed = i as f32 / 14.0;
                (progress, elevation, elapsed)
            })
            .collect();

        let geometry = ElevationGeometry::new(&plot, 1.0, &raw_points).unwrap();

        for &frac in &geometry.elapsed_fractions {
            assert!(
                (-f32::EPSILON..=1.0 + f32::EPSILON).contains(&frac),
                "elapsed_fraction {frac} out of [0, 1] range"
            );
        }
    }

    fn sample(x: f32, y: f32, progress01: f32) -> ElevationSample {
        ElevationSample {
            point: (x, y),
            progress01,
            elapsed_fraction: progress01,
            preserve: false,
        }
    }

    #[test]
    fn segment_straight_line_keeps_endpoints() {
        let points = vec![sample(0.0, 0.0, 0.0), sample(100.0, 50.0, 1.0)];
        let simplified = simplify_elevation_samples_segment(&points, 0.001);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn segment_collinear_removes_middle() {
        let points = vec![
            sample(0.0, 0.0, 0.0),
            sample(50.0, 50.0, 0.5),
            sample(100.0, 100.0, 1.0),
        ];
        let simplified = simplify_elevation_samples_segment(&points, 0.001);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn segment_empty_returns_empty() {
        let points: Vec<ElevationSample> = vec![];
        let simplified = simplify_elevation_samples_segment(&points, 0.001);
        assert!(simplified.is_empty());
    }

    #[test]
    fn segment_single_point_returns_same() {
        let points = vec![sample(42.0, 42.0, 0.0)];
        let simplified = simplify_elevation_samples_segment(&points, 0.001);
        assert_eq!(simplified.len(), 1);
    }

    #[test]
    fn segment_preserves_peak() {
        let points = vec![
            sample(0.0, 0.0, 0.0),
            sample(50.0, 100.0, 0.5),
            sample(100.0, 0.0, 1.0),
        ];
        let simplified = simplify_elevation_samples_segment(&points, 0.001);
        assert_eq!(simplified.len(), 3);
        let simplified = simplify_elevation_samples_segment(&points, 101.0);
        assert_eq!(simplified.len(), 2);
    }

    #[test]
    fn preserves_flagged_points() {
        let points = vec![
            sample(0.0, 0.0, 0.0),
            sample(25.0, 10.0, 0.25),
            sample(50.0, 20.0, 0.5),
            sample(75.0, 30.0, 0.75),
            sample(100.0, 40.0, 1.0),
        ];
        let mut with_flag = points.clone();
        with_flag[2].preserve = true;
        with_flag[3].preserve = true;
        let simplified = simplify_elevation_samples(&with_flag, 0.001);
        for pt in &simplified {
            if pt.preserve {
                let found = with_flag.iter().any(|p| p.point == pt.point && p.preserve);
                assert!(found);
            }
        }
    }

    /// 10 consecutive same-progress samples spanning 5m — real vertical flight.
    /// Multiple points must survive downsampling.
    #[test]
    fn downsample_preserves_meaningful_vertical_run() {
        let progress = 0.0f32;
        let elevation_base = 100.0f64;
        let points: Vec<(f32, f64, f32)> = (0..10)
            .map(|i| (progress, elevation_base + i as f64 * 0.5, i as f32 / 9.0))
            .collect();

        let target_count = 5;
        let result = downsample_elevation_points(&points, target_count);

        assert!(
            result.len() >= 3,
            "meaningful vertical run should preserve multiple points, got {}",
            result.len()
        );

        for pt in &result {
            assert!(
                (pt.progress01 - progress).abs() <= f32::EPSILON,
                "all points in vertical run should have same progress"
            );
        }
    }

    /// 10 consecutive same-progress samples spanning 0.1m — barometric noise.
    /// Output should collapse to a single point (noise threshold < 0.5m).
    #[test]
    fn downsample_collapses_noise_vertical_run() {
        let progress = 0.5f32;
        let elevation_base = 100.0f64;
        let points: Vec<(f32, f64, f32)> = (0..10)
            .map(|i| {
                (
                    progress,
                    elevation_base + i as f64 * 0.01, // 0.09m span — below 0.5m threshold
                    i as f32 / 9.0,
                )
            })
            .collect();

        let target_count = 5;
        let result = downsample_elevation_points(&points, target_count);

        // All points share same progress and span < 0.5m — only 1 or 2 should survive.
        assert!(
            result.len() <= 2,
            "noise-level vertical run should collapse, got {} points",
            result.len()
        );
    }

    /// Alternating forward progress and vertical runs — correct points in each.
    ///
    /// Structure: 5 forward-progress points (0.0, 0.25, 0.5, 0.75, 1.0)
    /// interleaved with a 3-point meaningful vertical run at progress 0.5 (5m span).
    /// The vertical run should preserve multiple points while forward points
    /// progress normally.
    #[test]
    fn downsample_mixed_runs() {
        // Forward progress points with normal elevation change
        let mut points: Vec<(f32, f64, f32)> = vec![
            (0.0, 100.0, 0.0),
            (0.25, 110.0, 0.1),
            (0.5, 120.0, 0.2),
            (0.75, 130.0, 0.3),
            (1.0, 140.0, 0.4),
        ];

        // Insert a 3-point vertical run at progress 0.5 spanning 5m
        // (indices 2-4 in the final array, after the 0.25 forward point)
        let vertical_run = vec![
            (0.5, 120.0, 0.21),
            (0.5, 122.0, 0.22),
            (0.5, 125.0, 0.23), // 5m span from 120 to 125
            (0.5, 120.0, 0.24),
        ];
        // Insert after index 2 (the original 0.5 point)
        points.splice(3..3, vertical_run);

        let target_count = 10;
        let result = downsample_elevation_points(&points, target_count);

        // Forward-progress points (0.0, 0.25, 0.75, 1.0) must survive
        let progresses: Vec<f32> = result.iter().map(|p| p.progress01).collect();
        assert!(
            progresses.contains(&0.0),
            "forward point at 0.0 should survive"
        );
        assert!(
            progresses.contains(&0.25),
            "forward point at 0.25 should survive"
        );
        assert!(
            progresses.contains(&0.75),
            "forward point at 0.75 should survive"
        );
        assert!(
            progresses.contains(&1.0),
            "forward point at 1.0 should survive"
        );

        // Vertical run at 0.5 should have multiple points (5m span > 0.5m)
        let at_0_5: Vec<_> = result
            .iter()
            .filter(|p| (p.progress01 - 0.5).abs() <= f32::EPSILON)
            .collect();
        assert!(
            at_0_5.len() >= 2,
            "vertical run at 0.5 should preserve >=2 points, got {}",
            at_0_5.len()
        );
    }
}
