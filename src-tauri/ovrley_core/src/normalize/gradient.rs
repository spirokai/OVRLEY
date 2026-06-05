//! Gradient widget validation.
//!
//! `validate_gradient_widget` verifies that every output-affecting field is
//! already explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.
//!
//! Gradient is a specialized render path (not a standard metric) with unique
//! fields: triangle colors, show_sign, show_triangle, triangle_width,
//! value_offset, and unit_color.

use super::helpers::{require_bool, require_f32, require_str, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::MetricKind;

// ---------------------------------------------------------------------------
// ValidatedGradientWidget — zero backend-side defaults
// ---------------------------------------------------------------------------

/// Explicit formatting contract for the gradient widget slice.
#[derive(Clone, Debug)]
pub enum ValidatedGradientFormatting {
    DecimalPlaces { decimals: usize },
    DecimalRounding { decimal_rounding: i32 },
}

impl ValidatedGradientFormatting {
    pub fn decimals(&self) -> usize {
        match self {
            Self::DecimalPlaces { decimals } => *decimals,
            Self::DecimalRounding { decimal_rounding } => (*decimal_rounding).max(0) as usize,
        }
    }
}

/// Every output-affecting field for the gradient widget is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedGradientWidget {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
    pub prefix: String,
    pub suffix: String,
    pub formatting: ValidatedGradientFormatting,
    pub show_sign: bool,
    pub show_triangle: bool,
    pub triangle_width: f32,
    pub value_offset: f32,
    pub unit_color: [u8; 4],
    pub triangle_positive_color: [u8; 4],
    pub triangle_negative_color: [u8; 4],
}

// ---------------------------------------------------------------------------
// Validation — every output-affecting field must be explicit
// ---------------------------------------------------------------------------

