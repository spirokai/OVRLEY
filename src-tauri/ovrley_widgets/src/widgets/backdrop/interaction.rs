//! Resize the frame, keeping border styling and the opposite corner fixed.
use super::{config::BackdropConfig, frame::prepare_backdrop_frame, validation::validate_backdrop};
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::{validate_scene_presentation, ScenePresentationConfig},
        types::BackdropType,
    },
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: BTreeMap<String, Value>,
}

pub fn resize_backdrop(
    widget: BackdropConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid backdrop resize size".into()));
    }
    let mut backdrop = validate_backdrop(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    // Preserve the editor's eight-pixel minimum and the required interior of the border.
    let minimum = 8.0_f32.max(backdrop.border_thickness * 2.0 + 1.0);
    let width = (size[0] / scene.scale).max(minimum);
    let height = (size[1] / scene.scale).max(minimum);
    let variant = match backdrop.display_type {
        BackdropType::Circle => {
            backdrop.diameter = width;
            json!({"diameter": width})
        }
        BackdropType::Rectangle => {
            backdrop.width = width;
            backdrop.height = height;
            json!({"width": width, "height": height})
        }
    };
    let bounds = prepare_backdrop_frame(&backdrop, scene.scale).content_bounds;
    Ok(ResizeUpdate {
        x: backdrop.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: backdrop.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: BTreeMap::from([(backdrop.display_type.as_str().to_owned(), variant)]),
    })
}
