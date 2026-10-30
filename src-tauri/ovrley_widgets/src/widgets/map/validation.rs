//! Map plot validation. Every field is required and parsed once, so consumers never see `pitch`
//! on a fixed camera.
use super::config::MapConfig;
use crate::{
    error::{WidgetError, WidgetResult},
    model::validation::{require_f32, require_non_negative_f32, require_positive_f32},
    widgets::route::validation::{validate_route_path, ValidatedRoutePath},
};
use ovrley_map::{MapLayers, MapStyle, MapStyleSpec};

/// Steeper than this, a follow camera with its top padding looks past the horizon.
const MAX_PITCH_DEG: f32 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapOrientation {
    NorthUp,
    RiderUp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapCameraMode {
    /// The whole route, north up, pitch 0.
    Fixed,
    Follow {
        orientation: MapOrientation,
        pitch_deg: f32,
        range_meters: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedMapWidget {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font: String,
    pub spec: MapStyleSpec,
    pub camera: MapCameraMode,
    pub simplify_tolerance_px: f32,
    pub show_full_activity: bool,
    pub path: ValidatedRoutePath,
}

pub fn validate_map_widget(map: &MapConfig, index: usize) -> WidgetResult<ValidatedMapWidget> {
    let p = |f: &str| format!("plots[{index}].{f}");
    let invalid = |field: &str, message: &str| {
        WidgetError::Config(format!("{}: {message}", p(field)))
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
        "fixed" => MapCameraMode::Fixed,
        "follow" => {
            let orientation = match map.orientation.as_str() {
                "north_up" => MapOrientation::NorthUp,
                "rider_up" => MapOrientation::RiderUp,
                _ => return Err(invalid("orientation", "unsupported orientation")),
            };
            let pitch_deg = require_f32(Some(map.pitch), &p("pitch"))?;
            if !(0.0..=MAX_PITCH_DEG).contains(&pitch_deg) {
                return Err(invalid("pitch", "must be between 0 and 60"));
            }
            MapCameraMode::Follow {
                orientation,
                pitch_deg,
                range_meters: require_positive_f32(Some(map.range_meters), &p("range_meters"))?,
            }
        }
        _ => return Err(invalid("camera_mode", "unsupported camera mode")),
    };
    let simplify_tolerance_px =
        require_f32(Some(map.simplify_tolerance_px), &p("simplify_tolerance_px"))?;
    require_non_negative_f32(simplify_tolerance_px, &p("simplify_tolerance_px"))?;

    Ok(ValidatedMapWidget {
        x: require_f32(Some(map.x), &p("x"))?,
        y: require_f32(Some(map.y), &p("y"))?,
        width: require_positive_f32(Some(map.width), &p("width"))?,
        height: require_positive_f32(Some(map.height), &p("height"))?,
        font: map.font.clone(),
        spec: MapStyleSpec {
            style,
            // The GPU renderer draws no labels, so the config has no toggle for them.
            layers: MapLayers {
                roads: map.show_roads,
                paths: map.show_paths,
                water: map.show_water,
                buildings: map.show_buildings,
                labels: false,
            },
        },
        camera,
        simplify_tolerance_px,
        show_full_activity: map.show_full_activity,
        path: validate_route_path(&map.path, &p)?,
    })
}
