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
    pub opacity: f32,
    /// The attribution's font. The attribution itself is always drawn.
    pub font: String,
    pub map_style: String,
    pub show_buildings: bool,
    pub camera_mode: String,
    pub orientation: String,
    pub pitch: f32,
    /// The chase camera's zoom level.
    pub follow_zoom: f32,
    /// Top-down zoom levels past the zoom that fits the whole route.
    pub fixed_zoom: f32,
    /// How far the top-down route sits from the centre, as a share of its longest side.
    pub fixed_pan_x: f32,
    pub fixed_pan_y: f32,
    pub msaa_samples: u32,
    pub shape: String,
    pub corner_radius: f32,
    pub border_color: String,
    pub border_thickness: f32,
    pub border_opacity: f32,
    pub simplify_tolerance_px: f32,
    pub show_full_activity: bool,
    #[serde(flatten)]
    pub path: RoutePathConfig,
}
