//! Interpolation and densification for activity telemetry.
//!
//! Source activities usually contain unevenly spaced samples, while the renderer
//! needs values at exact frame times. This module converts trimmed samples into
//! frame-aligned series using linear interpolation and conservative edge
//! clamping. Missing values are filtered out before interpolation so sparse
//! telemetry can still render wherever enough valid samples exist.

use super::schema::{
    CourseSeries, DenseActivityReport, DenseSeriesReport, NumericSeries, TimeSeries,
    TrimmedActivity,
};
use crate::normalize::RenderDataRequirements;
use crate::standard_metrics::{standard_metric_interpolation, StandardMetricInterpolationKind};
use chrono::{DateTime, SecondsFormat, Utc};

pub use crate::interpolation::{
    collect_valid_numeric_points, interpolate_numeric_series_value, interpolate_points,
};

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
        interpolate_numeric_series_value(x_values, &latitudes, target_x),
        interpolate_numeric_series_value(x_values, &longitudes, target_x),
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

    interpolate_points(&numeric_points, target_x).map(|millis| {
        DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

// Builds the per-frame time axis for a scene duration and frame rate.
fn build_target_x_values(duration: f64, fps: f64) -> Vec<f64> {
    // A frame exists at t=0, then every 1/fps seconds strictly before duration.
    // This mirrors video frame timing and avoids generating a duplicate final
    // frame exactly at the scene end.
    let safe_fps = fps.max(1.0);
    let mut values = Vec::new();
    let mut frame_index = 0usize;
    loop {
        let target = frame_index as f64 / safe_fps;
        if target + 1e-9 >= duration && frame_index > 0 {
            break;
        }
        values.push(target.min(duration));
        frame_index += 1;
        if frame_index > 10_000_000 {
            break;
        }
    }
    values
}

// Interpolates a numeric series over all target frame times.
fn interpolate_numeric_series(
    x_values: &[f64],
    y_values: &NumericSeries,
    target_x_values: &[f64],
) -> Vec<Option<f64>> {
    let points = collect_valid_numeric_points(x_values, y_values);
    target_x_values
        .iter()
        .map(|target| interpolate_points(&points, *target))
        .collect()
}

// Densifies a numeric series with hold (step) interpolation.
// Each frame takes the value of the last sample at or before that frame time.
fn densify_hold_series(
    x_values: &[f64],
    y_values: &NumericSeries,
    target_x_values: &[f64],
) -> Vec<Option<f64>> {
    if y_values.is_empty() {
        return Vec::new();
    }
    // Build (x, y) pairs from valid samples
    let points = collect_valid_numeric_points(x_values, y_values);
    let mut last_value: Option<f64> = None;
    let mut point_idx = 0;
    target_x_values
        .iter()
        .map(|target| {
            // Advance past all points before this target
            while point_idx < points.len() && points[point_idx].0 <= *target + 1e-9 {
                last_value = Some(points[point_idx].1);
                point_idx += 1;
            }
            last_value
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
    let interpolated = interpolate_numeric_series(x_values, y_values, target_x_values);
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
        interpolate_numeric_series(x_values, &latitudes, target_x_values),
        interpolate_numeric_series(x_values, &longitudes, target_x_values),
    )
}

// Generates or interpolates timestamps over all target frame times.
fn interpolate_time_series(
    source_start_time: Option<&str>,
    x_values: &[f64],
    y_values: &TimeSeries,
    target_x_values: &[f64],
) -> Vec<Option<String>> {
    // If the source start is known, generating timestamps from elapsed seconds
    // avoids drift caused by sparse or missing source timestamp samples.
    if let Some(start_time) = source_start_time
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

    target_x_values
        .iter()
        .map(|target| interpolate_time_series_value(x_values, y_values, *target))
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
    fps: f64,
    requirements: &RenderDataRequirements,
) -> DenseActivityReport {
    // ── Phase 1: build the canonical frame timeline ──────────────────────
    // Every enabled series uses this same target vector so all rendered
    // values and widgets stay frame-aligned.
    let duration = trimmed
        .sample_elapsed_seconds
        .last()
        .copied()
        .unwrap_or_default();
    let frame_elapsed_seconds = build_target_x_values(duration, fps);

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

    // Timestamps use the source_start_time to generate synthetic values when
    // available, falling back to interpolation from sparse source samples.
    let time = if requirements.time && !trimmed.time.is_empty() {
        interpolate_time_series(
            trimmed.source_start_time.as_deref(),
            &trimmed.sample_elapsed_seconds,
            &trimmed.time,
            &frame_elapsed_seconds,
        )
    } else {
        Vec::new()
    };

    // ── Phase 3: densify each requested numeric series ───────────────────
    // Empty vectors signal to render code that the series is not needed,
    // avoiding wasted per-frame lookups and allocations.
    // Interpolation mode (linear vs hold) is read from the manifest per metric.
    let densify = |x: &[f64],
                   y: &NumericSeries,
                   target: &[f64],
                   enabled: bool,
                   kind: crate::MetricKind| {
        if !enabled || y.is_empty() {
            return Vec::new();
        }
        match standard_metric_interpolation(kind) {
            Some(StandardMetricInterpolationKind::Hold) => densify_hold_series(x, y, target),
            _ => interpolate_numeric_series(x, y, target),
        }
    };

    DenseActivityReport {
        frame_count: frame_elapsed_seconds.len(),
        frame_elapsed_seconds: frame_elapsed_seconds.clone(),
        frame_distance_progress,
        series: DenseSeriesReport {
            speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.speed,
                &frame_elapsed_seconds,
                requirements.speed,
                crate::MetricKind::Speed,
            ),
            elevation: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.elevation,
                &frame_elapsed_seconds,
                requirements.elevation,
                crate::MetricKind::Elevation,
            ),
            gradient: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.gradient,
                &frame_elapsed_seconds,
                requirements.gradient,
                crate::MetricKind::Gradient,
            ),
            heartrate: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.heartrate,
                &frame_elapsed_seconds,
                requirements.heartrate,
                crate::MetricKind::Heartrate,
            ),
            cadence: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.cadence,
                &frame_elapsed_seconds,
                requirements.cadence,
                crate::MetricKind::Cadence,
            ),
            power: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.power,
                &frame_elapsed_seconds,
                requirements.power,
                crate::MetricKind::Power,
            ),
            temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.temperature,
                &frame_elapsed_seconds,
                requirements.temperature,
                crate::MetricKind::Temperature,
            ),
            pace: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.pace,
                &frame_elapsed_seconds,
                requirements.pace,
                crate::MetricKind::Pace,
            ),
            g_force: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.g_force,
                &frame_elapsed_seconds,
                requirements.g_force,
                crate::MetricKind::GForce,
            ),
            air_pressure: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.air_pressure,
                &frame_elapsed_seconds,
                requirements.air_pressure,
                crate::MetricKind::AirPressure,
            ),
            ground_contact_time: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.ground_contact_time,
                &frame_elapsed_seconds,
                requirements.ground_contact_time,
                crate::MetricKind::GroundContactTime,
            ),
            left_right_balance: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.left_right_balance,
                &frame_elapsed_seconds,
                requirements.left_right_balance,
                crate::MetricKind::LeftRightBalance,
            ),
            stride_length: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.stride_length,
                &frame_elapsed_seconds,
                requirements.stride_length,
                crate::MetricKind::StrideLength,
            ),
            stroke_rate: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.stroke_rate,
                &frame_elapsed_seconds,
                requirements.stroke_rate,
                crate::MetricKind::StrokeRate,
            ),
            torque: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.torque,
                &frame_elapsed_seconds,
                requirements.torque,
                crate::MetricKind::Torque,
            ),
            vertical_speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_speed,
                &frame_elapsed_seconds,
                requirements.vertical_speed,
                crate::MetricKind::VerticalSpeed,
            ),
            altitude: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.altitude,
                &frame_elapsed_seconds,
                requirements.altitude,
                crate::MetricKind::Altitude,
            ),
            iso: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.iso,
                &frame_elapsed_seconds,
                requirements.iso,
                crate::MetricKind::Iso,
            ),
            aperture: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.aperture,
                &frame_elapsed_seconds,
                requirements.aperture,
                crate::MetricKind::Aperture,
            ),
            shutter_speed: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.shutter_speed,
                &frame_elapsed_seconds,
                requirements.shutter_speed,
                crate::MetricKind::ShutterSpeed,
            ),
            focal_length: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.focal_length,
                &frame_elapsed_seconds,
                requirements.focal_length,
                crate::MetricKind::FocalLength,
            ),
            ev: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.ev,
                &frame_elapsed_seconds,
                requirements.ev,
                crate::MetricKind::Ev,
            ),
            color_temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.color_temperature,
                &frame_elapsed_seconds,
                requirements.color_temperature,
                crate::MetricKind::ColorTemperature,
            ),
            gear_position: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.gear_position,
                &frame_elapsed_seconds,
                requirements.gear_position,
                crate::MetricKind::GearPosition,
            ),
            vertical_ratio: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_ratio,
                &frame_elapsed_seconds,
                requirements.vertical_ratio,
                crate::MetricKind::VerticalRatio,
            ),
            vertical_oscillation: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.vertical_oscillation,
                &frame_elapsed_seconds,
                requirements.vertical_oscillation,
                crate::MetricKind::VerticalOscillation,
            ),
            core_temperature: densify(
                &trimmed.sample_elapsed_seconds,
                &trimmed.core_temperature,
                &frame_elapsed_seconds,
                requirements.core_temperature,
                crate::MetricKind::CoreTemperature,
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
        },
    }
}