pub fn validate_gradient_widget(
    value: ValueConfig,
    index: usize,
) -> CoreResult<ValidatedGradientWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if value.value != MetricKind::Gradient {
        return Err(CoreError::Config(format!(
            "{}: expected Gradient, got {:?}",
            p("value"),
            value.value
        )));
    }

    let font_name = require_string(value.font, &p("font"))?;

    let opacity = require_f32(value.opacity, &p("opacity"))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..=1.0, got {opacity}",
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

    let colour_hex = require_str(value.color.as_deref(), &p("color"))?;
    let color = rgba_from_hex(colour_hex, &p("color"), opacity)?;

    // -- affixes are output-affecting and must be explicit ----------------
    let prefix = require_string(value.prefix, &p("prefix"))?;
    let suffix = require_string(value.suffix, &p("suffix"))?;

    // -- formatting must be explicit, not inferred later ------------------
    let formatting = match (value.decimals, value.decimal_rounding) {
        (Some(decimals), None) => ValidatedGradientFormatting::DecimalPlaces { decimals },
        (None, Some(decimal_rounding)) => {
            ValidatedGradientFormatting::DecimalRounding { decimal_rounding }
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
    };

    // -- gradient-specific fields, all explicit --------------------------
    let show_sign = require_bool(value.show_sign, &p("show_sign"))?;
    let show_triangle = require_bool(value.show_triangle, &p("show_triangle"))?;

    let triangle_width = require_f32(value.triangle_width, &p("triangle_width"))?;
    if triangle_width < 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be >= 0, got {triangle_width}",
            p("triangle_width")
        )));
    }

    let value_offset = require_f32(value.value_offset, &p("value_offset"))?;

    let unit_color = rgba_from_hex(
        require_str(value.unit_color.as_deref(), &p("unit_color"))?,
        &p("unit_color"),
        opacity,
    )?;

    let triangle_positive_color = rgba_from_hex(
        require_str(
            value.triangle_positive_color.as_deref(),
            &p("triangle_positive_color"),
        )?,
        &p("triangle_positive_color"),
        opacity,
    )?;

    let triangle_negative_color = rgba_from_hex(
        require_str(
            value.triangle_negative_color.as_deref(),
            &p("triangle_negative_color"),
        )?,
        &p("triangle_negative_color"),
        opacity,
    )?;

    Ok(ValidatedGradientWidget {
        x: value.x,
        y: value.y,
        font_name,
        font_size,
        color,
        opacity,
        prefix,
        suffix,
        formatting,
        show_sign,
        show_triangle,
        triangle_width,
        value_offset,
        unit_color,
        triangle_positive_color,
        triangle_negative_color,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn explicit_gradient() -> ValueConfig {
        serde_json::from_value(json!({
            "value": "gradient", "x": 100.0, "y": 200.0,
            "font": "Arial.ttf", "font_size": 48.0,
            "color": "#ffffff", "opacity": 1.0,
            "decimals": 1,
            "prefix": "", "suffix": "",
            "show_sign": true, "show_triangle": true,
            "triangle_width": 72.0, "value_offset": 0.0,
            "unit_color": "#ffffff",
            "triangle_positive_color": "#40e0d0",
            "triangle_negative_color": "#c65102"
        }))
        .unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_gradient_widget(explicit_gradient(), 0).is_ok());
    }

    #[test]
    fn missing_font_rejected() {
        let mut v = explicit_gradient();
        v.font = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("font") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_opacity_rejected() {
        let mut v = explicit_gradient();
        v.opacity = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("opacity") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_color_rejected() {
        let mut v = explicit_gradient();
        v.color = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("color") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_prefix_rejected() {
        let mut v = explicit_gradient();
        v.prefix = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("prefix") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_suffix_rejected() {
        let mut v = explicit_gradient();
        v.suffix = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("suffix") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_precision_rejected() {
        let mut v = explicit_gradient();
        v.decimals = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("decimals") || e.contains("decimal_rounding"),
            "{e}"
        );
    }

    #[test]
    fn both_precision_fields_rejected() {
        let mut v = explicit_gradient();
        v.decimal_rounding = Some(0);
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("exactly one precision"), "{e}");
    }

    #[test]
    fn missing_show_sign_rejected() {
        let mut v = explicit_gradient();
        v.show_sign = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("show_sign") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_show_triangle_rejected() {
        let mut v = explicit_gradient();
        v.show_triangle = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("show_triangle") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_triangle_width_rejected() {
        let mut v = explicit_gradient();
        v.triangle_width = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("triangle_width") && e.contains("required"),
            "{e}"
        );
    }

    #[test]
    fn missing_value_offset_rejected() {
        let mut v = explicit_gradient();
        v.value_offset = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("value_offset") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_unit_color_rejected() {
        let mut v = explicit_gradient();
        v.unit_color = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("unit_color") && e.contains("required"), "{e}");
    }

    #[test]
    fn missing_triangle_positive_color_rejected() {
        let mut v = explicit_gradient();
        v.triangle_positive_color = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("triangle_positive_color") && e.contains("required"),
            "{e}"
        );
    }

    #[test]
    fn missing_triangle_negative_color_rejected() {
        let mut v = explicit_gradient();
        v.triangle_negative_color = None;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(
            e.contains("triangle_negative_color") && e.contains("required"),
            "{e}"
        );
    }

    #[test]
    fn invalid_color_hex_rejected() {
        let mut v = explicit_gradient();
        v.color = Some("bad".into());
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("color") && e.contains("hex"), "{e}");
    }

    #[test]
    fn opacity_out_of_range_rejected() {
        let mut v = explicit_gradient();
        v.opacity = Some(2.0);
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("opacity"), "{e}");
    }

    #[test]
    fn negative_font_size_rejected() {
        let mut v = explicit_gradient();
        v.font_size = Some(-10.0);
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("font_size"), "{e}");
    }

    #[test]
    fn negative_triangle_width_rejected() {
        let mut v = explicit_gradient();
        v.triangle_width = Some(-1.0);
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("triangle_width") && e.contains(">="), "{e}");
    }

    #[test]
    fn non_gradient_metric_rejected() {
        let mut v = explicit_gradient();
        v.value = MetricKind::Speed;
        let e = validate_gradient_widget(v, 0).unwrap_err().to_string();
        assert!(e.contains("Gradient"), "{e}");
    }

    #[test]
    fn index_in_error_path() {
        let mut v = explicit_gradient();
        v.font = None;
        let e = validate_gradient_widget(v, 3).unwrap_err().to_string();
        assert!(e.contains("values[3]"), "{e}");
    }

    #[test]
    fn rgba_hex_parsed_correctly() {
        let mut v = explicit_gradient();
        v.color = Some("#ff000080".into());
        let result = validate_gradient_widget(v, 0).unwrap();
        assert_eq!(result.color, [255, 0, 0, 128]);
    }
}
