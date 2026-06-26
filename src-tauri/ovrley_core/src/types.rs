//! Cross-cutting domain types shared by config, render, and activity modules.
//!
//! Owns: `MetricKind` — the canonical enum for supported telemetry metrics
//!       (speed, heartrate, cadence, power, temperature, pace, g_force, and
//!       the rest of the shared standard-metric family, plus specialized
//!       elevation/gradient/time values).
//!       Replaces the pre-refactor pattern of matching raw `"speed"` / `"heartrate"`
//!       string literals scattered across 5+ files.
//! Does not own: metric formatting (see [`crate::render::format`]), metric widget
//!       rendering, or the `RenderDataRequirements` derivation (see
//!       [`crate::normalize`]).
//!
//! Allowed dependencies: `serde` (for JSON compatibility).
//! Forbidden dependencies: all other crate modules (this is a leaf dependency
//!       consumed by `config`, `render`, `activity`, and `commands`).
//!
//! ## Serde Contract
//! `MetricKind` uses `#[serde(rename = "...")]` on every variant to preserve
//! exact frontend JSON compatibility. Adding or renaming a variant requires a
//! corresponding frontend migration. The enum derives `Serialize` and
//! `Deserialize` so config/activity DTOs can use it directly without conversion.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    #[serde(rename = "speed")]
    Speed,
    #[serde(rename = "distance")]
    Distance,
    #[serde(rename = "heartrate")]
    Heartrate,
    #[serde(rename = "elevation")]
    Elevation,
    #[serde(rename = "time")]
    Time,
    #[serde(rename = "gradient")]
    Gradient,
    #[serde(rename = "cadence")]
    Cadence,
    #[serde(rename = "power")]
    Power,
    #[serde(rename = "temperature")]
    Temperature,
    #[serde(rename = "pace")]
    Pace,
    #[serde(rename = "g_force")]
    GForce,
    #[serde(rename = "air_pressure")]
    AirPressure,
    #[serde(rename = "ground_contact_time")]
    GroundContactTime,
    #[serde(rename = "left_right_balance")]
    LeftRightBalance,
    #[serde(rename = "stride_length")]
    StrideLength,
    #[serde(rename = "stroke_rate")]
    StrokeRate,
    #[serde(rename = "torque")]
    Torque,
    #[serde(rename = "vertical_speed")]
    VerticalSpeed,
    #[serde(rename = "gear_position")]
    GearPosition,
    #[serde(rename = "vertical_ratio")]
    VerticalRatio,
    #[serde(rename = "vertical_oscillation")]
    VerticalOscillation,
    #[serde(rename = "core_temperature")]
    CoreTemperature,
    #[serde(rename = "heading")]
    Heading,
    #[serde(rename = "altitude")]
    Altitude,
    #[serde(rename = "iso")]
    Iso,
    #[serde(rename = "aperture")]
    Aperture,
    #[serde(rename = "shutter_speed")]
    ShutterSpeed,
    #[serde(rename = "focal_length")]
    FocalLength,
    #[serde(rename = "ev")]
    Ev,
    #[serde(rename = "color_temperature")]
    ColorTemperature,
}

/// Visual representation mode for a value widget.
///
/// Controls how a metric value widget is rendered. The default is `Text` which
/// matches the original icon + value + unit layout. Future slices introduce
/// rendering paths for the gauge variants (linear, bars, arc, corner, and
/// heading tape) and use this field to dispatch.
///
/// ## Backward compatibility
/// When deserializing older templates that omit the field, or that carry
/// `null` or an unrecognized string, this enum falls back to `Text`. The
/// field is therefore safe to add to existing widget configs without a
/// migration: anything that does not explicitly opt in keeps its old behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum DisplayType {
    #[serde(rename = "text")]
    #[default]
    Text,
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "bars")]
    Bars,
    #[serde(rename = "arc")]
    Arc,
    #[serde(rename = "corner")]
    Corner,
    #[serde(rename = "heading_tape")]
    Tape,
}

impl DisplayType {
    /// Serialises the variant to its shared-manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            DisplayType::Text => "text",
            DisplayType::Linear => "linear",
            DisplayType::Bars => "bars",
            DisplayType::Arc => "arc",
            DisplayType::Corner => "corner",
            DisplayType::Tape => "heading_tape",
        }
    }
}

impl<'de> Deserialize<'de> for DisplayType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = serde_json::Value::deserialize(deserializer)?;
        match raw {
            serde_json::Value::String(value) => match value.as_str() {
                "text" => Ok(DisplayType::Text),
                "linear" => Ok(DisplayType::Linear),
                "bars" => Ok(DisplayType::Bars),
                "arc" => Ok(DisplayType::Arc),
                "corner" => Ok(DisplayType::Corner),
                "heading_tape" => Ok(DisplayType::Tape),
                _ => Ok(DisplayType::default()),
            },
            _ => Ok(DisplayType::default()),
        }
    }
}
