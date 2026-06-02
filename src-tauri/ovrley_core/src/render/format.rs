//! Metric formatting for dynamic overlay values.
//!
//! This module converts densified telemetry samples into display text and
//! metric-widget parts. It owns unit conversion, date/time formatting variants,
//! icon selection, and missing-value fallbacks so drawing code can stay focused
//! on layout.

use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use crate::standard_metrics::{
    is_standard_metric, metric_icon_asset_key, standard_metric_default_display_unit,
    standard_metric_formatter, standard_metric_show_units, standard_metric_unit_label,
    StandardMetricFormatterKind,
};
use crate::MetricKind;
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
    CoreTemperature,
    Footprints,
    Wind,
    Scale,
    Ruler,
    Waves,
    TrendingUp,
    Percent,
    GForce,
    GroundContactTime,
    Torque,
    GearPosition,
    Compass,
    ArrowUpDown,
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
    second: f64,
) -> usize {
    if dense_activity.frame_count == 0 {
        return 0;
    }

    let relative_second =
        (second - config.scene.start).clamp(0.0, config.scene.end - config.scene.start);
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
    let raw = raw_value(value_config.value, dense_activity, frame_index);
    let mut formatted = match value_config.value {
        MetricKind::Time => format_time(
            config,
            value_config,
            dense_activity
                .series
                .time
                .get(frame_index)
                .and_then(|value| value.as_deref()),
        ),
        MetricKind::Gradient => format_gradient(config, value_config, raw),
        MetricKind::Elevation => format_elevation(config, value_config, raw),
        kind if is_standard_metric(kind) => format_standard_metric_text(config, value_config, raw),
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
fn raw_value(
    key: MetricKind,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<f64> {
    // Series vectors may be empty when the template did not request them.
    // Treat out-of-range and absent values as missing.
    match key {
        MetricKind::Speed => dense_activity
            .series
            .speed
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Elevation => dense_activity
            .series
            .elevation
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Gradient => dense_activity
            .series
            .gradient
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Heartrate => dense_activity
            .series
            .heartrate
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Cadence => dense_activity
            .series
            .cadence
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Power => dense_activity
            .series
            .power
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Temperature => dense_activity
            .series
            .temperature
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Pace => dense_activity
            .series
            .pace
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::GForce => dense_activity
            .series
            .g_force
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::AirPressure => dense_activity
            .series
            .air_pressure
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::GroundContactTime => dense_activity
            .series
            .ground_contact_time
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::LeftRightBalance => dense_activity
            .series
            .left_right_balance
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::StrideLength => dense_activity
            .series
            .stride_length
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::StrokeRate => dense_activity
            .series
            .stroke_rate
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Torque => dense_activity
            .series
            .torque
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::VerticalSpeed => dense_activity
            .series
            .vertical_speed
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::GearPosition => dense_activity
            .series
            .gear_position
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::VerticalRatio => dense_activity
            .series
            .vertical_ratio
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::VerticalOscillation => dense_activity
            .series
            .vertical_oscillation
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::CoreTemperature => dense_activity
            .series
            .core_temperature
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Heading => dense_activity
            .series
            .heading
            .get(frame_index)
            .copied()
            .flatten(),
        _ => None,
    }
}

/// Builds separated value/unit/icon display parts for rich metric widgets.
///
/// # Two-phase dispatch
///
/// 1. **Metric dispatch** — match the metric kind and produce raw `(value_text,
///    unit_text, icon_kind)` tuples, applying unit conversion and number
///    formatting per metric type. The `Time` and `Temperature` cases involve
///    secondary branching for format and unit selections.
/// 2. **Prefix/suffix application** — prepend and append user-configured affix
///    text around the resolved value string before returning the final parts.
pub fn format_metric_parts(
    config: &RenderConfig,
    value_config: &ValueConfig,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<MetricDisplayParts> {
    // Metric widgets need the value and units separately so units can be drawn
    // at a smaller size while sharing the same raw telemetry formatting rules.
    //
    // Phase 1: dispatch by metric kind — each arm normalizes raw telemetry into
    // a (value_text, unit_text, icon_kind) tuple with unit conversion applied.
    let raw = raw_value(value_config.value, dense_activity, frame_index);
    let (mut value_text, unit_text, icon_kind) = match value_config.value {
        MetricKind::Time => (
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
        kind if is_standard_metric(kind) => format_standard_metric_parts(config, value_config, raw),
        _ => return None,
    };

    // Phase 2: apply user-configured prefix and suffix around the resolved value.
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
            .unwrap_or(value_config.value != MetricKind::Gradient),
        icon_kind,
    })
}

fn format_standard_metric_text(
    config: &RenderConfig,
    value_config: &ValueConfig,
    raw: Option<f64>,
) -> String {
    let (value_text, unit_text, _) = format_standard_metric_parts(config, value_config, raw);
    if let Some(unit_text) = unit_text {
        format!("{value_text} {unit_text}")
    } else {
        value_text
    }
}

fn format_standard_metric_parts(
    config: &RenderConfig,
    value_config: &ValueConfig,
    raw: Option<f64>,
) -> (String, Option<String>, Option<MetricIconKind>) {
    let kind = value_config.value;
    let display_unit = resolved_standard_metric_display_unit(kind, value_config);
    let decimals = match standard_metric_formatter(kind) {
        Some(StandardMetricFormatterKind::Decimal) => {
            effective_decimals(config, value_config, Some(1))
        }
        Some(StandardMetricFormatterKind::Balance) => {
            effective_decimals(config, value_config, Some(0))
        }
        _ => effective_decimals(config, value_config, Some(0)),
    };
    let show_units = standard_metric_show_units(kind, value_config.show_units);
    let unit_text = show_units.then(|| standard_metric_unit_label(kind, display_unit).to_string());
    let value_text = match standard_metric_formatter(kind) {
        Some(StandardMetricFormatterKind::Speed) => raw
            .map(|speed_mps| {
                let factor = match display_unit.unwrap_or("kmh") {
                    "mph" | "imperial" => 2.23694,
                    "kn" => 1.943844,
                    "mps" => 1.0,
                    _ => 3.6,
                };
                format_number(speed_mps * factor, decimals)
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Temperature) => raw
            .map(|temp_c| {
                let resolved = if display_unit == Some("fahrenheit") {
                    (temp_c * 9.0 / 5.0) + 32.0
                } else {
                    temp_c
                };
                if decimals > 0 {
                    format!("{resolved:.decimals$}")
                } else {
                    (resolved as i64).to_string()
                }
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Pace) => raw
            .map(|seconds_per_km| {
                let total_seconds = if display_unit == Some("min_per_mi") {
                    seconds_per_km * 1.609_344
                } else {
                    seconds_per_km
                };
                format_pace_value(total_seconds)
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Integer) => raw
            .map(|value| {
                format_number(
                    convert_standard_metric_value(kind, display_unit, value),
                    decimals,
                )
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Decimal) => raw
            .map(|value| {
                format_number(
                    convert_standard_metric_value(kind, display_unit, value),
                    decimals,
                )
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Balance) => raw
            .map(|left_value| {
                format_balance_value(left_value, decimals, value_config.balance_format.as_deref())
            })
            .unwrap_or_else(|| "--".to_string()),
        None => "--".to_string(),
    };

    (value_text, unit_text, metric_icon_kind_for_metric(kind))
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

/// Applies one of the built-in date/time format presets.
///
/// Supported keys: `"time-24"`, `"time-12"`, `"date-dmy"`, `"date-mdy"`,
/// `"date-ymd"`, `"datetime"`, `"datetime-short-month"`. Unrecognized keys
/// fall back to the ISO 8601 datetime format.
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
//
// Zero-decimal values intentionally round instead of truncating so backend
// preview PNGs match the editor canvas' metric formatting.
fn format_number(value: f64, decimals: usize) -> String {
    if decimals == 0 {
        return value.round().to_string();
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

fn resolved_standard_metric_display_unit<'a>(
    kind: MetricKind,
    value_config: &'a ValueConfig,
) -> Option<&'a str> {
    value_config
        .display_unit
        .as_deref()
        .or(standard_metric_default_display_unit(kind))
}

fn convert_standard_metric_value(kind: MetricKind, display_unit: Option<&str>, value: f64) -> f64 {
    match kind {
        MetricKind::Heartrate
        | MetricKind::Cadence
        | MetricKind::Power
        | MetricKind::GroundContactTime
        | MetricKind::StrokeRate
        | MetricKind::GearPosition
        | MetricKind::VerticalRatio => value,
        MetricKind::VerticalOscillation => {
            if display_unit == Some("cm") {
                value / 10.0
            } else {
                value
            }
        }
        MetricKind::GForce => {
            if display_unit == Some("mps2") {
                value * 9.806_65
            } else {
                value
            }
        }
        MetricKind::AirPressure => match display_unit.unwrap_or("hpa") {
            "inhg" => value * 29.529_983_071_4,
            "mmhg" => value * 750.061_561_303,
            "mbar" => value * 1000.0,
            _ => value * 1000.0,
        },
        MetricKind::StrideLength => match display_unit.unwrap_or("m") {
            "cm" => value * 100.0,
            "ft" => value * 3.280_84,
            "in" => value * 39.370_1,
            _ => value,
        },
        MetricKind::Torque => value,
        MetricKind::VerticalSpeed => match display_unit.unwrap_or("mps") {
            "ftmin" => value * 196.850_394,
            "mph_vertical" => value * 3600.0,
            _ => value,
        },
        _ => value,
    }
}

fn format_pace_value(total_seconds: f64) -> String {
    if !total_seconds.is_finite() || total_seconds < 0.0 {
        return "--".to_string();
    }
    let rounded_seconds = total_seconds.round().max(0.0) as i64;
    let minutes = rounded_seconds / 60;
    let seconds = rounded_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn format_balance_value(left_value: f64, decimals: usize, balance_format: Option<&str>) -> String {
    let left = format_number(left_value.clamp(0.0, 100.0), decimals);
    let right = format_number((100.0 - left_value).clamp(0.0, 100.0), decimals);
    match balance_format.unwrap_or("plain") {
        "percent_label" => format!("{left}%/{right}%"),
        "plain" => format!("{left}/{right}"),
        "l_prefix" => format!("L{left}/R{right}"),
        "l_suffix" => format!("{left}L/{right}R"),
        _ => format!("{left}/{right}"),
    }
}

fn metric_icon_kind_for_metric(kind: MetricKind) -> Option<MetricIconKind> {
    match metric_icon_asset_key(kind)? {
        crate::standard_metrics::MetricIconAssetKey::Speed => Some(MetricIconKind::Gauge),
        crate::standard_metrics::MetricIconAssetKey::Heartrate => Some(MetricIconKind::Heart),
        crate::standard_metrics::MetricIconAssetKey::Cadence => Some(MetricIconKind::RefreshCw),
        crate::standard_metrics::MetricIconAssetKey::Power => Some(MetricIconKind::Zap),
        crate::standard_metrics::MetricIconAssetKey::Time => Some(MetricIconKind::Clock3),
        crate::standard_metrics::MetricIconAssetKey::Temperature => {
            Some(MetricIconKind::Thermometer)
        }
        crate::standard_metrics::MetricIconAssetKey::CoreTemperature => {
            Some(MetricIconKind::CoreTemperature)
        }
        crate::standard_metrics::MetricIconAssetKey::Pace => Some(MetricIconKind::Footprints),
        crate::standard_metrics::MetricIconAssetKey::AirPressure => Some(MetricIconKind::Wind),
        crate::standard_metrics::MetricIconAssetKey::LeftRightBalance => {
            Some(MetricIconKind::Scale)
        }
        crate::standard_metrics::MetricIconAssetKey::StrideLength => Some(MetricIconKind::Ruler),
        crate::standard_metrics::MetricIconAssetKey::StrokeRate => Some(MetricIconKind::Waves),
        crate::standard_metrics::MetricIconAssetKey::VerticalSpeed => {
            Some(MetricIconKind::TrendingUp)
        }
        crate::standard_metrics::MetricIconAssetKey::VerticalRatio => Some(MetricIconKind::Percent),
        crate::standard_metrics::MetricIconAssetKey::GForce => Some(MetricIconKind::GForce),
        crate::standard_metrics::MetricIconAssetKey::GroundContactTime => {
            Some(MetricIconKind::GroundContactTime)
        }
        crate::standard_metrics::MetricIconAssetKey::Torque => Some(MetricIconKind::Torque),
        crate::standard_metrics::MetricIconAssetKey::GearPosition => {
            Some(MetricIconKind::GearPosition)
        }
        crate::standard_metrics::MetricIconAssetKey::Heading => Some(MetricIconKind::Compass),
        crate::standard_metrics::MetricIconAssetKey::VerticalOscillation => {
            Some(MetricIconKind::ArrowUpDown)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_balance_value;

    #[test]
    fn balance_percent_label_omits_spaces_around_slash() {
        assert_eq!(
            format_balance_value(52.0, 0, Some("percent_label")),
            "52%/48%"
        );
    }

    #[test]
    fn balance_variants_omit_spaces_around_slash() {
        assert_eq!(format_balance_value(60.0, 0, Some("plain")), "60/40");
        assert_eq!(format_balance_value(48.0, 0, Some("l_prefix")), "L48/R52");
        assert_eq!(format_balance_value(70.0, 0, Some("l_suffix")), "70L/30R");
    }
}
