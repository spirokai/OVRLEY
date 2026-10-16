//! Scene-window trimming for parsed activities.
//!
//! Rendering works against a scene-local timeline, but source activity samples
//! are absolute relative to activity start. This module validates the requested
//! scene window, interpolates synthetic boundary samples at the exact start/end
//! positions, and emits a compact [`TrimmedActivity`] containing only data the
//! active template needs.

use super::interpolate::{
    densify_hold_series, interpolate_course_value, interpolate_time_series_value,
    interpolation_strategy, InterpolationStrategy,
};
use super::schema::{ParsedActivity, TrimmedActivity};
use crate::error::{CoreError, CoreResult};
use crate::normalize::RenderDataRequirements;
use chrono::{DateTime, SecondsFormat, Utc};
use ovrley_widgets::interpolation::{
    collect_valid_numeric_points, interpolate_heading_points, interpolate_numeric_series_value,
    MissingSamplePolicy,
};
use ovrley_widgets::model::telemetry::{ActivitySeries, NumericActivity};

// Validates that the requested scene window fits inside activity duration.
fn validate_trim_window(duration: f64, start: f64, end: f64) -> CoreResult<()> {
    // Keep validation messages frontend-friendly because they are surfaced
    // directly when a user configures an invalid export window.
    if start < 0.0 {
        return Err(CoreError::Activity(
            "The video starts before the activity. The activity has no data for the start of the video.".into(),
        ));
    }
    if start >= duration {
        return Err(CoreError::Activity(
            "The video starts after the activity ends. The activity has no data for the video."
                .into(),
        ));
    }
    if end <= start {
        return Err(CoreError::Activity(
            "The video end must be after the video start.".into(),
        ));
    }
    if end > duration {
        return Err(CoreError::Activity(
            "The video ends after the activity ends. The activity has no data for the end of the video.".into(),
        ));
    }
    Ok(())
}

// Finds the source-sample range that lies strictly inside the trim boundaries.
fn split_trim_indices(elapsed: &[f64], start: f64, end: f64) -> (usize, usize) {
    // Interior samples exclude values at the synthetic boundaries. Exact start
    // and end values are added explicitly by interpolation helpers below.
    let start_inner_index = elapsed.partition_point(|value| *value <= start);
    let end_inner_index = elapsed.partition_point(|value| *value < end);
    (start_inner_index, end_inner_index)
}

struct NumericTrim<'a> {
    activity: &'a ParsedActivity,
    requirements: &'a RenderDataRequirements,
    start: f64,
    end: f64,
    interior: std::ops::Range<usize>,
}

impl NumericTrim<'_> {
    fn series(&self, series: ActivitySeries) -> Vec<Option<f64>> {
        if !self.requirements.enabled(series) {
            return Vec::new();
        }
        trim_numeric_series(
            &self.activity.sample_elapsed_seconds,
            self.activity.numeric_series(series),
            self.start,
            self.end,
            self.interior.start,
            self.interior.end,
            interpolation_strategy(series),
        )
    }
}

// Trims one optional numeric series and adds interpolated boundary samples.
fn trim_numeric_series(
    elapsed: &[f64],
    data: &[Option<f64>],
    start: f64,
    end: f64,
    start_inner_index: usize,
    end_inner_index: usize,
    strategy: InterpolationStrategy,
) -> Vec<Option<f64>> {
    if data.is_empty() {
        return Vec::new();
    }
    // Boundary interpolation preserves continuity when the trim cuts through
    // the middle of a source sampling interval.
    let first_observation = data
        .iter()
        .position(Option::is_some)
        .map(|index| elapsed[index]);
    let last_observation = data
        .iter()
        .rposition(Option::is_some)
        .map(|index| elapsed[index]);
    let boundary_value = |target| {
        if first_observation.is_some_and(|first| target < first)
            || last_observation.is_some_and(|last| target > last)
        {
            None
        } else {
            match strategy {
                InterpolationStrategy::Angular => {
                    interpolate_heading_points(&collect_valid_numeric_points(elapsed, data), target)
                }
                InterpolationStrategy::Hold => densify_hold_series(elapsed, data, &[target])[0],
                InterpolationStrategy::Numeric(policy) => {
                    interpolate_numeric_series_value(elapsed, data, target, policy)
                }
            }
        }
    };
    let start_value = boundary_value(start);
    let end_value = boundary_value(end);
    trim_series(
        data,
        start_inner_index,
        end_inner_index,
        start_value,
        end_value,
    )
}

fn trim_series<T: Clone>(
    data: &[Option<T>],
    start_inner_index: usize,
    end_inner_index: usize,
    start_value: Option<T>,
    end_value: Option<T>,
) -> Vec<Option<T>> {
    let mut trimmed = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
    trimmed.push(start_value);
    trimmed.extend_from_slice(&data[start_inner_index..end_inner_index]);
    trimmed.push(end_value);
    trimmed
}

