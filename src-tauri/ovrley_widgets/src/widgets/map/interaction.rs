//! Framed resize: the map keeps its route styling and the opposite corner stays fixed.
use super::{config::MapConfig, validation::validate_map_widget};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
};

/// The editor's smallest widget edge.
const MIN_EDGE: f32 = 8.0;

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn resize_map(
    widget: MapConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid map resize size".into()));
    }
    validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let width = (size[0] / scene.scale).max(MIN_EDGE);
    let height = (size[1] / scene.scale).max(MIN_EDGE);
    Ok(ResizeUpdate {
        x: point[0] - width * scene.scale * fixed[0],
        y: point[1] - height * scene.scale * fixed[1],
        width,
        height,
    })
}
