//! Metric source selection, conversion, and validation for CSV columns.

use super::data::CsvColumnData;
use super::headers::{AccelerationKind, ControlKind, HeaderColumn};
use super::units::{convert, parse_declared_unit, resolve_unit, DeclaredUnit, Unit};
use super::Metric;
use crate::activity::schema::{GearSeries, NumericSeries};
use csv::StringRecord;

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
pub(super) fn selected_series_with_column<'a>(
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
pub(super) fn selected_acceleration_series(
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
pub(super) fn selected_gear_series(
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
pub(super) fn select_column<'a>(
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
/// Invalid coordinates, negative speed/distance/RPM, and out-of-range control
/// positions are discarded. Headings wrap into `[0, 360)`; signed metrics such
/// as lean angle and acceleration axes are retained unchanged.
fn validate(metric: Metric, value: f64) -> Option<f64> {
    match metric {
        Metric::Latitude if !(-90.0..=90.0).contains(&value) => None,
        Metric::Longitude if !(-180.0..=180.0).contains(&value) => None,
        Metric::Speed | Metric::Distance | Metric::DistanceToHome | Metric::Rpm if value < 0.0 => {
            None
        }
        Metric::ThrottlePosition | Metric::BrakePosition | Metric::EngineLoad
            if !(0.0..=100.0).contains(&value) =>
        {
            None
        }
        Metric::Heading => Some(value.rem_euclid(360.0)),
        _ => Some(value),
    }
}
