//! Metric formatting for dynamic overlay values.
//!
//! This module converts densified telemetry samples into display text and
//! metric-widget parts. It owns unit conversion, date/time formatting variants,
//! icon selection, and missing-value fallbacks so drawing code can stay focused
//! on layout.

use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};

/// Built-in metric icon kinds supported by value widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricIconKind {
    Gauge,
    Heart,
    RefreshCw,
    Zap,
    Clock3,
    Thermometer,
}

/// Split metric text used by icon+value+unit widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricDisplayParts {
    /// Main numeric or time text.
    pub value_text: String,
    /// Optional unit suffix drawn with a smaller font.
    pub unit_text: Option<String>,
    /// Whether the metric icon should be drawn.
    pub show_icon: bool,
    /// Icon kind matching the metric, if supported.
    pub icon_kind: Option<MetricIconKind>,
}

/// Returns the dense frame index corresponding to an absolute preview second.
pub fn frame_index_for_second(
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    second: u32,
) -> usize {
    if dense_activity.frame_count == 0 {
        return 0;
    }

    let relative_second =
        (f64::from(second) - config.scene.start).clamp(0.0, config.scene.end - config.scene.start);
    let index = (relative_second * config.scene.fps).round() as isize;
    index.clamp(0, dense_activity.frame_count.saturating_sub(1) as isize) as usize
}

