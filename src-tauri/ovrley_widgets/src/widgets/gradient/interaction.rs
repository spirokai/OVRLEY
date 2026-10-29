use super::{
    frame::{prepare_gradient_frame, validated_gradient_style},
    validation::validate_gradient_widget,
};
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
    pub triangle_width: f32,
    pub value_offset: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_gradient(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_gradient_widget(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    config.triangle_width = (config.triangle_width * factor).clamp(0.0, 600.0);
    config.value_offset *= factor;
    let style = validated_gradient_style(&config, &scene, scene.scale);
    let raw = activity.and_then(|activity| activity.gradient_at(second));
    let bounds = prepare_gradient_frame(&config, &style, raw, scene.scale, scene.opacity, fonts)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        triangle_width: config.triangle_width,
        value_offset: config.value_offset,
    })
}
