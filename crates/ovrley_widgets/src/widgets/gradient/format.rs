use super::validation::ValidatedGradientWidget;
use crate::widgets::value::format::format_number;

/// Formats the existing signed percentage and affixes. Missing telemetry shows `--%`.
pub fn format_validated_gradient(validated: &ValidatedGradientWidget, raw: Option<f64>) -> String {
    let Some(value) = raw else {
        return "--%".to_string();
    };
    let magnitude = format_number(value.abs(), validated.formatting.decimals());
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    let sign = if validated.show_sign { sign } else { "" };
    format!("{}{sign}{magnitude}%{}", validated.prefix, validated.suffix)
}
