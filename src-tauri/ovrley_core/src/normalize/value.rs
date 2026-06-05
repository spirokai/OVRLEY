//! Value widget validation.
//!
//! `validate_value_widget` verifies that every output-affecting field is
//! already explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config to the backend.
//!
//! Shadow, border, and other scene-level properties are NOT part of the
//! value contract — they belong to the scene validation contract.

use super::helpers::{require_bool, require_f32, require_str, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::standard_metrics::is_standard_metric;
use crate::types::DisplayType;
use crate::MetricKind;

// ---------------------------------------------------------------------------
// ValidatedValueWidget — zero backend-side defaults
// ---------------------------------------------------------------------------

/// Explicit formatting contract for the standard-metric text/value slice.
#[derive(Clone, Debug)]
pub enum ValidatedValueFormatting {
    DecimalPlaces {
        decimals: usize,
    },
    DecimalRounding {
        decimal_rounding: i32,
    },
    Balance {
        decimals: usize,
        balance_format: String,
    },
    BalanceRounded {
        decimal_rounding: i32,
        balance_format: String,
    },
}

impl ValidatedValueFormatting {
    /// Returns the balance format string if this is a balance variant.
    pub fn balance_format(&self) -> Option<&str> {
        match self {
            Self::Balance { balance_format, .. } | Self::BalanceRounded { balance_format, .. } => {
                Some(balance_format)
            }
            _ => None,
        }
    }
}

/// Every output-affecting field for the standard-metric text/value slice is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedValueWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub display_type: DisplayType,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub show_icon: bool,
    pub icon_color: [u8; 4],
    pub icon_size: f32,
    pub icon_offset_x: f32,
    pub icon_offset_y: f32,
    pub show_units: bool,
    pub unit_color: [u8; 4],
    pub display_unit: String,
    pub prefix: String,
    pub suffix: String,
    pub formatting: ValidatedValueFormatting,
    pub hours_offset: Option<i64>,
    pub format: Option<String>,
}

// ---------------------------------------------------------------------------
// Validation — every output-affecting field must be explicit
// ---------------------------------------------------------------------------

