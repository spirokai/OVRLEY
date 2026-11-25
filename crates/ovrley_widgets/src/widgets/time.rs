//! Time text: clock, elapsed and activity times through the shared value layout.
use crate::model::catalog::{DisplayType, MetricKind};
use crate::model::validation::Precision;
use crate::model::validation::{require_bool, require_string};
use crate::widgets::value::format::{MetricDisplayContent, MetricDisplayParts};
use crate::widgets::value::{TextFields, TextPlacement, ValidatedValueWidget, ValueConfig};
use crate::{WidgetError, WidgetResult};
use chrono::{DateTime, Datelike, Duration, SecondsFormat, TimeZone, Timelike, Utc};
use chrono_tz::Tz;

/// Explicit formatting mode for a validated time value.
#[derive(Clone, Debug)]
pub enum ValidatedTimeFormatting {
    Daytime(String),
    Elapsed {
        origin: ElapsedTimeOrigin,
        show_hundredths: bool,
    },
}

impl ValidatedTimeFormatting {
    pub fn mode(&self) -> TimeMode {
        match self {
            Self::Daytime(_) => TimeMode::Daytime,
            Self::Elapsed { .. } => TimeMode::Elapsed,
        }
    }
}

/// Whether time text shows the time of day or an elapsed time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeMode {
    Daytime,
    Elapsed,
}

impl TimeMode {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "daytime" => Some(Self::Daytime),
            "elapsed" => Some(Self::Elapsed),
            _ => None,
        }
    }

    /// The mode's manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daytime => "daytime",
            Self::Elapsed => "elapsed",
        }
    }
}

/// The moment an elapsed time counts from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElapsedTimeOrigin {
    Activity,
    Export,
}

impl ElapsedTimeOrigin {
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "activity" => Some(Self::Activity),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}

/// Every output-affecting field for a time text widget is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedTimeValue {
    pub base: ValidatedValueWidget,
    pub hours_offset: i64,
    pub formatting: ValidatedTimeFormatting,
}

