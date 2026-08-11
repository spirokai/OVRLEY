//! Lap timer widget validation.

use super::helpers::{require_bool, require_f32, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::types::{DisplayType, MetricKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapTimerMode {
    CurrentLap,
    BestLap,
    Delta,
    LapLog,
}

#[derive(Clone, Debug)]
pub struct ValidatedLapTimer {
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub positive_delta_color: [u8; 4],
    pub negative_delta_color: [u8; 4],
    pub opacity: f32,
    pub show_label: bool,
    pub label: String,
    pub label_font_name: String,
    pub label_font_size: f32,
    pub label_color: [u8; 4],
    pub mode: LapTimerMode,
}

pub fn validate_lap_timer(value: ValueConfig, index: usize) -> CoreResult<ValidatedLapTimer> {
    let field = |name: &str| format!("values[{index}].{name}");
    if value.value != MetricKind::LapTimer {
        return Err(CoreError::Config(format!(
            "{}: expected lap_timer metric",
            field("value")
        )));
    }
    if value.display_type != DisplayType::LapTimer {
        return Err(CoreError::Config(format!(
            "{}: lap_timer widgets require display_type 'lap_timer'",
            field("display_type")
        )));
    }

    let mode = match require_string(value.lap_timer_mode, &field("lap_timer_mode"))?.as_str() {
        "current_lap" => LapTimerMode::CurrentLap,
        "best_lap" => LapTimerMode::BestLap,
        "delta" => LapTimerMode::Delta,
        "lap_log" => LapTimerMode::LapLog,
        other => {
            return Err(CoreError::Config(format!(
                "{}: unsupported mode '{other}'",
                field("lap_timer_mode")
            )))
        }
    };
    let font_name = require_string(value.font, &field("font"))?;
    let font_size = require_f32(value.font_size, &field("font_size"))?;
    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {font_size}",
            field("font_size")
        )));
    }
    let label_font_size = require_f32(value.label_font_size, &field("label_font_size"))?;
    if label_font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {label_font_size}",
            field("label_font_size")
        )));
    }
    let opacity = require_f32(value.opacity, &field("opacity"))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..1.0, got {opacity}",
            field("opacity")
        )));
    }
    let color = rgba_from_hex(
        require_string(value.color, &field("color"))?.as_str(),
        &field("color"),
        opacity,
    )?;
    let label_color = rgba_from_hex(
        require_string(value.label_color, &field("label_color"))?.as_str(),
        &field("label_color"),
        opacity,
    )?;
    let positive_delta_color = rgba_from_hex(
        require_string(value.positive_delta_color, &field("positive_delta_color"))?.as_str(),
        &field("positive_delta_color"),
        opacity,
    )?;
    let negative_delta_color = rgba_from_hex(
        require_string(value.negative_delta_color, &field("negative_delta_color"))?.as_str(),
        &field("negative_delta_color"),
        opacity,
    )?;

    Ok(ValidatedLapTimer {
        x: value.x,
        y: value.y,
        font_name,
        font_size,
        color,
        positive_delta_color,
        negative_delta_color,
        opacity,
        show_label: require_bool(value.show_label, &field("show_label"))?,
        label: require_string(value.label, &field("label"))?,
        label_font_name: require_string(value.label_font, &field("label_font"))?,
        label_font_size,
        label_color,
        mode,
    })
}
