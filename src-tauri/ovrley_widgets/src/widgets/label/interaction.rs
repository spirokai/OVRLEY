use super::{config::LabelConfig, frame::prepare_label_frame, validation::validate_label};
use crate::{error::WidgetResult, model::scene::{validate_scene_presentation, ScenePresentationConfig}, render::text::FontResolver};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
}

pub fn resize_label(widget: LabelConfig, scene: ScenePresentationConfig, factor: f32, fixed: [f32; 2], point: [f32; 2], fonts: &dyn FontResolver) -> WidgetResult<ResizeUpdate> {
    let mut label = validate_label(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    label.font_size = (label.font_size * factor).clamp(8.0, 400.0);
    let bounds = prepare_label_frame(&label, &scene, fonts)?.content_bounds;
    Ok(ResizeUpdate {
        x: label.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: label.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: label.font_size,
    })
}
