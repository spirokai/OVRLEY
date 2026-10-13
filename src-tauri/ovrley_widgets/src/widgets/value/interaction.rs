//! Intrinsic resizing policy. The editor supplies gesture geometry; shared layout owns placement.
use crate::{
    error::WidgetResult,
    model::scene::validate_scene_presentation,
    preview::PreviewFonts,
    render::text::validated_value_style,
    widgets::value::{
        format::format_metric_parts, frame::prepare_metric_frame, preview::WidgetRequest,
        validation::validate_value_widget,
    },
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ResizeRequest {
    #[serde(flatten)]
    pub origin: WidgetRequest,
    pub factor: f32,
    /// Normalized point on the content rectangle that remains fixed (opposite handle).
    pub fixed: [f32; 2],
    /// Scene coordinates of that fixed point.
    pub point: [f32; 2],
    pub second: f64,
}

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub icon_size: f32,
    pub icon_offset_x: f32,
    pub icon_offset_y: f32,
}

pub fn resize_value(
    request: ResizeRequest,
    activity: Option<&crate::activity::ActivitySampler>,
    fonts: &PreviewFonts,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_value_widget(request.origin.widget, 0)?;
    let scene = validate_scene_presentation(request.origin.scene)?;
    // Match the editor's existing intrinsic size limits without rounding on release.
    config.font_size = (config.font_size * request.factor).clamp(8.0, 400.0);
    config.icon_size = (config.icon_size * request.factor).clamp(0.0, 400.0);
    config.icon_offset_x *= request.factor;
    config.icon_offset_y *= request.factor;
    let style = validated_value_style(&config, &scene, scene.scale);
    let sample = activity
        .map(|a| a.sample(&config, request.second))
        .unwrap_or_default();
    let parts = format_metric_parts(&config, &sample);
    let bounds = prepare_metric_frame(&style, &parts, scene.scale, fonts, &config)?.content_bounds;
    Ok(ResizeUpdate {
        x: config.x + request.point[0] - (bounds.left + bounds.width() * request.fixed[0]),
        y: config.y + request.point[1] - (bounds.top + bounds.height() * request.fixed[1]),
        font_size: config.font_size,
        icon_size: config.icon_size,
        icon_offset_x: config.icon_offset_x,
        icon_offset_y: config.icon_offset_y,
    })
}
