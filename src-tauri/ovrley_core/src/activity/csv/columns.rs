//! Canonical activity-column assembly for CSV input.
//!
//! The supporting concerns are split into [`data`], [`timing`], and [`metrics`].
//! This module coordinates those pieces: it builds the canonical timeline,
//! coalesces duplicate-time rows,
//! rebases distance, and assembles the raw columnar contract consumed by the
//! shared activity finalizer.

pub(super) use super::data::CsvColumnData;
use super::headers::{AccelerationKind, HeaderLayout, SourcePriority};
use super::metrics::{
    column_unit, parse_number, selected_acceleration_series, selected_gear_series, selected_series,
    selected_series_with_column,
};
pub(super) use super::timing::LocalPreamble;
use super::timing::{selected_absolute_timestamps, AbsoluteTimestamp};
use super::units::convert;
use super::Metric;
use crate::activity::schema::{
    ActivityColumns, DirectMetricGapPolicy, RawActivityOptions, SmoothingOption,
};
use crate::error::{CoreError, CoreResult};
use crate::media::telemetry_math::{
    backfill_lateral_g_from_lean_angle, derive_lean_from_speed_heading, lean_angle_from_lateral_g,
};
use csv::StringRecord;
use serde_json::json;
use std::{collections::BTreeMap, ops::Range};

