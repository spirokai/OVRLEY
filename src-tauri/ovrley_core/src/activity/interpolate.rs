//! Interpolation and densification for activity telemetry.
//!
//! Source activities usually contain unevenly spaced samples, while the renderer
//! needs values at exact frame times. This module converts trimmed samples into
//! frame-aligned series using linear interpolation and conservative edge
//! clamping. Missing-value behavior is selected per metric: bridge policies
//! filter gaps, while preserve policies retain missing frame samples.

use super::schema::{
    CourseSeries, DenseActivityReport, DenseSeriesReport, NumericSeries, TimeSeries,
    TrimmedActivity,
};
use crate::normalize::RenderDataRequirements;
use chrono::{DateTime, SecondsFormat, Utc};
use ovrley_widgets::model::telemetry::{ActivitySeries, InterpolationKind, NumericActivity};

use ovrley_widgets::interpolation::{
    collect_valid_numeric_points, interpolate_heading_points, interpolate_numeric_series_value,
    interpolate_points, MissingSamplePolicy,
};

#[derive(Clone, Copy)]
pub(crate) enum InterpolationStrategy {
    Angular,
    Hold,
    Numeric(MissingSamplePolicy),
}

pub(crate) fn interpolation_strategy(series: ActivitySeries) -> InterpolationStrategy {
    match series.interpolation() {
        InterpolationKind::Angular => InterpolationStrategy::Angular,
        InterpolationKind::Hold => InterpolationStrategy::Hold,
        InterpolationKind::Preserve => {
            InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve)
        }
        InterpolationKind::Linear => InterpolationStrategy::Numeric(MissingSamplePolicy::Bridge),
    }
}

struct NumericDensification<'a> {
    trimmed: &'a TrimmedActivity,
    requirements: &'a RenderDataRequirements,
    frames: &'a [f64],
}

impl NumericDensification<'_> {
    fn series(&self, series: ActivitySeries) -> NumericSeries {
        let values = self.trimmed.numeric_series(series);
        if !self.requirements.enabled(series) || values.is_empty() {
            return Vec::new();
        }
        let elapsed = &self.trimmed.sample_elapsed_seconds;
        let mut dense = match interpolation_strategy(series) {
            InterpolationStrategy::Angular => densify_heading_series(elapsed, values, self.frames),
            InterpolationStrategy::Hold => densify_hold_series(elapsed, values, self.frames),
            InterpolationStrategy::Numeric(policy) => {
                interpolate_numeric_series(elapsed, values, self.frames, policy)
            }
        };
        if series.limit_to_observed() {
            if let Some((first, last)) = values
                .iter()
                .position(Option::is_some)
                .zip(values.iter().rposition(Option::is_some))
            {
                for (second, value) in self.frames.iter().zip(&mut dense) {
                    if *second < elapsed[first] || *second > elapsed[last] {
                        *value = None;
                    }
                }
            }
        }
        dense
    }
}

/// Interpolates a latitude/longitude pair at `target_x`.
///
/// Latitude and longitude are resolved independently because either component
/// may be missing in source activity data.
pub fn interpolate_course_value(
    x_values: &[f64],
    course_series: &CourseSeries,
    target_x: f64,
) -> (Option<f64>, Option<f64>) {
    // Coordinates can be partially missing, so latitude and longitude are
    // interpolated independently and recombined at the target time.
    let latitudes = course_series
        .iter()
        .map(|point| point.0)
        .collect::<Vec<_>>();
    let longitudes = course_series
        .iter()
        .map(|point| point.1)
        .collect::<Vec<_>>();
    (
        interpolate_numeric_series_value(
            x_values,
            &latitudes,
            target_x,
            MissingSamplePolicy::Bridge,
        ),
        interpolate_numeric_series_value(
            x_values,
            &longitudes,
            target_x,
            MissingSamplePolicy::Bridge,
        ),
    )
}

