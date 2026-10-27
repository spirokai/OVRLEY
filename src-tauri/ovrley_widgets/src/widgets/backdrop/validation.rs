//! Backdrop widget validation.
//!
//! Backdrops are static geometric widgets. The frontend stores geometry under
//! `display_variants.<display_type>` while shared styling lives at the top
//! level; this validator promotes the active variant and rejects missing
//! render-affecting fields.

use super::config::BackdropConfig;
use crate::error::{WidgetError, WidgetResult};
use crate::model::types::BackdropType;
use crate::model::validation::{
    require_bool, require_border_fits_dimension, require_f32, require_hex_color,
    require_positive_f32, require_string, require_unit_opacity, rgba_from_hex,
};

#[derive(Clone, Debug)]
pub struct ValidatedBackdrop {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
    pub display_type: BackdropType,
    pub fill_color: String,
    pub fill_opacity: f32,
    pub border_thickness: f32,
    pub border_color: String,
    pub border_opacity: f32,
    pub diameter: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radius: f32,
    pub round_top_left: bool,
    pub round_top_right: bool,
    pub round_bottom_left: bool,
    pub round_bottom_right: bool,
}

struct BackdropShared {
    id: String,
    x: f32,
    y: f32,
    opacity: f32,
    fill_color: String,
    fill_opacity: f32,
    border_thickness: f32,
    border_color: String,
    border_opacity: f32,
}

pub fn validate_backdrop(
    backdrop: &BackdropConfig,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let promoted = backdrop.with_promoted_display_variant(backdrop.display_type.as_str())?;
    let p = |field: &str| format!("backdrops[{index}].{field}");

    let id = require_string(promoted.id.clone(), &p("id"))?;
    if id.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("id"))));
    }

    let fill_color = require_hex_color(promoted.fill_color.as_deref(), &p("fill_color"))?;
    rgba_from_hex(&fill_color, &p("fill_color"), 1.0)?;
    let border_color = require_hex_color(promoted.border_color.as_deref(), &p("border_color"))?;
    rgba_from_hex(&border_color, &p("border_color"), 1.0)?;

    let shared = BackdropShared {
        id,
        x: require_f32(promoted.x, &p("x"))?,
        y: require_f32(promoted.y, &p("y"))?,
        opacity: require_unit_opacity(promoted.opacity, &p("opacity"))?,
        fill_color,
        fill_opacity: require_unit_opacity(promoted.fill_opacity, &p("fill_opacity"))?,
        border_thickness: require_f32(promoted.border_thickness, &p("border_thickness"))?,
        border_color,
        border_opacity: require_unit_opacity(promoted.border_opacity, &p("border_opacity"))?,
    };

    if shared.border_thickness < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {}",
            p("border_thickness"),
            shared.border_thickness
        )));
    }

    match promoted.display_type {
        BackdropType::Circle => validate_circle_backdrop(promoted, shared, index),
        BackdropType::Rectangle => validate_rectangle_backdrop(promoted, shared, index),
    }
}

fn validate_circle_backdrop(
    backdrop: BackdropConfig,
    shared: BackdropShared,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let p = |field: &str| format!("backdrops[{index}].{field}");
    let diameter = require_positive_f32(backdrop.diameter, &p("diameter"))?;
    require_border_fits_dimension(shared.border_thickness, diameter, &p("border_thickness"))?;

    Ok(ValidatedBackdrop {
        id: shared.id,
        x: shared.x,
        y: shared.y,
        opacity: shared.opacity,
        display_type: BackdropType::Circle,
        fill_color: shared.fill_color,
        fill_opacity: shared.fill_opacity,
        border_thickness: shared.border_thickness,
        border_color: shared.border_color,
        border_opacity: shared.border_opacity,
        diameter,
        width: 0.0,
        height: 0.0,
        corner_radius: 0.0,
        round_top_left: false,
        round_top_right: false,
        round_bottom_left: false,
        round_bottom_right: false,
    })
}

fn validate_rectangle_backdrop(
    backdrop: BackdropConfig,
    shared: BackdropShared,
    index: usize,
) -> WidgetResult<ValidatedBackdrop> {
    let p = |field: &str| format!("backdrops[{index}].{field}");
    let width = require_positive_f32(backdrop.width, &p("width"))?;
    let height = require_positive_f32(backdrop.height, &p("height"))?;
    require_border_fits_dimension(
        shared.border_thickness,
        width.min(height),
        &p("border_thickness"),
    )?;

    let round_top_left = require_bool(backdrop.round_top_left, &p("round_top_left"))?;
    let round_top_right = require_bool(backdrop.round_top_right, &p("round_top_right"))?;
    let round_bottom_left = require_bool(backdrop.round_bottom_left, &p("round_bottom_left"))?;
    let round_bottom_right = require_bool(backdrop.round_bottom_right, &p("round_bottom_right"))?;
    let mut corner_radius = require_f32(backdrop.corner_radius, &p("corner_radius"))?;
    if corner_radius < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0, got {corner_radius}",
            p("corner_radius")
        )));
    }
    corner_radius = corner_radius.min(width.min(height) * 0.5);
    if [
        round_top_left,
        round_top_right,
        round_bottom_left,
        round_bottom_right,
    ]
    .iter()
    .any(|rounded| *rounded)
        && shared.border_thickness > corner_radius
    {
        corner_radius = shared.border_thickness;
    }

    Ok(ValidatedBackdrop {
        id: shared.id,
        x: shared.x,
        y: shared.y,
        opacity: shared.opacity,
        display_type: BackdropType::Rectangle,
        fill_color: shared.fill_color,
        fill_opacity: shared.fill_opacity,
        border_thickness: shared.border_thickness,
        border_color: shared.border_color,
        border_opacity: shared.border_opacity,
        diameter: 0.0,
        width,
        height,
        corner_radius,
        round_top_left,
        round_top_right,
        round_bottom_left,
        round_bottom_right,
    })
}
