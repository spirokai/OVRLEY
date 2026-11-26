//! The canonical activity and its preparation for rendering: ingress normalization, timeline
//! validation, scene trimming and per-frame densification of the series a template needs.

pub mod interpolate;
pub(crate) mod laps;
pub mod sampling;
pub mod schema;
pub mod splits;
pub mod trim;

use crate::activity::interpolate::{densify_activity, frame_timeline_for_fps};
use crate::activity::schema::{DebugPayload, DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::error::{CoreError, CoreResult};
use crate::config::ValidatedRenderConfig;
use chrono_tz::Tz;
use serde_json::Value;

/// Parses and normalizes an activity JSON document. A debug dump of the import, which wraps the
/// activity under `parsed_activity`, is accepted too, so dumps can feed tools and tests.
pub fn parse_activity_json(input: &str) -> CoreResult<ParsedActivity> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Activity(format!("Invalid parsedActivity JSON: {error}")))?;

    let activity = if value.get("parsed_activity").is_some() {
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
    normalize_parsed_activity(activity)
}

/// JSON ingress of an activity inside a request (batch submission): deserialized, then
/// normalized once, and shared from there on.
pub(crate) fn deserialize_parsed_activity<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<std::sync::Arc<ParsedActivity>, D::Error> {
    let activity = <ParsedActivity as serde::Deserialize>::deserialize(deserializer)?;
    normalize_parsed_activity(activity)
        .map(std::sync::Arc::new)
        .map_err(serde::de::Error::custom)
}

/// Normalizes a deserialized activity: its timezone and lap timing.
pub fn normalize_parsed_activity(mut activity: ParsedActivity) -> CoreResult<ParsedActivity> {
    activity.timezone = parse_activity_timezone(&activity.metadata)?;
    laps::validate(&activity)?;
    Ok(activity)
}

/// Checks that the activity has a renderable timeline (at least two finite, non-decreasing
/// samples ending after zero) and returns its end.
pub fn validate_render_activity(activity: &ParsedActivity) -> CoreResult<f64> {
    let activity_end = activity.trim_end_seconds.max(
        activity
            .sample_elapsed_seconds
            .last()
            .copied()
            .unwrap_or_default(),
    );
    if !activity_end.is_finite()
        || activity_end <= 0.0
        || activity.sample_elapsed_seconds.len() < 2
        || activity
            .sample_elapsed_seconds
            .iter()
            .any(|time| !time.is_finite())
        || activity
            .sample_elapsed_seconds
            .windows(2)
            .any(|pair| pair[1] < pair[0])
    {
        return Err(CoreError::Activity(
            "Rendering requires a positive activity timeline".into(),
        ));
    }
    Ok(activity_end)
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

/// Trims an activity to the validated scene window and densifies it at the scene frame rate.
pub fn build_dense_report(
    activity: &ParsedActivity,
    config: &ValidatedRenderConfig,
) -> CoreResult<DenseActivityReport> {
    let frames = frame_timeline_for_fps(config.scene.end - config.scene.start, config.scene.fps)?;
    build_dense_report_for_timeline(activity, config, frames)
}

/// Trims an activity to the validated scene window and densifies it on a planned frame
/// timeline. Fractional coverage may start between frame ticks; a positive overlap between
/// ticks may have no covered frames.
pub(crate) fn build_dense_report_for_timeline(
    activity: &ParsedActivity,
    config: &ValidatedRenderConfig,
    frame_elapsed_seconds: Vec<f64>,
) -> CoreResult<DenseActivityReport> {
    let requirements = config.render_data_requirements();
    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &requirements,
    )?;
    Ok(densify_activity(
        &trimmed,
        frame_elapsed_seconds,
        &requirements,
    ))
}
