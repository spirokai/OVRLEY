//! Framed resize with the opposite corner fixed. A side handle crops the map. A corner handle scales
//! the whole view: the frame keeps its aspect ratio, and its corner radius, the route, the marker and
//! the zoom scale with it.
//!
//! The view mode's drag and wheel pan and zoom the map inside a fixed frame.
use super::{
    config::MapConfig,
    content::RouteExtent,
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
            MapCameraMode::Fixed { zoom: fixed, .. } => ZoomUpdate::Fixed {
                fixed_zoom: zoom(fixed, FIXED_ZOOM_RANGE),
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
    /// The top-down view that centres the whole route in the frame, in either camera mode, so a map
    /// can switch to top down already fitted.
    Fit,
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

/// `route` is what a top-down camera pans across: without GPS there is none, and only zoom moves.
pub fn adjust_map_view(
    widget: MapConfig,
    scene: ScenePresentationConfig,
    gesture: MapViewGesture,
    route: Option<RouteExtent>,
) -> WidgetResult<MapViewUpdate> {
    let finite = match gesture {
        MapViewGesture::Pan(delta) => delta.iter().all(|value| value.is_finite()),
        MapViewGesture::Zoom { wheel_delta, point } => {
            wheel_delta.is_finite() && point.iter().all(|value| value.is_finite())
        }
        MapViewGesture::Fit => true,
    };
    if !finite {
        return Err(WidgetError::Config("invalid map view gesture".into()));
    }
    let config = validate_map_widget(&widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let levels = |wheel_delta: f32| -wheel_delta / WHEEL_PIXELS_PER_LEVEL;
    let (zoom, pan) = match (config.camera, gesture) {
        (_, MapViewGesture::Fit) => {
            // Without GPS there is nothing to fit, and the zoom stays.
            let zoom = route.map_or(widget.fixed_zoom, |route| {
                route.fit_zoom([f64::from(config.width), f64::from(config.height)]) as f32
            });
            return Ok(MapViewUpdate::Fixed {
                fixed_zoom: within(zoom, FIXED_ZOOM_RANGE),
                fixed_pan_x: 0.0,
                fixed_pan_y: 0.0,
            });
        }
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
        (MapCameraMode::Fixed { zoom, pan }, _) => (zoom, pan),
    };
    // Shares of the route per widget unit at `zoom`. A route that is one point has no pan to move.
    let per_unit = |zoom: f32| {
        route
            .map(|route| route.span(f64::from(zoom)))
            .filter(|span| *span > 0.0)
            .map_or(0.0, |span| (1.0 / span) as f32)
    };
    let (next, shift) = match gesture {
        MapViewGesture::Pan(delta) => (
            zoom,
            delta.map(|value| value / scene.scale * per_unit(zoom)),
        ),
        MapViewGesture::Fit => unreachable!("fit returned above"),
        MapViewGesture::Zoom { wheel_delta, point } => {
            let next = within(zoom + levels(wheel_delta), FIXED_ZOOM_RANGE);
            // The cursor's offset from the frame's centre, in widget units, shows the same place.
            let anchor = [
                (point[0] - config.x) / scene.scale - config.width / 2.0,
                (point[1] - config.y) / scene.scale - config.height / 2.0,
            ];
            let change = per_unit(next) - per_unit(zoom);
            (next, anchor.map(|value| value * change))
        }
    };
    let pan = [pan[0] + shift[0], pan[1] + shift[1]];
    let [fixed_pan_x, fixed_pan_y] = match route {
        Some(route) => route
            .clamp_pan(pan.map(f64::from))
            .map(|value| value as f32),
        None => pan,
    };
    Ok(MapViewUpdate::Fixed {
        fixed_zoom: next,
        fixed_pan_x,
        fixed_pan_y,
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
            "orientation": "north_up", "pitch": 0, "follow_zoom": 16, "fixed_zoom": 15, "fixed_pan_x": 0,
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

    const LONGEST: f64 = 420.0 / (ovrley_map::TILE_SIZE as f64 * 32768.0);

    /// A route whose longest side spans 420 widget units at zoom 15, 840 scene pixels at scale 2.
    fn route() -> Option<RouteExtent> {
        Some(RouteExtent {
            centre: (0.5, 0.5),
            size: (LONGEST, LONGEST / 2.0),
            longest: LONGEST,
        })
    }

    #[test]
    fn the_view_follows_the_drag_and_zooms_around_the_cursor() {
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Pan([84.0, -42.0]),
                route(),
            )
            .unwrap(),
        );
        assert_eq!(zoom, 15.0);
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
                route(),
            )
            .unwrap(),
        );
        assert_eq!([zoom, pan_x, pan_y], [16.0, 0.0, 0.0]);
        let right = [centre[0] + 84.0, centre[1]];
        let [_, pan_x, _] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: -500.0,
                    point: right,
                },
                route(),
            )
            .unwrap(),
        );
        assert!((pan_x + 0.05).abs() < 1e-6);

        assert!(adjust_map_view(
            map("follow"),
            scene(),
            MapViewGesture::Pan([1.0, 0.0]),
            route()
        )
        .is_err());
    }

    #[test]
    fn fit_centres_the_route_and_fills_the_frame_inside_its_margin() {
        // A 200 by 100 frame: the route's 420 by 210 units at zoom 15 fit its height, 84 units inside
        // the margin, at zoom 15 + log2(84 / 210).
        let mut panned = map("follow");
        panned.fixed_pan_x = 3.0;
        let [zoom, pan_x, pan_y] =
            fixed(adjust_map_view(panned, scene(), MapViewGesture::Fit, route()).unwrap());
        assert!((zoom - (15.0 + (84.0f32 / 210.0).log2())).abs() < 1e-4);
        assert_eq!([pan_x, pan_y], [0.0, 0.0]);
        // Without GPS only the pan resets.
        let [zoom, ..] =
            fixed(adjust_map_view(map("fixed"), scene(), MapViewGesture::Fit, None).unwrap());
        assert_eq!(zoom, 15.0);
    }

    #[test]
    fn the_zoom_reaches_out_to_the_minimum_and_a_map_without_gps_only_zooms() {
        let [zoom, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Zoom {
                    wheel_delta: 500.0 * 30.0,
                    point: [400.0, 150.0],
                },
                None,
            )
            .unwrap(),
        );
        assert_eq!([zoom, pan_x, pan_y], [*FIXED_ZOOM_RANGE.start(), 0.0, 0.0]);
        let [_, pan_x, pan_y] = fixed(
            adjust_map_view(
                map("fixed"),
                scene(),
                MapViewGesture::Pan([84.0, 42.0]),
                None,
            )
            .unwrap(),
        );
        assert_eq!([pan_x, pan_y], [0.0, 0.0]);
    }
}
