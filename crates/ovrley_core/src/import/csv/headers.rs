//! CSV header resolution: exact alias parsing, source qualifiers and units-row recognition.
//!
//! Exporters use different spellings and qualifiers for the same telemetry. Only superficial
//! syntax is normalized before an explicit alias registry produces each [`HeaderColumn`].
//! Source qualifiers become a deterministic priority, so one canonical metric is selected even
//! when a file holds GPS, calculated and vehicle copies. A following row is a units row only
//! when every recognized column holds no declaration or a dimension-compatible one, and at
//! least one declaration is present, so ordinary data rows are never consumed as metadata.

use super::units::{
    declaration_compatible, parse_declared_unit, parse_units_row_unit, DeclaredUnit, Unit,
    UnitDimension,
};
use super::Metric;
use crate::error::{CoreError, CoreResult};
use csv::StringRecord;

/// Precedence used when multiple columns provide the same metric.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum SourcePriority {
    /// OBD speed reported by the vehicle.
    ObdSpeed,
    /// Unqualified or logger-provided direct speed.
    DirectSpeed,
    /// Preferred direct or source-qualified measurement.
    Preferred,
    /// Accelerometer source for acceleration axes.
    AccelerationSensor,
    /// Accelerator-pedal source for throttle position.
    Pedal,
    /// Vehicle-state source for engine and control metrics.
    VehicleState,
    /// Unqualified direct measurement.
    Direct,
    /// Calculated or derived source.
    Calculated,
    /// Generic vehicle source.
    Vehicle,
}

/// Interpretation of a control-position column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ControlKind {
    /// Values are already percentages in the range zero to one hundred.
    Percentage,
    /// Values are binary states mapped to zero or one hundred percent.
    Binary,
    /// Infer binary semantics when the complete present column is zero/one.
    Infer,
}

/// Distinguishes semantic acceleration axes from literal X/Y/Z axes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AccelerationKind {
    /// The header names a literal sensor axis.
    Literal,
    /// The header names a vehicle semantic axis such as lateral acceleration.
    Semantic,
}

/// Qualifier identifying the system that produced a CSV column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceQualifier {
    /// GPS-derived source.
    Gps,
    /// On-board diagnostics source.
    Obd,
    /// Software-calculated source.
    Calculated,
    /// Vehicle bus or vehicle-state source.
    Vehicle,
    /// Dedicated accelerometer source.
    Accelerometer,
    /// Generic logger source.
    Logger,
}

/// Resolved meaning and selection metadata for one CSV column.
#[derive(Clone, Debug)]
pub(super) struct HeaderColumn {
    /// Zero-based position of the column in the source record.
    pub index: usize,
    /// Canonical metric represented by this column.
    pub metric: Metric,
    /// Source-selection precedence for duplicate metric candidates.
    pub priority: SourcePriority,
    /// Unit declared in the header annotation, if any.
    pub declared_unit: DeclaredUnit,
    /// Timing interpretation attached to this column, if it is a timing field.
    pub timing: Option<TimingKind>,
    /// Control-value interpretation, if this is throttle or brake data.
    pub control: Option<ControlKind>,
    /// Literal or semantic acceleration interpretation, if applicable.
    pub acceleration: Option<AccelerationKind>,
}

/// Timing role assigned to a recognized header alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TimingKind {
    /// Bare `Time`, interpreted as elapsed time unless paired with elapsed time.
    BareTime,
    /// Bare `Timestamp`, eligible for absolute timestamp parsing or local fallback.
    BareTimestamp,
    /// Explicit elapsed-time column.
    ExplicitElapsed,
    /// Explicit UTC/absolute timestamp column.
    ExplicitUtc,
    /// Bare numeric time paired with an explicit elapsed-time column.
    PairedUnix,
    /// Time-of-day value paired with a companion Date column for absolute
    /// timestamps.
    TimeOfDay,
}

