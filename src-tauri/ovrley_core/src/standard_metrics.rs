//! Shared standard-metric widget definitions used by config, activity, and render.
//!
//! This module owns the backend contract for metadata-driven standard metrics:
//! display-unit defaults, formatter families, default unit visibility, and
//! shared SVG icon bindings. `time`, `gradient`, and `elevation` remain
//! specialized render paths outside this contract.

use crate::MetricKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardMetricFormatterKind {
    Speed,
    Temperature,
    Pace,
    Integer,
    Decimal,
    Balance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricIconAssetKey {
    Speed,
    Heartrate,
    Cadence,
    Power,
    Time,
    Temperature,
    Pace,
    AirPressure,
    LeftRightBalance,
    StrideLength,
    StrokeRate,
    VerticalSpeed,
    VerticalRatio,
    CoreTemperature,
    GForce,
    GroundContactTime,
    Torque,
    GearPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandardMetricDefinition {
    pub kind: MetricKind,
    pub label: &'static str,
    pub default_display_unit: &'static str,
    pub show_units_by_default: bool,
    pub formatter: StandardMetricFormatterKind,
    pub icon: MetricIconAssetKey,
}

pub const CURRENT_STANDARD_METRIC_KINDS: &[MetricKind] = &[
    MetricKind::Speed,
    MetricKind::Heartrate,
    MetricKind::Cadence,
    MetricKind::Power,
    MetricKind::Temperature,
];

pub const STANDARD_METRIC_KINDS: &[MetricKind] = &[
    MetricKind::Speed,
    MetricKind::Heartrate,
    MetricKind::Cadence,
    MetricKind::Power,
    MetricKind::Temperature,
    MetricKind::Pace,
    MetricKind::GForce,
    MetricKind::AirPressure,
    MetricKind::GroundContactTime,
    MetricKind::LeftRightBalance,
    MetricKind::StrideLength,
    MetricKind::StrokeRate,
    MetricKind::Torque,
    MetricKind::VerticalSpeed,
    MetricKind::GearPosition,
    MetricKind::VerticalRatio,
    MetricKind::CoreTemperature,
];

pub fn standard_metric_definition(kind: MetricKind) -> Option<StandardMetricDefinition> {
    let definition = match kind {
        MetricKind::Speed => StandardMetricDefinition {
            kind,
            label: "Speed",
            default_display_unit: "kmh",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Speed,
            icon: MetricIconAssetKey::Speed,
        },
        MetricKind::Heartrate => StandardMetricDefinition {
            kind,
            label: "Heart Rate",
            default_display_unit: "bpm",
            show_units_by_default: false,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::Heartrate,
        },
        MetricKind::Cadence => StandardMetricDefinition {
            kind,
            label: "Cadence",
            default_display_unit: "rpm",
            show_units_by_default: false,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::Cadence,
        },
        MetricKind::Power => StandardMetricDefinition {
            kind,
            label: "Power",
            default_display_unit: "w",
            show_units_by_default: false,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::Power,
        },
        MetricKind::Temperature => StandardMetricDefinition {
            kind,
            label: "Temperature",
            default_display_unit: "celsius",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Temperature,
            icon: MetricIconAssetKey::Temperature,
        },
        MetricKind::Pace => StandardMetricDefinition {
            kind,
            label: "Pace",
            default_display_unit: "min_per_km",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Pace,
            icon: MetricIconAssetKey::Pace,
        },
        MetricKind::GForce => StandardMetricDefinition {
            kind,
            label: "G-Force",
            default_display_unit: "g",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Decimal,
            icon: MetricIconAssetKey::GForce,
        },
        MetricKind::AirPressure => StandardMetricDefinition {
            kind,
            label: "Air Pressure",
            default_display_unit: "hpa",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::AirPressure,
        },
        MetricKind::GroundContactTime => StandardMetricDefinition {
            kind,
            label: "Ground Contact Time",
            default_display_unit: "ms",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::GroundContactTime,
        },
        MetricKind::LeftRightBalance => StandardMetricDefinition {
            kind,
            label: "Left/Right Balance",
            default_display_unit: "percent",
            show_units_by_default: false,
            formatter: StandardMetricFormatterKind::Balance,
            icon: MetricIconAssetKey::LeftRightBalance,
        },
        MetricKind::StrideLength => StandardMetricDefinition {
            kind,
            label: "Stride Length",
            default_display_unit: "m",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Decimal,
            icon: MetricIconAssetKey::StrideLength,
        },
        MetricKind::StrokeRate => StandardMetricDefinition {
            kind,
            label: "Stroke Rate",
            default_display_unit: "spm",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::StrokeRate,
        },
        MetricKind::Torque => StandardMetricDefinition {
            kind,
            label: "Torque",
            default_display_unit: "nm",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Decimal,
            icon: MetricIconAssetKey::Torque,
        },
        MetricKind::VerticalSpeed => StandardMetricDefinition {
            kind,
            label: "Vertical Speed",
            default_display_unit: "mps",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Decimal,
            icon: MetricIconAssetKey::VerticalSpeed,
        },
        MetricKind::GearPosition => StandardMetricDefinition {
            kind,
            label: "Gear Position",
            default_display_unit: "gear",
            show_units_by_default: false,
            formatter: StandardMetricFormatterKind::Integer,
            icon: MetricIconAssetKey::GearPosition,
        },
        MetricKind::VerticalRatio => StandardMetricDefinition {
            kind,
            label: "Vertical Ratio",
            default_display_unit: "percent",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Decimal,
            icon: MetricIconAssetKey::VerticalRatio,
        },
        MetricKind::CoreTemperature => StandardMetricDefinition {
            kind,
            label: "Core Temperature",
            default_display_unit: "celsius",
            show_units_by_default: true,
            formatter: StandardMetricFormatterKind::Temperature,
            icon: MetricIconAssetKey::CoreTemperature,
        },
        _ => return None,
    };

    Some(definition)
}

pub fn is_standard_metric(kind: MetricKind) -> bool {
    standard_metric_definition(kind).is_some()
}

pub fn standard_metric_default_display_unit(kind: MetricKind) -> Option<&'static str> {
    standard_metric_definition(kind).map(|definition| definition.default_display_unit)
}

pub fn standard_metric_show_units(kind: MetricKind, configured: Option<bool>) -> bool {
    configured.unwrap_or_else(|| {
        standard_metric_definition(kind)
            .map(|definition| definition.show_units_by_default)
            .unwrap_or(false)
    })
}

pub fn standard_metric_formatter(kind: MetricKind) -> Option<StandardMetricFormatterKind> {
    standard_metric_definition(kind).map(|definition| definition.formatter)
}

pub fn metric_icon_asset_key(kind: MetricKind) -> Option<MetricIconAssetKey> {
    if kind == MetricKind::Time {
        return Some(MetricIconAssetKey::Time);
    }

    standard_metric_definition(kind).map(|definition| definition.icon)
}

pub fn standard_metric_unit_label(kind: MetricKind, display_unit: Option<&str>) -> &'static str {
    let resolved = display_unit.or(standard_metric_default_display_unit(kind));

    match kind {
        MetricKind::Speed => match resolved.unwrap_or("kmh") {
            "mph" | "imperial" => "MPH",
            "kn" => "KN",
            "mps" => "M/S",
            _ => "KM/H",
        },
        MetricKind::Heartrate => "BPM",
        MetricKind::Cadence => "RPM",
        MetricKind::Power => "W",
        MetricKind::Temperature | MetricKind::CoreTemperature => {
            if resolved == Some("fahrenheit") {
                "\u{00B0}F"
            } else {
                "\u{00B0}C"
            }
        }
        MetricKind::Pace => {
            if resolved == Some("min_per_mi") {
                "MIN/MI"
            } else {
                "MIN/KM"
            }
        }
        MetricKind::GForce => {
            if resolved == Some("mps2") {
                "M/S^2"
            } else {
                "G"
            }
        }
        MetricKind::AirPressure => match resolved.unwrap_or("hpa") {
            "inhg" => "INHG",
            "mmhg" => "MMHG",
            "mbar" => "MBAR",
            _ => "HPA",
        },
        MetricKind::GroundContactTime => "MS",
        MetricKind::LeftRightBalance => "%",
        MetricKind::StrideLength => match resolved.unwrap_or("m") {
            "cm" => "CM",
            "ft" => "FT",
            "in" => "IN",
            _ => "M",
        },
        MetricKind::StrokeRate => "SPM",
        MetricKind::Torque => "NM",
        MetricKind::VerticalSpeed => match resolved.unwrap_or("mps") {
            "ftmin" => "FT/MIN",
            "mph_vertical" => "M/H",
            _ => "M/S",
        },
        MetricKind::GearPosition => "GEAR",
        MetricKind::VerticalRatio => "%",
        _ => "",
    }
}
