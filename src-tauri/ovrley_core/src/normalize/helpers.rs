//! Shared validation helpers used by all normalize submodules.
//!
//! Every `require_*` function rejects `None` or out-of-range values with a
//! descriptive error. The `rgba_from_hex` function converts a hex color string
//! + opacity into a premultiplied `[u8; 4]` RGBA array.

use crate::error::{CoreError, CoreResult};

// ---------------------------------------------------------------------------
// Option unwrappers — reject None
// ---------------------------------------------------------------------------

pub(crate) fn require_f32(v: Option<f32>, field: &str) -> CoreResult<f32> {
    v.ok_or_else(|| CoreError::Config(format!("{field}: required")))
        .and_then(|v| {
            if !v.is_finite() {
                Err(CoreError::Config(format!("{field}: must be finite")))
            } else {
                Ok(v)
            }
        })
}

pub(crate) fn require_bool(v: Option<bool>, field: &str) -> CoreResult<bool> {
    v.ok_or_else(|| CoreError::Config(format!("{field}: required")))
}

pub(crate) fn require_string(v: Option<String>, field: &str) -> CoreResult<String> {
    v.ok_or_else(|| CoreError::Config(format!("{field}: required")))
}

pub(crate) fn require_str<'a>(v: Option<&'a str>, field: &str) -> CoreResult<&'a str> {
    v.ok_or_else(|| CoreError::Config(format!("{field}: required")))
}

// ---------------------------------------------------------------------------
// Value range checkers
// ---------------------------------------------------------------------------

pub(crate) fn require_positive_f32(v: Option<f32>, field: &str) -> CoreResult<f32> {
    let v = v.ok_or_else(|| CoreError::Config(format!("{field}: required")))?;
    if v <= 0.0 || !v.is_finite() {
        Err(CoreError::Config(format!(
            "{field}: must be a positive finite number"
        )))
    } else {
        Ok(v)
    }
}

pub(crate) fn require_non_negative_f32(v: f32, field: &str) -> CoreResult<f32> {
    if v < 0.0 || !v.is_finite() {
        Err(CoreError::Config(format!("{field}: must be >= 0")))
    } else {
        Ok(v)
    }
}

pub(crate) fn require_percentage(v: f32, field: &str) -> CoreResult<f32> {
    if !(0.0..=100.0).contains(&v) || !v.is_finite() {
        Err(CoreError::Config(format!("{field}: must be 0.0–100.0")))
    } else {
        Ok(v)
    }
}

/// Accepts 0.0–1.0 or 0–100 (percentage), normalizes to 0.0–1.0.
pub(crate) fn require_opacity(v: Option<f32>, field: &str) -> CoreResult<f32> {
    let v = v.ok_or_else(|| CoreError::Config(format!("{field}: required")))?;
    if !v.is_finite() {
        return Err(CoreError::Config(format!("{field}: must be finite")));
    }
    let normalized = if v > 1.0 { v / 100.0 } else { v };
    if !(0.0..=1.0).contains(&normalized) {
        return Err(CoreError::Config(format!(
            "{field}: must be between 0.0 and 1.0 (or 0-100)"
        )));
    }
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Hex color
// ---------------------------------------------------------------------------

/// Validates a hex color string (`#rrggbb` or `#rrggbbaa`) and returns it.
pub(crate) fn require_hex_color(v: Option<&str>, field: &str) -> CoreResult<String> {
    let s = v.ok_or_else(|| CoreError::Config(format!("{field}: required")))?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Config(format!("{field}: required")));
    }
    if !trimmed.starts_with('#') || !(trimmed.len() == 7 || trimmed.len() == 9) {
        return Err(CoreError::Config(format!(
            "{field}: invalid hex color '{trimmed}' — expected #rrggbb or #rrggbbaa"
        )));
    }
    Ok(trimmed.to_string())
}

/// Converts a hex color string + opacity into a premultiplied `[u8; 4]` RGBA array.
pub(crate) fn rgba_from_hex(hex: &str, field: &str, opacity: f32) -> CoreResult<[u8; 4]> {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() != 6 && trimmed.len() != 8 {
        return Err(CoreError::Config(format!(
            "{field}: expected 6-digit or 8-digit hex, got '{hex}'"
        )));
    }
    let r = u8::from_str_radix(&trimmed[0..2], 16)
        .map_err(|_| CoreError::Config(format!("{field}: invalid hex color '{hex}'")))?;
    let g = u8::from_str_radix(&trimmed[2..4], 16)
        .map_err(|_| CoreError::Config(format!("{field}: invalid hex color '{hex}'")))?;
    let b = u8::from_str_radix(&trimmed[4..6], 16)
        .map_err(|_| CoreError::Config(format!("{field}: invalid hex color '{hex}'")))?;
    let source_alpha = if trimmed.len() == 8 {
        u8::from_str_radix(&trimmed[6..8], 16)
            .map_err(|_| CoreError::Config(format!("{field}: invalid hex color '{hex}'")))?
    } else {
        255
    };
    let a = ((source_alpha as f32) * opacity.clamp(0.0, 1.0)).round() as u8;
    Ok([r, g, b, a])
}

// ---------------------------------------------------------------------------
// Enum normalization
// ---------------------------------------------------------------------------

pub(crate) fn normalize_marker_variant(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "ring" => "ring".to_string(),
        "halo" => "halo".to_string(),
        _ => "single".to_string(),
    }
}
