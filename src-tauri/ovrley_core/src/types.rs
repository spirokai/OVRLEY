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
//!       [`crate::config`]).
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

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    #[serde(rename = "speed")]
    Speed,
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
    #[serde(rename = "core_temperature")]
    CoreTemperature,
}
