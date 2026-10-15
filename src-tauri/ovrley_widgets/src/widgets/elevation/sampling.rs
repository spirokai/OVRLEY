//! Original-density indices, prepared once at activity ingress.
use crate::interpolation::interpolate_points;
use crate::widgets::route::sampling::RouteWindow;
use std::sync::Arc;

pub struct ElevationActivity {
    times: Vec<f64>,
    preferred: Vec<(f64, f64)>,
    elevation_range: Option<(f64, f64)>,
    preferred_values: Vec<Option<f64>>,
    profile: Vec<(f64, f64)>,
    progress: Vec<(f64, f64)>,
    full: Arc<Vec<(f32, f64, f32)>>,
    pub end: f64,
    duration: f64,
}

impl ElevationActivity {
    pub fn new(
        times: &[f64],
        profile: &[Option<f64>],
        preferred: &[Option<f64>],
        progress: &[f64],
        end: f64,
    ) -> Self {
        let numeric = |values: &[Option<f64>]| {
            times
                .iter()
                .copied()
                .zip(values)
                .filter_map(|(time, value)| {
                    value.filter(|v| v.is_finite()).map(|value| (time, value))
                })
                .collect::<Vec<_>>()
        };
        let profile = numeric(profile);
        let preferred_values = preferred.to_vec();
        let preferred = numeric(preferred);
        let elevation_range = preferred.iter().fold(None, |range, &(_, value)| {
            Some(range.map_or((value, value), |(min, max): (f64, f64)| {
                (min.min(value), max.max(value))
            }))
        });
        let duration = times.last().copied().unwrap_or(1.0).max(1e-9);
        let progress = times
            .iter()
            .copied()
            .enumerate()
            .map(|(i, time)| {
                (
                    time,
                    progress
                        .get(i)
                        .copied()
                        .filter(|v| v.is_finite())
                        .unwrap_or(i as f64 / times.len().saturating_sub(1).max(1) as f64),
                )
            })
            .collect::<Vec<_>>();
        let full = Arc::new(
            profile
                .iter()
                .map(|&(time, elevation)| {
                    (
                        interpolate_points(&progress, time)
                            .unwrap_or(0.0)
                            .clamp(0.0, 1.0) as f32,
                        elevation,
                        (time / duration).clamp(0.0, 1.0) as f32,
                    )
                })
                .collect(),
        );
        Self {
            times: times.to_vec(),
            preferred,
            elevation_range,
            preferred_values,
            profile,
            progress,
            full,
            end,
            duration,
        }
    }

    pub fn samples(&self, window: Option<RouteWindow>) -> Arc<Vec<(f32, f64, f32)>> {
        let Some(window) = window else {
            return self.full.clone();
        };
        let start = self.times.partition_point(|t| *t <= window.start);
        let end = self.times.partition_point(|t| *t < window.end).max(start);
        let mut times = Vec::with_capacity(end - start + 2);
        times.push(window.start);
        times.extend_from_slice(&self.times[start..end]);
        times.push(window.end);
        let first = interpolate_points(&self.progress, window.start);
        let last = interpolate_points(&self.progress, window.end);
        let span = first.zip(last).filter(|(a, b)| b > a);
        Arc::new(
            times
                .iter()
                .enumerate()
                .filter_map(|(i, &time)| {
                    let elevation = if i == 0 || i + 1 == times.len() {
                        interpolate_points(&self.preferred, time)
                    } else {
                        self.preferred_values
                            .get(start + i - 1)
                            .copied()
                            .flatten()
                            .filter(|v| v.is_finite())
                    }?;
                    let progress = span
                        .and_then(|(a, b)| {
                            interpolate_points(&self.progress, time).map(|v| (v - a) / (b - a))
                        })
                        .unwrap_or(i as f64 / (times.len() - 1) as f64);
                    Some((
                        progress.clamp(0.0, 1.0) as f32,
                        elevation,
                        ((time - window.start) / (window.end - window.start)).clamp(0.0, 1.0)
                            as f32,
                    ))
                })
                .collect(),
        )
    }

    pub fn progress_at(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        let absolute = interpolate_points(&self.progress, second).unwrap_or(second / self.duration);
        let relative = window.map_or(absolute, |window| {
            interpolate_points(&self.progress, window.start)
                .zip(interpolate_points(&self.progress, window.end))
                .filter(|(a, b)| b > a)
                .map_or(
                    (second - window.start) / (window.end - window.start),
                    |(a, b)| (absolute - a) / (b - a),
                )
        });
        relative.clamp(0.0, 1.0) as f32
    }

    pub fn elapsed_fraction(&self, second: f64, window: Option<RouteWindow>) -> f32 {
        window
            .map_or(second / self.duration, |window| {
                (second - window.start) / (window.end - window.start)
            })
            .clamp(0.0, 1.0) as f32
    }

    /// Native elevation interpolation bridges gaps and leaves outside coverage missing.
    pub fn elevation_at(&self, second: f64) -> Option<f64> {
        self.preferred
            .first()
            .zip(self.preferred.last())
            .filter(|(a, b)| second >= a.0 && second <= b.0)
            .and_then(|_| interpolate_points(&self.preferred, second))
    }

    /// The same progress, elevation and chronological fill drive both rendering hosts.
    pub fn frame_state(
        &self,
        geometry: &super::geometry::ElevationGeometry,
        second: f64,
        window: Option<RouteWindow>,
        elevation: Option<f64>,
    ) -> super::geometry::ElevationFrameState {
        geometry.state(
            self.progress_at(second, window),
            Some(elevation.unwrap_or(0.0)),
            self.elapsed_fraction(second, window),
        )
    }

    pub fn altitude_offset(&self, target: Option<f64>) -> f64 {
        target
            .zip(self.profile.first())
            .map_or(0.0, |(target, first)| target - first.1)
    }

    pub fn elevation_range(&self) -> Option<(f64, f64)> {
        self.elevation_range
    }
}

/// Selects barometric altitude when the source provides it, otherwise generic elevation.
pub fn preferred_elevation_series<'a>(
    barometric: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if barometric.iter().any(Option::is_some) {
        barometric
    } else {
        elevation
    }
}

/// Finalized profile samples are authoritative when present.
pub fn elevation_profile_series<'a>(
    profile: &'a [Option<f64>],
    elevation: &'a [Option<f64>],
) -> &'a [Option<f64>] {
    if profile.is_empty() {
        elevation
    } else {
        profile
    }
}
