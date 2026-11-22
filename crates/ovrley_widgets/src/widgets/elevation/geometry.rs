//! Prepared profile vectors. Resizing reuses them until the gesture commits.
use super::{
    reduction::{
        downsample_elevation_points, project_elevation_points, project_single_elevation_y,
        simplify_elevation_samples,
    },
    validation::ValidatedElevationPlot,
};
use crate::error::{WidgetError, WidgetResult};
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
