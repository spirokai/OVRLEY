//! Scene-window trimming for parsed activities.
//!
//! Rendering works against a scene-local timeline, but source activity samples
//! are absolute relative to activity start. This module validates the requested
//! scene window, interpolates synthetic boundary samples at the exact start/end
//! positions, and emits a compact [`TrimmedActivity`] containing only data the
//! active template needs.

use super::interpolate::{
    densify_hold_series, interpolate_course_value, interpolate_numeric_series_value,
    interpolate_time_series_value, interpolation_strategy, InterpolationStrategy,
};
use super::schema::{ParsedActivity, TrimmedActivity};
use crate::error::{CoreError, CoreResult};
use crate::interpolation::MissingSamplePolicy;
use crate::normalize::RenderDataRequirements;
use chrono::{DateTime, SecondsFormat, Utc};

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
    let missing_sample_policy = match strategy {
        InterpolationStrategy::Hold => MissingSamplePolicy::Bridge,
        InterpolationStrategy::Numeric(policy) => policy,
    };
    let start_value = interpolate_numeric_series_value(elapsed, data, start, missing_sample_policy);
    let end_value = interpolate_numeric_series_value(elapsed, data, end, missing_sample_policy);
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

/// Keeps durations only for laps whose opening and closing boundaries are both
/// inside the trim. Aligned live series retain their original session lap IDs.
fn scope_lap_metadata(
    activity: &ParsedActivity,
    start: f64,
    end: f64,
) -> CoreResult<(Vec<f64>, Vec<f64>)> {
    if activity.lap_number.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let sample_count = activity.sample_elapsed_seconds.len();
    if activity.lap_number.len() != sample_count || activity.lap_time_seconds.len() != sample_count
    {
        return Err(CoreError::Activity(
            "lap timing series must align with sample_elapsed_seconds".into(),
        ));
    }

    let mut lap_starts = Vec::new();
    let mut previous_lap = -1;
    for (index, lap) in activity.lap_number.iter().copied().enumerate() {
        if lap >= 0 && lap != previous_lap {
            let lap_time = activity.lap_time_seconds[index].ok_or_else(|| {
                CoreError::Activity(format!("lap {lap} starts without a lap_time_seconds value"))
            })?;
            // This also recovers interpolated VBO/AiM boundaries that fall
            // between the first sample of the lap and its preceding sample.
            lap_starts.push((lap, activity.sample_elapsed_seconds[index] - lap_time));
        }
        previous_lap = lap;
    }

    let mut lap_durations_seconds = Vec::new();
    for pair in lap_starts.windows(2) {
        let (source_lap, lap_start) = pair[0];
        let completion = pair[1].1;
        if lap_start < start || completion > end {
            continue;
        }
        let duration = activity
            .lap_durations_seconds
            .get(source_lap as usize)
            .copied()
            .ok_or_else(|| {
                CoreError::Activity(format!(
                    "completed lap {source_lap} has no lap duration metadata"
                ))
            })?;
        lap_durations_seconds.push(duration);
    }
    let mut best = f64::INFINITY;
    let lap_durations_best_so_far_seconds = lap_durations_seconds
        .iter()
        .map(|duration| {
            best = best.min(*duration);
            best
        })
        .collect();

    Ok((lap_durations_seconds, lap_durations_best_so_far_seconds))
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
    let (lap_durations_seconds, lap_durations_best_so_far_seconds) = if requirements.lap_number
        || requirements.lap_time_seconds
        || requirements.delta_to_best_lap_seconds
    {
        scope_lap_metadata(activity, start, end)?
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
    Ok(TrimmedActivity {
        sync_time: start_time,
        sample_elapsed_seconds: trimmed_elapsed,
        sample_distance_progress: trimmed_distance_progress,
        course,
        elevation: if requirements.elevation {
            trim_numeric_series(
                elapsed,
                &activity.elevation,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Elevation),
            )
        } else {
            Vec::new()
        },
        calories: if requirements.calories {
            trim_numeric_series(
                elapsed,
                &activity.calories,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Calories),
            )
        } else {
            Vec::new()
        },
        distance_to_home: if requirements.distance_to_home {
            trim_numeric_series(
                elapsed,
                &activity.distance_to_home,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::DistanceToHome),
            )
        } else {
            Vec::new()
        },
        total_ascent: if requirements.total_ascent {
            trim_numeric_series(
                elapsed,
                &activity.total_ascent,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::TotalAscent),
            )
        } else {
            Vec::new()
        },
        full_activity_total_ascent: last_finite(&activity.total_ascent),
        barometric_altitude: if requirements.barometric_altitude {
            trim_numeric_series(
                elapsed,
                &activity.barometric_altitude,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Elevation),
            )
        } else {
            Vec::new()
        },
        speed: if requirements.speed {
            trim_numeric_series(
                elapsed,
                &activity.speed,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Speed),
            )
        } else {
            Vec::new()
        },
        distance: if requirements.distance {
            trim_numeric_series(
                elapsed,
                &activity.distance,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Distance),
            )
        } else {
            Vec::new()
        },
        heartrate: if requirements.heartrate {
            trim_numeric_series(
                elapsed,
                &activity.heartrate,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Heartrate),
            )
        } else {
            Vec::new()
        },
        cadence: if requirements.cadence {
            trim_numeric_series(
                elapsed,
                &activity.cadence,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Cadence),
            )
        } else {
            Vec::new()
        },
        power: if requirements.power {
            trim_numeric_series(
                elapsed,
                &activity.power,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Power),
            )
        } else {
            Vec::new()
        },
        temperature: if requirements.temperature {
            trim_numeric_series(
                elapsed,
                &activity.temperature,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Temperature),
            )
        } else {
            Vec::new()
        },
        pace: if requirements.pace {
            trim_numeric_series(
                elapsed,
                &activity.pace,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Pace),
            )
        } else {
            Vec::new()
        },
        g_force: if requirements.g_force {
            trim_numeric_series(
                elapsed,
                &activity.g_force,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::GForce),
            )
        } else {
            Vec::new()
        },
        g_force_x: if requirements.g_force_x {
            trim_numeric_series(
                elapsed,
                &activity.g_force_x,
                start,
                end,
                start_inner_index,
                end_inner_index,
                InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve),
            )
        } else {
            Vec::new()
        },
        g_force_y: if requirements.g_force_y {
            trim_numeric_series(
                elapsed,
                &activity.g_force_y,
                start,
                end,
                start_inner_index,
                end_inner_index,
                InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve),
            )
        } else {
            Vec::new()
        },
        g_force_z: if requirements.g_force_z {
            trim_numeric_series(
                elapsed,
                &activity.g_force_z,
                start,
                end,
                start_inner_index,
                end_inner_index,
                InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve),
            )
        } else {
            Vec::new()
        },
        rpm: if requirements.rpm {
            trim_numeric_series(
                elapsed,
                &activity.rpm,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Rpm),
            )
        } else {
            Vec::new()
        },
        throttle_position: if requirements.throttle_position {
            trim_numeric_series(
                elapsed,
                &activity.throttle_position,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::ThrottlePosition),
            )
        } else {
            Vec::new()
        },
        brake_position: if requirements.brake_position {
            trim_numeric_series(
                elapsed,
                &activity.brake_position,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::BrakePosition),
            )
        } else {
            Vec::new()
        },
        lean_angle: if requirements.lean_angle {
            trim_numeric_series(
                elapsed,
                &activity.lean_angle,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::LeanAngle),
            )
        } else {
            Vec::new()
        },
        air_pressure: if requirements.air_pressure {
            trim_numeric_series(
                elapsed,
                &activity.air_pressure,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::AirPressure),
            )
        } else {
            Vec::new()
        },
        ground_contact_time: if requirements.ground_contact_time {
            trim_numeric_series(
                elapsed,
                &activity.ground_contact_time,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::GroundContactTime),
            )
        } else {
            Vec::new()
        },
        left_right_balance: if requirements.left_right_balance {
            trim_numeric_series(
                elapsed,
                &activity.left_right_balance,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::LeftRightBalance),
            )
        } else {
            Vec::new()
        },
        stride_length: if requirements.stride_length {
            trim_numeric_series(
                elapsed,
                &activity.stride_length,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::StrideLength),
            )
        } else {
            Vec::new()
        },
        stroke_rate: if requirements.stroke_rate {
            trim_numeric_series(
                elapsed,
                &activity.stroke_rate,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::StrokeRate),
            )
        } else {
            Vec::new()
        },
        torque: if requirements.torque {
            trim_numeric_series(
                elapsed,
                &activity.torque,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Torque),
            )
        } else {
            Vec::new()
        },
        vertical_speed: if requirements.vertical_speed {
            trim_numeric_series(
                elapsed,
                &activity.vertical_speed,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::VerticalSpeed),
            )
        } else {
            Vec::new()
        },
        iso: if requirements.iso {
            trim_numeric_series(
                elapsed,
                &activity.iso,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Iso),
            )
        } else {
            Vec::new()
        },
        aperture: if requirements.aperture {
            trim_numeric_series(
                elapsed,
                &activity.aperture,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Aperture),
            )
        } else {
            Vec::new()
        },
        shutter_speed: if requirements.shutter_speed {
            trim_numeric_series(
                elapsed,
                &activity.shutter_speed,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::ShutterSpeed),
            )
        } else {
            Vec::new()
        },
        focal_length: if requirements.focal_length {
            trim_numeric_series(
                elapsed,
                &activity.focal_length,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::FocalLength),
            )
        } else {
            Vec::new()
        },
        ev: if requirements.ev {
            trim_numeric_series(
                elapsed,
                &activity.ev,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Ev),
            )
        } else {
            Vec::new()
        },
        color_temperature: if requirements.color_temperature {
            trim_numeric_series(
                elapsed,
                &activity.color_temperature,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::ColorTemperature),
            )
        } else {
            Vec::new()
        },
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
        vertical_ratio: if requirements.vertical_ratio {
            trim_numeric_series(
                elapsed,
                &activity.vertical_ratio,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::VerticalRatio),
            )
        } else {
            Vec::new()
        },
        vertical_oscillation: if requirements.vertical_oscillation {
            trim_numeric_series(
                elapsed,
                &activity.vertical_oscillation,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::VerticalOscillation),
            )
        } else {
            Vec::new()
        },
        core_temperature: if requirements.core_temperature {
            trim_numeric_series(
                elapsed,
                &activity.core_temperature,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::CoreTemperature),
            )
        } else {
            Vec::new()
        },
        gradient: if requirements.gradient {
            trim_numeric_series(
                elapsed,
                &activity.gradient,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Gradient),
            )
        } else {
            Vec::new()
        },
        heading: if requirements.heading {
            trim_numeric_series(
                elapsed,
                &activity.heading,
                start,
                end,
                start_inner_index,
                end_inner_index,
                interpolation_strategy(crate::MetricKind::Heading),
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
        lap_number: if requirements.lap_number && !activity.lap_number.is_empty() {
            let lap_number_options: Vec<Option<i64>> =
                activity.lap_number.iter().map(|v| Some(*v)).collect();
            let boundary_values = densify_hold_series(elapsed, &lap_number_options, &[start, end]);
            let start_lap = boundary_values[0].unwrap_or(-1);
            let end_lap = boundary_values[1].unwrap_or(-1);
            trim_series(
                &lap_number_options,
                start_inner_index,
                end_inner_index,
                Some(start_lap),
                Some(end_lap),
            )
            .into_iter()
            .map(|v| v.unwrap_or(-1))
            .collect()
        } else {
            Vec::new()
        },
        lap_time_seconds: if requirements.lap_time_seconds {
            // TODO(lap-timing): replace Preserve with lap-aware interpolation that does not cross resets.
            trim_numeric_series(
                elapsed,
                &activity.lap_time_seconds,
                start,
                end,
                start_inner_index,
                end_inner_index,
                InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve),
            )
        } else {
            Vec::new()
        },
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
        lap_durations_seconds,
        lap_durations_best_so_far_seconds,
    })
}
