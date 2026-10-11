//! Intrinsic resizing policy. The editor supplies gesture geometry; shared layout owns placement.
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::validate_scene_presentation,
    preview::PreviewFonts,
    render::text::validated_value_style,
    widgets::value::{
        format::format_speed_parts, frame::prepare_metric_frame, preview::WidgetRequest,
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
    speed: Option<f64>,
    fonts: &PreviewFonts,
) -> WidgetResult<ResizeUpdate> {
    if !request.factor.is_finite()
        || request.factor <= 0.0
        || request
            .fixed
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || request.point.iter().any(|v| !v.is_finite())
    {
        return Err(WidgetError::Config("invalid widget resize gesture".into()));
    }
    let mut config = validate_value_widget(request.origin.widget, 0)?;
    let scene = validate_scene_presentation(request.origin.scene)?;
    // Match the editor's existing intrinsic size limits without rounding on release.
    config.font_size = (config.font_size * request.factor).clamp(8.0, 400.0);
    config.icon_size = (config.icon_size * request.factor).clamp(0.0, 400.0);
    config.icon_offset_x *= request.factor;
    config.icon_offset_y *= request.factor;
    let style = validated_value_style(&config, &scene, scene.scale);
    let parts = format_speed_parts(&config, speed);
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
