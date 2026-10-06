//! Numeric bounds from finalized source telemetry, before scene trimming.

use super::elevation::preferred_elevation_series;
use super::schema::ParsedActivity;
use crate::MetricKind;
use std::collections::HashMap;

/// Returns observed finite bounds; missing external telemetry has no range.
pub fn calculate_metric_range(values: &[Option<f64>]) -> Option<(f64, f64)> {
    values
        .iter()
        .copied()
        .flatten()
        .filter(|value| value.is_finite())
        .fold(None, |range, value| {
            Some(match range {
                None => (value, value),
                Some((min, max)) => (min.min(value), max.max(value)),
            })
        })
}

/// Calculates each numeric metric's bounds in canonical units from the full activity.
pub fn calculate_metric_ranges(activity: &ParsedActivity) -> HashMap<MetricKind, (f64, f64)> {
    use MetricKind::*;
    let series: &[(MetricKind, &[Option<f64>])] = &[
        (Speed, &activity.speed),
        (Distance, &activity.distance),
        (Elevation, &activity.elevation),
        (Gradient, &activity.gradient),
        (Heartrate, &activity.heartrate),
        (Cadence, &activity.cadence),
        (Power, &activity.power),
        (EnginePower, &activity.engine_power),
        (EngineLoad, &activity.engine_load),
        (Temperature, &activity.temperature),
        (Calories, &activity.calories),
        (Pace, &activity.pace),
        (GForce, &activity.g_force),
        (AirPressure, &activity.air_pressure),
        (GroundContactTime, &activity.ground_contact_time),
        (LeftRightBalance, &activity.left_right_balance),
        (StrideLength, &activity.stride_length),
        (StrokeRate, &activity.stroke_rate),
        (Torque, &activity.torque),
        (VerticalSpeed, &activity.vertical_speed),
        (VerticalRatio, &activity.vertical_ratio),
        (VerticalOscillation, &activity.vertical_oscillation),
        (CoreTemperature, &activity.core_temperature),
        (Heading, &activity.heading),
        (
            Altitude,
            preferred_elevation_series(&activity.barometric_altitude, &activity.elevation),
        ),
        (Iso, &activity.iso),
        (Aperture, &activity.aperture),
        (ShutterSpeed, &activity.shutter_speed),
        (FocalLength, &activity.focal_length),
        (Ev, &activity.ev),
        (ColorTemperature, &activity.color_temperature),
        (Rpm, &activity.rpm),
        (ThrottlePosition, &activity.throttle_position),
        (BrakePosition, &activity.brake_position),
        (LeanAngle, &activity.lean_angle),
        (DistanceToHome, &activity.distance_to_home),
        (TotalAscent, &activity.total_ascent),
    ];
    series
        .iter()
        .filter_map(|(metric, values)| calculate_metric_range(values).map(|range| (*metric, range)))
        .collect()
}
