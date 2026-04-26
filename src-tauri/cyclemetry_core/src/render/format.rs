use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricIconKind {
    Gauge,
    Activity,
    Timer,
    Zap,
    Clock3,
    Thermometer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricDisplayParts {
    pub value_text: String,
    pub unit_text: Option<String>,
    pub show_icon: bool,
    pub icon_kind: Option<MetricIconKind>,
}

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

fn raw_value(key: &str, dense_activity: &DenseActivityReport, frame_index: usize) -> Option<f64> {
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

pub fn format_metric_parts(
    config: &RenderConfig,
    value_config: &ValueConfig,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> Option<MetricDisplayParts> {
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
            let (value_text, unit_text) = match raw {
                Some(temp_c) if unit == "fahrenheit" => (
                    format_number(
                        (temp_c * 9.0 / 5.0) + 32.0,
                        effective_decimals(config, value_config, Some(0)),
                    ),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some("F".to_string()),
                ),
                Some(temp_c) => (
                    format_number(temp_c, effective_decimals(config, value_config, Some(0))),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some("C".to_string()),
                ),
                None => (
                    "--".to_string(),
                    value_config
                        .show_units
                        .unwrap_or(true)
                        .then_some(if unit == "fahrenheit" { "F" } else { "C" }.to_string()),
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
            Some(MetricIconKind::Activity),
        ),
        "cadence" => (
            format_generic_numeric(config, value_config, raw).unwrap_or_else(|| "--".to_string()),
            value_config
                .show_units
                .unwrap_or(false)
                .then_some("RPM".to_string()),
            Some(MetricIconKind::Timer),
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
            if unit == "fahrenheit" { "F" } else { "C" },
        );
    };
    let (value, units) = if unit == "fahrenheit" {
        ((temp_c * 9.0 / 5.0) + 32.0, "F")
    } else {
        (temp_c, "C")
    };
    let mut text = format_number(value, effective_decimals(config, value_config, Some(0)));
    if value_config.show_units.unwrap_or(false) {
        text.push(' ');
        text.push_str(units);
    }
    text
}

fn format_elevation(config: &RenderConfig, value_config: &ValueConfig, raw: Option<f64>) -> String {
    let Some(mut value) = raw else {
        return "--".to_string();
    };
    if matches!(value_config.unit.as_deref(), Some("imperial")) {
        value *= 3.28084;
    }
    format_generic_numeric(config, value_config, Some(value)).unwrap_or_else(|| "--".to_string())
}

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

fn format_time(config: &RenderConfig, value_config: &ValueConfig, raw: Option<&str>) -> String {
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

fn format_time_key<Tz>(format_key: &str, value: DateTime<Tz>) -> String
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

fn format_generic_numeric(
    config: &RenderConfig,
    value_config: &ValueConfig,
    raw: Option<f64>,
) -> Option<String> {
    raw.map(|value| format_number(value, effective_decimals(config, value_config, None)))
}

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

fn format_number(value: f64, decimals: usize) -> String {
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

fn speed_unit_key(value_config: &ValueConfig) -> &str {
    value_config
        .speed_unit
        .as_deref()
        .or(value_config.unit.as_deref())
        .unwrap_or("kmh")
}

fn speed_units(value_config: &ValueConfig) -> &'static str {
    match speed_unit_key(value_config) {
        "mph" | "imperial" => "MPH",
        "kn" => "KN",
        "mps" => "M/S",
        _ => "KM/H",
    }
}

fn missing_value_with_units(show_units: bool, units: &str) -> String {
    if show_units {
        format!("-- {units}")
    } else {
        "--".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{format_metric_parts, format_time_key, MetricIconKind};
    use crate::activity::schema::{DenseActivityReport, DenseSeriesReport};
    use crate::config::{RenderConfig, SceneConfig, ValueConfig};
    use chrono::DateTime;
    use serde_json::json;

    #[test]
    fn formats_time_key_variants() {
        let timestamp = DateTime::parse_from_rfc3339("2025-04-21T13:05:00Z")
            .unwrap()
            .to_utc();
        assert_eq!(format_time_key("time-24", timestamp), "13:05");
        assert_eq!(format_time_key("time-12", timestamp), "01:05 PM");
        assert_eq!(
            format_time_key("date-dd-mmm-yyyy", timestamp),
            "21 APR 2025"
        );
    }

    #[test]
    fn formats_metric_parts_for_speed() {
        let config = RenderConfig {
            scene: SceneConfig {
                width: None,
                height: None,
                fps: 30.0,
                start: 0.0,
                end: 1.0,
                font: None,
                font_size: None,
                color: None,
                decimal_rounding: None,
                overlay_filename: None,
                ffmpeg: json!({}),
                opacity: None,
                scale: None,
                time_format: None,
                extra: Default::default(),
            },
            labels: vec![],
            values: vec![],
            plots: json!([]),
            extra: Default::default(),
        };
        let value = ValueConfig {
            value: "speed".to_string(),
            x: 0.0,
            y: 0.0,
            font: None,
            font_family: None,
            font_size: None,
            color: None,
            opacity: None,
            suffix: None,
            prefix: None,
            unit: None,
            hours_offset: None,
            time_format: None,
            format: None,
            decimal_rounding: None,
            decimals: Some(0),
            show_icon: None,
            icon_color: None,
            icon_size: None,
            icon_offset_x: None,
            icon_offset_y: None,
            show_units: Some(true),
            speed_unit: Some("kmh".to_string()),
            temperature_unit: None,
            value_offset: None,
            triangle_positive_color: None,
            triangle_negative_color: None,
            show_sign: None,
            show_triangle: None,
            triangle_width: None,
            shadow_color: None,
            shadow_strength: None,
            shadow_distance: None,
            border_color: None,
            border_thickness: None,
            border_strength: None,
            border_distance: None,
            extra: Default::default(),
        };
        let dense = DenseActivityReport {
            frame_count: 1,
            frame_elapsed_seconds: vec![0.0],
            frame_distance_progress: vec![Some(0.0)],
            series: DenseSeriesReport {
                speed: vec![Some(10.0)],
                elevation: vec![],
                gradient: vec![],
                heartrate: vec![],
                cadence: vec![],
                power: vec![],
                temperature: vec![],
                course_lat: vec![],
                course_lon: vec![],
                time: vec![],
            },
        };

        let parts = format_metric_parts(&config, &value, &dense, 0).unwrap();
        assert_eq!(parts.value_text, "36");
        assert_eq!(parts.unit_text.as_deref(), Some("KM/H"));
        assert_eq!(parts.icon_kind, Some(MetricIconKind::Gauge));
        assert!(parts.show_icon);
    }
}
