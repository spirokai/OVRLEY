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
use ovrley_widgets::model::standard_metrics::{
    standard_metric_interpolation, StandardMetricInterpolationKind,
};

use ovrley_widgets::interpolation::{
    collect_valid_numeric_points, interpolate_numeric_series_value, interpolate_points,
    MissingSamplePolicy,
};

#[derive(Clone, Copy)]
pub(crate) enum InterpolationStrategy {
    Hold,
    Numeric(MissingSamplePolicy),
}

pub(crate) fn interpolation_strategy(
    kind: ovrley_widgets::model::types::MetricKind,
) -> InterpolationStrategy {
    match standard_metric_interpolation(kind) {
        Some(StandardMetricInterpolationKind::Hold) => InterpolationStrategy::Hold,
        Some(StandardMetricInterpolationKind::Preserve) => {
            InterpolationStrategy::Numeric(MissingSamplePolicy::Preserve)
        }
        Some(StandardMetricInterpolationKind::Linear) => {
            InterpolationStrategy::Numeric(MissingSamplePolicy::Bridge)
        }
        None => match kind {
            ovrley_widgets::model::types::MetricKind::Elevation
            | ovrley_widgets::model::types::MetricKind::Gradient => {
                InterpolationStrategy::Numeric(MissingSamplePolicy::Bridge)
            }
            _ => panic!("standard metric has no interpolation policy: {kind:?}"),
        },
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
    y_values: &NumericSeries,
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

// Densifies a numeric series with forward-fill of nulls.
// Null values are replaced by the last known valid value, so the widget
// holds its last known state during data gaps.
fn densify_forward_fill_series(
    x_values: &[f64],
    y_values: &NumericSeries,
    target_x_values: &[f64],
) -> Vec<Option<f64>> {
    if y_values.is_empty() {
        return Vec::new();
    }
    // First, do standard interpolation to get frame-aligned values
    let interpolated = interpolate_numeric_series(
        x_values,
        y_values,
        target_x_values,
        MissingSamplePolicy::Bridge,
    );
    // Then forward-fill: carry last known value across null gaps
    let mut last_known: Option<f64> = None;
    interpolated
        .into_iter()
        .map(|value| match value {
            Some(v) => {
                last_known = Some(v);
                Some(v)
            }
            None => last_known,
        })
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
    // The source time series is authoritative. In particular, using sync_time
    // here would hide a mismatch between the activity start and its actual
    // timestamp samples and would make the time widget display the wrong time.
    if y_values.iter().any(Option::is_some) {
        // Build the filtered source points once. Rebuilding them for every
        // frame makes densification quadratic for large activities.
        let points = x_values
            .iter()
            .copied()
            .zip(y_values.iter())
            .filter_map(|(x, value)| {
                value
                    .as_deref()
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|time| (x, time.timestamp_millis() as f64))
            })
            .collect::<Vec<_>>();
        return target_x_values
            .iter()
            .map(|target| {
                if points.first().is_some_and(|(first, _)| *target < *first)
                    || points.last().is_some_and(|(last, _)| *target > *last)
                {
                    return None;
                }
                interpolate_points(&points, *target).map(|millis| {
                    DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
                        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
                        .to_rfc3339_opts(SecondsFormat::Millis, true)
                })
            })
            .collect();
    }

    // sync_time is only a fallback for formats that do not provide absolute
    // timestamp samples at all.
    if let Some(start_time) = sync_time
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
    {
        return target_x_values
            .iter()
            .map(|target| {
                Some(
                    (start_time
                        + chrono::TimeDelta::milliseconds((target * 1000.0).round() as i64))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                )
            })
            .collect();
    }

    vec![None; target_x_values.len()]
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
                    crate::activity::lap_timing::lap_time_at(
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
    // Resolve each metric's manifest strategy once per series; internal
    // derived series use their explicit bridge default.
    let densify = |x: &[f64],
                   y: &NumericSeries,
                   target: &[f64],
                   enabled: bool,
                   kind: ovrley_widgets::model::types::MetricKind| {
        if !enabled || y.is_empty() {
            return Vec::new();
        }
        let mut dense = match interpolation_strategy(kind) {
            InterpolationStrategy::Hold => densify_hold_series(x, y, target),
            InterpolationStrategy::Numeric(policy) => {
                interpolate_numeric_series(x, y, target, policy)
            }
        };
        let first = y.iter().position(Option::is_some).map(|index| x[index]);
        let last = y.iter().rposition(Option::is_some).map(|index| x[index]);
        for (elapsed, value) in target.iter().zip(&mut dense) {
            if first.is_some_and(|first| *elapsed < first)
                || last.is_some_and(|last| *elapsed > last)
            {
                *value = None;
            }
        }
        dense
    };

    DenseActivityReport {
        full_activity_metric_ranges: trimmed.full_activity_metric_ranges.clone(),
        frame_count: frame_elapsed_seconds.len(),
        frame_elapsed_seconds: frame_elapsed_seconds.clone(),
        frame_distance_progress,
        full_activity_distance: trimmed.full_activity_distance,
        full_activity_total_ascent: trimmed.full_activity_total_ascent,
        series: DenseSeriesReport {
            speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.speed,
                &frame_elapsed_seconds,
                requirements.speed,
                ovrley_widgets::model::types::MetricKind::Speed,
            ),
            distance: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.distance,
                &frame_elapsed_seconds,
                requirements.distance,
                ovrley_widgets::model::types::MetricKind::Distance,
            ),
            elevation: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.elevation,
                &frame_elapsed_seconds,
                requirements.elevation,
                ovrley_widgets::model::types::MetricKind::Elevation,
            ),
            calories: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.calories,
                &frame_elapsed_seconds,
                requirements.calories,
                ovrley_widgets::model::types::MetricKind::Calories,
            ),
            distance_to_home: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.distance_to_home,
                &frame_elapsed_seconds,
                requirements.distance_to_home,
                ovrley_widgets::model::types::MetricKind::DistanceToHome,
            ),
            total_ascent: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.total_ascent,
                &frame_elapsed_seconds,
                requirements.total_ascent,
                ovrley_widgets::model::types::MetricKind::TotalAscent,
            ),
            barometric_altitude: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.barometric_altitude,
                &frame_elapsed_seconds,
                requirements.barometric_altitude,
                ovrley_widgets::model::types::MetricKind::Elevation,
            ),
            gradient: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.gradient,
                &frame_elapsed_seconds,
                requirements.gradient,
                ovrley_widgets::model::types::MetricKind::Gradient,
            ),
            heartrate: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.heartrate,
                &frame_elapsed_seconds,
                requirements.heartrate,
                ovrley_widgets::model::types::MetricKind::Heartrate,
            ),
            cadence: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.cadence,
                &frame_elapsed_seconds,
                requirements.cadence,
                ovrley_widgets::model::types::MetricKind::Cadence,
            ),
            power: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.power,
                &frame_elapsed_seconds,
                requirements.power,
                ovrley_widgets::model::types::MetricKind::Power,
            ),
            engine_power: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.engine_power,
                &frame_elapsed_seconds,
                requirements.engine_power,
                ovrley_widgets::model::types::MetricKind::EnginePower,
            ),
            engine_load: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.engine_load,
                &frame_elapsed_seconds,
                requirements.engine_load,
                ovrley_widgets::model::types::MetricKind::EngineLoad,
            ),
            temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.temperature,
                &frame_elapsed_seconds,
                requirements.temperature,
                ovrley_widgets::model::types::MetricKind::Temperature,
            ),
            pace: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.pace,
                &frame_elapsed_seconds,
                requirements.pace,
                ovrley_widgets::model::types::MetricKind::Pace,
            ),
            g_force: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.g_force,
                &frame_elapsed_seconds,
                requirements.g_force,
                ovrley_widgets::model::types::MetricKind::GForce,
            ),
            g_force_x: if requirements.g_force_x {
                interpolate_numeric_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.g_force_x,
                    &frame_elapsed_seconds,
                    MissingSamplePolicy::Preserve,
                )
            } else {
                Vec::new()
            },
            g_force_y: if requirements.g_force_y {
                interpolate_numeric_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.g_force_y,
                    &frame_elapsed_seconds,
                    MissingSamplePolicy::Preserve,
                )
            } else {
                Vec::new()
            },
            g_force_z: if requirements.g_force_z {
                interpolate_numeric_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.g_force_z,
                    &frame_elapsed_seconds,
                    MissingSamplePolicy::Preserve,
                )
            } else {
                Vec::new()
            },
            rpm: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.rpm,
                &frame_elapsed_seconds,
                requirements.rpm,
                ovrley_widgets::model::types::MetricKind::Rpm,
            ),
            throttle_position: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.throttle_position,
                &frame_elapsed_seconds,
                requirements.throttle_position,
                ovrley_widgets::model::types::MetricKind::ThrottlePosition,
            ),
            brake_position: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.brake_position,
                &frame_elapsed_seconds,
                requirements.brake_position,
                ovrley_widgets::model::types::MetricKind::BrakePosition,
            ),
            lean_angle: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.lean_angle,
                &frame_elapsed_seconds,
                requirements.lean_angle,
                ovrley_widgets::model::types::MetricKind::LeanAngle,
            ),
            air_pressure: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.air_pressure,
                &frame_elapsed_seconds,
                requirements.air_pressure,
                ovrley_widgets::model::types::MetricKind::AirPressure,
            ),
            ground_contact_time: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.ground_contact_time,
                &frame_elapsed_seconds,
                requirements.ground_contact_time,
                ovrley_widgets::model::types::MetricKind::GroundContactTime,
            ),
            left_right_balance: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.left_right_balance,
                &frame_elapsed_seconds,
                requirements.left_right_balance,
                ovrley_widgets::model::types::MetricKind::LeftRightBalance,
            ),
            stride_length: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.stride_length,
                &frame_elapsed_seconds,
                requirements.stride_length,
                ovrley_widgets::model::types::MetricKind::StrideLength,
            ),
            stroke_rate: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.stroke_rate,
                &frame_elapsed_seconds,
                requirements.stroke_rate,
                ovrley_widgets::model::types::MetricKind::StrokeRate,
            ),
            torque: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.torque,
                &frame_elapsed_seconds,
                requirements.torque,
                ovrley_widgets::model::types::MetricKind::Torque,
            ),
            vertical_speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_speed,
                &frame_elapsed_seconds,
                requirements.vertical_speed,
                ovrley_widgets::model::types::MetricKind::VerticalSpeed,
            ),
            iso: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.iso,
                &frame_elapsed_seconds,
                requirements.iso,
                ovrley_widgets::model::types::MetricKind::Iso,
            ),
            aperture: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.aperture,
                &frame_elapsed_seconds,
                requirements.aperture,
                ovrley_widgets::model::types::MetricKind::Aperture,
            ),
            shutter_speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.shutter_speed,
                &frame_elapsed_seconds,
                requirements.shutter_speed,
                ovrley_widgets::model::types::MetricKind::ShutterSpeed,
            ),
            focal_length: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.focal_length,
                &frame_elapsed_seconds,
                requirements.focal_length,
                ovrley_widgets::model::types::MetricKind::FocalLength,
            ),
            ev: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.ev,
                &frame_elapsed_seconds,
                requirements.ev,
                ovrley_widgets::model::types::MetricKind::Ev,
            ),
            color_temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.color_temperature,
                &frame_elapsed_seconds,
                requirements.color_temperature,
                ovrley_widgets::model::types::MetricKind::ColorTemperature,
            ),
            gear_position: if requirements.gear_position {
                densify_hold_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.gear_position,
                    &frame_elapsed_seconds,
                )
            } else {
                Vec::new()
            },
            vertical_ratio: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_ratio,
                &frame_elapsed_seconds,
                requirements.vertical_ratio,
                ovrley_widgets::model::types::MetricKind::VerticalRatio,
            ),
            vertical_oscillation: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_oscillation,
                &frame_elapsed_seconds,
                requirements.vertical_oscillation,
                ovrley_widgets::model::types::MetricKind::VerticalOscillation,
            ),
            core_temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.core_temperature,
                &frame_elapsed_seconds,
                requirements.core_temperature,
                ovrley_widgets::model::types::MetricKind::CoreTemperature,
            ),
            heading: if requirements.heading && !trimmed.heading.is_empty() {
                densify_forward_fill_series(
                    &trimmed.sample_elapsed_seconds,
                    &trimmed.heading,
                    &frame_elapsed_seconds,
                )
            } else {
                Vec::new()
            },
            course_lat,
            course_lon,
            time,
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
        },
    }
}
