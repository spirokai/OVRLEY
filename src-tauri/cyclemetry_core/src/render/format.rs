use crate::activity::schema::DenseActivityReport;
use crate::config::{RenderConfig, ValueConfig};
use chrono::{DateTime, Datelike, Duration, Local, Timelike, TimeZone};

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
    let adjusted =
        parsed.with_timezone(&Local) + Duration::hours(value_config.hours_offset.unwrap_or(0) as i64);

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
    use super::format_time_key;
    use chrono::DateTime;

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
}
