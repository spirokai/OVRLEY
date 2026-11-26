//! Canonical activity-column assembly for CSV input: sparse column storage, metric source
//! selection and conversion, the canonical timeline, duplicate-time coalescing and distance
//! rebasing.

use super::headers::{AccelerationKind, ControlKind, HeaderColumn, HeaderLayout, SourcePriority};
pub(super) use super::timing::LocalPreamble;
use super::timing::{selected_absolute_timestamps, AbsoluteTimestamp};
use super::units::{convert, parse_declared_unit, resolve_unit, DeclaredUnit, Unit};
use super::Metric;
use crate::activity::schema::{
    ActivityColumns, DirectMetricGapPolicy, GearSeries, NumericSeries, RawActivityOptions,
    SmoothingOption,
};
use crate::error::{CoreError, CoreResult};
use crate::import::backfill_lateral_g_from_lean_angle;
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
    let elapsed_column = select_column(
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
    let (cadence, _) = series(Metric::Cadence);
    let (power, _) = series(Metric::Power);
    let (engine_power, _) = series(Metric::EnginePower);
    let (engine_load, _) = series(Metric::EngineLoad);
    let (coolant_temperature, _) = series(Metric::CoolantTemperature);
    let (boost_pressure, _) = series(Metric::BoostPressure);
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
    let (lean_angle, _) = series(Metric::LeanAngle);
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
    // Keep each gas observation atomic, including duplicate-time coalescing.
    // Missing helium is unknown, not an air/nitrox mixture with zero helium.
    let gas = selected_series(Metric::FractionO2, &header.columns, units_row, data)
        .into_iter()
        .zip(selected_series(
            Metric::FractionHe,
            &header.columns,
            units_row,
            data,
        ))
        .map(|(o2, he)| o2.zip(he).filter(|(o2, he)| o2 + he <= 1.0 + 1e-9))
        .collect();
    let (fraction_o2, fraction_he) = coalesce_series(gas, &groups)
        .into_iter()
        .map(|gas| (gas.map(|gas| gas.0), gas.map(|gas| gas.1)))
        .unzip();
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
            "pace".to_string(),
            SmoothingOption {
                enabled: true,
                method: "zero_phase_ma".to_string(),
                window_seconds: 5.0,
            },
        )])
        .chain([(
            "heading".to_string(),
            SmoothingOption {
                enabled: true,
                method: "circular_ema".to_string(),
                window_seconds: 1.0,
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
        cadence,
        power,
        engine_power,
        engine_load,
        coolant_temperature,
        boost_pressure,
        temperature: empty(),
        depth: series(Metric::Depth).0,
        first_stop_depth: series(Metric::FirstStopDepth).0,
        first_stop_time: series(Metric::FirstStopTime).0,
        time_to_surface: series(Metric::TimeToSurface).0,
        average_ppo2: series(Metric::AveragePpo2).0,
        fraction_o2,
        fraction_he,
        water_temperature: series(Metric::WaterTemperature).0,
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
    let mut parts = value?.split_whitespace();
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

/// Sparse, row-oriented storage for the columns selected from a CSV header.
///
/// Unrecognized columns are not retained. Recognized columns preserve their
/// original indices so short rows can still be read as missing values and
/// timeline errors can refer to the source record number.
pub(super) struct CsvColumnData {
    /// Zero-based source record index for each retained data row.
    record_indices: Vec<usize>,
    /// Values for selected columns, indexed by their original CSV position.
    columns: Vec<Option<Vec<String>>>,
    /// Original CSV positions whose values are retained in `columns`.
    selected_indices: Vec<usize>,
}

impl CsvColumnData {
    /// Creates empty storage sized for the recognized columns in `header`.
    pub(super) fn new(header: &HeaderLayout) -> Self {
        let width = header
            .columns
            .iter()
            .map(|column| column.index + 1)
            .max()
            .unwrap_or_default();
        let mut columns = (0..width).map(|_| None).collect::<Vec<_>>();
        let mut selected_indices = header
            .columns
            .iter()
            .map(|column| column.index)
            .collect::<Vec<_>>();
        if let Some(index) = header.gps_update_index {
            if index >= columns.len() {
                columns.resize_with(index + 1, || None);
            }
            selected_indices.push(index);
        }
        selected_indices.sort_unstable();
        selected_indices.dedup();
        for index in &selected_indices {
            columns[*index] = Some(Vec::new());
        }
        Self {
            record_indices: Vec::new(),
            columns,
            selected_indices,
        }
    }

    /// Appends a source record, treating absent cells in a short row as empty.
    pub(super) fn push(&mut self, record_index: usize, record: &StringRecord) {
        self.record_indices.push(record_index);
        for index in &self.selected_indices {
            self.columns[*index]
                .as_mut()
                .expect("selected CSV column has storage")
                .push(record.get(*index).unwrap_or_default().to_string());
        }
    }

    /// Returns the number of retained data rows.
    pub(super) fn len(&self) -> usize {
        self.record_indices.len()
    }

    /// Returns the original zero-based record index for a retained row.
    pub(super) fn record_index(&self, row: usize) -> usize {
        self.record_indices[row]
    }

    /// Returns a retained cell by retained row and original CSV column index.
    ///
    /// `None` means that the column was not selected, the row is short, or the
    /// requested row is outside the retained data range.
    pub(super) fn value(&self, row: usize, column: usize) -> Option<&str> {
        self.columns
            .get(column)?
            .as_ref()?
            .get(row)
            .map(String::as_str)
    }
}

/// Selects and converts the best usable source for a metric.
///
/// When no compatible source exists, returns a row-aligned all-missing series
/// so every canonical column retains the same shape before finalization.
pub(super) fn selected_series(
    metric: Metric,
    columns: &[HeaderColumn],
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> NumericSeries {
    selected_series_with_column(metric, columns, units_row, data).1
}

/// Selects a metric source and returns both its identity and normalized values.
fn selected_series_with_column<'a>(
    metric: Metric,
    columns: &'a [HeaderColumn],
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> (Option<&'a HeaderColumn>, NumericSeries) {
    let column = select_column(metric, None, columns, units_row, data);
    let series = column.map_or_else(
        || vec![None; data.len()],
        |column| series_from_column(column, units_row, data),
    );
    (column, series)
}

/// Selects a usable acceleration source of the requested representation.
///
/// Returns `None` when no matching column is available; unlike
/// [`selected_series`], this distinction is needed while choosing g-force
/// derivation fallbacks.
fn selected_acceleration_series(
    metric: Metric,
    acceleration: AccelerationKind,
    columns: &[HeaderColumn],
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> Option<NumericSeries> {
    let column = select_column(metric, Some(acceleration), columns, units_row, data)?;
    Some(series_from_column(column, units_row, data))
}

/// Selects and parses the best usable gear source into the canonical mixed-type series.
fn selected_gear_series(
    columns: &[HeaderColumn],
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> GearSeries {
    for column in candidate_columns(Metric::GearPosition, None, columns, units_row) {
        let series = (0..data.len())
            .map(|row| data.value(row, column.index).and_then(parse_gear_value))
            .collect::<GearSeries>();
        if series.iter().any(Option::is_some) {
            return series;
        }
    }
    vec![None; data.len()]
}

fn parse_gear_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("n/a")
        || trimmed.eq_ignore_ascii_case("na")
        || trimmed.eq_ignore_ascii_case("null")
    {
        return None;
    }
    Some(
        trimmed
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .map_or_else(|| trimmed.to_string(), |number| number.to_string()),
    )
}

/// Converts every value in one selected CSV column to a canonical series.
///
/// Empty, non-numeric, non-finite, and metric-invalid observations become
/// missing values. Binary controls are expanded to zero or one hundred percent
/// before metric validation.
fn series_from_column(
    column: &HeaderColumn,
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> NumericSeries {
    let unit = column_unit(column, units_row).expect("selected column has a unit");
    let binary = control_is_binary(column, data);
    (0..data.len())
        .map(|row| parse_metric_value(data.value(row, column.index), column.metric, unit, binary))
        .collect()
}

/// Chooses the highest-priority compatible column containing usable data.
///
/// Candidates are ordered by [`SourcePriority`](super::types::SourcePriority)
/// and then by source position. A candidate with only missing or invalid
/// observations is skipped, allowing a lower-priority source to provide the
/// metric.
fn select_column<'a>(
    metric: Metric,
    acceleration: Option<AccelerationKind>,
    columns: &'a [HeaderColumn],
    units_row: Option<&StringRecord>,
    data: &CsvColumnData,
) -> Option<&'a HeaderColumn> {
    candidate_columns(metric, acceleration, columns, units_row)
        .into_iter()
        .find(|column| {
            let unit = column_unit(column, units_row).expect("candidate has a compatible unit");
            let binary = control_is_binary(column, data);
            (0..data.len()).any(|row| {
                parse_metric_value(data.value(row, column.index), metric, unit, binary).is_some()
            })
        })
}

fn candidate_columns<'a>(
    metric: Metric,
    acceleration: Option<AccelerationKind>,
    columns: &'a [HeaderColumn],
    units_row: Option<&StringRecord>,
) -> Vec<&'a HeaderColumn> {
    let mut candidates = columns
        .iter()
        .filter(|column| {
            column.metric == metric
                && acceleration.is_none_or(|kind| column.acceleration == Some(kind))
                && column_unit(column, units_row).is_some()
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|column| {
        (
            power_unit_priority(
                metric,
                column_unit(column, units_row).expect("candidate has a compatible unit"),
            ),
            column.priority,
            column.index,
        )
    });
    candidates
}

/// Prefers explicit watts over kilowatts and metric horsepower for engine power.
fn power_unit_priority(metric: Metric, unit: Unit) -> u8 {
    if metric != Metric::EnginePower {
        return 0;
    }

    match unit {
        Unit::Watts => 0,
        Unit::Kilowatts => 1,
        Unit::MetricHorsepower => 2,
        Unit::MechanicalHorsepower => 3,
        _ => unreachable!("engine power candidates only have power-compatible units"),
    }
}

/// Parses, converts, and validates one metric observation.
///
/// `binary` indicates that a source's zero/one values represent a control
/// state and must be mapped to zero/one hundred percent.
fn parse_metric_value(
    value: Option<&str>,
    metric: Metric,
    unit: Unit,
    binary: bool,
) -> Option<f64> {
    parse_number(value)
        .map(|value| convert(value, unit))
        .map(|value| if binary { value * 100.0 } else { value })
        .and_then(|value| validate(metric, value))
}

/// Determines whether a control column represents a binary state.
///
/// Explicit binary and percentage declarations are honored. For inferred
/// controls, the entire column is treated as binary only when it contains at
/// least one value and every present value is exactly zero or one.
fn control_is_binary(column: &HeaderColumn, data: &CsvColumnData) -> bool {
    match column.control {
        Some(ControlKind::Binary) => true,
        Some(ControlKind::Infer) => {
            let values = (0..data.len())
                .filter_map(|row| parse_number(data.value(row, column.index)))
                .collect::<Vec<_>>();
            !values.is_empty() && values.iter().all(|value| matches!(*value, 0.0 | 1.0))
        }
        Some(ControlKind::Percentage) | None => false,
    }
}

/// Resolves a header column's unit against its optional units-row declaration.
///
/// `None` means the declarations conflict, are unsupported for the metric, or
/// cannot be resolved to a canonical unit.
pub(super) fn column_unit(column: &HeaderColumn, units_row: Option<&StringRecord>) -> Option<Unit> {
    let row_declaration = units_row.map_or(DeclaredUnit::Absent, |record| {
        parse_declared_unit(record.get(column.index).unwrap_or_default())
    });
    resolve_unit(column.metric, column.declared_unit, row_declaration)
}

/// Parses a finite numeric cell, treating common missing markers as absent.
pub(super) fn parse_number(value: Option<&str>) -> Option<f64> {
    let value = value?.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("n/a")
        || value.eq_ignore_ascii_case("na")
        || value.eq_ignore_ascii_case("null")
    {
        return None;
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

/// Applies metric-specific bounds and canonical heading normalization.
///
/// Invalid coordinates, negative speed/distance/RPM/cadence/power, and
/// out-of-range control positions are discarded. Headings wrap into
/// `[0, 360)`; signed metrics such as lean angle and acceleration axes are
/// retained unchanged.
fn validate(metric: Metric, value: f64) -> Option<f64> {
    match metric {
        Metric::Latitude if !(-90.0..=90.0).contains(&value) => None,
        Metric::Longitude if !(-180.0..=180.0).contains(&value) => None,
        Metric::Speed
        | Metric::Distance
        | Metric::DistanceToHome
        | Metric::Rpm
        | Metric::Cadence
        | Metric::Power
        | Metric::Depth
        | Metric::FirstStopDepth
        | Metric::AveragePpo2
            if value < 0.0 =>
        {
            None
        }
        Metric::FirstStopTime | Metric::TimeToSurface if value < 0.0 || value.fract() != 0.0 => {
            None
        }
        Metric::FractionO2 | Metric::FractionHe if !(0.0..=1.0).contains(&value) => None,
        Metric::ThrottlePosition | Metric::BrakePosition | Metric::EngineLoad
            if !(0.0..=100.0).contains(&value) =>
        {
            None
        }
        Metric::Heading => Some(value.rem_euclid(360.0)),
        _ => Some(value),
    }
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
