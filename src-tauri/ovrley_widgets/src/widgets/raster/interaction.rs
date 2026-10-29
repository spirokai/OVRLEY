use super::{config::RasterConfig, validation::validate_raster_geometry};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
};

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub fn resize_raster(
    widget: RasterConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(WidgetError::Config("invalid raster resize size".into()));
    }
    let config = validate_raster_geometry(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let mut width = size[0] / scene.scale;
    let mut height = size[1] / scene.scale;
    if fixed[0] != 0.5 && fixed[1] != 0.5 {
        let x_factor = width / config.width;
        let y_factor = height / config.height;
        let factor = if (x_factor - 1.0).abs() >= (y_factor - 1.0).abs() {
            x_factor
        } else {
            y_factor
        };
        width = config.width * factor;
        height = config.height * factor;
    }
    let width = width.max(8.0);
    let height = height.max(8.0);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let dx = width * scene.scale * fixed[0];
    let dy = height * scene.scale * fixed[1];
    Ok(ResizeUpdate {
        x: point[0] - dx * cos + dy * sin,
        y: point[1] - dx * sin - dy * cos,
        width,
        height,
    })
}
