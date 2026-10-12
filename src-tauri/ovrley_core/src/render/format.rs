//! Metric formatting for dynamic overlay values.
//!
//! Supplies dense samples to shared metric formatting. Specialized time and
//! gradient formatting remain native until those widget families migrate.

use crate::activity::schema::DenseActivityReport;
use crate::normalize::{
    ElapsedTimeOrigin, ValidatedGradientWidget, ValidatedTimeFormatting, ValidatedTimeValue,
};
use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use ovrley_widgets::model::standard_metrics::{
    standard_metric_formatter, StandardMetricFormatterKind,
};
use ovrley_widgets::model::types::MetricKind;
use ovrley_widgets::widgets::value::validation::ValidatedValueWidget;

use ovrley_widgets::widgets::value::format::{MetricDisplayContent, MetricDisplayParts};

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

/// Formats a gradient widget value using the validated contract.
///
/// All output-affecting fields are already explicit — no backend-owned defaults
/// are applied. Raw telemetry is looked up by metric kind and formatted using
/// the validated formatting contract.
pub fn format_validated_gradient(validated: &ValidatedGradientWidget, raw: Option<f64>) -> String {
    let Some(value) = raw else {
        return "--%".to_string();
    };
    let decimals = validated.formatting.decimals();
    let magnitude = format_number(value.abs(), decimals);
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    let prefix = if validated.show_sign { sign } else { "" };
    let mut formatted = format!("{prefix}{magnitude}%");
    if !validated.prefix.is_empty() {
        formatted = format!("{}{formatted}", validated.prefix);
    }
    if !validated.suffix.is_empty() {
        formatted.push_str(&validated.suffix);
    }
    formatted
}

