//! Cross-cutting domain types shared by config, render, and activity modules.
//!
//! Owns: `MetricKind` — the canonical enum for supported telemetry metrics
//!       (speed, heartrate, elevation, cadence, power, temperature, gradient, time).
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
}
