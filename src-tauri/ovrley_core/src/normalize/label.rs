//! Label widget validation.
//!
//! `validate_label` verifies that every output-affecting field for a static
//! text label is already explicit. Missing fields are rejected — the backend
//! owns zero render-affecting defaults. The frontend must materialise all
//! defaults before sending the config.
//!
//! Shadow, border, and other scene-level properties are NOT part of the
//! label contract — they belong to the scene validation contract.

use super::helpers::rgba_from_hex;
use super::raw::{LabelConfig, LabelTypography};
use crate::error::{CoreError, CoreResult};

// ---------------------------------------------------------------------------
// ValidatedLabel — zero backend-side defaults
// ---------------------------------------------------------------------------

/// Every output-affecting field for a static text label is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub typography: LabelTypography,
    pub color: [u8; 4],
    pub opacity: f32,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validates a label config and returns a `ValidatedLabel` with all
/// output-affecting fields explicit. Missing or invalid fields cause an
/// immediate error.
pub fn validate_label(label: &LabelConfig, index: usize) -> CoreResult<ValidatedLabel> {
    let p = |f: &str| format!("labels[{index}].{f}");

    // text: allow empty (labels can be empty by design)
    let text = label.text.clone();

    // x, y: required
    // (already non-optional in LabelConfig, so always present)

    // font_name: required — frontend must resolve from label.font →
    // label.font_family → scene.font before sending
    let font_name = label
        .font
        .clone()
        .or_else(|| label.font_family.clone())
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("font"))))?;

    if font_name.is_empty() {
        return Err(CoreError::Config(format!(
            "{}: font name must not be empty",
            p("font")
        )));
    }

    // font_size: required
    let font_size = label
        .font_size
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("font_size"))))?;

    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be positive, got {font_size}",
            p("font_size")
        )));
    }

    // color: required
    let color_hex = label
        .color
        .as_deref()
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("color"))))?;

    let opacity = label
        .opacity
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("opacity"))))?;

    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..=1.0, got {opacity}",
            p("opacity")
        )));
    }

    let color = rgba_from_hex(color_hex, &p("color"), opacity)?;

    Ok(ValidatedLabel {
        text,
        x: label.x,
        y: label.y,
        font_name,
        font_size,
        typography: label.typography.clone(),
        color,
        opacity,
    })
}
