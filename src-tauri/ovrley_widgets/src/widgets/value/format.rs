use crate::model::standard_metrics::{
    standard_metric_formatter, standard_metric_unit_label, StandardMetricFormatterKind,
};
use crate::model::telemetry::{metric_interpolation, InterpolationKind};
use crate::model::types::MetricKind;
use crate::widgets::value::validation::{ValidatedValueFormatting, ValidatedValueWidget};

/// Starting-altitude presentation adjustment, independent of host storage.
pub fn altitude_offset_m(target: Option<f64>, first: Option<f64>) -> f64 {
    target
        .zip(first)
        .map_or(0.0, |(target, first)| target - first)
}

/// One sample and activity context supplied by either host. Empty external series are unavailable.
#[derive(Default)]
pub struct MetricSample<'a> {
    pub numeric: Option<f64>,
    pub available: bool,
    pub gear: Option<&'a str>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub full_activity_distance: Option<f64>,
    pub full_activity_total_ascent: Option<f64>,
}

/// Display content used by icon+value+unit widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricDisplayContent {
    /// Main metric/time text and optional unit suffix.
    Standard {
        value_text: String,
        unit_text: Option<String>,
    },
    /// Coordinate lines with independently colored direction letters.
    Coordinates(MetricCoordinateDisplay),
}

/// Formatted metric content plus its shared icon configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricDisplayParts {
    pub content: MetricDisplayContent,
    pub show_icon: bool,
    pub icon_asset_file: Option<&'static str>,
}

impl MetricDisplayParts {
    /// Returns standard value/unit text. Coordinate content is invalid here.
    pub fn standard_text(&self) -> (&str, Option<&str>) {
        match &self.content {
            MetricDisplayContent::Standard {
                value_text,
                unit_text,
            } => (value_text, unit_text.as_deref()),
            MetricDisplayContent::Coordinates(_) => {
                panic!("coordinate display content has no standard value/unit text")
            }
        }
    }
}

/// One formatted latitude or longitude line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricCoordinateLine {
    pub direction: Option<String>,
    pub value_text: String,
}

/// Coordinate display content. Two lines are used for the `both` unit mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricCoordinateDisplay {
    pub lines: Vec<MetricCoordinateLine>,
}

pub fn format_number(value: f64, decimals: usize) -> String {
    if decimals == 0 {
        let rounded = value.round();
        return if rounded == 0.0 {
            "0".to_string()
        } else {
            rounded.to_string()
        };
    }

    let factor = 10_f64.powi(decimals as i32);
    let rounded = (value * factor).round() / factor;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    format!("{rounded:.decimals$}")
}

pub fn resolve_metric_display_value(
    kind: MetricKind,
    raw: Option<f64>,
    available: bool,
) -> Option<f64> {
    if raw.is_some() {
        return raw;
    }

    if metric_interpolation(kind) == Some(InterpolationKind::Preserve) && available {
        Some(0.0)
    } else {
        None
    }
}

/// Formats a validated metric identically in native export and WASM preview.
pub fn format_metric_parts(
    validated: &ValidatedValueWidget,
    sample: &MetricSample<'_>,
) -> MetricDisplayParts {
    let icon_asset_file = crate::render::icons::metric_icon_asset_file(validated.metric);
    if validated.metric == MetricKind::GpsCoordinates {
        let latitude = sample.latitude;
        let longitude = sample.longitude;
        let mut coordinate = format_coordinates(
            latitude,
            longitude,
            &validated.display_unit,
            validated
                .coordinate_format
                .as_deref()
                .expect("gps_coordinates widget must have a validated coordinate_format"),
        );
        for line in &mut coordinate.lines {
            line.value_text = format!(
                "{}{}{}",
                validated.prefix, line.value_text, validated.suffix
            );
        }

        return MetricDisplayParts {
            content: MetricDisplayContent::Coordinates(coordinate),
            show_icon: validated.show_icon,
            icon_asset_file,
        };
    }

    let (mut value_text, unit_text) = format_validated_standard_metric_parts(validated, sample);

    if !validated.prefix.is_empty() {
        value_text = format!("{}{value_text}", validated.prefix);
    }
    if !validated.suffix.is_empty() {
        value_text.push_str(&validated.suffix);
    }

    MetricDisplayParts {
        content: MetricDisplayContent::Standard {
            value_text,
            unit_text,
        },
        show_icon: validated.show_icon,
        icon_asset_file,
    }
}

