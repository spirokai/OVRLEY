//! G-force friction-circle widget validation.

use crate::error::{WidgetError, WidgetResult};
use crate::model::types::{DisplayType, MetricKind};
use crate::model::validation::{
    require_bool, require_f32, require_hex_color, require_positive_f32, require_string,
    require_unit_opacity,
};
use crate::widgets::value::config::ValueConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GForceAxis {
    X,
    Y,
    Z,
}

impl GForceAxis {
    fn parse(value: &str, field: &str) -> WidgetResult<Self> {
        match value {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            "z" => Ok(Self::Z),
            _ => Err(WidgetError::Config(format!(
                "{field}: invalid value '{value}', expected 'x', 'y', or 'z'"
            ))),
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedGForceWidget {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub diameter: f32,
    pub fill_color: String,
    pub fill_opacity: f32,
    pub border_thickness: f32,
    pub border_color: String,
    pub border_opacity: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub axis_horizontal: GForceAxis,
    pub axis_vertical: GForceAxis,
    pub invert_horizontal: bool,
    pub invert_vertical: bool,
    pub clip_percentile: f32,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_color: String,
    pub label_decimals: usize,
    pub label_unit: String,
    pub label_unit_color: String,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
}

pub fn validate_g_force(value: ValueConfig, index: usize) -> WidgetResult<ValidatedGForceWidget> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::GForce {
        return Err(WidgetError::Config(format!(
            "{}: expected g_force metric",
            p("value")
        )));
    }
    if value.display_type != DisplayType::GForce {
        return Err(WidgetError::Config(format!(
            "{}: expected g_force display_type",
            p("display_type")
        )));
    }

    let width = require_positive_f32(value.width, &p("width"))?;
    let height = require_positive_f32(value.height, &p("height"))?;
    let diameter = require_positive_f32(value.diameter, &p("diameter"))?;
    if diameter > width.min(height) {
        return Err(WidgetError::Config(format!(
            "{}: must fit inside the widget frame",
            p("diameter")
        )));
    }

    let border_thickness = require_f32(value.border_thickness, &p("border_thickness"))?;
    if border_thickness < 0.0 || border_thickness * 2.0 >= diameter {
        return Err(WidgetError::Config(format!(
            "{}: must be non-negative and leave a positive inner circle",
            p("border_thickness")
        )));
    }
    let marker_size = require_positive_f32(value.marker_size, &p("marker_size"))?;
    let label_font_size = require_positive_f32(value.label_font_size, &p("label_font_size"))?;
    let clip_percentile = require_f32(value.clip_percentile, &p("clip_percentile"))?;
    if !(0.0..=100.0).contains(&clip_percentile) || clip_percentile == 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be greater than 0 and at most 100",
            p("clip_percentile")
        )));
    }

    let axis_horizontal = GForceAxis::parse(
        &require_string(value.axis_horizontal, &p("axis_horizontal"))?,
        &p("axis_horizontal"),
    )?;
    let axis_vertical = GForceAxis::parse(
        &require_string(value.axis_vertical, &p("axis_vertical"))?,
        &p("axis_vertical"),
    )?;
    if axis_horizontal == axis_vertical {
        return Err(WidgetError::Config(format!(
            "{} and {}: axes must be different",
            p("axis_horizontal"),
            p("axis_vertical")
        )));
    }

    let fill_color = require_hex_color(value.fill_color.as_deref(), &p("fill_color"))?;
    let border_color = require_hex_color(value.border_color.as_deref(), &p("border_color"))?;
    let marker_color = require_hex_color(value.marker_color.as_deref(), &p("marker_color"))?;
    let label_color = require_hex_color(value.label_color.as_deref(), &p("label_color"))?;
    let label_unit_color =
        require_hex_color(value.label_unit_color.as_deref(), &p("label_unit_color"))?;
    let label_unit = require_string(value.label_unit, &p("label_unit"))?;
    if label_unit != "G" {
        return Err(WidgetError::Config(format!(
            "{}: must be 'G'",
            p("label_unit")
        )));
    }

    Ok(ValidatedGForceWidget {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width,
        height,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        diameter,
        fill_color,
        fill_opacity: require_unit_opacity(value.fill_opacity, &p("fill_opacity"))?,
        border_thickness,
        border_color,
        border_opacity: require_unit_opacity(value.border_opacity, &p("border_opacity"))?,
        marker_size,
        marker_color,
        marker_opacity: require_unit_opacity(value.marker_opacity, &p("marker_opacity"))?,
        axis_horizontal,
        axis_vertical,
        invert_horizontal: require_bool(value.invert_horizontal, &p("invert_horizontal"))?,
        invert_vertical: require_bool(value.invert_vertical, &p("invert_vertical"))?,
        clip_percentile,
        label_font: require_string(value.label_font, &p("label_font"))?,
        label_font_size,
        label_color,
        label_decimals: value
            .label_decimals
            .ok_or_else(|| WidgetError::Config(format!("{}: required", p("label_decimals"))))?,
        label_unit,
        label_unit_color,
        label_offset_x: require_f32(value.label_offset_x, &p("label_offset_x"))?,
        label_offset_y: require_f32(value.label_offset_y, &p("label_offset_y"))?,
    })
}
