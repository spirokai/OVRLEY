//! Uniform intrinsic resizing with the opposite visible corner fixed at the current playhead.
use super::{format::GForceSample, frame::prepare_g_force_frame, validation::validate_g_force};
use crate::{
    activity::ActivitySampler,
    error::WidgetResult,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::PreviewFonts,
    widgets::value::config::ValueConfig,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub g_force: GForceResizeVariant,
}

#[derive(Serialize)]
pub struct GForceResizeVariant {
    pub width: f32,
    pub height: f32,
    pub diameter: f32,
    pub border_thickness: f32,
    pub marker_size: f32,
    pub label_font_size: f32,
    pub label_offset_x: f32,
    pub label_offset_y: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_g_force(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &PreviewFonts,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_g_force(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let width = (config.width * factor).clamp(8.0, 10_000.0);
    let frame_factor = width / config.width;
    config.width = width;
    config.height = width;
    config.diameter = (config.diameter * frame_factor).clamp(8.0, width);
    config.border_thickness =
        (config.border_thickness * frame_factor).clamp(0.0, (config.diameter - 1.0) * 0.5);
    config.marker_size = (config.marker_size * frame_factor).clamp(1.0, config.diameter);
    config.label_font_size = (config.label_font_size * frame_factor).clamp(8.0, 400.0);
    config.label_offset_x = (config.label_offset_x * frame_factor).clamp(-10_000.0, 10_000.0);
    config.label_offset_y = (config.label_offset_y * frame_factor).clamp(-10_000.0, 10_000.0);
    let sample = activity.map_or_else(GForceSample::default, |activity| {
        activity.g_force_at(&config, second)
    });
    let bounds = prepare_g_force_frame(&config, &scene, sample, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: ResizeVariants {
            g_force: GForceResizeVariant {
                width: config.width,
                height: config.height,
                diameter: config.diameter,
                border_thickness: config.border_thickness,
                marker_size: config.marker_size,
                label_font_size: config.label_font_size,
                label_offset_x: config.label_offset_x,
                label_offset_y: config.label_offset_y,
            },
        },
    })
}
