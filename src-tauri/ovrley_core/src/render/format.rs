//! Metric formatting for dynamic overlay values.
//!
//! Supplies dense samples to shared metric formatting.

use crate::activity::schema::DenseActivityReport;
use ovrley_widgets::model::standard_metrics::{
    standard_metric_formatter, StandardMetricFormatterKind,
};
use ovrley_widgets::model::types::MetricKind;
use ovrley_widgets::widgets::value::validation::ValidatedValueWidget;

use ovrley_widgets::widgets::value::format::MetricDisplayParts;

/// Returns the dense frame index corresponding to an absolute preview second.
pub fn frame_index_for_second(
    scene: &crate::normalize::ValidatedSceneConfig,
    dense_activity: &DenseActivityReport,
    second: f64,
) -> usize {
    if dense_activity.frame_count == 0 {
        return 0;
    }

    let relative_second = (second - scene.start).clamp(0.0, scene.end - scene.start);
    let index = (relative_second * scene.fps).round() as isize;
    index.clamp(0, dense_activity.frame_count.saturating_sub(1) as isize) as usize
}

pub(crate) fn altitude_offset_m(starting_altitude_m: Option<f64>, series: &[Option<f64>]) -> f64 {
    starting_altitude_m
        .zip(series.iter().flatten().next().copied())
        .map(|(target, first)| target - first)
        .unwrap_or(0.0)
}

/// Looks up one raw numeric sample by metric key and frame index.
fn raw_value(
    validated: &ValidatedValueWidget,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    altitude_offset_m: f64,
) -> Option<f64> {
    dense_activity
        .series
        .numeric_series_for(validated.metric)
        .and_then(|series| series.get(frame_index).copied().flatten())
        .map(|value| value + altitude_offset_m)
}

/// Resolves the numeric value shown by text and numeric gauge widgets.
///
/// Preserve gaps remain `None` in the dense series and therefore do not affect
/// ranges or geometry. Once a present preserve series reaches display, its gap
/// is the documented numeric zero. An empty series remains unavailable.
pub(crate) fn resolve_metric_display_value(
    kind: MetricKind,
    raw: Option<f64>,
    dense_activity: &DenseActivityReport,
) -> Option<f64> {
    ovrley_widgets::widgets::value::format::resolve_metric_display_value(
        kind,
        raw,
        dense_metric_series_is_available(kind, dense_activity),
    )
}

/// Builds metric display parts from a validated value widget.
///
/// All output-affecting fields are already explicit — no backend-owned defaults
/// are applied. Raw telemetry is looked up by metric kind and formatted using
/// the validated formatting contract.
pub fn format_validated_metric_parts(
    validated: &ValidatedValueWidget,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<MetricDisplayParts> {
    format_metric_presentation_parts(validated, dense_activity, frame_index, 0.0)
}

/// Builds metric display parts with render-prepared presentation offset.
pub(crate) fn format_metric_presentation_parts(
    validated: &ValidatedValueWidget,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    altitude_offset_m: f64,
) -> Option<MetricDisplayParts> {
    Some(ovrley_widgets::widgets::value::format::format_metric_parts(
        validated,
        &ovrley_widgets::widgets::value::format::MetricSample {
            numeric: raw_value(validated, dense_activity, frame_index, altitude_offset_m),
            available: dense_metric_series_is_available(validated.metric, dense_activity),
            gear: dense_activity
                .series
                .gear_position
                .get(frame_index)
                .and_then(Option::as_deref),
            latitude: dense_activity
                .series
                .course_lat
                .get(frame_index)
                .copied()
                .flatten(),
            longitude: dense_activity
                .series
                .course_lon
                .get(frame_index)
                .copied()
                .flatten(),
            full_activity_distance: dense_activity.full_activity_distance,
            full_activity_total_ascent: dense_activity.full_activity_total_ascent,
        },
    ))
}

/// Formats the signed lean-angle sample as the absolute integer shown by the
/// dedicated lean-angle presentation.
pub fn format_lean_angle_value(raw: Option<f64>) -> String {
    let Some(raw) = raw else {
        return "--".to_string();
    };

    match standard_metric_formatter(MetricKind::LeanAngle) {
        Some(StandardMetricFormatterKind::Integer) => format_number(raw.abs(), 0),
        _ => unreachable!("lean_angle must use the integer standard metric formatter"),
    }
}

/// Returns whether the finalized metric has any dense samples.
///
/// Finalization represents an unavailable metric with an empty series, so this
/// remains an O(1) distinction between an unavailable metric and a preserve
/// gap at the current frame.
fn dense_metric_series_is_available(
    kind: MetricKind,
    dense_activity: &DenseActivityReport,
) -> bool {
    dense_activity
        .series
        .numeric_series_for(kind)
        .is_some_and(|series| !series.is_empty())
}

// Converts a number to display text, preserving requested fractional places.
//
// Zero-decimal values intentionally round instead of truncating so backend
// preview PNGs match the editor canvas' metric formatting.
use ovrley_widgets::widgets::value::format::format_number;
