//! Framed resize with the opposite corner fixed. A side handle crops the map. A corner handle scales
//! the whole view: the frame keeps its aspect ratio, and its corner radius, the route, the marker and
//! the zoom scale with it.
use super::{
    config::MapConfig,
    validation::{
        validate_map_widget, MapCameraMode, MapShape, FIXED_ZOOM_RANGE, FOLLOW_ZOOM_RANGE,
    },
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
};
use std::ops::RangeInclusive;

/// The editor's smallest widget edge.
const MIN_EDGE: f32 = 8.0;

/// The zoom field of the widget's camera mode.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum ZoomUpdate {
    Fixed { fixed_zoom: f32 },
    Follow { follow_zoom: f32 },
}

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// A circle has no corner radius of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f32>,
    pub line_width: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
    #[serde(flatten)]
    pub zoom: ZoomUpdate,
}

/// The factor of the edge that moved further, so a corner follows the pointer's stronger axis.
fn dominant_factor(requested: [f32; 2], current: [f32; 2]) -> f32 {
    let [x, y] = [requested[0] / current[0], requested[1] / current[1]];
    if (x - 1.0).abs() >= (y - 1.0).abs() {
        x
    } else {
        y
    }
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
    let config = validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let requested = [size[0] / scene.scale, size[1] / scene.scale];
    // A circle has no sides, so it always scales.
    let corner =
        matches!(config.shape, MapShape::Circle) || fixed.iter().all(|share| *share != 0.5);
    let (width, height, factor) = if corner {
        let factor = dominant_factor(requested, [config.width, config.height])
            .max(MIN_EDGE / config.width.min(config.height));
        (config.width * factor, config.height * factor, factor)
    } else {
        (requested[0].max(MIN_EDGE), requested[1].max(MIN_EDGE), 1.0)
    };
    // One zoom level doubles the map, so this scales it by `factor` until the camera's limit.
    let zoom = |zoom: f32, range: RangeInclusive<f32>| {
        (zoom + factor.log2()).clamp(*range.start(), *range.end())
    };
    Ok(ResizeUpdate {
        x: point[0] - width * scene.scale * fixed[0],
        y: point[1] - height * scene.scale * fixed[1],
        width,
        height,
        corner_radius: match config.shape {
            MapShape::Rectangle { corner_radius } => Some(corner_radius * factor),
            MapShape::Circle => None,
        },
        line_width: config.line_width * factor,
        marker_size: config.path.marker_size * factor,
        marker_variant_diameter: config.path.marker_variant_diameter * factor,
        zoom: match config.camera {
            MapCameraMode::Fixed { zoom_offset, .. } => ZoomUpdate::Fixed {
                fixed_zoom: zoom(zoom_offset, FIXED_ZOOM_RANGE),
            },
            MapCameraMode::Follow { zoom: follow, .. } => ZoomUpdate::Follow {
                follow_zoom: zoom(follow, FOLLOW_ZOOM_RANGE),
            },
        },
    })
}