/// Builds canonical raw activity columns from a resolved CSV layout.
///
/// Elapsed time is selected from the best usable elapsed column, with absolute
/// timestamps filling missing rows when available. The resulting timeline is
/// rebased to zero, must not decrease, and must contain at least two distinct
/// samples. Adjacent rows with equal canonical time are reduced by taking the
/// last non-missing value for each metric.
///
/// Direct metric sources are selected by header priority and value usability.
/// Scalar g-force is preferred when present, then derived from semantic
/// lateral/longitudinal acceleration, and finally derived from literal X/Y/Z
/// acceleration after removing the one-g component of gravity. Distance is
/// converted to metres and rebased to the first usable value.
pub(super) fn build_activity_columns(
    header: &HeaderLayout,
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
    preamble: &LocalPreamble,
    file_name: &str,
) -> CoreResult<ActivityColumns> {
    let elapsed_column = super::metrics::select_column(
        Metric::ElapsedSeconds,
        None,
        &header.columns,
        units_row,
        data,
    );
    let elapsed_unit = elapsed_column
        .map(|column| column_unit(column, units_row).expect("selected elapsed column has a unit"));
    let absolute_timestamps = selected_absolute_timestamps(
        header,
        units_row,
        data,
        preamble,
        elapsed_column,
        elapsed_unit,
    );
    let absolute_seconds = absolute_timestamps
        .iter()
        .map(|value| value.as_ref().map(AbsoluteTimestamp::seconds))
        .collect::<Vec<_>>();
    let absolute_origin = absolute_seconds.iter().flatten().next().copied();
    let source_elapsed = (0..data.len())
        .map(|row| {
            elapsed_column.and_then(|column| {
                parse_number(data.value(row, column.index))
                    .map(|value| convert(value, elapsed_unit.expect("elapsed column has a unit")))
            })
        })
        .collect::<Vec<_>>();
    let absolute_elapsed_anchor =
        absolute_seconds
            .iter()
            .zip(&source_elapsed)
            .find_map(|(absolute, elapsed)| {
                absolute
                    .zip(*elapsed)
                    .map(|(absolute, elapsed)| absolute - elapsed)
            });
    let mut elapsed_seconds = Vec::with_capacity(data.len());
    for (row, (absolute, elapsed)) in absolute_seconds.iter().zip(&source_elapsed).enumerate() {
        let canonical = elapsed.or_else(|| {
            absolute.map(|value| {
                value
                    - absolute_elapsed_anchor
                        .or(absolute_origin)
                        .expect("absolute value has an origin")
            })
        });
        let value = canonical.ok_or_else(|| {
            CoreError::Activity(format!(
                "CSV row {} has neither usable elapsed time nor usable absolute timestamp",
                data.record_index(row) + 1
            ))
        })?;
        elapsed_seconds.push(value);
    }
    if let Some(index) = elapsed_seconds
        .windows(2)
        .position(|pair| pair[0] > pair[1])
    {
        return Err(CoreError::Activity(format!(
            "CSV row {} canonical time must not decrease",
            data.record_index(index + 1) + 1
        )));
    }
    let groups = equal_time_groups(&elapsed_seconds);
    if groups.len() < 2 {
        return Err(CoreError::Activity(
            "CSV activity must contain at least two timed samples".to_string(),
        ));
    }
    if groups.len() != elapsed_seconds.len() {
        elapsed_seconds = groups
            .iter()
            .map(|group| elapsed_seconds[group.start])
            .collect();
    }
    let origin = elapsed_seconds[0];
    elapsed_seconds
        .iter_mut()
        .for_each(|value| *value -= origin);

    let sample_count = groups.len();
    let gps_updates = parse_gps_updates(header, data)?;
    let timestamp = coalesce_series(absolute_timestamps, &groups)
        .into_iter()
        .map(|value| value.map(|value| value.rfc3339()))
        .collect();
    let series = |metric| {
        let (column, mut values) =
            selected_series_with_column(metric, &header.columns, units_row, data);
        let uses_gps_update = metric.uses_gps_update()
            && column.is_some_and(|column| {
                matches!(
                    column.priority,
                    SourcePriority::Preferred
                        | SourcePriority::Direct
                        | SourcePriority::DirectSpeed
                )
            });
        if uses_gps_update {
            if let Some(updates) = &gps_updates {
                values.iter_mut().zip(updates).for_each(|(value, updated)| {
                    if !updated {
                        *value = None;
                    }
                });
            }
        }
        (
            coalesce_series(values, &groups),
            uses_gps_update && gps_updates.is_some(),
        )
    };
    let (mut latitude, _) = series(Metric::Latitude);
    let (mut longitude, _) = series(Metric::Longitude);

    // Some loggers emit a single GPS column containing a space-separated
    // "lat lon" pair. When no dedicated latitude/longitude columns produced
    // values, derive them from that combined column.
    if latitude.iter().all(Option::is_none) && longitude.iter().all(Option::is_none) {
        if let Some(gps_column) = header
            .columns
            .iter()
            .find(|column| column.metric == Metric::GpsCoordinate)
        {
            let split = (0..data.len())
                .map(|row| parse_gps_coordinate(data.value(row, gps_column.index)))
                .collect::<Vec<_>>();
            let coalesced = coalesce_series(split, &groups);
            latitude = coalesced
                .iter()
                .map(|value| value.map(|(lat, _)| lat))
                .collect();
            longitude = coalesced
                .iter()
                .map(|value| value.map(|(_, lon)| lon))
                .collect();
        }
    }

    let (elevation, _) = series(Metric::Elevation);
    let (barometric_altitude, _) = series(Metric::BarometricAltitude);
    let (speed, preserve_speed_gaps) = series(Metric::Speed);
    let (heading, preserve_heading_gaps) = series(Metric::Heading);
    let (mut g_force_x, _) = series(Metric::GForceX);
    let (mut g_force_y, _) = series(Metric::GForceY);
    let (g_force_z, _) = series(Metric::GForceZ);
    let mut g_force_source = selected_series(Metric::GForce, &header.columns, units_row, data);
    if g_force_source.iter().all(Option::is_none) {
        let lateral = selected_acceleration_series(
            Metric::GForceX,
            AccelerationKind::Semantic,
            &header.columns,
            units_row,
            data,
        );
        let longitudinal = selected_acceleration_series(
            Metric::GForceY,
            AccelerationKind::Semantic,
            &header.columns,
            units_row,
            data,
        );
        if let (Some(lateral), Some(longitudinal)) = (lateral, longitudinal) {
            g_force_source = lateral
                .iter()
                .zip(longitudinal)
                .map(|(x, y)| x.zip(y).map(|(x, y)| x.hypot(y)))
                .collect();
        } else {
            let x = selected_acceleration_series(
                Metric::GForceX,
                AccelerationKind::Literal,
                &header.columns,
                units_row,
                data,
            );
            let y = selected_acceleration_series(
                Metric::GForceY,
                AccelerationKind::Literal,
                &header.columns,
                units_row,
                data,
            );
            let z = selected_acceleration_series(
                Metric::GForceZ,
                AccelerationKind::Literal,
                &header.columns,
                units_row,
                data,
            );
            if let (Some(x), Some(y), Some(z)) = (x, y, z) {
                g_force_source = x
                    .iter()
                    .zip(y)
                    .zip(z)
                    .map(|((x, y), z)| {
                        x.zip(y)
                            .zip(z)
                            .map(|((x, y), z)| (x * x + y * y + z * z - 1.0).max(0.0).sqrt())
                    })
                    .collect();
            }
        }
    }
    let g_force = coalesce_series(g_force_source, &groups);
    let (mut distance, _) = series(Metric::Distance);
    if let Some(origin) = distance.iter().flatten().next().copied() {
        distance.iter_mut().for_each(|value| {
            *value = value
                .map(|distance| distance - origin)
                .filter(|distance| *distance >= 0.0)
        });
    }
    let (distance_to_home, _) = series(Metric::DistanceToHome);
    let preserve_direct_metric_gaps = DirectMetricGapPolicy {
        speed: preserve_speed_gaps,
        heading: preserve_heading_gaps,
    };
    let (rpm, _) = series(Metric::Rpm);
    let (engine_power, _) = series(Metric::EnginePower);
    let (torque, _) = series(Metric::Torque);
    let (throttle_position, _) = series(Metric::ThrottlePosition);
    let (brake_position, _) = series(Metric::BrakePosition);
    let (source_lap_number, _) = series(Metric::LapNumber);
    let has_lap_number_column = header
        .columns
        .iter()
        .any(|column| column.metric == Metric::LapNumber);
    let lap_number = if has_lap_number_column {
        source_lap_number
            .into_iter()
            .map(|value| match value {
                Some(value) if value > 0.0 && value.fract() == 0.0 && value <= i64::MAX as f64 => {
                    Some(value as i64)
                }
                _ => Some(-1),
            })
            .collect()
    } else {
        vec![None; sample_count]
    };
    let (mut lean_angle, _) = series(Metric::LeanAngle);
    if lean_angle.iter().all(Option::is_none) {
        lean_angle = derive_lean_from_speed_heading(&speed, &heading, &elapsed_seconds);
    }
    if lean_angle.iter().all(Option::is_none) {
        if let Some(lateral) = selected_acceleration_series(
            Metric::GForceX,
            AccelerationKind::Semantic,
            &header.columns,
            units_row,
            data,
        ) {
            let derived = lateral
                .into_iter()
                .map(|value| value.and_then(lean_angle_from_lateral_g))
                .collect::<Vec<_>>();
            lean_angle = coalesce_series(derived, &groups);
        }
    }
    if lean_angle.iter().any(Option::is_some) {
        let (new_x, new_y) =
            backfill_lateral_g_from_lean_angle(&g_force_x, &g_force_y, &lean_angle);
        g_force_x = new_x;
        g_force_y = new_y;
    }
    let gear_position = coalesce_series(
        selected_gear_series(&header.columns, units_row, data),
        &groups,
    );
    let empty = || vec![None; sample_count];
    let smoothing = ["g_force_x", "g_force_y", "g_force_z"]
        .into_iter()
        .map(|metric| {
            (
                metric.to_string(),
                SmoothingOption {
                    enabled: true,
                    method: "zero_phase_ma".to_string(),
                    window_seconds: 0.2,
                },
            )
        })
        .chain([(
            "heading".to_string(),
            SmoothingOption {
                enabled: true,
                method: "circular_ema".to_string(),
                window_seconds: 0.0,
            },
        )])
        .collect::<BTreeMap<_, _>>();

    Ok(ActivityColumns {
        file_name: file_name.to_string(),
        file_format: "csv".to_string(),
        metadata: json!({}),
        sync_time: None,
        options: RawActivityOptions {
            smoothing,
            ..RawActivityOptions::default()
        },
        preserve_direct_metric_gaps,
        timestamp,
        elapsed_seconds: elapsed_seconds.into_iter().map(Some).collect(),
        latitude,
        longitude,
        elevation,
        barometric_altitude,
        speed,
        heading,
        distance,
        distance_to_home,
        g_force,
        g_force_x,
        g_force_y,
        g_force_z,
        rpm,
        throttle_position,
        brake_position,
        lean_angle,
        gear_position,
        original_sample_count: data.len(),
        include_original_sample_count_metadata: false,
        heartrate: empty(),
        cadence: empty(),
        power: empty(),
        engine_power,
        temperature: empty(),
        calories: empty(),
        gradient: empty(),
        pace: empty(),
        vertical_speed: empty(),
        torque,
        stroke_rate: empty(),
        stride_length: empty(),
        vertical_oscillation: empty(),
        ground_contact_time: empty(),
        left_right_balance: empty(),
        core_temperature: empty(),
        air_pressure: empty(),
        iso: empty(),
        aperture: empty(),
        shutter_speed: empty(),
        focal_length: empty(),
        ev: empty(),
        color_temperature: empty(),
        lap_number,
        lap_markers: if preamble.beacon_markers.is_empty() {
            crate::activity::schema::LapMarkers::None
        } else {
            crate::activity::schema::LapMarkers::BeaconMarkers(preamble.beacon_markers.clone())
        },
    })
}