/// Formats a time widget value using the validated contract.
pub fn format_validated_time_parts(
    validated: &ValidatedTimeValue,
    raw: Option<&str>,
    elapsed_time: ElapsedTimeValues,
    timezone: Option<Tz>,
) -> MetricDisplayParts {
    let mut value_text = match &validated.formatting {
        ValidatedTimeFormatting::Daytime(_) => format_validated_time_text(validated, raw, timezone),
        ValidatedTimeFormatting::Elapsed {
            origin,
            show_hundredths,
        } => {
            let elapsed = match origin {
                ElapsedTimeOrigin::Activity => elapsed_time.activity_seconds,
                ElapsedTimeOrigin::Export => elapsed_time.export_seconds,
            };
            format_elapsed_time(elapsed, *show_hundredths)
        }
    };
    if !validated.base.prefix.is_empty() {
        value_text = format!("{}{value_text}", validated.base.prefix);
    }
    if !validated.base.suffix.is_empty() {
        value_text.push_str(&validated.base.suffix);
    }

    MetricDisplayParts {
        content: MetricDisplayContent::Standard {
            value_text,
            unit_text: None,
        },
        show_icon: validated.base.show_icon,
        icon_asset_file: ovrley_widgets::render::icons::metric_icon_asset_file(MetricKind::Time),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ElapsedTimeValues {
    pub activity_seconds: f64,
    pub export_seconds: f64,
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

fn format_validated_time_text(
    validated: &ValidatedTimeValue,
    raw: Option<&str>,
    timezone: Option<Tz>,
) -> String {
    let Some(raw) = raw else {
        return "--:--".to_string();
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return raw.to_string();
    };

    match timezone {
        Some(timezone) => format_time_in_zone(validated, parsed.with_timezone(&timezone)),
        None => format_time_in_zone(validated, parsed.with_timezone(&Utc)),
    }
}

fn format_time_in_zone<TzValue>(validated: &ValidatedTimeValue, value: DateTime<TzValue>) -> String
where
    TzValue: TimeZone,
    TzValue::Offset: std::fmt::Display,
{
    let adjusted = value + Duration::hours(validated.hours_offset);
    match &validated.formatting {
        ValidatedTimeFormatting::Daytime(format_key) => format_time_key(format_key, adjusted),
        ValidatedTimeFormatting::Elapsed { .. } => {
            unreachable!("elapsed time is formatted without an absolute timestamp")
        }
    }
}

pub fn format_elapsed_time(elapsed_seconds: f64, show_hundredths: bool) -> String {
    let units_per_second = if show_hundredths { 100 } else { 1 };
    let total_units = if show_hundredths {
        (elapsed_seconds.abs() * units_per_second as f64).round() as u64
    } else {
        elapsed_seconds.abs().floor() as u64
    };
    let total_seconds = total_units / units_per_second;
    let hours = total_seconds / 3600;
    let minutes = total_seconds % 3600 / 60;
    let seconds = total_seconds % 60;
    let sign = if elapsed_seconds < 0.0 && total_units > 0 {
        "-"
    } else {
        ""
    };

    if show_hundredths {
        format!(
            "{sign}{hours}:{minutes:02}:{seconds:02}.{:02}",
            total_units % units_per_second
        )
    } else {
        format!("{sign}{hours}:{minutes:02}:{seconds:02}")
    }
}

/// Applies one of the built-in date/time format presets.
///
/// Supported keys: `"time-24"`, `"time-24s"`, `"time-12"`, `"date-dd-mm-yyyy"`,
/// `"date-mm-dd-yyyy"`, `"date-yyyy-mm-dd"`, `"date-dd-mmm-yyyy"`,
/// `"date-mmm-dd-yyyy"`, `"date-dd-mmmm-yyyy"`, `"date-mmmm-dd-yyyy"`,
/// `"date-time-24"`, `"date-time-12"`, `"date-mmm-time-24"`, `"date-mmm-time-12"`,
/// `"date-mmmm-time-24"`, `"date-mmmm-time-12"`. Keys are validated before
/// rendering, so an unrecognized value indicates a broken internal contract.
pub fn format_time_key<Tz>(format_key: &str, value: DateTime<Tz>) -> String
// test seam
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let day = format!("{:02}", value.day());
    let month = format!("{:02}", value.month());
    let year = value.year();
    let short_month = value.format("%b").to_string().to_uppercase();
    let long_month = value.format("%B").to_string().to_uppercase();
    let hour24 = format!("{:02}", value.hour());
    let hour12_raw = match value.hour() % 12 {
        0 => 12,
        other => other,
    };
    let hour12 = format!("{hour12_raw:02}");
    let minutes = format!("{:02}", value.minute());
    let seconds = format!("{:02}", value.second());
    let suffix = if value.hour() >= 12 { "PM" } else { "AM" };

    match format_key {
        "date-dd-mm-yyyy" => format!("{day}-{month}-{year}"),
        "date-mm-dd-yyyy" => format!("{month}-{day}-{year}"),
        "date-yyyy-mm-dd" => format!("{year}-{month}-{day}"),
        "date-dd-mmm-yyyy" => format!("{day} {short_month} {year}"),
        "date-mmm-dd-yyyy" => format!("{short_month} {day} {year}"),
        "date-dd-mmmm-yyyy" => format!("{day} {long_month} {year}"),
        "date-mmmm-dd-yyyy" => format!("{long_month} {day} {year}"),
        "time-24" => format!("{hour24}:{minutes}"),
        "time-24s" => format!("{hour24}:{minutes}:{seconds}"),
        "time-12" => format!("{hour12}:{minutes} {suffix}"),
        "time-12s" => format!("{hour12}:{minutes}:{seconds} {suffix}"),
        "date-time-24" => format!("{day}-{month}-{year} {hour24}:{minutes}"),
        "date-time-24s" => format!("{day}-{month}-{year} {hour24}:{minutes}:{seconds}"),
        "date-time-12" => format!("{day}-{month}-{year} {hour12}:{minutes} {suffix}"),
        "date-time-12s" => format!("{day}-{month}-{year} {hour12}:{minutes}:{seconds} {suffix}"),
        "date-mmm-time-24" => format!("{day} {short_month} {hour24}:{minutes}"),
        "date-mmm-time-12" => format!("{day} {short_month} {hour12}:{minutes} {suffix}"),
        "date-mmmm-time-24" => format!("{day} {long_month} {hour24}:{minutes}"),
        "date-mmmm-time-12" => format!("{day} {long_month} {hour12}:{minutes} {suffix}"),
        _ => unreachable!("time format key was validated at ingress: {format_key}"),
    }
}

// Converts a number to display text, preserving requested fractional places.
//
// Zero-decimal values intentionally round instead of truncating so backend
// preview PNGs match the editor canvas' metric formatting.
use ovrley_widgets::widgets::value::format::format_number;
