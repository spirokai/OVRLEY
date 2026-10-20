//! Diameter-based uniform resizing, preserving the opposite visible corner.
use super::layout::prepare_heading_compass_cache;
use super::validation::validate_heading_compass;
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
    pub font_size: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub heading_compass: HeadingCompassResizeVariant,
}

/// Every pixel-sized field scales; the relative needle corner radius needs no change.
#[derive(Serialize)]
pub struct HeadingCompassResizeVariant {
    pub diameter: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub indicator_size: f32,
    pub cardinal_font_size: f32,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_heading_compass(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &PreviewFonts,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_heading_compass(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.diameter = (config.diameter * factor).clamp(8.0, 10_000.0);
    config.font_size = (config.font_size * factor).clamp(6.0, 400.0);
    config.cardinal_font_size = (config.cardinal_font_size * factor).clamp(4.0, 400.0);
    config.track_thickness = (config.track_thickness * factor).clamp(1.0, 10_000.0);
    config.track_corner_radius = (config.track_corner_radius * factor).clamp(0.0, 10_000.0);
    config.indicator_size = (config.indicator_size * factor).clamp(1.0, 10_000.0);
    config.value_offset_x = (config.value_offset_x * factor).clamp(-10_000.0, 10_000.0);
    config.value_offset_y = (config.value_offset_y * factor).clamp(-10_000.0, 10_000.0);
    let raw = activity.and_then(|activity| activity.heading_at(second));
    let bounds = prepare_heading_compass_cache(&config, &scene, fonts)?
        .prepare_frame(raw, fonts)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        display_variants: ResizeVariants {
            heading_compass: HeadingCompassResizeVariant {
                diameter: config.diameter,
                track_thickness: config.track_thickness,
                track_corner_radius: config.track_corner_radius,
                indicator_size: config.indicator_size,
                cardinal_font_size: config.cardinal_font_size,
                value_offset_x: config.value_offset_x,
                value_offset_y: config.value_offset_y,
            },
        },
    })
}