pub fn validate_value_widget(value: ValueConfig, index: usize) -> CoreResult<ValidatedValueWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if !is_standard_metric(value.value) {
        return Err(CoreError::Config(format!(
            "{}: metric {:?} is outside the standard metric text/value validation slice",
            p("value"),
            value.value
        )));
    }

    if value.display_type != DisplayType::Text {
        return Err(CoreError::Config(format!(
            "{}: display_type '{}' is outside the standard metric text/value validation slice",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let font_name = require_string(value.font, &p("font"))?;

    // -- opacity ----------------------------------------------------------
    let opacity = require_f32(value.opacity, &p("opacity"))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..1.0, got {opacity}",
            p("opacity")
        )));
    }

    let font_size = require_f32(value.font_size, &p("font_size"))?;
    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {font_size}",
            p("font_size")
        )));
    }

    // -- colour -----------------------------------------------------------
    let colour_hex = require_str(value.color.as_deref(), &p("color"))?;
    let color = rgba_from_hex(colour_hex, &p("color"), opacity)?;

    // -- icon -- all explicit ---------------------------------------------
    let show_icon = require_bool(value.show_icon, &p("show_icon"))?;
    let icon_color = rgba_from_hex(
        require_str(value.icon_color.as_deref(), &p("icon_color"))?,
        &p("icon_color"),
        opacity,
    )?;
    let icon_size = require_f32(value.icon_size, &p("icon_size"))?;
    if icon_size < 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be >= 0, got {icon_size}",
            p("icon_size")
        )));
    }
    let icon_offset_x = require_f32(value.icon_offset_x, &p("icon_offset_x"))?;
    let icon_offset_y = require_f32(value.icon_offset_y, &p("icon_offset_y"))?;

    // -- units -- all explicit --------------------------------------------
    let show_units = require_bool(value.show_units, &p("show_units"))?;
    let unit_color = rgba_from_hex(
        require_str(value.unit_color.as_deref(), &p("unit_color"))?,
        &p("unit_color"),
        opacity,
    )?;
    let display_unit = require_string(value.display_unit, &p("display_unit"))?;

    // -- affixes are output-affecting and must be explicit ----------------
    let prefix = require_string(value.prefix, &p("prefix"))?;
    let suffix = require_string(value.suffix, &p("suffix"))?;

    // -- formatting must be explicit, not inferred later ------------------
    let formatting = if value.value == MetricKind::LeftRightBalance {
        match (value.decimals, value.decimal_rounding) {
            (Some(decimals), None) => ValidatedValueFormatting::Balance {
                decimals,
                balance_format: require_string(value.balance_format, &p("balance_format"))?,
            },
            (None, Some(decimal_rounding)) => ValidatedValueFormatting::BalanceRounded {
                decimal_rounding,
                balance_format: require_string(value.balance_format, &p("balance_format"))?,
            },
            (Some(_), Some(_)) => {
                return Err(CoreError::Config(format!(
                    "{} and {}: provide exactly one precision field for balance widgets",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
            (None, None) => {
                return Err(CoreError::Config(format!(
                    "{} or {}: one precision field must be explicit",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
        }
    } else {
        match (value.decimals, value.decimal_rounding) {
            (Some(decimals), None) => ValidatedValueFormatting::DecimalPlaces { decimals },
            (None, Some(decimal_rounding)) => {
                ValidatedValueFormatting::DecimalRounding { decimal_rounding }
            }
            (Some(_), Some(_)) => {
                return Err(CoreError::Config(format!(
                    "{} and {}: provide exactly one precision field",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
            (None, None) => {
                return Err(CoreError::Config(format!(
                    "{} or {}: one precision field must be explicit",
                    p("decimals"),
                    p("decimal_rounding")
                )));
            }
        }
    };

    Ok(ValidatedValueWidget {
        metric: value.value,
        x: value.x,
        y: value.y,
        display_type: value.display_type,
        font_name,
        font_size,
        color,
        opacity,
        show_icon,
        icon_color,
        icon_size,
        icon_offset_x,
        icon_offset_y,
        show_units,
        unit_color,
        display_unit,
        prefix,
        suffix,
        formatting,
        hours_offset: value.hours_offset.map(i64::from),
        format: value.format,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn explicit_speed() -> ValueConfig {
        serde_json::from_value(serde_json::json!({
            "value": "speed", "x": 100.0, "y": 200.0,
            "font": "Arial.ttf", "font_family": "Arial", "font_size": 100.0,
            "color": "#ffffff", "opacity": 1.0,
            "show_icon": true, "icon_color": "#ffffff", "icon_size": 45.0,
            "icon_offset_x": 0.0, "icon_offset_y": 0.0,
            "show_units": true, "unit_color": "#ffffff", "display_unit": "kmh",
            "decimals": 0,
            "prefix": "", "suffix": ""
        }))
        .unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_value_widget(explicit_speed(), 0).is_ok());
    }

    #[test]
    fn missing_color_rejected() {
        let mut v = explicit_speed();
        v.color = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("color") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_opacity_rejected() {
        let mut v = explicit_speed();
        v.opacity = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("opacity") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_icon_color_rejected() {
        let mut v = explicit_speed();
        v.icon_color = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("icon_color") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_icon_size_rejected() {
        let mut v = explicit_speed();
        v.icon_size = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("icon_size") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_icon_offset_rejected() {
        let mut v = explicit_speed();
        v.icon_offset_x = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("icon_offset_x") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_show_icon_rejected() {
        let mut v = explicit_speed();
        v.show_icon = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("show_icon") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_unit_color_rejected() {
        let mut v = explicit_speed();
        v.unit_color = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("unit_color") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_display_unit_rejected() {
        let mut v = explicit_speed();
        v.display_unit = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("display_unit") && e.contains("required"), "{e}");
    }

    #[test]
    fn invalid_color_hex_rejected() {
        let mut v = explicit_speed();
        v.color = Some("bad".into());
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("color") && e.contains("hex"), "{e}");
    }

    #[test]
    fn opacity_out_of_range_rejected() {
        let mut v = explicit_speed();
        v.opacity = Some(2.0);
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("opacity"), "{e}");
    }

    #[test]
    fn negative_font_size_rejected() {
        let mut v = explicit_speed();
        v.font_size = Some(-10.0);
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("font_size"), "{e}");
    }

    #[test]
    fn missing_font_size_rejected() {
        let mut v = explicit_speed();
        v.font_size = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("font_size") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_font_rejected() {
        let mut v = explicit_speed();
        v.font = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("font") && e.contains("required"), "{e}");
    }

    #[test]
    fn rgba_hex_rejected_only_when_invalid() {
        let mut v = explicit_speed();
        v.color = Some("#ffffffff".into());
        assert!(validate_value_widget(v, 0).is_ok());
    }

    #[test]
    fn missing_prefix_rejected() {
        let mut v = explicit_speed();
        v.prefix = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("prefix") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_suffix_rejected() {
        let mut v = explicit_speed();
        v.suffix = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("suffix") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_precision_rejected() {
        let mut v = explicit_speed();
        v.decimals = None;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("decimals") || e.contains("decimal_rounding"),
            "{e}"
        );
    }

    #[test]
    fn both_precision_fields_rejected() {
        let mut v = explicit_speed();
        v.decimal_rounding = Some(0);
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("exactly one precision"), "{e}");
    }

    #[test]
    fn non_text_display_type_rejected() {
        let mut v = explicit_speed();
        v.display_type = DisplayType::Linear;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("display_type") && e.contains("validation slice"),
            "{e}"
        );
    }

    #[test]
    fn non_standard_metric_rejected() {
        let mut v = explicit_speed();
        v.value = MetricKind::Gradient;
        let e = validate_value_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("validation slice"), "{e}");
    }
}
