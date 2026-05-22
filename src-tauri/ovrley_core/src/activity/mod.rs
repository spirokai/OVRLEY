//! Activity ingestion and frame-density preparation.
//!
//! The frontend supplies parsed GPX/FIT activity data as JSON. This module
//! accepts both the plain `ParsedActivity` shape and the debug wrapper shape,
//! validates the render window against the activity duration, and returns a
//! dense per-frame report containing only the telemetry series required by the
//! selected template.

/// Interpolation helpers used for numeric, coordinate, and timestamp series.
pub mod interpolate;
/// Serializable activity payloads and internal dense/trimmed report types.
pub mod schema;
/// Scene-window trimming for parsed activity samples.
pub mod trim;

use crate::activity::interpolate::densify_activity;
use crate::activity::schema::{DebugPayload, DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::config::RenderConfig;
use crate::error::{CoreError, CoreResult};
use serde_json::Value;

/// Parses frontend activity JSON from either production or debug payload shapes.
pub fn parse_activity_json(input: &str) -> CoreResult<ParsedActivity> {
    // Debug exports wrap the real payload under `parsed_activity`; production
    // calls pass the payload directly. Accept both to keep diagnostics reusable
    // in tests and local tooling.
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Activity(format!("Invalid parsedActivity JSON: {error}")))?;

    if value.get("parsed_activity").is_some() {
        serde_json::from_value::<DebugPayload>(value)
            .map(|payload| payload.parsed_activity)
            .map_err(|error| {
                CoreError::Activity(format!("Invalid parsedActivity debug payload: {error}"))
            })
    } else {
        serde_json::from_value(value).map_err(|error| {
            CoreError::Activity(format!("Invalid parsedActivity payload: {error}"))
        })
    }
}

/// Trims and densifies parsed activity data for the provided render config.
pub fn build_dense_activity_report(
    activity: &ParsedActivity,
    config: &RenderConfig,
) -> CoreResult<DenseActivityReport> {
    // Data requirements are derived from the template before trimming so unused
    // high-cardinality series never get copied or densified.
    let requirements = config.render_data_requirements()?;
    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &requirements,
    )?;
    Ok(densify_activity(&trimmed, config.scene.fps, &requirements))
}
