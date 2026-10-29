//! Dedicated sector labels show magnitude; the fill retains the signed angle.
use super::layout::LEAN_ANGLE_MAX_FILL_SWEEP;
use crate::widgets::value::format::format_number;

pub fn format_lean_angle_value(raw: Option<f64>) -> String {
    raw.map_or_else(|| "--".into(), |value| format_number(value.abs(), 0))
}

pub fn lean_angle_fill_sweep(raw: Option<f64>) -> f32 {
    raw.map_or(0.0, |value| {
        value.clamp(
            -f64::from(LEAN_ANGLE_MAX_FILL_SWEEP),
            f64::from(LEAN_ANGLE_MAX_FILL_SWEEP),
        ) as f32
    })
}