/// Interpolates an RFC 3339 timestamp series at `target_x`.
///
/// Invalid timestamp strings are ignored. The returned value is normalized to
/// UTC with millisecond precision.
pub fn interpolate_time_series_value(
    x_values: &[f64],
    time_series: &TimeSeries,
    target_x: f64,
) -> Option<String> {
    // Convert timestamps to milliseconds for interpolation, then round back to
    // a stable RFC 3339 UTC timestamp for serialization.
    let numeric_points = x_values
        .iter()
        .copied()
        .zip(time_series.iter())
        .filter_map(|(x, value)| {
            value
                .as_deref()
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|time| (x, time.timestamp_millis() as f64))
        })
        .collect::<Vec<_>>();

    if numeric_points
        .first()
        .is_some_and(|(first, _)| target_x < *first)
        || numeric_points
            .last()
            .is_some_and(|(last, _)| target_x > *last)
    {
        return None;
    }
    interpolate_points(&numeric_points, target_x).map(|millis| {
        DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

// Builds the per-frame time axis for a scene duration and frame rate.
pub fn frame_timeline_for_fps(duration: f64, fps: f64) -> crate::error::CoreResult<Vec<f64>> {
    // A frame exists at t=0, then every 1/fps seconds strictly before duration.
    // This mirrors video frame timing and avoids generating a duplicate final
    // frame exactly at the scene end.
    if !duration.is_finite() || duration <= 0.0 {
        return Err(crate::error::CoreError::Activity(format!(
            "Dense timeline duration must be finite and positive: {duration}"
        )));
    }
    if !fps.is_finite() || fps <= 0.0 {
        return Err(crate::error::CoreError::Activity(format!(
            "Dense timeline FPS must be finite and positive: {fps}"
        )));
    }
    let frame_count = (duration * fps).ceil() as usize;
    Ok((0..frame_count)
        .map(|frame_index| frame_index as f64 / fps)
        .collect())
}

// Interpolates a numeric series over all target frame times.
fn interpolate_numeric_series(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x_values: &[f64],
    missing_sample_policy: MissingSamplePolicy,
) -> Vec<Option<f64>> {
    if y_values.is_empty() {
        return Vec::new();
    }

    match missing_sample_policy {
        MissingSamplePolicy::Bridge => {
            // Build the filtered source points once. Rebuilding them for every
            // frame makes densification quadratic for large activities.
            let points = collect_valid_numeric_points(x_values, y_values);
            target_x_values
                .iter()
                .map(|target| interpolate_points(&points, *target))
                .collect()
        }
        MissingSamplePolicy::Preserve => target_x_values
            .iter()
            .map(|target| {
                interpolate_numeric_series_value(x_values, y_values, *target, missing_sample_policy)
            })
            .collect(),
    }
}

// Densifies a series with hold (step) interpolation.
// Each frame takes the value of the last sample at or before that frame time.
pub(super) fn densify_hold_series<T: Clone>(
    x_values: &[f64],
    y_values: &[Option<T>],
    target_x_values: &[f64],
) -> Vec<Option<T>> {
    if y_values.is_empty() {
        return Vec::new();
    }
    let points = x_values
        .iter()
        .zip(y_values)
        .filter_map(|(x, value)| value.clone().map(|value| (*x, value)))
        .collect::<Vec<_>>();
    let mut last_value = None;
    let mut point_index = 0;
    target_x_values
        .iter()
        .map(|target| {
            while point_index < points.len() && points[point_index].0 <= *target + 1e-9 {
                last_value = Some(points[point_index].1.clone());
                point_index += 1;
            }
            last_value.clone()
        })
        .collect()
}

// Prepare valid heading points once; missing gaps and edges retain bridge/clamp behavior.
fn densify_heading_series(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x_values: &[f64],
) -> Vec<Option<f64>> {
    if y_values.is_empty() {
        return Vec::new();
    }
    let points = collect_valid_numeric_points(x_values, y_values);
    target_x_values
        .iter()
        .map(|target| interpolate_heading_points(&points, *target))
        .collect()
}

// Interpolates latitude and longitude vectors over all target frame times.
fn interpolate_course_series(
    x_values: &[f64],
    y_values: &CourseSeries,
    target_x_values: &[f64],
) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let latitudes = y_values.iter().map(|point| point.0).collect::<Vec<_>>();
    let longitudes = y_values.iter().map(|point| point.1).collect::<Vec<_>>();
    (
        interpolate_numeric_series(
            x_values,
            &latitudes,
            target_x_values,
            MissingSamplePolicy::Bridge,
        ),
        interpolate_numeric_series(
            x_values,
            &longitudes,
            target_x_values,
            MissingSamplePolicy::Bridge,
        ),
    )
}

// Interpolates source timestamps, falling back to sync_time only when the
// activity has no timestamp series at all.
fn interpolate_time_series(
    sync_time: Option<&str>,
    x_values: &[f64],
    y_values: &TimeSeries,
    target_x_values: &[f64],
) -> Vec<Option<String>> {
    let sampler =
        ovrley_widgets::widgets::time::sampling::TimeSampler::new(sync_time, x_values, y_values);
    target_x_values
        .iter()
        .map(|second| sampler.at(*second))
        .collect()
}

/// Converts a trimmed activity into frame-aligned render data.
///
/// Only the series requested through [`RenderDataRequirements`] are produced;
/// all other vectors are left empty to reduce allocation and per-frame work.
///
/// Phases:
/// 1. Build the canonical frame timeline from scene duration and FPS.
/// 2. Interpolate distance progress, course, and timestamps onto the frame
///    timeline (only if the active template requested each one).
/// 3. Densify every requested numeric telemetry series into per-frame vectors.
pub fn densify_activity(
    trimmed: &TrimmedActivity,
    frame_elapsed_seconds: Vec<f64>,
    requirements: &RenderDataRequirements,
) -> DenseActivityReport {
    // ── Phase 1: build the canonical frame timeline ──────────────────────
    // Every enabled series uses this same target vector so all rendered
    // values and widgets stay frame-aligned.
    // ── Phase 2: interpolate distance progress, course, timestamps ───────
    // Distance progress is absolute (not trim-relative) so route/elevation
    // widgets can use it without additional normalization.
    let frame_distance_progress =
        if !requirements.distance_progress || trimmed.sample_distance_progress.is_empty() {
            Vec::new()
        } else {
            interpolate_numeric_series(
                &trimmed.sample_elapsed_seconds,
                &trimmed.sample_distance_progress,
                &frame_elapsed_seconds,
                MissingSamplePolicy::Bridge,
            )
        };

    // Course lat/lon are interpolated independently because either component
    // may be missing in source data.
    let (course_lat, course_lon) = if requirements.course && !trimmed.course.is_empty() {
        interpolate_course_series(
            &trimmed.sample_elapsed_seconds,
            &trimmed.course,
            &frame_elapsed_seconds,
        )
    } else {
        (Vec::new(), Vec::new())
    };

    // Timestamps use the source time series when available. sync_time is only
    // used for formats that have no absolute timestamp samples at all.
    let time = if requirements.time && !trimmed.time.is_empty() {
        interpolate_time_series(
            trimmed.sync_time.as_deref(),
            &trimmed.sample_elapsed_seconds,
            &trimmed.time,
            &frame_elapsed_seconds,
        )
    } else {
        Vec::new()
    };
    let dense_lap_number = if requirements.lap_number || requirements.lap_time_seconds {
        let source_lap_number = trimmed
            .lap_number
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        densify_hold_series(
            &trimmed.sample_elapsed_seconds,
            &source_lap_number,
            &frame_elapsed_seconds,
        )
    } else {
        Vec::new()
    };
    let dense_lap_time_seconds = if requirements.lap_time_seconds {
        frame_elapsed_seconds
            .iter()
            .zip(&dense_lap_number)
            .map(|(elapsed, lap_number)| {
                lap_number.and_then(|lap_number| {
                    ovrley_widgets::widgets::lap_timer::sampling::lap_time_at(
                        &trimmed.lap_start_elapsed_seconds,
                        lap_number,
                        *elapsed,
                    )
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // ── Phase 3: densify each requested numeric series ───────────────────
    // Empty vectors signal to render code that the series is not needed,
    // avoiding wasted per-frame lookups and allocations.
    let numeric = NumericDensification {
        trimmed,
        requirements,
        frames: &frame_elapsed_seconds,
    };

    DenseActivityReport {
        full_activity_metric_ranges: trimmed.full_activity_metric_ranges.clone(),
        frame_count: frame_elapsed_seconds.len(),
        frame_elapsed_seconds: frame_elapsed_seconds.clone(),
        frame_distance_progress,
        full_activity_distance: trimmed.full_activity_distance,
        full_activity_total_ascent: trimmed.full_activity_total_ascent,
        series: ovrley_widgets::numeric_activity!(DenseSeriesReport, numeric, series, {
            gear_position: if requirements.gear_position {
                densify_hold_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.gear_position,
                    &frame_elapsed_seconds,
                )
            } else {
                Vec::new()
            },
            course_lat: course_lat,
            course_lon: course_lon,
            time: time,
            lap_number: if requirements.lap_number {
                dense_lap_number
            } else {
                Vec::new()
            },
            lap_time_seconds: if requirements.lap_time_seconds {
                dense_lap_time_seconds
            } else {
                Vec::new()
            },
            lap_start_elapsed_seconds: trimmed.lap_start_elapsed_seconds.clone(),
            delta_to_best_lap_seconds: if requirements.delta_to_best_lap_seconds {
                interpolate_numeric_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.delta_to_best_lap_seconds,
                    &frame_elapsed_seconds,
                    MissingSamplePolicy::Preserve,
                )
            } else {
                Vec::new()
            },
            lap_durations_seconds: trimmed.lap_durations_seconds.clone(),
            lap_durations_best_so_far_seconds: trimmed.lap_durations_best_so_far_seconds.clone(),
        }),
    }
}