/// Resolved telemetry layout.
#[derive(Clone, Debug)]
pub(super) struct HeaderLayout {
    /// Recognized columns from that header, in source order.
    pub columns: Vec<HeaderColumn>,
    /// Optional TrackAddict-style flag identifying rows with fresh GPS data.
    pub gps_update_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeaderProfile {
    Generic,
    DjiAirData,
    Shearwater,
}

/// Parses one record as a usable telemetry-header candidate.
///
/// A candidate must contain at least one recognized timing column and one
/// recognized non-timing metric. When explicit elapsed time is present, a bare
/// seconds-valued `Time` column is reclassified as a paired absolute Unix-time
/// column, matching RaceChrono's two-column export shape.
pub(super) fn parse_header_candidate(record: &StringRecord) -> CoreResult<Option<HeaderLayout>> {
    let gps_update_index = record
        .iter()
        .position(|value| normalize_syntax(value) == "gps update");
    let profile = detect_header_profile(record);
    let mut columns = record
        .iter()
        .enumerate()
        .filter_map(|(index, value)| parse_header(index, value, profile))
        .collect::<Vec<_>>();
    let has_timing = columns.iter().any(|column| column.metric.is_timing());
    let has_telemetry = columns.iter().any(|column| !column.metric.is_timing());
    if has_timing && has_telemetry {
        resolve_profile_aliases(record, &mut columns, profile)?;
    }
    let has_explicit_elapsed = columns
        .iter()
        .any(|column| column.timing == Some(TimingKind::ExplicitElapsed));
    if has_explicit_elapsed {
        for column in &mut columns {
            if column.timing == Some(TimingKind::BareTime)
                && column.declared_unit == DeclaredUnit::Supported(Unit::Seconds)
            {
                column.metric = Metric::Timestamp;
                column.timing = Some(TimingKind::PairedUnix);
            }
        }
    }
    // A bare Time column paired with a Date column is a local time-of-day that
    // needs the companion date to reconstruct an absolute timestamp.
    let has_companion_date = columns
        .iter()
        .any(|column| column.metric == Metric::CompanionDate);
    if has_companion_date {
        for column in &mut columns {
            if column.timing == Some(TimingKind::BareTime) {
                column.metric = Metric::Timestamp;
                column.timing = Some(TimingKind::TimeOfDay);
            }
        }
    }
    Ok((has_timing && has_telemetry).then_some(HeaderLayout {
        columns,
        gps_update_index,
    }))
}

fn detect_header_profile(record: &StringRecord) -> HeaderProfile {
    if ["depth", "average ppo2", "fraction o2", "fraction he"]
        .iter()
        .all(|semantic| has_header_semantic(record, semantic))
    {
        return HeaderProfile::Shearwater;
    }
    let has_distance = has_header_semantic(record, "distance");
    let has_mileage = has_header_semantic(record, "mileage");
    let has_airdata_marker = has_header_semantic(record, "compass heading")
        || has_header_semantic(record, "altitude above sealevel");

    if has_distance && has_mileage && has_airdata_marker {
        HeaderProfile::DjiAirData
    } else {
        HeaderProfile::Generic
    }
}

fn has_header_semantic(record: &StringRecord, semantic: &str) -> bool {
    record.iter().any(|value| {
        split_header(value).is_some_and(|(header_semantic, _, _)| header_semantic == semantic)
    })
}

fn resolve_profile_aliases(
    record: &StringRecord,
    columns: &mut [HeaderColumn],
    profile: HeaderProfile,
) -> CoreResult<()> {
    let has_distance = has_header_semantic(record, "distance");
    let has_mileage = has_header_semantic(record, "mileage");
    if has_distance && has_mileage && profile == HeaderProfile::Generic {
        return Err(CoreError::Activity(
            "CSV has ambiguous Distance and Mileage headers without a recognized exporter profile"
                .to_string(),
        ));
    }

    if profile == HeaderProfile::DjiAirData {
        for column in columns {
            let is_distance_alias = record
                .get(column.index)
                .and_then(split_header)
                .is_some_and(|(semantic, _, _)| semantic == "distance");
            if is_distance_alias {
                column.metric = Metric::DistanceToHome;
            }
        }
    }

    Ok(())
}

/// Checks whether a record is a compatible units row for resolved headers.
///
/// Unknown unit text, incompatible dimensions, or conflicting declarations
/// reject the entire row. Empty cells are allowed, but at least one recognized
/// unit declaration must be present for the row to qualify.
pub(super) fn is_compatible_units_row(record: &StringRecord, columns: &[HeaderColumn]) -> bool {
    let mut declaration_count = 0;
    for column in columns {
        let Some(declaration) = parse_units_row_unit(record.get(column.index).unwrap_or_default())
        else {
            return false;
        };
        match declaration {
            DeclaredUnit::Absent => {}
            declaration if declaration_compatible(column.metric, declaration) => {
                declaration_count += 1;
            }
            _ => return false,
        }
    }
    declaration_count > 0
}

/// Resolves one normalized header cell through the explicit alias registry.
///
/// Unknown aliases return `None`. Header annotations supply units or control
/// semantics, while source qualifiers resolve source-specific meanings such as
/// absolute GPS time and otherwise adjust precedence.
fn parse_header(index: usize, value: &str, profile: HeaderProfile) -> Option<HeaderColumn> {
    let (semantic, annotation, source) = split_header(value)?;
    let declared_unit = annotation
        .as_deref()
        .map(parse_declared_unit)
        .unwrap_or(DeclaredUnit::Absent);
    let (metric, alias_priority, timing, mut control, acceleration) = match semantic.as_str() {
        "datetime" => (
            Metric::Timestamp,
            SourcePriority::Direct,
            Some(TimingKind::ExplicitUtc),
            None,
            None,
        ),
        "time" if source == Some(SourceQualifier::Gps) => (
            Metric::Timestamp,
            SourcePriority::Direct,
            Some(TimingKind::ExplicitUtc),
            None,
            None,
        ),
        "time" => (
            Metric::ElapsedSeconds,
            SourcePriority::Direct,
            Some(TimingKind::BareTime),
            None,
            None,
        ),
        "date" => (
            Metric::CompanionDate,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "timestamp" => (
            Metric::ElapsedSeconds,
            SourcePriority::Direct,
            Some(TimingKind::BareTimestamp),
            None,
            None,
        ),
        "elapsed time" | "time s" | "elapsedseconds" => (
            Metric::ElapsedSeconds,
            SourcePriority::Preferred,
            Some(TimingKind::ExplicitElapsed),
            None,
            None,
        ),
        "utc time" => (
            Metric::Timestamp,
            SourcePriority::Direct,
            Some(TimingKind::ExplicitUtc),
            None,
            None,
        ),
        "latitude" => (Metric::Latitude, SourcePriority::Direct, None, None, None),
        "longitude" => (Metric::Longitude, SourcePriority::Direct, None, None, None),
        "depth" if profile == HeaderProfile::Shearwater => {
            (Metric::Depth, SourcePriority::Direct, None, None, None)
        }
        "first stop depth" if profile == HeaderProfile::Shearwater => (
            Metric::FirstStopDepth,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "first stop time" if profile == HeaderProfile::Shearwater => (
            Metric::FirstStopTime,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "time to surface" if profile == HeaderProfile::Shearwater => (
            Metric::TimeToSurface,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "average ppo2" if profile == HeaderProfile::Shearwater => (
            Metric::AveragePpo2,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "fraction o2" if profile == HeaderProfile::Shearwater => {
            (Metric::FractionO2, SourcePriority::Direct, None, None, None)
        }
        "fraction he" if profile == HeaderProfile::Shearwater => {
            (Metric::FractionHe, SourcePriority::Direct, None, None, None)
        }
        "water temp" if profile == HeaderProfile::Shearwater => (
            Metric::WaterTemperature,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "gps" => (
            Metric::GpsCoordinate,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "speed" | "kph" | "speed kmh" => {
            (Metric::Speed, SourcePriority::DirectSpeed, None, None, None)
        }
        "gspd" | "speedmps" => (Metric::Speed, SourcePriority::DirectSpeed, None, None, None),
        "vehspd1" | "controllerspeedmps" => {
            (Metric::Speed, SourcePriority::Vehicle, None, None, None)
        }
        "distance" | "trip distance" => {
            (Metric::Distance, SourcePriority::Direct, None, None, None)
        }
        "mileage" => (Metric::Distance, SourcePriority::Direct, None, None, None),
        "distance to home" | "distance from home" | "home distance" => (
            Metric::DistanceToHome,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "distance 2d" | "distance on gps speed" => (
            Metric::Distance,
            SourcePriority::Preferred,
            None,
            None,
            None,
        ),
        "elevation" | "altitude" | "alt" | "altitude above sealevel" | "altitudemeters" => {
            (Metric::Elevation, SourcePriority::Direct, None, None, None)
        }
        "pressure altitude" | "barometric altitude" => (
            Metric::BarometricAltitude,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "heading" | "bearing" | "hdg" | "compass heading" | "coursedegrees" => {
            (Metric::Heading, SourcePriority::Direct, None, None, None)
        }
        "accel xyz" | "combined acceleration" => {
            (Metric::GForce, SourcePriority::Direct, None, None, None)
        }
        "gravity x" | "gforcex" | "x" | "accel x" | "x acceleration" => (
            Metric::GForceX,
            SourcePriority::AccelerationSensor,
            None,
            None,
            Some(AccelerationKind::Literal),
        ),
        "gravity y" | "gforcey" | "y" | "accel y" | "y acceleration" => (
            Metric::GForceY,
            SourcePriority::AccelerationSensor,
            None,
            None,
            Some(AccelerationKind::Literal),
        ),
        "gravity z" | "gforcez" | "z" | "accel z" | "z acceleration" => (
            Metric::GForceZ,
            SourcePriority::AccelerationSensor,
            None,
            None,
            Some(AccelerationKind::Literal),
        ),
        "lateralacc" | "lateral acceleration" => (
            Metric::GForceX,
            SourcePriority::Direct,
            None,
            None,
            Some(AccelerationKind::Semantic),
        ),
        "inlineacc" | "inline acceleration" | "longitudinal acceleration" => (
            Metric::GForceY,
            SourcePriority::Direct,
            None,
            None,
            Some(AccelerationKind::Semantic),
        ),
        "verticalacc" | "vertical acceleration" => (
            Metric::GForceZ,
            SourcePriority::Direct,
            None,
            None,
            Some(AccelerationKind::Semantic),
        ),
        "rpm" | "engine rpm" | "motorrpm" => {
            (Metric::Rpm, SourcePriority::Direct, None, None, None)
        }
        "cadencerpm" => (Metric::Cadence, SourcePriority::Direct, None, None, None),
        "humanpowerw" => (Metric::Power, SourcePriority::Direct, None, None, None),
        "power"
        | "estimated power"
        | "engine kw (at the wheels)"
        | "horsepower (at the wheels)"
        | "motorinputpowerw" => (
            Metric::EnginePower,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "torque" | "estimated torque" | "pedaltorquenm" => {
            (Metric::Torque, SourcePriority::Direct, None, None, None)
        }
        "coolant temperature"
        | "coolant temp"
        | "coolant"
        | "engine coolant temperature"
        | "engine coolant temp"
        | "watertemp"
        | "water temp" => (
            Metric::CoolantTemperature,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "coolant temperature c" | "coolant temp c" => (
            Metric::CoolantTemperature,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "boost"
        | "boost pressure"
        | "turbo boost"
        | "turbo boost pressure"
        | "turbo boost & vacuum gauge" => (
            Metric::BoostPressure,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "engine load" | "engine load absolute" | "absolute engine load" => (
            Metric::EngineLoad,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Percentage),
            None,
        ),
        "accelerator position" | "accelerator pedal position" | "pedal pos" => (
            Metric::ThrottlePosition,
            SourcePriority::Pedal,
            None,
            Some(ControlKind::Percentage),
            None,
        ),
        "throttle position"
        | "throttlepos"
        | "throttle position(manifold)"
        | "throttle position (manifold)"
        | "throttle position manifold"
        | "manifold throttle position"
        | "relative throttle position"
        | "tps pct" => (
            Metric::ThrottlePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Percentage),
            None,
        ),
        "throttle" => (
            Metric::ThrottlePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Infer),
            None,
        ),
        "throttle state" | "throttle on/off" => (
            Metric::ThrottlePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Binary),
            None,
        ),
        "brake position" | "brake pedal" => (
            Metric::BrakePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Percentage),
            None,
        ),
        "braking" => (
            Metric::BrakePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Infer),
            None,
        ),
        "brake state" | "brake on/off" => (
            Metric::BrakePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Binary),
            None,
        ),
        "brake" => (
            Metric::BrakePosition,
            SourcePriority::Direct,
            None,
            Some(ControlKind::Infer),
            None,
        ),
        "lean angle" | "leanangle" => (Metric::LeanAngle, SourcePriority::Direct, None, None, None),
        "lap" | "lap #" | "lap number" => {
            (Metric::LapNumber, SourcePriority::Direct, None, None, None)
        }
        "gear" | "gear position" | "estimated gear" => (
            Metric::GearPosition,
            SourcePriority::Direct,
            None,
            None,
            None,
        ),
        "calculatedgear" | "calculated gear" => (
            Metric::GearPosition,
            SourcePriority::Calculated,
            None,
            None,
            None,
        ),
        _ => return None,
    };
    // camelCase exporters spell the unit into the header name instead of an
    // annotation; a contradicting annotation makes the column unusable.
    let embedded_unit = match semantic.as_str() {
        "elapsedseconds" => Some(Unit::Seconds),
        "speedmps" | "controllerspeedmps" => Some(Unit::MetresPerSecond),
        "altitudemeters" => Some(Unit::Metres),
        "coursedegrees" => Some(Unit::Degrees),
        "motorrpm" | "cadencerpm" => Some(Unit::RevolutionsPerMinute),
        "coolant temperature c" | "coolant temp c" => Some(Unit::Celsius),
        "humanpowerw" | "motorinputpowerw" => Some(Unit::Watts),
        "pedaltorquenm" => Some(Unit::NewtonMetres),
        _ => None,
    };
    let mut declared_unit = match (declared_unit, embedded_unit) {
        (DeclaredUnit::Absent, Some(unit)) => DeclaredUnit::Supported(unit),
        (DeclaredUnit::Supported(declared), Some(unit)) if declared != unit => {
            DeclaredUnit::Unsupported(UnitDimension::Unknown)
        }
        (declared, _) => declared,
    };
    if metric == Metric::Timestamp
        && annotation.as_deref().is_some_and(|ann| {
            ann.eq_ignore_ascii_case("utc")
                || ann.eq_ignore_ascii_case("gmt")
                || ann.eq_ignore_ascii_case("z")
        })
    {
        declared_unit = DeclaredUnit::Absent;
    }
    if matches!(
        (metric, declared_unit),
        (
            Metric::ThrottlePosition | Metric::BrakePosition | Metric::EngineLoad,
            DeclaredUnit::Supported(Unit::Percent)
        )
    ) {
        control = Some(ControlKind::Percentage);
    }
    let source_priority = match (metric, source) {
        (Metric::Speed, Some(SourceQualifier::Obd)) => SourcePriority::ObdSpeed,
        (Metric::Speed, Some(SourceQualifier::Accelerometer | SourceQualifier::Logger)) => {
            SourcePriority::DirectSpeed
        }
        (
            Metric::Latitude
            | Metric::Longitude
            | Metric::Speed
            | Metric::Distance
            | Metric::Elevation
            | Metric::Heading,
            Some(SourceQualifier::Gps),
        ) => SourcePriority::Preferred,
        (Metric::GForceX | Metric::GForceY | Metric::GForceZ, Some(SourceQualifier::Gps)) => {
            SourcePriority::Direct
        }
        (_, Some(SourceQualifier::Calculated)) => SourcePriority::Calculated,
        (
            Metric::ThrottlePosition,
            Some(SourceQualifier::Obd | SourceQualifier::Vehicle | SourceQualifier::Logger),
        ) if alias_priority == SourcePriority::Pedal => SourcePriority::Preferred,
        (
            Metric::GearPosition
            | Metric::Rpm
            | Metric::ThrottlePosition
            | Metric::BrakePosition
            | Metric::EngineLoad,
            Some(SourceQualifier::Obd | SourceQualifier::Vehicle | SourceQualifier::Logger),
        ) => SourcePriority::VehicleState,
        (
            Metric::GForceX | Metric::GForceY | Metric::GForceZ,
            Some(SourceQualifier::Accelerometer | SourceQualifier::Logger),
        ) => SourcePriority::AccelerationSensor,
        (_, Some(SourceQualifier::Obd | SourceQualifier::Vehicle)) => SourcePriority::Vehicle,
        (_, Some(SourceQualifier::Accelerometer | SourceQualifier::Logger)) => {
            SourcePriority::Direct
        }
        (_, Some(SourceQualifier::Gps)) => alias_priority,
        _ => alias_priority,
    };

    Some(HeaderColumn {
        index,
        metric,
        priority: source_priority,
        declared_unit,
        timing,
        control,
        acceleration,
    })
}

/// Splits a header cell into semantic name, unit annotation, and source.
///
/// Supported forms include parenthetical units with or without a leading space
/// (`Alt(m)` and `Altitude (m)`), a trailing percent marker, parenthetical
/// source qualifiers, combined `(source)(unit)` suffixes, and a ` *source`
/// suffix. Contradictory source qualifiers are rejected.
fn split_header(value: &str) -> Option<(String, Option<String>, Option<SourceQualifier>)> {
    let normalized = normalize_syntax(value);
    let (without_source, explicit_source) = match normalized.rsplit_once(" *") {
        Some((base, source)) => (base, Some(parse_source(source)?)),
        None => (normalized.as_str(), None),
    };
    if let Some((base, annotation)) = split_parenthetical(without_source) {
        if let Some(parenthetical_source) = parse_source(annotation) {
            let source = combine_sources(explicit_source, Some(parenthetical_source))?;
            return split_semantic_source(base, None, source);
        }
        if let Some((semantic, qualifier)) = split_parenthetical(base) {
            if let Some(parenthetical_source) = parse_source(qualifier) {
                let source = combine_sources(explicit_source, Some(parenthetical_source))?;
                return split_semantic_source(semantic, Some(annotation.to_string()), source);
            }
        }
        return split_semantic_source(base, Some(annotation.to_string()), explicit_source);
    }
    if let Some(semantic) = without_source.strip_suffix(" %") {
        return split_semantic_source(semantic, Some("%".to_string()), explicit_source);
    }
    split_semantic_source(without_source, None, explicit_source)
}

fn split_parenthetical(value: &str) -> Option<(&str, &str)> {
    if !value.ends_with(')') {
        return None;
    }
    let open = value.rfind('(')?;
    Some((value[..open].trim(), &value[open + 1..value.len() - 1]))
}

/// Removes a leading source qualifier from a normalized semantic name.
///
/// The optional explicit suffix/parenthetical source must agree with the
/// leading qualifier when both are present.
fn split_semantic_source(
    semantic: &str,
    annotation: Option<String>,
    explicit_source: Option<SourceQualifier>,
) -> Option<(String, Option<String>, Option<SourceQualifier>)> {
    let (semantic, semantic_source) = [
        ("gps ", SourceQualifier::Gps),
        ("calculated ", SourceQualifier::Calculated),
        ("obd ", SourceQualifier::Obd),
        ("can ", SourceQualifier::Vehicle),
        ("vehicle ", SourceQualifier::Vehicle),
        ("accelerometer ", SourceQualifier::Accelerometer),
        ("logger ", SourceQualifier::Logger),
    ]
    .into_iter()
    .find_map(|(prefix, source)| semantic.strip_prefix(prefix).map(|base| (base, source)))
    .map_or((semantic, None), |(base, source)| (base, Some(source)));
    let source = combine_sources(explicit_source, semantic_source)?;
    Some((semantic.to_string(), annotation, source))
}

/// Combines two optional source qualifiers when they are consistent.
///
/// `None` as the return value means both qualifiers were present but differed;
/// `Some(None)` means neither qualifier was supplied.
fn combine_sources(
    left: Option<SourceQualifier>,
    right: Option<SourceQualifier>,
) -> Option<Option<SourceQualifier>> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => None,
        (Some(source), _) | (_, Some(source)) => Some(Some(source)),
        (None, None) => Some(None),
    }
}

/// Parses one normalized source-qualifier spelling.
fn parse_source(source: &str) -> Option<SourceQualifier> {
    match source {
        "gps" => Some(SourceQualifier::Gps),
        "calc" | "calculated" => Some(SourceQualifier::Calculated),
        "obd" => Some(SourceQualifier::Obd),
        "vehicle" | "can" => Some(SourceQualifier::Vehicle),
        "acc" | "accelerometer" => Some(SourceQualifier::Accelerometer),
        "logger" => Some(SourceQualifier::Logger),
        _ => None,
    }
}

/// Normalizes BOMs, whitespace, separators, and ASCII case in a header cell.
///
/// This deliberately does not perform fuzzy or substring matching; the result
/// is still resolved only by the exact aliases in [`parse_header`].
fn normalize_syntax(value: &str) -> String {
    value
        .trim_start_matches('\u{feff}')
        .trim()
        .chars()
        .map(|character| match character {
            '_' | '-' => ' ',
            other => other.to_ascii_lowercase(),
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
