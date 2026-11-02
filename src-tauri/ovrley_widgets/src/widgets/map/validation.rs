//! Map plot validation. Every field is required and parsed once, so consumers never see `pitch`
//! on a fixed camera.
use super::config::MapConfig;
use crate::{
    error::{WidgetError, WidgetResult},
    model::validation::{
        require_f32, require_hex_color, require_non_negative_f32, require_positive_f32,
        require_unit_opacity,
    },
    widgets::route::validation::{validate_route_path, ValidatedRoutePath},
};
use ovrley_map::{MapLayers, MapStyle, MapStyleSpec, MAX_TILE_ZOOM};
use std::ops::RangeInclusive;

const MAX_PITCH_DEG: f32 = 70.0;
/// Further out, a follow camera's pitched view takes in too many tiles.
const MIN_FOLLOW_ZOOM: f32 = 14.0;
pub(super) const FOLLOW_ZOOM_RANGE: RangeInclusive<f32> = MIN_FOLLOW_ZOOM..=MAX_TILE_ZOOM as f32;
/// Top-down zoom levels around the zoom that fits the whole route.
pub(super) const FIXED_ZOOM_RANGE: RangeInclusive<f32> = -4.0..=8.0;
pub(super) const FIXED_PAN_RANGE: RangeInclusive<f32> = -1.0..=1.0;
/// Metres. The low end looks almost raw, and past the high end a road's own bends start to go.
const ROUTE_SMOOTHING_RANGE: RangeInclusive<f32> = 1.0..=20.0;
/// The sample counts every GPU supports, so a project draws the same on any machine.
const MSAA_SAMPLES: [u32; 2] = [1, 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapOrientation {
    NorthUp,
    RiderUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapCameraMode {
    /// North up and pitch 0. The tiles never move: only the marker and the route's split do.
    Fixed { zoom_offset: f32, pan: [f32; 2] },
    Follow {
        orientation: MapOrientation,
        pitch_deg: f32,
        zoom: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapShape {
    Rectangle { corner_radius: f32 },
    Circle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapBorder {
    pub color: String,
    pub thickness: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedMapWidget {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub font: String,
    pub spec: MapStyleSpec,
    pub camera: MapCameraMode,
    pub msaa_samples: u32,
    pub shape: MapShape,
    pub border: MapBorder,
    pub route_smoothing_m: f32,
    pub show_full_activity: bool,
    pub line_width: f32,
    pub path: ValidatedRoutePath,
}

pub fn validate_map_widget(map: &MapConfig, index: usize) -> WidgetResult<ValidatedMapWidget> {
    let p = |f: &str| format!("plots[{index}].{f}");
    let invalid = |field: &str, message: &str| {
        WidgetError::Config(format!("{}: {message}", p(field)))
    };
    let in_range = |value: f32, field: &str, range: RangeInclusive<f32>| -> WidgetResult<f32> {
        let value = require_f32(Some(value), &p(field))?;
        if range.contains(&value) {
            Ok(value)
        } else {
            Err(invalid(
                field,
                &format!("must be between {} and {}", range.start(), range.end()),
            ))
        }
    };
    if map.value != "map" {
        return Err(invalid("value", "must be map"));
    }
    if map.font.trim().is_empty() {
        return Err(invalid("font", "must identify a font"));
    }
    let style = MapStyle::from_name(&map.map_style)
        .ok_or_else(|| invalid("map_style", "unsupported map style"))?;
    let camera = match map.camera_mode.as_str() {
        "fixed" => MapCameraMode::Fixed {
            zoom_offset: in_range(map.fixed_zoom, "fixed_zoom", FIXED_ZOOM_RANGE)?,
            pan: [
                in_range(map.fixed_pan_x, "fixed_pan_x", FIXED_PAN_RANGE)?,
                in_range(map.fixed_pan_y, "fixed_pan_y", FIXED_PAN_RANGE)?,
            ],
        },
        "follow" => MapCameraMode::Follow {
            orientation: match map.orientation.as_str() {
                "north_up" => MapOrientation::NorthUp,
                "rider_up" => MapOrientation::RiderUp,
                _ => return Err(invalid("orientation", "unsupported orientation")),
            },
            pitch_deg: in_range(map.pitch, "pitch", 0.0..=MAX_PITCH_DEG)?,
            zoom: in_range(map.follow_zoom, "follow_zoom", FOLLOW_ZOOM_RANGE)?,
        },
        _ => return Err(invalid("camera_mode", "unsupported camera mode")),
    };
    // Seen from straight above, a building is only its roof, so the toggle needs a pitched camera.
    let buildings = map.show_buildings
        && matches!(camera, MapCameraMode::Follow { pitch_deg, .. } if pitch_deg > 0.0);
    if !MSAA_SAMPLES.contains(&map.msaa_samples) {
        return Err(invalid("msaa_samples", "must be 1 or 4"));
    }
    let width = require_positive_f32(Some(map.width), &p("width"))?;
    let height = require_positive_f32(Some(map.height), &p("height"))?;
    let shape = match map.shape.as_str() {
        "rectangle" => MapShape::Rectangle {
            corner_radius: require_non_negative_f32(map.corner_radius, &p("corner_radius"))?,
        },
        "circle" if width == height => MapShape::Circle,
        "circle" => return Err(invalid("shape", "a circular map must be square")),
        _ => return Err(invalid("shape", "unsupported shape")),
    };

    Ok(ValidatedMapWidget {
        x: require_f32(Some(map.x), &p("x"))?,
        y: require_f32(Some(map.y), &p("y"))?,
        width,
        height,
        opacity: require_unit_opacity(Some(map.opacity), &p("opacity"))?,
        font: map.font.clone(),
        spec: MapStyleSpec {
            style,
            // The GPU renderer draws no labels, so the config has no toggle for them.
            layers: MapLayers {
                buildings,
                labels: false,
                paths: map.show_paths,
            },
        },
        camera,
        msaa_samples: map.msaa_samples,
        shape,
        border: MapBorder {
            color: require_hex_color(Some(&map.border_color), &p("border_color"))?,
            thickness: require_non_negative_f32(map.border_thickness, &p("border_thickness"))?,
            opacity: require_unit_opacity(Some(map.border_opacity), &p("border_opacity"))?,
        },
        route_smoothing_m: in_range(
            map.route_smoothing_m,
            "route_smoothing_m",
            ROUTE_SMOOTHING_RANGE,
        )?,
        show_full_activity: map.show_full_activity,
        line_width: require_positive_f32(Some(map.line_width), &p("line_width"))?,
        path: validate_route_path(&map.path, &p)?,
    })
}