/// Parses a single space-separated "lat lon" GPS coordinate string.
///
/// Both values must be finite and lie within the valid latitude/longitude
/// ranges. Any extra tokens are ignored; missing or malformed values return
/// `None`.
fn parse_gps_coordinate(value: Option<&str>) -> Option<(f64, f64)> {
    let mut parts = value?.trim().split_whitespace();
    let lat: f64 = parts.next()?.parse().ok()?;
    let lon: f64 = parts.next()?.parse().ok()?;
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some((lat, lon))
}

/// Parses an optional GPS freshness signal as a strict row-aligned contract.
fn parse_gps_updates(header: &HeaderLayout, data: &CsvColumnData) -> CoreResult<Option<Vec<bool>>> {
    let Some(column) = header.gps_update_index else {
        return Ok(None);
    };
    let updates = (0..data.len())
        .map(|row| match data.value(row, column).map(str::trim) {
            Some("0") => Ok(false),
            Some("1") => Ok(true),
            _ => Err(CoreError::Activity(format!(
                "CSV row {} GPS_Update must be 0 or 1",
                data.record_index(row) + 1
            ))),
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(Some(updates))
}

/// Groups adjacent source rows that share exactly the same elapsed time.
///
/// The input must already be in non-decreasing order. Each range identifies
/// one output sample and is later reduced by [`coalesce_series`].
fn equal_time_groups(elapsed_seconds: &[f64]) -> Vec<Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0;
    for index in 1..elapsed_seconds.len() {
        if elapsed_seconds[index] != elapsed_seconds[start] {
            groups.push(start..index);
            start = index;
        }
    }
    groups.push(start..elapsed_seconds.len());
    groups
}

/// Reduces each equal-time group to its last non-missing series value.
///
/// This lets a later duplicate row update only the fields it supplies while
/// preserving earlier values for fields omitted by that row. When every group
/// contains one row, the already-owned series is returned without copying it.
fn coalesce_series<T: Clone>(series: Vec<Option<T>>, groups: &[Range<usize>]) -> Vec<Option<T>> {
    if groups.len() == series.len() {
        return series;
    }

    groups
        .iter()
        .map(|group| series[group.clone()].iter().rev().find_map(Clone::clone))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{coalesce_series, equal_time_groups};

    struct CloneMustNotRun;

    impl Clone for CloneMustNotRun {
        fn clone(&self) -> Self {
            panic!("unique timestamp series must not be cloned")
        }
    }

    #[test]
    fn unique_time_series_is_returned_without_cloning_values() {
        let groups = equal_time_groups(&[0.0, 1.0]);
        let series = vec![Some(CloneMustNotRun), None];

        let coalesced = coalesce_series(series, &groups);

        assert_eq!(coalesced.len(), 2);
        assert!(coalesced[0].is_some());
        assert!(coalesced[1].is_none());
    }

    #[test]
    fn duplicate_time_series_keeps_the_last_present_value() {
        let groups = equal_time_groups(&[0.0, 0.0, 1.0]);
        let series = vec![Some(1), None, Some(2)];

        assert_eq!(coalesce_series(series, &groups), vec![Some(1), Some(2)]);
    }
}
