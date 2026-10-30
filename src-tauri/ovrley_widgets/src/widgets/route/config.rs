use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Effective course config; every output-affecting field is required at ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CoursePlotConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub show_full_activity: bool,
    #[serde(flatten)]
    pub path: RoutePathConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The line and marker fields shared by every family that draws a route.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RoutePathConfig {
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
}
