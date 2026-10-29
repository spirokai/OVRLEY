//! Shared daytime and elapsed-time formatting.
use super::validation::{ElapsedTimeOrigin, ValidatedTimeFormatting, ValidatedTimeValue};
use crate::model::types::MetricKind;
use crate::widgets::value::format::{MetricDisplayContent, MetricDisplayParts};
use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

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
        icon_asset_file: crate::render::icons::metric_icon_asset_file(MetricKind::Time),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ElapsedTimeValues {
    pub activity_seconds: f64,
    pub export_seconds: f64,
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
