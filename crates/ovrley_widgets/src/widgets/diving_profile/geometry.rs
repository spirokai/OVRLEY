//! Prepared depth vectors reuse elevation's smoothing/reduction; ceiling paths are static.
use super::{sampling::DivingSamples, validation::ValidatedDivingProfile};
use crate::widgets::elevation::{
    geometry::build_elevation_completed_points,
    reduction::{downsample_elevation_points, simplify_elevation_samples, ElevationSample},
};
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
    pub fn new(config: &ValidatedDivingProfile, scale: f32) -> Self {
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
pub struct DepthSegment {
    pub points: Vec<(f32, f32)>,
    pub progress: Vec<f32>,
}

#[derive(Debug)]
pub struct DivingPaths {
    pub remaining: Path,
    pub ceiling_area: Path,
    pub ceiling_line: Path,
}

#[derive(Debug)]
pub struct DivingGeometry {
    pub key: GeometryKey,
    pub segments: Vec<DepthSegment>,
    pub source_point_count: usize,
    pub paths: Mutex<DivingPaths>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DivingFrameState {
    pub progress01: f32,
    pub marker: Option<(f32, f32)>,
    pub depth_m: Option<f64>,
}

impl DivingGeometry {
    pub fn new(config: &ValidatedDivingProfile, scale: f32, samples: &DivingSamples) -> Self {
        let key = GeometryKey::new(config, scale);
        let max_depth = samples
            .depth
            .iter()
            .flatten()
            .map(|p| p.1)
            .chain(samples.ceiling.iter().flatten().map(|p| p.1))
            .fold(0.0, f64::max)
            .max(1e-9);
        // Zero stays at the water level even when the vertical exaggeration changes.
        let project_y = |depth: f64| {
            ((depth.max(0.0) / max_depth) as f32 * key.y_scale).clamp(0.0, 1.0) * key.height
        };
        let count = (key.width * config.target_density).round().max(2.0) as usize;
        let segments: Vec<_> = samples
            .depth
            .iter()
            .map(|run| {
                let reduced = downsample_elevation_points(run, count.min(run.len()));
                let projected: Vec<_> = reduced
                    .iter()
                    .map(|p| ElevationSample {
                        point: (p.progress01 * key.width, project_y(p.elevation)),
                        progress01: p.progress01,
                        elapsed_fraction: p.elapsed_fraction,
                        preserve: p.preserve,
                    })
                    .collect();
                let simplified =
                    simplify_elevation_samples(&projected, config.simplify_tolerance_px);
                DepthSegment {
                    points: simplified.iter().map(|p| p.point).collect(),
                    progress: simplified.iter().map(|p| p.progress01).collect(),
                }
            })
            .collect();
        let mut remaining = PathBuilder::new();
        for segment in &segments {
            append_line(&mut remaining, &segment.points);
        }
        let mut ceiling_area = PathBuilder::new();
        let mut ceiling_line = PathBuilder::new();
        for run in &samples.ceiling {
            // Split at surface runs: the water-level edge is never stroked.
            for pair in run.windows(2) {
                if pair[0].1 == 0.0 && pair[1].1 == 0.0 {
                    continue;
                }
                let (a, b) = (
                    (pair[0].0 * key.width, project_y(pair[0].1)),
                    (pair[1].0 * key.width, project_y(pair[1].1)),
                );
                ceiling_line.move_to(a).line_to(b);
            }
            if run.len() < 2 {
                continue;
            }
            ceiling_area.move_to((run[0].0 * key.width, 0.0));
            for &(x, depth) in run {
                ceiling_area.line_to((x * key.width, project_y(depth)));
            }
            ceiling_area
                .line_to((run.last().unwrap().0 * key.width, 0.0))
                .close();
        }
        Self {
            key,
            segments,
            source_point_count: samples.depth.iter().map(Vec::len).sum(),
            paths: Mutex::new(DivingPaths {
                remaining: remaining.detach(),
                ceiling_area: ceiling_area.detach(),
                ceiling_line: ceiling_line.detach(),
            }),
        }
    }

    pub fn placeholder(config: &ValidatedDivingProfile, scale: f32) -> Self {
        Self::new(
            config,
            scale,
            &DivingSamples {
                depth: vec![vec![
                    (0.0, 0.0, 0.0),
                    (0.05, 36.0, 0.05),
                    (0.20, 36.0, 0.20),
                    (0.25, 18.0, 0.25),
                    (0.45, 6.0, 0.45),
                    (0.75, 3.0, 0.75),
                    (1.0, 0.0, 1.0),
                ]],
                ceiling: vec![vec![
                    (0.06, 0.0),
                    (0.14, 12.0),
                    (0.25, 9.0),
                    (0.45, 3.0),
                    (0.65, 0.0),
                ]],
            },
        )
    }

    pub fn state(&self, progress01: f32, depth_m: Option<f64>) -> DivingFrameState {
        let marker = self.segments.iter().find_map(|s| {
            if progress01 < s.progress[0] || progress01 > *s.progress.last().unwrap() {
                return None;
            }
            let i = s.progress.partition_point(|p| *p < progress01);
            if i == 0 {
                return Some(s.points[0]);
            }
            let u = (progress01 - s.progress[i - 1]) / (s.progress[i] - s.progress[i - 1]);
            Some((
                s.points[i - 1].0 + (s.points[i].0 - s.points[i - 1].0) * u,
                s.points[i - 1].1 + (s.points[i].1 - s.points[i - 1].1) * u,
            ))
        });
        DivingFrameState {
            progress01,
            marker,
            depth_m,
        }
    }

    pub fn completed_path(&self, state: &DivingFrameState) -> Path {
        let mut path = PathBuilder::new();
        for s in &self.segments {
            if state.progress01 < s.progress[0] {
                break;
            }
            let points = build_elevation_completed_points(
                &s.points,
                &s.progress,
                &s.progress,
                state.progress01,
                state.progress01,
            );
            append_line(&mut path, &points);
        }
        path.detach()
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.key.width, self.key.height)
    }
}

fn append_line(path: &mut PathBuilder, points: &[(f32, f32)]) {
    if let Some(&first) = points.first() {
        path.move_to(first);
        for &point in &points[1..] {
            path.line_to(point);
        }
    }
}
