//! Field validation shared by every widget's ingress.
//!
//! Each `require_*` function rejects an absent (`None`) or out-of-range value with an error naming
//! the field, and returns the value it accepted.

use crate::{WidgetError, WidgetResult};

fn present<T>(value: Option<T>, field: &str) -> WidgetResult<T> {
    value.ok_or_else(|| WidgetError::Config(format!("{field}: required")))
}

pub fn require_f32(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let value = present(value, field)?;
    if !value.is_finite() {
        return Err(WidgetError::Config(format!("{field}: must be finite")));
    }
    Ok(value)
}

pub fn require_finite_f64(value: f64, field: &str) -> WidgetResult<f64> {
    if !value.is_finite() {
        return Err(WidgetError::Config(format!(
            "{field}: must be a finite number"
        )));
    }
    Ok(value)
}

pub fn require_bool(value: Option<bool>, field: &str) -> WidgetResult<bool> {
    present(value, field)
}

pub fn require_string(value: Option<String>, field: &str) -> WidgetResult<String> {
    present(value, field)
}

pub fn require_str<'a>(value: Option<&'a str>, field: &str) -> WidgetResult<&'a str> {
    present(value, field)
}

pub fn require_u32(value: Option<u32>, field: &str) -> WidgetResult<u32> {
    present(value, field)
}

pub fn require_positive_f64(value: f64, field: &str) -> WidgetResult<f64> {
    if value <= 0.0 || !value.is_finite() {
        return Err(WidgetError::Config(format!(
            "{field}: must be a positive finite number"
        )));
    }
    Ok(value)
}

pub fn require_positive_f32(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let value = present(value, field)?;
    if value <= 0.0 || !value.is_finite() {
        return Err(WidgetError::Config(format!(
            "{field}: must be a positive finite number"
        )));
    }
    Ok(value)
}

pub fn require_positive_u32(value: Option<u32>, field: &str) -> WidgetResult<u32> {
    match present(value, field)? {
        0 => Err(WidgetError::Config(format!("{field}: must be > 0, got 0"))),
        value => Ok(value),
    }
}

pub fn require_non_negative_f32(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let value = require_f32(value, field)?;
    if value < 0.0 {
        return Err(WidgetError::Config(format!("{field}: must be >= 0")));
    }
    Ok(value)
}

pub fn require_percentage(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let value = require_f32(value, field)?;
    if !(0.0..=100.0).contains(&value) {
        return Err(WidgetError::Config(format!("{field}: must be 0.0–100.0")));
    }
    Ok(value)
}

/// An opacity in `0.0..=1.0`.
pub fn require_unit_opacity(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let opacity = require_f32(value, field)?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(WidgetError::Config(format!(
            "{field}: must be 0.0..=1.0, got {opacity}"
        )));
    }
    Ok(opacity)
}

/// An opacity in `0.0..=1.0`, or a percentage above 1, normalized to `0.0..=1.0`. Plot templates
/// store either.
pub fn require_opacity(value: Option<f32>, field: &str) -> WidgetResult<f32> {
    let value = require_f32(value, field)?;
    let normalized = if value > 1.0 { value / 100.0 } else { value };
    if !(0.0..=1.0).contains(&normalized) {
        return Err(WidgetError::Config(format!(
            "{field}: must be between 0.0 and 1.0 (or 0-100)"
        )));
    }
    Ok(normalized)
}

pub fn require_border_fits_dimension(
    border_thickness: f32,
    dimension: f32,
    field: &str,
) -> WidgetResult<()> {
    if border_thickness * 2.0 >= dimension {
        return Err(WidgetError::Config(format!(
            "{field}: 2 * border_thickness must be less than the dimension"
        )));
    }
    Ok(())
}

/// A `#rrggbb` or `#rrggbbaa` colour, trimmed.
pub fn require_hex_color(value: Option<&str>, field: &str) -> WidgetResult<String> {
    let trimmed = present(value, field)?.trim();
    if trimmed.is_empty() {
        return Err(WidgetError::Config(format!("{field}: required")));
    }
    if !trimmed.starts_with('#')
        || !(trimmed.len() == 7 || trimmed.len() == 9)
        || !trimmed.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err(WidgetError::Config(format!(
            "{field}: invalid hex color '{trimmed}' — expected #rrggbb or #rrggbbaa"
        )));
    }
    Ok(trimmed.to_string())
}

/// A required hex colour as straight RGBA, its alpha scaled by `opacity`.
pub fn require_rgba(value: Option<&str>, field: &str, opacity: f32) -> WidgetResult<[u8; 4]> {
    rgba_from_hex(require_str(value, field)?, field, opacity)
}

/// A 6- or 8-digit hex colour, with or without `#`, as straight RGBA whose alpha is scaled by
/// `opacity`.
pub fn rgba_from_hex(hex: &str, field: &str, opacity: f32) -> WidgetResult<[u8; 4]> {
    let digits = hex.trim().trim_start_matches('#');
    if (digits.len() != 6 && digits.len() != 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(WidgetError::Config(format!(
            "{field}: expected 6-digit or 8-digit hex, got '{hex}'"
        )));
    }
    let channel = |index: usize| {
        u8::from_str_radix(&digits[index..index + 2], 16)
            .expect("validated hex digits parse as a byte")
    };
    let source_alpha = if digits.len() == 8 { channel(6) } else { 255 };
    let alpha = (f32::from(source_alpha) * opacity.clamp(0.0, 1.0)).round() as u8;
    Ok([channel(0), channel(2), channel(4), alpha])
}

/// Decimal places, configured directly or as the scene's rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Decimals(usize),
    /// Negative rounding shows no decimals.
    Rounding(i32),
}

impl Precision {
    pub fn decimals(self) -> usize {
        match self {
            Self::Decimals(decimals) => decimals,
            Self::Rounding(rounding) => usize::try_from(rounding).unwrap_or(0),
        }
    }
}

/// Exactly one of `decimals` and `decimal_rounding`; `path` names a field.
pub fn require_precision(
    decimals: Option<usize>,
    decimal_rounding: Option<i32>,
    path: impl Fn(&str) -> String,
) -> WidgetResult<Precision> {
    match (decimals, decimal_rounding) {
        (Some(decimals), None) => Ok(Precision::Decimals(decimals)),
        (None, Some(rounding)) => Ok(Precision::Rounding(rounding)),
        (Some(_), Some(_)) => Err(WidgetError::Config(format!(
            "{} and {}: provide exactly one precision field",
            path("decimals"),
            path("decimal_rounding")
        ))),
        (None, None) => Err(WidgetError::Config(format!(
            "{} or {}: one precision field must be explicit",
            path("decimals"),
            path("decimal_rounding")
        ))),
    }
}
