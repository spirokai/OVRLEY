//! Lean-angle sector metric widget validation.

use crate::error::{WidgetError, WidgetResult};
use crate::model::types::{DisplayType, MetricKind};
use crate::model::validation::{
    require_bool, require_f32, require_str, require_string, rgba_from_hex,
};
use crate::widgets::value::config::ValueConfig;

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
) -> WidgetResult<ValidatedLeanAngleWidget> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.value != MetricKind::LeanAngle {
        return Err(WidgetError::Config(format!(
            "{}: expected lean_angle metric",
            p("value")
        )));
    }
    if value.display_type != DisplayType::LeanAngle {
        return Err(WidgetError::Config(format!(
            "{}: expected lean_angle display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    if value.width.is_some() || value.height.is_some() {
        return Err(WidgetError::Config(format!(
            "{} and {}: must be absent for diameter-based geometry",
            p("width"),
            p("height")
        )));
    }

    let diameter = require_f32(value.diameter, &p("diameter"))?;
    if diameter <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be > 0",
            p("diameter")
        )));
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
            return Err(WidgetError::Config(format!(
                "{}: must be 0.0..1.0, got {field_opacity}",
                p(field)
            )));
        }
    }

    let track_border_thickness =
        require_f32(value.track_border_thickness, &p("track_border_thickness"))?;
    if track_border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0",
            p("track_border_thickness")
        )));
    }
    let track_thickness = require_f32(value.track_thickness, &p("track_thickness"))?;
    if track_thickness <= 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be > 0",
            p("track_thickness")
        )));
    }
    if track_thickness >= diameter * 0.5 {
        return Err(WidgetError::Config(format!(
            "{}: must be less than diameter / 2",
            p("track_thickness")
        )));
    }
    if track_border_thickness * 2.0 >= track_thickness {
        return Err(WidgetError::Config(format!(
            "{}: must leave a positive usable width inside {}",
            p("track_border_thickness"),
            p("track_thickness")
        )));
    }
    let font_size = require_f32(value.font_size, &p("font_size"))?;
    if font_size <= 0.0 {
        return Err(WidgetError::Config(format!(
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
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        diameter,
        // Rotation is optional legacy metadata; this presentation has no rotation gesture.
        rotation: value
            .rotation
            .map(|rotation| require_f32(Some(rotation), &p("rotation")))
            .transpose()?
            .unwrap_or(0.0),
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
