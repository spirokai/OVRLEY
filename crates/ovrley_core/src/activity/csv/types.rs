//! Resolved types shared by CSV header parsing and column selection.

use super::units::DeclaredUnit;
use super::Metric;

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
pub(super) enum SourceQualifier {
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
