//! Original-density course/progress indices; window extraction runs only on preparation.
use super::geometry::RouteSample;
use crate::{
    error::{WidgetError, WidgetResult},
    interpolation::interpolate_points,
    model::scene::ScenePresentationConfig,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteWindow {
    pub start: f64,
    pub end: f64,
}

impl RouteWindow {
    pub fn from_scene(
        scene: &ScenePresentationConfig,
        show_full_activity: bool,
    ) -> WidgetResult<Option<Self>> {
        // Preview timing is host-supplied, using the native scene's canonical range flag.
        let active = scene
            .extra
            .get("custom_export_range_active")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    WidgetError::Config("custom_export_range_active must be boolean".into())
                })
            })
            .transpose()?
            .unwrap_or(false);
        if show_full_activity || !active {
            return Ok(None);
        }
        let read = |field| {
            scene
                .extra
                .get(field)
                .and_then(serde_json::Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or_else(|| WidgetError::Config(format!("{field}: required finite second")))
        };
        let start = read("start")?;
        let end = read("end")?;
        if start < 0.0 || end <= start {
            return Err(WidgetError::Config("invalid route export window".into()));
        }
        Ok(Some(Self { start, end }))
    }
}

pub struct RouteActivity {
    times: Vec<f64>,
    course: Vec<(Option<f64>, Option<f64>)>,
    latitude: Vec<(f64, f64)>,
    longitude: Vec<(f64, f64)>,
    progress: Vec<(f64, f64)>,
    full: Arc<Vec<RouteSample>>,
    pub end: f64,
}

impl RouteActivity {
    pub fn new(
        times: &[f64],
        course: &[(Option<f64>, Option<f64>)],
        progress: &[f64],
        end: f64,
    ) -> Self {
        let full = Arc::new(project_course_samples(course, progress));
        let column = |component: usize| {
            times
                .iter()
                .copied()
                .zip(course)
                .filter_map(|(time, p)| {
                    let value = if component == 0 { p.0 } else { p.1 };
                    value.filter(|v| v.is_finite()).map(|value| (time, value))
                })
                .collect()
        };
        Self {
            times: times.to_vec(),
            course: course.to_vec(),
            end,
            full,
            latitude: column(0),
            longitude: column(1),
            progress: times
                .iter()
                .copied()
                .zip(progress.iter().copied())
                .filter(|p| p.1.is_finite())
                .collect(),
        }
    }

    pub fn samples(&self, window: Option<RouteWindow>) -> Arc<Vec<RouteSample>> {
        let Some(window) = window else {
            return self.full.clone();
        };
        let start = self.times.partition_point(|value| *value <= window.start);
        let end = self
            .times
            .partition_point(|value| *value < window.end)
            .max(start);
        let coordinate = |points: &[(f64, f64)], second| {
            points
                .first()
                .zip(points.last())
                .filter(|(a, b)| second >= a.0 && second <= b.0)
                .and_then(|_| interpolate_points(points, second))
        };
        let boundary = |second| {
            (
                coordinate(&self.latitude, second),
                coordinate(&self.longitude, second),
            )
        };
        let mut course = Vec::with_capacity(end - start + 2);
        let mut progress = Vec::with_capacity(end - start + 2);
        course.push(boundary(window.start));
        progress.push(interpolate_points(&self.progress, window.start));
        for index in start..end {
            course.push(self.course.get(index).copied().unwrap_or((None, None)));
            progress.push(interpolate_points(&self.progress, self.times[index]));
        }
        course.push(boundary(window.end));
        progress.push(interpolate_points(&self.progress, window.end));
        let first = progress.iter().find_map(|value| *value);
        let last = progress.iter().rev().find_map(|value| *value);
        let span = first.zip(last).filter(|(a, b)| b > a);
        let normalized = progress
            .iter()
            .enumerate()
            .map(|(index, value)| {
                span.and_then(|(a, b)| value.map(|v| ((v - a) / (b - a)).clamp(0.0, 1.0)))
                    .unwrap_or(index as f64 / (progress.len() - 1) as f64)
            })
            .collect::<Vec<_>>();
        Arc::new(project_course_samples(&course, &normalized))
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let absolute = interpolate_points(&self.progress, second)
            .unwrap_or_else(|| (second / self.end.max(1e-9)).clamp(0.0, 1.0));
        if let Some(window) = window {
            if let Some((start, end)) = interpolate_points(&self.progress, window.start)
                .zip(interpolate_points(&self.progress, window.end))
                .filter(|(a, b)| b > a)
            {
                return ((absolute - start) / (end - start)).clamp(0.0, 1.0) as f32;
            }
            return ((second - window.start) / (window.end - window.start)).clamp(0.0, 1.0) as f32;
        }
        absolute.clamp(0.0, 1.0) as f32
    }
}

/// Native and preview share equirectangular projection and missing-GPS handling.
pub fn project_course_samples(
    course: &[(Option<f64>, Option<f64>)],
    progress: &[f64],
) -> Vec<RouteSample> {
    let valid = course
        .iter()
        .enumerate()
        .filter_map(|(index, (lat, lon))| {
            lat.zip(*lon)
                .filter(|(lat, lon)| lat.is_finite() && lon.is_finite())
                .map(|(lat, lon)| (index, lat, lon))
        })
        .collect::<Vec<_>>();
    if valid.is_empty() {
        return Vec::new();
    }
    let latitude = valid.iter().map(|p| p.1).sum::<f64>() / valid.len() as f64;
    valid
        .into_iter()
        .map(|(index, lat, lon)| RouteSample {
            point: (lon * latitude.to_radians().cos(), lat),
            progress01: progress
                .get(index)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or_else(|| index as f64 / course.len().saturating_sub(1).max(1) as f64)
                .clamp(0.0, 1.0) as f32,
        })
        .collect()
}
