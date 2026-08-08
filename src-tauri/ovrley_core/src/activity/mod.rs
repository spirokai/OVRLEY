//! Activity ingestion and frame-density preparation.
//!
//! The frontend supplies parsed GPX/FIT activity data as JSON. This module
//! accepts both the plain `ParsedActivity` shape and the debug wrapper shape,
//! validates the render window against the activity duration, and returns a
//! dense per-frame report containing only the telemetry series required by the
//! selected template.

/// Native CSV extraction into canonical activity columns.
pub mod csv;
pub(crate) mod elevation;
/// Backend-owned post-processing from raw extraction samples to ParsedActivity.
pub mod finalize;
/// Interpolation helpers used for numeric, coordinate, and timestamp series.
pub mod interpolate;
pub mod lap;
/// Serializable activity payloads and internal dense/trimmed report types.
pub mod schema;
/// Scene-window trimming for parsed activity samples.
pub mod trim;
/// Native Racelogic VBOX extraction into canonical activity columns.
pub mod vbo;

use crate::activity::interpolate::{densify_activity, frame_timeline_for_fps};
use crate::activity::schema::{DebugPayload, DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedRenderConfig;
use chrono_tz::Tz;
use serde_json::Value;

/// Parses frontend activity JSON from either production or debug payload shapes.
pub fn parse_activity_json(input: &str) -> CoreResult<ParsedActivity> {
    // Debug exports wrap the real payload under `parsed_activity`; production
    // calls pass the payload directly. Accept both to keep diagnostics reusable
    // in tests and local tooling.
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Activity(format!("Invalid parsedActivity JSON: {error}")))?;

    let mut activity = if value.get("parsed_activity").is_some() {
        serde_json::from_value::<DebugPayload>(value)
            .map(|payload| payload.parsed_activity)
            .map_err(|error| {
                CoreError::Activity(format!("Invalid parsedActivity debug payload: {error}"))
            })
    } else {
        serde_json::from_value(value).map_err(|error| {
            CoreError::Activity(format!("Invalid parsedActivity payload: {error}"))
        })
    }?;
    activity.timezone = parse_activity_timezone(&activity.metadata)?;
    lap::validate_lap_timing_contract(&activity)?;
    Ok(activity)
}

fn parse_activity_timezone(metadata: &Value) -> CoreResult<Option<Tz>> {
    let Some(value) = metadata.get("timezone") else {
        return Ok(None);
    };
    let Some(timezone) = value.as_str() else {
        if value == &Value::Null {
            return Ok(None);
        }
        return Err(CoreError::Activity(
            "parsedActivity metadata.timezone must be an IANA timezone string or null".into(),
        ));
    };
    timezone.parse().map(Some).map_err(|_| {
        CoreError::Activity(format!(
            "parsedActivity metadata.timezone is not a valid IANA timezone: {timezone}"
        ))
    })
}

/// Trims and densifies parsed activity data for a validated render config.
pub fn build_dense_activity_report_validated(
    activity: &ParsedActivity,
    config: &ValidatedRenderConfig,
) -> CoreResult<DenseActivityReport> {
    let requirements = config.render_data_requirements()?;
    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &requirements,
    )?;
    let duration = trimmed
        .sample_elapsed_seconds
        .last()
        .copied()
        .ok_or_else(|| CoreError::Activity("Trimmed activity has no timeline".to_string()))?;
    let frame_elapsed_seconds = frame_timeline_for_fps(duration, config.scene.fps)?;
    Ok(densify_activity(
        &trimmed,
        frame_elapsed_seconds,
        &requirements,
    ))
}

/// Trims activity through the validated scene window and densifies it on an
/// exact caller-owned frame timeline.
pub fn build_dense_activity_report_for_timeline(
    activity: &ParsedActivity,
    config: &ValidatedRenderConfig,
    frame_elapsed_seconds: Vec<f64>,
) -> CoreResult<DenseActivityReport> {
    if frame_elapsed_seconds.is_empty() {
        return Err(CoreError::Activity(
            "Dense frame timeline must contain at least one timestamp".to_string(),
        ));
    }
    let requirements = config.render_data_requirements()?;
    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &requirements,
    )?;
    let duration = *trimmed
        .sample_elapsed_seconds
        .last()
        .ok_or_else(|| CoreError::Activity("Trimmed activity has no timeline".to_string()))?;
    if frame_elapsed_seconds[0] != 0.0
        || frame_elapsed_seconds
            .iter()
            .any(|timestamp| !timestamp.is_finite() || *timestamp < 0.0 || *timestamp >= duration)
        || frame_elapsed_seconds
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(CoreError::Activity(format!(
            "Dense frame timeline must start at zero, increase strictly, and remain below duration {duration}"
        )));
    }
    Ok(densify_activity(
        &trimmed,
        frame_elapsed_seconds,
        &requirements,
    ))
}
