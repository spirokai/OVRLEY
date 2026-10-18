use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Effective profile config. Presentation defaults are materialized by the host.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ElevationPlotConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub area_remaining_color: String,
    pub area_remaining_opacity: f32,
    pub area_completed_color: String,
    pub area_completed_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_elevation_metric: bool,
    pub show_elevation_imperial: bool,
    /// Optional label correction; null/absence leaves measured altitude unchanged.
    pub starting_altitude: Option<f64>,
    pub starting_altitude_unit: Option<String>,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub point_label: PointLabelConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PointLabelConfig {
    /// Omitted font inherits the scene's optional native font selection.
    pub font: Option<String>,
    pub font_size: f32,
    pub color: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
