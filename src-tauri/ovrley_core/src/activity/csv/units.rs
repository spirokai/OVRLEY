//! Unit declarations, dimensional compatibility, and canonical conversion.
//!
//! CSV headers and optional units rows can declare different spellings for
//! supported units. This module parses those declarations, distinguishes an
//! absent declaration from an unsupported one, rejects conflicting or
//! dimensionally invalid combinations, and converts accepted values to the
//! canonical units used by activity finalization.

use super::Metric;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Units understood by the CSV activity importer.
pub(super) enum Unit {
    /// Seconds.
    Seconds,
    /// Milliseconds.
    Milliseconds,
    /// Decimal degrees for coordinates.
    DecimalDegrees,
    /// Metres per second.
    MetresPerSecond,
    /// Kilometres per hour.
    KilometresPerHour,
    /// Miles per hour.
    MilesPerHour,
    /// Metres.
    Metres,
    /// Kilometres.
    Kilometres,
    /// Feet.
    Feet,
    /// Degrees.
    Degrees,
    /// Standard gravity units.
    G,
    /// Percentage points.
    Percent,
    /// Revolutions per minute.
    RevolutionsPerMinute,
    /// Unscaled numeric value.
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Result of parsing a unit declaration.
pub(super) enum DeclaredUnit {
    /// The source did not declare a unit.
    Absent,
    /// The source declared a supported unit.
    Supported(Unit),
    /// The source declared a known dimension that this importer cannot convert.
    Unsupported(UnitDimension),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Dimension of an unsupported but recognizable unit declaration.
pub(super) enum UnitDimension {
    /// Speed dimension, such as knots.
    Speed,
    /// Length dimension, such as centimetres or yards.
    Length,
    /// Unknown or unrecognized dimension.
    Unknown,
}

/// Parses a header or units-row unit spelling.
///
/// Empty values are [`DeclaredUnit::Absent`]. Recognized supported spellings
/// become [`DeclaredUnit::Supported`]; all other non-empty values become an
/// unknown unsupported declaration. Use [`parse_units_row_unit`] when parsing
/// a units row so a small set of known unsupported dimensions can be retained
/// for compatibility checks.
pub(super) fn parse_declared_unit(value: &str) -> DeclaredUnit {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return DeclaredUnit::Absent;
    }

