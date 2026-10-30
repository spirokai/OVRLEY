use crate::widgets::route::config::RoutePathConfig;
use serde::{Deserialize, Serialize};

/// Effective map config; every output-affecting field is required at ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct MapConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// The attribution's font. The attribution itself is always drawn.
    pub font: String,
    pub map_style: String,
    pub show_roads: bool,
    pub show_paths: bool,
    pub show_water: bool,
    pub show_buildings: bool,
    pub camera_mode: String,
    pub orientation: String,
    pub pitch: f32,
    pub range_meters: f32,
    pub simplify_tolerance_px: f32,
    pub show_full_activity: bool,
    #[serde(flatten)]
    pub path: RoutePathConfig,
}
