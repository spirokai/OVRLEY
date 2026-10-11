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

use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::types::MetricKind;
use ovrley_widgets::model::validation::{
    require_bool, require_f32, require_str, require_string, rgba_from_hex,
};
use ovrley_widgets::widgets::value::config::ValueConfig;

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