fn last_finite(series: &[Option<f64>]) -> Option<f64> {
    series
        .iter()
        .rev()
        .copied()
        .flatten()
        .find(|value| value.is_finite())
}

/// Trims a parsed activity to a scene range.
///
/// The returned timeline starts at `0.0` seconds and ends at `end - start`.
/// Optional telemetry series are only copied when requested by
/// [`RenderDataRequirements`].
///
/// Phases:
/// 1. Validate that the activity has enough samples and the trim window fits
///    within the activity duration.
/// 2. Find the source-sample indices that lie strictly inside the trim
///    boundaries (synthetic boundary values are added separately below).
/// 3. Build a trim-relative elapsed-seconds vector (first value is `0.0`,
///    last value is `end - start`).
/// 4. Trim distance progress, course, and compute the trim-adjusted start
///    timestamp.
/// 5. Trim every requested numeric telemetry series, each with interpolated
///    boundary values so downstream interpolation has exact endpoints.
#[must_use = "trimmed activity must be consumed for densification"]
pub fn trim_activity(
    activity: &ParsedActivity,
    start: f64,
    end: f64,
    requirements: &RenderDataRequirements,
) -> CoreResult<TrimmedActivity> {
    // ── Phase 1: validate inputs ─────────────────────────────────────────
    if activity.sample_elapsed_seconds.len() < 2 {
        return Err(CoreError::Activity(
            "parsedActivity must contain at least two sample_elapsed_seconds values".into(),
        ));
    }

    let duration = activity.trim_end_seconds.max(
        activity
            .sample_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default(),
    );
    validate_trim_window(duration, start, end)?;

    // ── Phase 2: find interior source-sample indices ─────────────────────
    let elapsed = &activity.sample_elapsed_seconds;
    let (start_inner_index, end_inner_index) = split_trim_indices(elapsed, start, end);
    let lap_metadata_required = requirements.lap_number
        || requirements.lap_time_seconds
        || requirements.delta_to_best_lap_seconds;
    let (lap_durations_seconds, lap_durations_best_so_far_seconds) = if lap_metadata_required {
        (
            activity.lap_durations_seconds.clone(),
            activity.lap_durations_best_so_far_seconds.clone(),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    // ── Phase 3: build trim-relative elapsed timeline ────────────────────
    // The first entry is always 0.0, the last is end - start. Interior
    // samples are offset so the timeline is contiguous from zero.
    let mut trimmed_elapsed =
        Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
    trimmed_elapsed.push(0.0);
    trimmed_elapsed.extend(
        elapsed[start_inner_index..end_inner_index]
            .iter()
            .map(|value| *value - start),
    );
    trimmed_elapsed.push(end - start);
    let lap_start_elapsed_seconds = if lap_metadata_required {
        activity
            .lap_start_elapsed_seconds
            .iter()
            .map(|lap_start| *lap_start - start)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let lap_number = if requirements.lap_number || requirements.lap_time_seconds {
        trimmed_elapsed
            .iter()
            .map(|trimmed_elapsed| {
                let source_elapsed = *trimmed_elapsed + start;
                let source_index = elapsed.partition_point(|value| *value <= source_elapsed) - 1;
                activity.lap_number[source_index]
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let lap_time_seconds = if requirements.lap_time_seconds {
        trimmed_elapsed
            .iter()
            .zip(&lap_number)
            .map(|(elapsed, lap_number)| {
                ovrley_widgets::widgets::lap_timer::sampling::lap_time_at(
                    &lap_start_elapsed_seconds,
                    *lap_number,
                    *elapsed,
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // ── Phase 4: trim distance progress, course, and start timestamp ─────
    // Absolute distance progress is not re-normalized to the trim — route
    // and elevation widgets decide whether they need absolute or
    // trim-relative progress at render time.
    let trimmed_distance_progress = if !requirements.distance_progress
        || activity.sample_distance_progress.is_empty()
    {
        Vec::new()
    } else {
        // Distance progress is absolute to the full activity, not normalized to
        // the trim. Route/elevation widgets decide later whether they need
        // absolute progress or trim-relative progress.
        let source = activity
            .sample_distance_progress
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        let start_progress =
            interpolate_numeric_series_value(elapsed, &source, start, MissingSamplePolicy::Bridge)
                .unwrap_or(0.0);
        let end_progress =
            interpolate_numeric_series_value(elapsed, &source, end, MissingSamplePolicy::Bridge)
                .unwrap_or(start_progress);
        let mut trimmed = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
        trimmed.push(Some(start_progress));
        trimmed.extend(
            activity.sample_distance_progress[start_inner_index..end_inner_index]
                .iter()
                .copied()
                .map(Some),
        );
        trimmed.push(Some(end_progress));
        trimmed
    };

    let course = if requirements.course && !activity.course.is_empty() {
        let start_course = interpolate_course_value(elapsed, &activity.course, start);
        let end_course = interpolate_course_value(elapsed, &activity.course, end);
        let mut course = Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
        course.push(start_course);
        course.extend_from_slice(&activity.course[start_inner_index..end_inner_index]);
        course.push(end_course);
        course
    } else {
        Vec::new()
    };

    // The trim-adjusted sync time is the activity sync time offset forward by
    // the scene start, so per-frame timestamps in the dense report always
    // correspond to the correct wall-clock moment.
    let start_time = activity
        .sync_time
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            (value + chrono::TimeDelta::milliseconds((start * 1000.0).round() as i64))
                .with_timezone(&Utc)
        })
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true));

    // ── Phase 5: trim each requested numeric series ──────────────────────
    // Each series gets interpolated boundary values at the exact start/end
    // positions so downstream interpolation has precise endpoints even when
    // the trim window cuts through a source sampling interval.
    let numeric = NumericTrim {
        activity,
        requirements,
        start,
        end,
        interior: start_inner_index..end_inner_index,
    };
    Ok(
        ovrley_widgets::numeric_activity!(TrimmedActivity, numeric, series, {
            full_activity_metric_ranges: super::sampling::calculate_metric_ranges(activity),
            sync_time: start_time,
            sample_elapsed_seconds: trimmed_elapsed,
            sample_distance_progress: trimmed_distance_progress,
            course: course,
            full_activity_total_ascent: last_finite(&activity.total_ascent),
            gear_position: if requirements.gear_position && !activity.gear_position.is_empty() {
                let boundary_values =
                    densify_hold_series(elapsed, &activity.gear_position, &[start, end]);
                trim_series(
                    &activity.gear_position,
                    start_inner_index,
                    end_inner_index,
                    boundary_values[0].clone(),
                    boundary_values[1].clone(),
                )
            } else {
                Vec::new()
            },
            time: if requirements.time && !activity.time.is_empty() {
                let start_value = interpolate_time_series_value(elapsed, &activity.time, start);
                let end_value = interpolate_time_series_value(elapsed, &activity.time, end);
                let mut trimmed =
                    Vec::with_capacity(end_inner_index.saturating_sub(start_inner_index) + 2);
                trimmed.push(start_value);
                trimmed.extend_from_slice(&activity.time[start_inner_index..end_inner_index]);
                trimmed.push(end_value);
                trimmed
            } else {
                Vec::new()
            },
            full_activity_distance: last_finite(&activity.distance),
            lap_number: if requirements.lap_number {
                lap_number
            } else {
                Vec::new()
            },
            lap_time_seconds: if requirements.lap_time_seconds {
                lap_time_seconds
            } else {
                Vec::new()
            },
            lap_start_elapsed_seconds: lap_start_elapsed_seconds,
            delta_to_best_lap_seconds: if requirements.delta_to_best_lap_seconds {
                trim_numeric_series(
                    elapsed,
                    &activity.delta_to_best_lap_seconds,
                    start,
                    end,
                    start_inner_index,
                    end_inner_index,
                    InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve),
                )
            } else {
                Vec::new()
            },
            lap_durations_seconds: lap_durations_seconds,
            lap_durations_best_so_far_seconds: lap_durations_best_so_far_seconds,
        }),
    )
}

#[cfg(test)]
mod lap_tests {
    use super::trim_activity;
    use crate::activity::schema::ParsedActivity;
    use crate::normalize::RenderDataRequirements;

    #[test]
    fn preserves_original_lap_time_and_best_history_when_scene_starts_mid_lap() {
        let activity: ParsedActivity = serde_json::from_value(serde_json::json!({
            "sample_elapsed_seconds": [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0],
            "lap_number": [-1, -1, 0, 0, 1, 1, 2],
            "lap_time_seconds": [null, null, 0.0, 2.0, 0.0, 2.0, 0.0],
            "lap_start_elapsed_seconds": [4.0, 8.0, 12.0],
            "lap_durations_seconds": [4.0, 4.0],
            "lap_durations_best_so_far_seconds": [4.0, 4.0],
            "trim_end_seconds": 12.0
        }))
        .unwrap();
        let requirements = RenderDataRequirements {
            lap_number: true,
            lap_time_seconds: true,
            ..RenderDataRequirements::default()
        };

        let trimmed = trim_activity(&activity, 5.0, 11.0, &requirements).unwrap();

        assert_eq!(trimmed.lap_number[0], 0);
        assert_eq!(trimmed.lap_time_seconds[0], Some(1.0));
        assert_eq!(trimmed.lap_number.last(), Some(&1));
        assert_eq!(trimmed.lap_time_seconds.last(), Some(&Some(3.0)));
        assert_eq!(trimmed.lap_durations_seconds, vec![4.0, 4.0]);
        assert_eq!(trimmed.lap_durations_best_so_far_seconds, vec![4.0, 4.0]);
    }
}
