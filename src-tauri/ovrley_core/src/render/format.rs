//! Metric formatting for dynamic overlay values.
//!
//! This module converts densified telemetry samples into display text and
//! metric-widget parts. It owns unit conversion, date/time formatting variants,
//! icon selection, and missing-value fallbacks so drawing code can stay focused
//! on layout.

use crate::activity::schema::DenseActivityReport;
use crate::normalize::{
    ValidatedGradientWidget, ValidatedTimeFormatting, ValidatedTimeValue, ValidatedValueFormatting,
    ValidatedValueWidget,
};
use crate::standard_metrics::{
    standard_metric_formatter, standard_metric_unit_label, StandardMetricFormatterKind,
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
    Altitude,
    Iso,
    Aperture,
    ShutterSpeed,
    FocalLength,
    Ev,
    ColorTemperature,
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

/// Looks up one raw numeric sample by metric key and frame index.
fn raw_value(
    key: MetricKind,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<f64> {
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
        MetricKind::Altitude => dense_activity
            .series
            .altitude
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Iso => dense_activity
            .series
            .iso
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Aperture => dense_activity
            .series
            .aperture
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::ShutterSpeed => dense_activity
            .series
            .shutter_speed
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::FocalLength => dense_activity
            .series
            .focal_length
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Ev => dense_activity
            .series
            .ev
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::ColorTemperature => dense_activity
            .series
            .color_temperature
            .get(frame_index)
            .copied()
            .flatten(),
        MetricKind::Time => None,
    }
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
    let raw = raw_value(validated.metric, dense_activity, frame_index);

    let (mut value_text, unit_text, icon_kind) =
        format_validated_standard_metric_parts(validated, raw);

    if !validated.prefix.is_empty() {
        value_text = format!("{}{value_text}", validated.prefix);
    }
    if !validated.suffix.is_empty() {
        value_text.push_str(&validated.suffix);
    }

    Some(MetricDisplayParts {
        value_text,
        unit_text,
        show_icon: validated.show_icon,
        icon_kind,
    })
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
) -> MetricDisplayParts {
    let mut value_text = format_validated_time_text(validated, raw);
    if !validated.base.prefix.is_empty() {
        value_text = format!("{}{value_text}", validated.base.prefix);
    }
    if !validated.base.suffix.is_empty() {
        value_text.push_str(&validated.base.suffix);
    }

    MetricDisplayParts {
        value_text,
        unit_text: None,
        show_icon: validated.base.show_icon,
        icon_kind: super::widgets::value::metric_icon_kind_for_value(MetricKind::Time),
    }
}

fn format_validated_standard_metric_parts(
    validated: &ValidatedValueWidget,
    raw: Option<f64>,
) -> (String, Option<String>, Option<MetricIconKind>) {
    let kind = validated.metric;
    let display_unit = Some(validated.display_unit.as_str());
    let decimals = validated_decimals(&validated.formatting);
    let show_units = validated.show_units;
    let unit_text = show_units.then(|| standard_metric_unit_label(kind, display_unit).to_string());

    let value_text = match standard_metric_formatter(kind) {
        Some(StandardMetricFormatterKind::Speed) => raw
            .map(|speed_mps| {
                let factor = match validated.display_unit.as_str() {
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
                let resolved = if validated.display_unit == "fahrenheit" {
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
                let total_seconds = if validated.display_unit == "min_per_mi" {
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
        Some(StandardMetricFormatterKind::Balance) => {
            let balance_format = validated.formatting.balance_format();
            raw.map(|left_value| format_balance_value(left_value, decimals, balance_format))
                .unwrap_or_else(|| "--".to_string())
        }
        Some(StandardMetricFormatterKind::Shutter) => raw
            .map(|seconds| format_shutter_value(seconds))
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Aperture) => raw
            .map(|fnum| format_aperture_value(fnum, decimals))
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Ev) => raw
            .map(|ev| {
                if decimals > 0 {
                    format!("{ev:.decimals$}")
                } else {
                    (ev.round() as i64).to_string()
                }
            })
            .unwrap_or_else(|| "--".to_string()),
        None => "--".to_string(),
    };

    (
        value_text,
        unit_text,
        super::widgets::value::metric_icon_kind_for_value(kind),
    )
}

fn validated_decimals(formatting: &ValidatedValueFormatting) -> usize {
    match formatting {
        ValidatedValueFormatting::DecimalPlaces { decimals } => *decimals,
        ValidatedValueFormatting::DecimalRounding { decimal_rounding } => {
            (*decimal_rounding).max(0) as usize
        }
        ValidatedValueFormatting::Balance { decimals, .. } => *decimals,
        ValidatedValueFormatting::BalanceRounded {
            decimal_rounding, ..
        } => (*decimal_rounding).max(0) as usize,
    }
}

fn format_validated_time_text(validated: &ValidatedTimeValue, raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "--:--".to_string();
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return raw.to_string();
    };
    let adjusted = parsed.with_timezone(&Local) + Duration::hours(validated.hours_offset);
    match &validated.formatting {
        ValidatedTimeFormatting::Preset(format_key) => format_time_key(format_key, adjusted),
        ValidatedTimeFormatting::Strftime(strftime) => adjusted.format(strftime).to_string(),
    }
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
        MetricKind::Altitude => {
            if display_unit == Some("ft") {
                value * 3.280_84
            } else {
                value
            }
        }
        MetricKind::StrideLength => match display_unit.unwrap_or("m") {
            "cm" => value * 100.0,
            "ft" => value * 3.280_84,
            "in" => value * 39.370_1,
            _ => value,
        },
        MetricKind::Torque => value,
        MetricKind::VerticalSpeed => match display_unit.unwrap_or("mps") {
            "ftmin" => value * 196.850_394,
            "ftph" => value * 11_811.023_64,
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

/// Formats shutter speed as reciprocal text (e.g., `1/3200`).
fn format_shutter_value(seconds: f64) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "--".to_string();
    }
    if seconds >= 1.0 {
        // Whole seconds: 0.5 → 1/2, 1.0 → 1/1
        let reciprocal = (1.0 / seconds).round();
        if reciprocal >= 1.0 {
            return format!("1/{}", reciprocal as i64);
        }
    }
    // Fast shutter: 0.0003125 → 1/3200
    let reciprocal = (1.0 / seconds).round();
    format!("1/{}", reciprocal as i64)
}

/// Formats aperture as `F/x.x` (e.g., `F/1.7`).
/// Always uses 1 decimal place regardless of widget `decimals` setting,
/// because aperture values like f/1.7, f/2.8, f/4.0 require fractional display.
fn format_aperture_value(fnum: f64, _decimals: usize) -> String {
    if !fnum.is_finite() || fnum <= 0.0 {
        return "--".to_string();
    }
    let formatted = format_number(fnum, 1);
    format!("F/{formatted}")
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