/// Formats a metric value as a single text string.
///
/// This path is used for ordinary text values and for gradient value text.
pub fn format_value(
    config: &RenderConfig,
    value_config: &ValueConfig,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> String {
    let raw = raw_value(value_config.value.as_str(), dense_activity, frame_index);
    let mut formatted = match value_config.value.as_str() {
        "speed" => format_speed(config, value_config, raw),
        "temperature" => format_temperature(config, value_config, raw),
        "time" => format_time(
            config,
            value_config,
            dense_activity
                .series
                .time
                .get(frame_index)
                .and_then(|value| value.as_deref()),
        ),
        "gradient" => format_gradient(config, value_config, raw),
        "elevation" => format_elevation(config, value_config, raw),
        _ => format_generic_numeric(config, value_config, raw).unwrap_or_else(|| "--".to_string()),
    };

    if let Some(prefix) = &value_config.prefix {
        formatted = format!("{prefix}{formatted}");
    }
    if let Some(suffix) = &value_config.suffix {
        formatted.push_str(suffix);
    }

    formatted
}

// Looks up one raw numeric sample by metric key and frame index.
fn raw_value(key: &str, dense_activity: &DenseActivityReport, frame_index: usize) -> Option<f64> {
    // Series vectors may be empty when the template did not request them.
    // Treat out-of-range and absent values as missing.
    match key {
        "speed" => dense_activity
            .series
            .speed
            .get(frame_index)
            .copied()
            .flatten(),
        "elevation" => dense_activity
            .series
            .elevation
            .get(frame_index)
            .copied()
            .flatten(),
        "gradient" => dense_activity
            .series
            .gradient
            .get(frame_index)
            .copied()
            .flatten(),
        "heartrate" => dense_activity
            .series
            .heartrate
            .get(frame_index)
            .copied()
            .flatten(),
        "cadence" => dense_activity
            .series
            .cadence
            .get(frame_index)
            .copied()
            .flatten(),
        "power" => dense_activity
            .series
            .power
            .get(frame_index)
            .copied()
            .flatten(),
        "temperature" => dense_activity
            .series
            .temperature
            .get(frame_index)
            .copied()
            .flatten(),
        _ => None,
    }
}

/// Builds separated value/unit/icon display parts for rich metric widgets.
pub fn format_metric_parts(
    config: &RenderConfig,
    value_config: &ValueConfig,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<MetricDisplayParts> {
    // Metric widgets need the value and units separately so units can be drawn
    // at a smaller size while sharing the same raw telemetry formatting rules.
    let raw = raw_value(value_config.value.as_str(), dense_activity, frame_index);
    let (mut value_text, unit_text, icon_kind) = match value_config.value.as_str() {
        "speed" => {
            let unit = speed_units(value_config).to_string();
            let value_text = raw
                .map(|speed_mps| {
                    let factor = match speed_unit_key(value_config) {
                        "mph" | "imperial" => 2.23694,
                        "kn" => 1.943844,
                        "mps" => 1.0,
                        _ => 3.6,
                    };
                    format_number(
                        speed_mps * factor,
                        effective_decimals(config, value_config, Some(0)),
                    )
                })
                .unwrap_or_else(|| "--".to_string());
            let unit_text = value_config.show_units.unwrap_or(true).then_some(unit);
            (value_text, unit_text, Some(MetricIconKind::Gauge))
        }
        "temperature" => {
            let unit = value_config
                .temperature_unit
                .as_deref()
                .unwrap_or("celsius");
            let formatted_unit = temperature_units(unit).to_string();
            let (value_text, unit_text) = match raw {
                Some(temp_c) if unit == "fahrenheit" => (
                    format_number(
                        (temp_c * 9.0 / 5.0) + 32.0,
                        effective_decimals(config, value_config, Some(0)),
                    ),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some(formatted_unit.clone()),
                ),
                Some(temp_c) => (
                    format_number(temp_c, effective_decimals(config, value_config, Some(0))),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some(formatted_unit.clone()),
                ),
                None => (
                    "--".to_string(),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some(formatted_unit),
                ),
            };
            (value_text, unit_text, Some(MetricIconKind::Thermometer))
        }
        "heartrate" => (
            format_generic_numeric(config, value_config, raw).unwrap_or_else(|| "--".to_string()),
            value_config
                .show_units
                .unwrap_or(false)
                .then_some("BPM".to_string()),
            Some(MetricIconKind::Heart),
        ),
        "cadence" => (
            format_generic_numeric(config, value_config, raw).unwrap_or_else(|| "--".to_string()),
            value_config
                .show_units
                .unwrap_or(false)
                .then_some("RPM".to_string()),
            Some(MetricIconKind::RefreshCw),
        ),
        "power" => (
            format_generic_numeric(config, value_config, raw).unwrap_or_else(|| "--".to_string()),
            value_config
                .show_units
                .unwrap_or(false)
                .then_some("W".to_string()),
            Some(MetricIconKind::Zap),
        ),
        "time" => (
            format_time(
                config,
                value_config,
                dense_activity
                    .series
                    .time
                    .get(frame_index)
                    .and_then(|value| value.as_deref()),
            ),
            None,
            Some(MetricIconKind::Clock3),
        ),
        _ => return None,
    };

    if let Some(prefix) = &value_config.prefix {
        value_text = format!("{prefix}{value_text}");
    }
    if let Some(suffix) = &value_config.suffix {
        value_text.push_str(suffix);
    }

    Some(MetricDisplayParts {
        value_text,
        unit_text,
        show_icon: value_config
            .show_icon
            .unwrap_or(value_config.value != "gradient"),
        icon_kind,
    })
}

// Formats speed with unit conversion and optional unit suffix.
fn format_speed(config: &RenderConfig, value_config: &ValueConfig, raw: Option<f64>) -> String {
    let Some(speed_mps) = raw else {
        return missing_value_with_units(
            value_config.show_units.unwrap_or(false),
            speed_units(value_config),
        );
    };
    let unit = speed_unit_key(value_config);
    let (factor, units) = match unit {
        "mph" | "imperial" => (2.23694, "MPH"),
        "kn" => (1.943844, "KN"),
        "mps" => (1.0, "M/S"),
        _ => (3.6, "KM/H"),
    };
    let mut text = format_number(
        speed_mps * factor,
        effective_decimals(config, value_config, Some(0)),
    );
    if value_config.show_units.unwrap_or(false) {
        text.push(' ');
        text.push_str(units);
    }
    text
}

// Formats temperature as Celsius or Fahrenheit.
fn format_temperature(
    config: &RenderConfig,
    value_config: &ValueConfig,
    raw: Option<f64>,
) -> String {
    let unit = value_config
        .temperature_unit
        .as_deref()
        .unwrap_or("celsius");
    let Some(temp_c) = raw else {
        return missing_value_with_units(
            value_config.show_units.unwrap_or(false),
            temperature_units(unit),
        );
    };
    let (value, units) = if unit == "fahrenheit" {
        ((temp_c * 9.0 / 5.0) + 32.0, temperature_units(unit))
    } else {
        (temp_c, temperature_units(unit))
    };
    let mut text = format_number(value, effective_decimals(config, value_config, Some(0)));
    if value_config.show_units.unwrap_or(false) {
        text.push(' ');
        text.push_str(units);
    }
    text
}

// Formats elevation, converting to feet for imperial templates.
fn format_elevation(config: &RenderConfig, value_config: &ValueConfig, raw: Option<f64>) -> String {
    let Some(mut value) = raw else {
        return "--".to_string();
    };
    if matches!(value_config.unit.as_deref(), Some("imperial")) {
        value *= 3.28084;
    }
    format_generic_numeric(config, value_config, Some(value)).unwrap_or_else(|| "--".to_string())
}

// Formats gradient with sign and percent suffix.
fn format_gradient(config: &RenderConfig, value_config: &ValueConfig, raw: Option<f64>) -> String {
    let Some(value) = raw else {
        return "--%".to_string();
    };
    let decimals = effective_decimals(config, value_config, Some(0));
    let magnitude = format_number(value.abs(), decimals);
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    let prefix = if value_config.show_sign.unwrap_or(true) {
        sign
    } else {
        ""
    };
    format!("{prefix}{magnitude}%")
}

// Formats an RFC 3339 timestamp according to template time settings.
fn format_time(config: &RenderConfig, value_config: &ValueConfig, raw: Option<&str>) -> String {
    // Convert to local time for display after applying user-configured offsets.
    // Invalid strings are returned unchanged so source parser issues remain
    // visible in previews instead of silently disappearing.
    let Some(raw) = raw else {
        return "--:--".to_string();
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return raw.to_string();
    };
    let adjusted = parsed.with_timezone(&Local)
        + Duration::hours(value_config.hours_offset.unwrap_or(0) as i64);

    if let Some(format_key) = value_config.format.as_deref() {
        return format_time_key(format_key, adjusted);
    }
    if let Some(strftime) = value_config
        .time_format
        .as_deref()
        .or(config.scene.time_format.as_deref())
    {
        return adjusted.format(strftime).to_string();
    }

    format_time_key("time-24", adjusted)
}

// Applies one of the built-in date/time format presets.
pub fn format_time_key<Tz>(format_key: &str, value: DateTime<Tz>) -> String // test seam
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
    let suffix = if value.hour() >= 12 { "PM" } else { "AM" };

    match format_key {
        "date-dd-mm-yyyy" => format!("{day}-{month}-{year}"),
        "date-mm-dd-yyyy" => format!("{month}-{day}-{year}"),
        "date-yyyy-mm-dd" => format!("{year}-{month}-{day}"),
        "date-dd-mmm-yyyy" => format!("{day} {short_month} {year}"),
        "date-mmm-dd-yyyy" => format!("{short_month} {day} {year}"),
        "date-dd-mmmm-yyyy" => format!("{day} {long_month} {year}"),
        "date-mmmm-dd-yyyy" => format!("{long_month} {day} {year}"),
        "time-12" => format!("{hour12}:{minutes} {suffix}"),
        "date-time-24" => format!("{day}-{month}-{year} {hour24}:{minutes}"),
        "date-time-12" => format!("{day}-{month}-{year} {hour12}:{minutes} {suffix}"),
        "date-mmm-time-24" => format!("{day} {short_month} {hour24}:{minutes}"),
        "date-mmm-time-12" => format!("{day} {short_month} {hour12}:{minutes} {suffix}"),
        "date-mmmm-time-24" => format!("{day} {long_month} {hour24}:{minutes}"),
        "date-mmmm-time-12" => format!("{day} {long_month} {hour12}:{minutes} {suffix}"),
        _ => format!("{hour24}:{minutes}"),
    }
}

// Formats a generic numeric metric with the configured decimal precision.
fn format_generic_numeric(
    config: &RenderConfig,
    value_config: &ValueConfig,
    raw: Option<f64>,
) -> Option<String> {
    raw.map(|value| format_number(value, effective_decimals(config, value_config, None)))
}

// Resolves the decimal precision for a value from value and scene defaults.
fn effective_decimals(
    config: &RenderConfig,
    value_config: &ValueConfig,
    default: Option<usize>,
) -> usize {
    if let Some(decimals) = value_config.decimals {
        return decimals;
    }
    if let Some(rounding) = value_config
        .decimal_rounding
        .or(config.scene.decimal_rounding)
    {
        return rounding.max(0) as usize;
    }
    default.unwrap_or(0)
}

// Converts a number to display text, trimming unnecessary fractional zeros.
fn format_number(value: f64, decimals: usize) -> String {
    // The historical renderer truncated zero-decimal values by casting. Preserve
    // that behavior for visual compatibility with existing templates.
    if decimals == 0 {
        return (value as i64).to_string();
    }

    let factor = 10_f64.powi(decimals as i32);
    let rounded = (value * factor).round() / factor;
    let mut text = format!("{rounded:.decimals$}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

// Resolves the configured speed unit key.
fn speed_unit_key(value_config: &ValueConfig) -> &str {
    value_config
        .speed_unit
        .as_deref()
        .or(value_config.unit.as_deref())
        .unwrap_or("kmh")
}

// Returns the display unit label for the configured speed unit.
fn speed_units(value_config: &ValueConfig) -> &'static str {
    match speed_unit_key(value_config) {
        "mph" | "imperial" => "MPH",
        "kn" => "KN",
        "mps" => "M/S",
        _ => "KM/H",
    }
}

// Returns the display unit label for a temperature unit key.
fn temperature_units(unit: &str) -> &'static str {
    if unit == "fahrenheit" {
        "\u{00B0}F"
    } else {
        "\u{00B0}C"
    }
}

// Formats a missing-value placeholder with optional unit text.
fn missing_value_with_units(show_units: bool, units: &str) -> String {
    if show_units {
        format!("-- {units}")
    } else {
        "--".to_string()
    }
}

