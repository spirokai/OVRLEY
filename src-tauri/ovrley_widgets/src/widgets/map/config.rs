use crate::widgets::route::config::RoutePathConfig;
use ovrley_map::MapStyleId;
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
    /// Need not be installed: drawing resolves it. See [`ovrley_map::MapStyleCatalog::resolve`].
    pub map_style: MapStyleId,
    pub show_buildings: bool,
    pub show_paths: bool,
    pub show_labels: bool,
    pub camera_mode: String,
    pub orientation: String,
    pub pitch: f32,
    /// The chase camera's zoom level.
    pub follow_zoom: f32,
    /// The top-down camera's zoom level.
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
    /// How strongly GPS noise is smoothed out of the route, in metres.
    pub route_smoothing_m: f32,
    pub show_full_activity: bool,
    /// Both sides of the split draw at this width.
    pub line_width: f32,
    #[serde(flatten)]
    pub path: RoutePathConfig,
}