fn format_validated_standard_metric_parts(
    validated: &ValidatedValueWidget,
    sample: &MetricSample<'_>,
) -> (String, Option<String>) {
    let gear = sample.gear;
    let kind = validated.metric;
    let raw = resolve_metric_display_value(kind, sample.numeric, sample.available);
    let display_unit = Some(validated.display_unit.as_str());
    let decimals = validated_decimals(&validated.formatting);
    let show_units = validated.show_units;
    let unit_text = show_units.then(|| standard_metric_unit_label(kind, display_unit).to_string());

    let value_text = match standard_metric_formatter(kind) {
        Some(StandardMetricFormatterKind::Speed) => raw
            .map(|speed_mps| {
                format_number(
                    convert_standard_metric_value(kind, display_unit, speed_mps),
                    decimals,
                )
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Temperature) => raw
            .map(|temp_c| {
                let resolved = convert_standard_metric_value(kind, display_unit, temp_c);
                if decimals > 0 {
                    format!("{resolved:.decimals$}")
                } else {
                    (resolved as i64).to_string()
                }
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Pace) => raw
            .map(|seconds_per_km| {
                let total_seconds =
                    convert_standard_metric_value(kind, display_unit, seconds_per_km);
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
        Some(
            StandardMetricFormatterKind::Decimal
            | StandardMetricFormatterKind::Distance
            | StandardMetricFormatterKind::Elevation,
        ) => raw
            .map(|value| {
                if matches!(kind, MetricKind::Distance | MetricKind::DistanceToHome) {
                    let current = format_number(
                        convert_standard_metric_value(kind, display_unit, value),
                        decimals,
                    );
                    if kind == MetricKind::Distance && validated.show_full_distance == Some(true) {
                        if let Some(total) = sample.full_activity_distance {
                            let total = format_number(
                                convert_standard_metric_value(kind, display_unit, total),
                                decimals,
                            );
                            format!("{current}/{total}")
                        } else {
                            current
                        }
                    } else {
                        current
                    }
                } else {
                    let current = format_number(
                        convert_standard_metric_value(kind, display_unit, value),
                        decimals,
                    );
                    if kind == MetricKind::TotalAscent && validated.show_full_ascent == Some(true) {
                        if let Some(total) = sample.full_activity_total_ascent {
                            let total = format_number(
                                convert_standard_metric_value(kind, display_unit, total),
                                decimals,
                            );
                            format!("{current}/{total}")
                        } else {
                            current
                        }
                    } else {
                        current
                    }
                }
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
                let d = if ev == 0.0 { 1 } else { decimals };
                if d > 0 {
                    format!("{ev:.d$}")
                } else {
                    (ev.round() as i64).to_string()
                }
            })
            .unwrap_or_else(|| "--".to_string()),
        Some(StandardMetricFormatterKind::Gear) => match gear {
            Some("0") => "N".to_string(),
            Some(value) => value.to_string(),
            None => "--".to_string(),
        },
        Some(StandardMetricFormatterKind::Coordinates) => {
            unreachable!("planned metric formatter reached the active renderer")
        }
        Some(StandardMetricFormatterKind::LapTimer) => {
            unreachable!("lap_timer must use the dedicated lap timer renderer")
        }
        None => "--".to_string(),
    };

    (value_text, unit_text)
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

fn format_coordinates(
    latitude: Option<f64>,
    longitude: Option<f64>,
    display_unit: &str,
    coordinate_format: &str,
) -> MetricCoordinateDisplay {
    let lines = match display_unit {
        "latitude" => vec![format_coordinate_line(latitude, true, coordinate_format)],
        "longitude" => vec![format_coordinate_line(longitude, false, coordinate_format)],
        "both" => vec![
            format_coordinate_line(latitude, true, coordinate_format),
            format_coordinate_line(longitude, false, coordinate_format),
        ],
        _ => unreachable!("gps_coordinates display_unit was validated at ingress"),
    };
    MetricCoordinateDisplay { lines }
}

fn format_coordinate_line(
    value: Option<f64>,
    latitude: bool,
    coordinate_format: &str,
) -> MetricCoordinateLine {
    let Some(value) = value else {
        return MetricCoordinateLine {
            direction: None,
            value_text: format_coordinate_placeholder(coordinate_format),
        };
    };

    let direction = if latitude {
        if value < 0.0 {
            "S"
        } else {
            "N"
        }
    } else if value < 0.0 {
        "W"
    } else {
        "E"
    };
    let absolute = value.abs();
    let mut degrees = absolute.floor() as i64;
    let minutes_total = (absolute - degrees as f64) * 60.0;
    let mut minutes = minutes_total.floor() as i64;

    let value_text = match coordinate_format {
        "dms" => {
            let mut seconds = ((minutes_total - minutes as f64) * 60.0).round() as i64;
            if seconds == 60 {
                seconds = 0;
                minutes += 1;
            }
            if minutes == 60 {
                minutes = 0;
                degrees += 1;
            }
            format!("{degrees}\u{00B0}{minutes:02}\u{2032}{seconds:02}\u{2033}")
        }
        "ddm" => {
            let mut decimal_minutes = minutes_total;
            if decimal_minutes >= 59.9995 {
                decimal_minutes = 0.0;
                degrees += 1;
            }
            format!("{degrees}\u{00B0}{decimal_minutes:05.3}\u{2032}")
        }
        _ => unreachable!("gps_coordinates coordinate_format was validated at ingress"),
    };

    MetricCoordinateLine {
        direction: Some(direction.to_string()),
        value_text,
    }
}

fn format_coordinate_placeholder(coordinate_format: &str) -> String {
    match coordinate_format {
        "dms" => "--°--′--″".to_string(),
        "ddm" => "--°--.---′".to_string(),
        _ => unreachable!("gps_coordinates coordinate_format was validated at ingress"),
    }
}

/// Converts validated units. Native gauge callers may omit units to use the metric default.
pub fn convert_standard_metric_value(
    kind: MetricKind,
    display_unit: Option<&str>,
    value: f64,
) -> f64 {
    match kind {
        MetricKind::Heartrate
        | MetricKind::Cadence
        | MetricKind::Power
        | MetricKind::EnginePower
        | MetricKind::GroundContactTime
        | MetricKind::StrokeRate
        | MetricKind::GearPosition
        | MetricKind::VerticalRatio
        | MetricKind::Torque => value,
        MetricKind::Speed => match display_unit.unwrap_or("kmh") {
            "mph" => value * 2.23694,
            "kn" => value * 1.943844,
            "mps" => value,
            "kmh" => value * 3.6,
            _ => unreachable!("speed unit validated at ingress"),
        },
        MetricKind::Temperature | MetricKind::CoreTemperature => {
            if display_unit == Some("fahrenheit") {
                (value * 9.0 / 5.0) + 32.0
            } else {
                value
            }
        }
        MetricKind::Pace => {
            if display_unit == Some("min_per_mi") {
                value * 1.609_344
            } else {
                value
            }
        }
        MetricKind::VerticalOscillation => {
            if display_unit == Some("cm") {
                value / 10.0
            } else {
                value
            }
        }
        MetricKind::Distance | MetricKind::DistanceToHome => match display_unit.unwrap_or("km") {
            "m" => value,
            "mi" => value / 1609.344,
            "ft" => value * 3.280_84,
            "km" => value / 1000.0,
            _ => unreachable!("distance unit validated at ingress"),
        },
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
            "mbar" | "hpa" => value * 1000.0,
            _ => unreachable!("pressure unit validated at ingress"),
        },
        MetricKind::Altitude => {
            if display_unit == Some("ft") {
                value * 3.280_84
            } else {
                value
            }
        }
        MetricKind::TotalAscent => {
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
            "m" => value,
            _ => unreachable!("stride length unit validated at ingress"),
        },
        MetricKind::VerticalSpeed => match display_unit.unwrap_or("mps") {
            "ftmin" => value * 196.850_394,
            "ftph" => value * 11_811.023_64,
            "mph_vertical" => value * 3600.0,
            "mps" => value,
            _ => unreachable!("vertical speed unit validated at ingress"),
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
    // FIT's missing-balance sentinel can decode as 127; show degenerate values as neutral.
    let normalized_left = if left_value >= 100.0 {
        50.0
    } else {
        left_value.clamp(0.0, 100.0)
    };
    let left = format_number(normalized_left, decimals);
    let right = format_number(100.0 - normalized_left, decimals);
    match balance_format.expect("balance format validated at ingress") {
        "percent_label" => format!("{left}%/{right}%"),
        "plain" => format!("{left}/{right}"),
        "l_prefix" => format!("L{left}/R{right}"),
        "l_suffix" => format!("{left}L/{right}R"),
        _ => unreachable!("balance format validated at ingress"),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_balance_value, format_coordinates, format_number};

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

    #[test]
    fn invalid_or_degenerate_balance_is_displayed_as_neutral() {
        assert_eq!(format_balance_value(100.0, 0, Some("plain")), "50/50");
        assert_eq!(
            format_balance_value(127.0, 0, Some("percent_label")),
            "50%/50%"
        );
    }

    #[test]
    fn number_format_preserves_requested_trailing_zeroes() {
        assert_eq!(format_number(2.0, 1), "2.0");
        assert_eq!(format_number(2.3, 2), "2.30");
    }

    #[test]
    fn coordinate_format_handles_equator_and_prime_meridian() {
        let display = format_coordinates(Some(0.0), Some(0.0), "both", "dms");
        assert_eq!(display.lines[0].direction.as_deref(), Some("N"));
        assert_eq!(display.lines[0].value_text, "0°00′00″");
        assert_eq!(display.lines[1].direction.as_deref(), Some("E"));
        assert_eq!(display.lines[1].value_text, "0°00′00″");
    }

    #[test]
    fn coordinate_format_uses_padded_fields_without_spaces() {
        let dms = format_coordinates(Some(8.1), Some(-8.1), "both", "dms");
        assert_eq!(dms.lines[0].value_text, "8°06′00″");
        assert_eq!(dms.lines[1].value_text, "8°06′00″");

        let ddm = format_coordinates(Some(8.0), Some(-8.0), "both", "ddm");
        assert_eq!(ddm.lines[0].value_text, "8°0.000′");
        assert_eq!(ddm.lines[1].value_text, "8°0.000′");

        let missing = format_coordinates(None, None, "both", "dms");
        assert_eq!(missing.lines[0].value_text, "--°--′--″");
        assert_eq!(missing.lines[1].value_text, "--°--′--″");
    }
}