    let unit = match normalized.as_str() {
        "s" | "sec" | "second" | "seconds" => Unit::Seconds,
        "ms" | "millisecond" | "milliseconds" => Unit::Milliseconds,
        "deg" | "degree" | "degrees" | "°" => Unit::Degrees,
        "m/s" | "mps" => Unit::MetresPerSecond,
        "km/h" | "kmh" | "kph" => Unit::KilometresPerHour,
        "mph" | "mi/h" => Unit::MilesPerHour,
        "m" | "meter" | "meters" | "metre" | "metres" => Unit::Metres,
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" => Unit::Kilometres,
        "ft" | "foot" | "feet" => Unit::Feet,
        "g" => Unit::G,
        "%" | "percent" | "percentage" => Unit::Percent,
        "rpm" => Unit::RevolutionsPerMinute,
        "#" | "raw" => Unit::Raw,
        _ => return DeclaredUnit::Unsupported(UnitDimension::Unknown),
    };
    DeclaredUnit::Supported(unit)
}

/// Parses a units-row cell while retaining known unsupported dimensions.
///
/// Unsupported speed and length spellings are returned as dimensional values
/// so callers can reject them only for the affected metric. Unknown text is
/// not a units declaration and returns `None`.
pub(super) fn parse_units_row_unit(value: &str) -> Option<DeclaredUnit> {
    let normalized = value.trim().to_ascii_lowercase();
    match parse_declared_unit(&normalized) {
        DeclaredUnit::Unsupported(_) if matches!(normalized.as_str(), "knot" | "knots" | "kt") => {
            Some(DeclaredUnit::Unsupported(UnitDimension::Speed))
        }
        DeclaredUnit::Unsupported(_) if matches!(normalized.as_str(), "cm" | "yard" | "yards") => {
            Some(DeclaredUnit::Unsupported(UnitDimension::Length))
        }
        DeclaredUnit::Unsupported(_) => None,
        declaration => Some(declaration),
    }
}

/// Reports whether a unit declaration is dimensionally valid for a metric.
///
/// Absent declarations are compatible because the metric's default unit may be
/// used. Known unsupported dimensions are compatible only for metrics where
/// their dimensions are meaningful; conversion still rejects those sources.
pub(super) fn declaration_compatible(metric: Metric, declaration: DeclaredUnit) -> bool {
    match declaration {
        DeclaredUnit::Absent => true,
        DeclaredUnit::Supported(unit) => compatible(metric, unit),
        DeclaredUnit::Unsupported(UnitDimension::Speed) => metric == Metric::Speed,
        DeclaredUnit::Unsupported(UnitDimension::Length) => {
            matches!(
                metric,
                Metric::Elevation
                    | Metric::BarometricAltitude
                    | Metric::Distance
                    | Metric::DistanceToHome
            )
        }
        DeclaredUnit::Unsupported(UnitDimension::Unknown) => false,
    }
}

/// Resolves header and units-row declarations to one supported unit.
///
/// Conflicting supported units and every unsupported declaration return
/// `None`. When both declarations are absent, the metric-specific canonical
/// default is returned.
pub(super) fn resolve_unit(
    metric: Metric,
    header: DeclaredUnit,
    units_row: DeclaredUnit,
) -> Option<Unit> {
    let declared = match (header, units_row) {
        (DeclaredUnit::Unsupported(_), _) | (_, DeclaredUnit::Unsupported(_)) => return None,
        (DeclaredUnit::Supported(left), DeclaredUnit::Supported(right)) if left != right => {
            return None
        }
        (DeclaredUnit::Supported(unit), _) | (_, DeclaredUnit::Supported(unit)) => unit,
        (DeclaredUnit::Absent, DeclaredUnit::Absent) => default_unit(metric),
    };

    compatible(metric, declared).then_some(declared)
}

/// Reports whether a supported unit can represent a canonical metric.
pub(super) fn compatible(metric: Metric, unit: Unit) -> bool {
    match metric {
        Metric::ElapsedSeconds | Metric::Timestamp => {
            matches!(unit, Unit::Seconds | Unit::Milliseconds)
        }
        Metric::Latitude | Metric::Longitude => {
            matches!(unit, Unit::Degrees | Unit::DecimalDegrees)
        }
        Metric::Speed => matches!(
            unit,
            Unit::MetresPerSecond | Unit::KilometresPerHour | Unit::MilesPerHour
        ),
        Metric::Elevation
        | Metric::BarometricAltitude
        | Metric::Distance
        | Metric::DistanceToHome => {
            matches!(unit, Unit::Metres | Unit::Kilometres | Unit::Feet)
        }
        Metric::Heading => matches!(unit, Unit::Degrees),
        Metric::GForce | Metric::GForceX | Metric::GForceY | Metric::GForceZ => {
            matches!(unit, Unit::G)
        }
        Metric::Rpm => matches!(unit, Unit::RevolutionsPerMinute),
        Metric::ThrottlePosition | Metric::BrakePosition => matches!(unit, Unit::Percent),
        Metric::LeanAngle => matches!(unit, Unit::Degrees),
        Metric::GearPosition => matches!(unit, Unit::Raw),
        Metric::CompanionDate | Metric::GpsCoordinate | Metric::LapNumber => matches!(unit, Unit::Raw),
    }
}

pub(super) fn convert(value: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Milliseconds => value / 1000.0,
        Unit::KilometresPerHour => value / 3.6,
        Unit::MilesPerHour => value * 0.44704,
        Unit::Kilometres => value * 1000.0,
        Unit::Feet => value * 0.3048,
        Unit::Seconds
        | Unit::DecimalDegrees
        | Unit::MetresPerSecond
        | Unit::Metres
        | Unit::Degrees
        | Unit::G
        | Unit::Percent
        | Unit::RevolutionsPerMinute
        | Unit::Raw => value,
    }
}

/// Returns the canonical default unit for a metric with no declaration.
fn default_unit(metric: Metric) -> Unit {
    match metric {
        Metric::ElapsedSeconds | Metric::Timestamp => Unit::Seconds,
        Metric::Latitude | Metric::Longitude => Unit::DecimalDegrees,
        Metric::Speed => Unit::KilometresPerHour,
        Metric::Elevation
        | Metric::BarometricAltitude
        | Metric::Distance
        | Metric::DistanceToHome => Unit::Metres,
        Metric::Heading => Unit::Degrees,
        Metric::GForce | Metric::GForceX | Metric::GForceY | Metric::GForceZ => Unit::G,
        Metric::Rpm => Unit::RevolutionsPerMinute,
        Metric::ThrottlePosition | Metric::BrakePosition => Unit::Percent,
        Metric::LeanAngle => Unit::Degrees,
        Metric::GearPosition => Unit::Raw,
        Metric::CompanionDate | Metric::GpsCoordinate | Metric::LapNumber => Unit::Raw,
    }
}
