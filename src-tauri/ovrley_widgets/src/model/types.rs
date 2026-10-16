//! Cross-cutting domain types shared by config, render, and activity modules.
//!
//! Owns: `MetricKind` — the canonical enum for supported telemetry metrics
//!       (speed, heartrate, cadence, power, temperature, pace, g_force, and
//!       the rest of the shared standard-metric family, plus specialized
//!       elevation/gradient/time values).
//!       Replaces the pre-refactor pattern of matching raw `"speed"` / `"heartrate"`
//!       string literals scattered across 5+ files.
//! Metric formatting and drawing live in [`crate::widgets`]. Native data requirements
//! and export scheduling remain in `ovrley_core`.
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

include!(concat!(env!("OUT_DIR"), "/metric_kinds.rs"));

/// Visual representation mode for a value widget.
///
/// Controls how a metric value widget is rendered. The default is `Text` which
/// matches the original icon + value + unit layout. Future slices introduce
/// rendering paths for the gauge variants (linear, arc, corner, and
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
    #[serde(rename = "arc")]
    Arc,
    #[serde(rename = "corner")]
    Corner,
    #[serde(rename = "heading_tape")]
    Tape,
    #[serde(rename = "lean_angle")]
    LeanAngle,
    #[serde(rename = "g_force")]
    GForce,
    #[serde(rename = "lap_timer")]
    LapTimer,
}

impl DisplayType {
    /// Serialises the variant to its shared-manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            DisplayType::Text => "text",
            DisplayType::Linear => "linear",
            DisplayType::Arc => "arc",
            DisplayType::Corner => "corner",
            DisplayType::Tape => "heading_tape",
            DisplayType::LeanAngle => "lean_angle",
            DisplayType::GForce => "g_force",
            DisplayType::LapTimer => "lap_timer",
        }
    }
}

/// Visual representation mode for a backdrop widget.
///
/// Backdrops have no legacy representation, so serde deserialization is
/// intentionally strict: unknown strings and `null` are rejected by serde
/// instead of falling back to a default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackdropType {
    Circle,
    #[default]
    Rectangle,
}

impl BackdropType {
    /// Serialises the variant to its shared-manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            BackdropType::Circle => "circle",
            BackdropType::Rectangle => "rectangle",
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
                "arc" => Ok(DisplayType::Arc),
                "corner" => Ok(DisplayType::Corner),
                "heading_tape" => Ok(DisplayType::Tape),
                "lean_angle" => Ok(DisplayType::LeanAngle),
                "g_force" => Ok(DisplayType::GForce),
                "lap_timer" => Ok(DisplayType::LapTimer),
                _ => Ok(DisplayType::default()),
            },
            _ => Ok(DisplayType::default()),
        }
    }
}

/// Geometry/paint mode used by gauge tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackFillStyle {
    #[serde(rename = "fill")]
    #[default]
    Fill,
    #[serde(rename = "bars")]
    Bars,
}

impl TrackFillStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Bars => "bars",
        }
    }
}
