//! Lean-angle sector metric widget validation.

use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::types::{DisplayType, MetricKind};
use ovrley_widgets::model::validation::{
    require_bool, require_f32, require_str, require_string, rgba_from_hex,
};
use ovrley_widgets::widgets::value::config::ValueConfig;

const START_ANGLE: f32 = 210.0;
const SWEEP_ANGLE: f32 = 120.0;
pub(crate) const LEAN_ANGLE_MAX_FILL_SWEEP: f32 = SWEEP_ANGLE * 0.5;
const LABEL_LINE_HEIGHT_RATIO: f32 = 0.92;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeanAngleLayout {
    pub width: f32,
    pub height: f32,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub outer_radius: f32,
    pub inner_radius: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
}

/// Derives the logical lean-angle frame from the canonical diameter geometry.
pub fn lean_angle_layout(diameter: f32, track_thickness: f32, font_size: f32) -> LeanAngleLayout {
    let outer_radius = diameter * 0.5;
    let inner_radius = outer_radius - track_thickness;
    let half_sweep_radians = (SWEEP_ANGLE * 0.5).to_radians();
    let sector_min_x = -outer_radius * half_sweep_radians.sin();
    let sector_max_x = outer_radius * half_sweep_radians.sin();
    let sector_min_y = -outer_radius;
    let sector_max_y = -inner_radius * half_sweep_radians.cos();
    let label_line_height = font_size * LABEL_LINE_HEIGHT_RATIO;
    let min_x = sector_min_x;
    let max_x = sector_max_x;
    let min_y = sector_min_y.min(-label_line_height * 0.5);
    let max_y = sector_max_y.max(label_line_height * 0.5);

    LeanAngleLayout {
        width: max_x - min_x,
        height: max_y - min_y,
        min_x,
        min_y,
        max_x,
        max_y,
        center_x: -min_x,
        center_y: -min_y,
        outer_radius,
        inner_radius,
        start_angle: START_ANGLE,
        sweep_angle: SWEEP_ANGLE,
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedLeanAngleWidget {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub show_icon: bool,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: String,
    pub track_filled_opacity: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_thickness: f32,
    pub font: String,
    pub font_size: f32,
    pub color: String,
    pub unit_color: String,
    pub show_units: bool,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn validate_lean_angle(
    value: ValueConfig,
    index: usize,
) -> CoreResult<ValidatedLeanAngleWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.value != MetricKind::LeanAngle {
        return Err(CoreError::Config(format!(
            "{}: expected lean_angle metric",
            p("value")
        )));
    }
    if value.display_type != DisplayType::LeanAngle {
        return Err(CoreError::Config(format!(
            "{}: expected lean_angle display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    if value.width.is_some() || value.height.is_some() {
        return Err(CoreError::Config(format!(
            "{} and {}: must be absent for diameter-based geometry",
            p("width"),
            p("height")
        )));
    }

    let diameter = require_f32(value.diameter, &p("diameter"))?;
    if diameter <= 0.0 {
        return Err(CoreError::Config(format!("{}: must be > 0", p("diameter"))));
    }

    let opacity = require_f32(value.opacity, &p("opacity"))?;
    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    for (field, field_opacity) in [
        ("opacity", opacity),
        ("track_empty_opacity", track_empty_opacity),
        ("track_filled_opacity", track_filled_opacity),
    ] {
        if !(0.0..=1.0).contains(&field_opacity) {
            return Err(CoreError::Config(format!(
                "{}: must be 0.0..1.0, got {field_opacity}",
                p(field)
            )));
        }
    }

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be >= 0",
            p("track_border_thickness")
        )));
    }
    let track_thickness = require_f32(value.track_thickness, &p("track_thickness"))?;
    if track_thickness <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0",
            p("track_thickness")
        )));
    }
    if track_thickness >= diameter * 0.5 {
        return Err(CoreError::Config(format!(
            "{}: must be less than diameter / 2",
            p("track_thickness")
        )));
    }
    if track_border_thickness * 2.0 >= track_thickness {
        return Err(CoreError::Config(format!(
            "{}: must leave a positive usable width inside {}",
            p("track_border_thickness"),
            p("track_thickness")
        )));
    }
    let font_size = require_f32(value.font_size, &p("font_size"))?;
    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0",
            p("font_size")
        )));
    }

    let track_empty_color = require_string(value.track_empty_color, &p("track_empty_color"))?;
    let track_filled_color = require_string(value.track_filled_color, &p("track_filled_color"))?;
    let track_border_color = require_string(value.track_border_color, &p("track_border_color"))?;
    let color = require_string(value.color, &p("color"))?;
    let unit_color = require_string(value.unit_color, &p("unit_color"))?;
    for (field, hex) in [
        ("track_empty_color", track_empty_color.as_str()),
        ("track_filled_color", track_filled_color.as_str()),
        ("track_border_color", track_border_color.as_str()),
        ("color", color.as_str()),
        ("unit_color", unit_color.as_str()),
    ] {
        rgba_from_hex(require_str(Some(hex), &p(field))?, &p(field), 1.0)?;
    }

    Ok(ValidatedLeanAngleWidget {
        x: value.x,
        y: value.y,
        diameter,
        rotation: value.rotation.unwrap_or(0.0),
        opacity,
        show_icon: require_bool(value.show_icon, &p("show_icon"))?,
        track_empty_color,
        track_empty_opacity,
        track_filled_color,
        track_filled_opacity,
        track_border_thickness,
        track_border_color,
        track_thickness,
        font: require_string(value.font, &p("font"))?,
        font_size,
        color,
        unit_color,
        show_units: require_bool(value.show_units, &p("show_units"))?,
        value_offset_x: require_f32(value.value_offset_x, &p("value_offset_x"))?,
        value_offset_y: require_f32(value.value_offset_y, &p("value_offset_y"))?,
    })
}