pub fn validate_time_value(value: ValueConfig, index: usize) -> WidgetResult<ValidatedTimeValue> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.value != MetricKind::Time {
        return Err(WidgetError::Config(format!(
            "{}: expected Time, got {:?}",
            p("value"),
            value.value
        )));
    }

    if value.display_type != DisplayType::Text {
        return Err(WidgetError::Config(format!(
            "{}: display_type '{}' is not supported by time text",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let mut value = value;
    let text = TextFields::validate(&mut value, TextPlacement::Standalone, &p)?;
    if value.time_format.is_some() {
        return Err(WidgetError::Config(format!(
            "{}: is not supported; use format",
            p("time_format")
        )));
    }
    let hours_offset = i64::from(
        value
            .hours_offset
            .ok_or_else(|| WidgetError::Config(format!("{}: required", p("hours_offset"))))?,
    );
    let format_key = require_string(value.format, &p("format"))?;
    validate_time_format_key(&format_key, &p("format"))?;
    let time_mode = require_string(value.time_mode, &p("time_mode"))?;
    let elapsed_origin = require_string(value.elapsed_origin, &p("elapsed_origin"))?;
    let elapsed_origin = ElapsedTimeOrigin::parse(&elapsed_origin).ok_or_else(|| {
        WidgetError::Config(format!(
            "{}: expected 'activity' or 'export', got '{elapsed_origin}'",
            p("elapsed_origin")
        ))
    })?;
    let show_hundredths = require_bool(value.show_hundredths, &p("show_hundredths"))?;
    let formatting = match TimeMode::parse(&time_mode) {
        Some(TimeMode::Daytime) => ValidatedTimeFormatting::Daytime(format_key),
        Some(TimeMode::Elapsed) => ValidatedTimeFormatting::Elapsed {
            origin: elapsed_origin,
            show_hundredths,
        },
        None => {
            return Err(WidgetError::Config(format!(
                "{}: expected 'daytime' or 'elapsed', got '{time_mode}'",
                p("time_mode")
            )))
        }
    };

    Ok(ValidatedTimeValue {
        base: ValidatedValueWidget {
            metric: MetricKind::Time,
            x: value.x,
            y: value.y,
            display_type: value.display_type,
            content_alignment: text.content_alignment,
            font_name: text.font_name,
            font_size: text.font_size,
            color: text.color,
            opacity: text.opacity,
            show_icon: text.show_icon,
            icon_color: text.icon_color,
            icon_size: text.icon_size,
            icon_offset_x: text.icon_offset_x,
            icon_offset_y: text.icon_offset_y,
            show_units: false,
            show_full_distance: None,
            show_full_ascent: None,
            coordinate_format: None,
            unit_color: text.color,
            display_unit: String::new(),
            starting_altitude_m: None,
            prefix: text.prefix,
            suffix: text.suffix,
            precision: Precision::Decimals(0),
            balance_format: None,
            hours_offset: Some(hours_offset),
            format: None,
        },
        hours_offset,
        formatting,
    })
}

fn validate_time_format_key(format_key: &str, field: &str) -> WidgetResult<()> {
    match format_key {
        "time-24" | "time-24s" | "time-12" | "time-12s" | "date-dd-mm-yyyy" | "date-mm-dd-yyyy"
        | "date-yyyy-mm-dd" | "date-dd-mmm-yyyy" | "date-mmm-dd-yyyy" | "date-dd-mmmm-yyyy"
        | "date-mmmm-dd-yyyy" | "date-time-24" | "date-time-24s" | "date-time-12"
        | "date-time-12s" | "date-mmm-time-24" | "date-mmm-time-12" | "date-mmmm-time-24"
        | "date-mmmm-time-12" => Ok(()),
        _ => Err(WidgetError::Config(format!(
            "{field}: unsupported time format '{format_key}'"
        ))),
    }
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

pub enum TimeSampler {
    Recorded(Vec<(f64, f64)>),
    Sync(Option<DateTime<Utc>>),
}

impl TimeSampler {
    pub fn new(sync_time: Option<&str>, seconds: &[f64], values: &[Option<String>]) -> Self {
        // Recorded timestamps are authoritative, even when malformed external samples
        // leave no usable points. Only an absent series permits the sync-time fallback.
        if values.iter().any(Option::is_some) {
            Self::Recorded(
                seconds
                    .iter()
                    .copied()
                    .zip(values)
                    .filter_map(|(second, raw)| {
                        raw.as_deref()
                            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                            .map(|time| (second, time.timestamp_millis() as f64))
                    })
                    .collect(),
            )
        } else {
            Self::Sync(
                sync_time
                    .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                    .map(|time| time.to_utc()),
            )
        }
    }

    pub fn at(&self, second: f64) -> Option<String> {
        let time = match self {
            Self::Recorded(points) => {
                if points.first().is_some_and(|p| second < p.0)
                    || points.last().is_some_and(|p| second > p.0)
                {
                    return None;
                }
                crate::series::interpolate_points(points, second).map(|millis| {
                    DateTime::<Utc>::from_timestamp_millis(millis.round() as i64)
                        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
                })
            }
            Self::Sync(start) => start.map(|start| {
                start + chrono::TimeDelta::milliseconds((second * 1000.0).round() as i64)
            }),
        };
        time.map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

#[cfg(test)]
mod tests {
    use super::TimeSampler;

    #[test]
    fn recorded_times_bridge_gaps_and_seek_but_only_absent_samples_use_sync_time() {
        let sync = Some("2000-01-01T00:00:00Z");
        let samples = vec![
            Some("2025-04-21T13:05:00Z".into()),
            None,
            Some("2025-04-21T13:05:02Z".into()),
        ];
        let sampler = TimeSampler::new(sync, &[0.0, 1.0, 2.0], &samples);
        assert_eq!(sampler.at(1.5).as_deref(), Some("2025-04-21T13:05:01.500Z"));
        assert_eq!(
            sampler.at(0.25).as_deref(),
            Some("2025-04-21T13:05:00.250Z")
        );
        assert_eq!(sampler.at(-1.0), None);
        assert_eq!(sampler.at(3.0), None);
        assert_eq!(
            TimeSampler::new(sync, &[0.0], &[Some("invalid".into())]).at(0.0),
            None
        );
        assert_eq!(
            TimeSampler::new(sync, &[], &[]).at(-1.25).as_deref(),
            Some("1999-12-31T23:59:58.750Z")
        );
        assert_eq!(TimeSampler::new(None, &[], &[]).at(0.0), None);
    }
}
