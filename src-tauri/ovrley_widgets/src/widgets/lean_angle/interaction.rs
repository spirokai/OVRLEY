//! Diameter-based intrinsic resizing, preserving the opposite visible corner.
use super::{frame::prepare_lean_angle_frame, validation::validate_lean_angle};
use crate::{
    activity::ActivitySampler,
    error::WidgetResult,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    render::text::FontResolver,
    widgets::value::config::ValueConfig,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub lean_angle: LeanAngleResizeVariant,
}

#[derive(Serialize)]
pub struct LeanAngleResizeVariant {
    pub diameter: f32,
    pub track_thickness: f32,
    pub track_border_thickness: f32,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_lean_angle(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_lean_angle(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.diameter = (config.diameter * factor).clamp(8.0, 10_000.0);
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    // Keep both strict geometric inequalities valid even at product size limits.
    config.track_thickness =
        (config.track_thickness * factor).clamp(1.0, config.diameter * 0.5 * (1.0 - f32::EPSILON));
    config.track_border_thickness = (config.track_border_thickness * factor)
        .clamp(0.0, config.track_thickness * 0.5 * (1.0 - f32::EPSILON));
    config.value_offset_x = (config.value_offset_x * factor).clamp(-10_000.0, 10_000.0);
    config.value_offset_y = (config.value_offset_y * factor).clamp(-10_000.0, 10_000.0);
    let raw = activity.and_then(|activity| activity.lean_angle_at(second));
    let bounds = prepare_lean_angle_frame(&config, &scene, raw, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        display_variants: ResizeVariants {
            lean_angle: LeanAngleResizeVariant {
                diameter: config.diameter,
                track_thickness: config.track_thickness,
                track_border_thickness: config.track_border_thickness,
                value_offset_x: config.value_offset_x,
                value_offset_y: config.value_offset_y,
            },
        },
    })
}
