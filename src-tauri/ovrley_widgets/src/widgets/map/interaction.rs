//! Framed resize with the opposite corner fixed. A side handle crops the map. A corner handle scales
//! the whole view: the frame keeps its aspect ratio, and its corner radius, the route, the marker and
//! the zoom scale with it.
//!
//! The view mode's drag and wheel pan and zoom the map inside a fixed frame.
use super::{
    camera::fixed_route_span,
    config::MapConfig,
    validation::{
        validate_map_widget, MapCameraMode, MapShape, FIXED_PAN_RANGE, FIXED_ZOOM_RANGE,
        FOLLOW_ZOOM_RANGE,
    },
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
};
use std::ops::RangeInclusive;

/// The editor's smallest widget edge.
const MIN_EDGE: f32 = 8.0;
/// Wheel pixels per zoom level, so a 100-pixel mouse notch is a fifth of a level.
const WHEEL_PIXELS_PER_LEVEL: f32 = 500.0;

fn within(value: f32, range: RangeInclusive<f32>) -> f32 {
    value.clamp(*range.start(), *range.end())
}

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
    let zoom = |zoom: f32, range: RangeInclusive<f32>| within(zoom + factor.log2(), range);
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

/// A view-mode gesture, in scene pixels.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapViewGesture {
    /// How far a top-down map has been dragged.
    Pan([f32; 2]),
    /// A wheel's `deltaY` at a scene point. A top-down map keeps that point in place.
    Zoom { wheel_delta: f32, point: [f32; 2] },
}

/// The view fields of the widget's camera mode.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum MapViewUpdate {
    Fixed {
        fixed_zoom: f32,
        fixed_pan_x: f32,
        fixed_pan_y: f32,
    },
    Follow {
        follow_zoom: f32,
    },
}

pub fn adjust_map_view(
    widget: MapConfig,
    scene: ScenePresentationConfig,
    gesture: MapViewGesture,
) -> WidgetResult<MapViewUpdate> {
    let finite = match gesture {
        MapViewGesture::Pan(delta) => delta.iter().all(|value| value.is_finite()),
        MapViewGesture::Zoom { wheel_delta, point } => {
            wheel_delta.is_finite() && point.iter().all(|value| value.is_finite())
        }
    };
    if !finite {
        return Err(WidgetError::Config("invalid map view gesture".into()));
    }
    let config = validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let levels = |wheel_delta: f32| -wheel_delta / WHEEL_PIXELS_PER_LEVEL;
    let (zoom_offset, pan) = match (config.camera, gesture) {
        (MapCameraMode::Follow { zoom, .. }, MapViewGesture::Zoom { wheel_delta, .. }) => {
            return Ok(MapViewUpdate::Follow {
                follow_zoom: within(zoom + levels(wheel_delta), FOLLOW_ZOOM_RANGE),
            });
        }
        (MapCameraMode::Follow { .. }, MapViewGesture::Pan(_)) => {
            return Err(WidgetError::Config(
                "a chase camera follows the rider and has no pan".into(),
            ));
        }
        (MapCameraMode::Fixed { zoom_offset, pan }, _) => (zoom_offset, pan),
    };
    let span = |zoom: f32| fixed_route_span(f64::from(zoom)) as f32;
    let (zoom, shift) = match gesture {
        MapViewGesture::Pan(delta) => (
            zoom_offset,
            delta.map(|value| value / scene.scale / span(zoom_offset)),
        ),
        MapViewGesture::Zoom { wheel_delta, point } => {
            let zoom = within(zoom_offset + levels(wheel_delta), FIXED_ZOOM_RANGE);
            // The cursor's offset from the frame's centre, in widget units, shows the same place.
            let anchor = [
                (point[0] - config.x) / scene.scale - config.width / 2.0,
                (point[1] - config.y) / scene.scale - config.height / 2.0,
            ];
            let per_unit = 1.0 / span(zoom) - 1.0 / span(zoom_offset);
            (zoom, anchor.map(|value| value * per_unit))
        }
    };
    Ok(MapViewUpdate::Fixed {
        fixed_zoom: zoom,
        fixed_pan_x: within(pan[0] + shift[0], FIXED_PAN_RANGE),
        fixed_pan_y: within(pan[1] + shift[1], FIXED_PAN_RANGE),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(camera_mode: &str) -> MapConfig {
        serde_json::from_value(json!({
            "value": "map", "x": 100, "y": 50, "opacity": 1, "font": "Inter.ttf", "width": 200, "height": 100,
            "map_style": "liberty", "show_buildings": false, "show_paths": true, "show_labels": true, "camera_mode": camera_mode,
            "orientation": "north_up", "pitch": 0, "follow_zoom": 16, "fixed_zoom": 0, "fixed_pan_x": 0,
            "fixed_pan_y": 0, "msaa_samples": 4, "shape": "rectangle", "corner_radius": 0,
            "border_color": "#ffffff", "border_thickness": 0, "border_opacity": 1, "route_smoothing_m": 8,
            "show_full_activity": true, "line_width": 6, "completed_line_opacity": 1, "remaining_line_opacity": 1,
            "completed_line_color": "#ff4d1f", "remaining_line_color": "#1f7bff", "marker_variant": "single",
            "marker_variant_diameter": 44, "marker_size": 10, "marker_opacity": 1, "marker_color": "#ffffff"
        }))
        .unwrap()
    }

    fn scene() -> ScenePresentationConfig {
        serde_json::from_value(json!({
            "width": 1920, "height": 1080, "scale": 2, "opacity": 1, "shadow_strength": 0, "shadow_distance": 0,
            "shadow_color": "#000000", "border_thickness": 0, "border_color": "#000000"
        }))
        .unwrap()
    }

    fn fixed(update: MapViewUpdate) -> [f32; 3] {
        match update {
            MapViewUpdate::Fixed {
                fixed_zoom,
                fixed_pan_x,
                fixed_pan_y,
            } => [fixed_zoom, fixed_pan_x, fixed_pan_y],
            MapViewUpdate::Follow { .. } => panic!("expected a top-down update"),
        }
    }

    #[test]
    fn the_view_follows_the_drag_and_zooms_around_the_cursor() {
        // At zoom offset 0 the route's longest side spans 420 widget units, 840 scene pixels at scale 2.
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(map("fixed"), scene(), MapViewGesture::Pan([84.0, -42.0])).unwrap(),
        );
        assert_eq!(zoom, 0.0);
        assert!((pan_x - 0.1).abs() < 1e-6 && (pan_y + 0.05).abs() < 1e-6);

        // The frame's centre stays put; a point right of it moves the route away from the cursor.
        let centre = [100.0 + 200.0, 50.0 + 100.0];
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: -500.0,
                    point: centre,
                },
            )
            .unwrap(),
        );
        assert_eq!([zoom, pan_x, pan_y], [1.0, 0.0, 0.0]);
        let right = [centre[0] + 84.0, centre[1]];
        let [_, pan_x, _] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: -500.0,
                    point: right,
                },
            )
            .unwrap(),
        );
        assert!((pan_x + 0.05).abs() < 1e-6);

        assert!(adjust_map_view(map("follow"), scene(), MapViewGesture::Pan([1.0, 0.0])).is_err());
    }
}
