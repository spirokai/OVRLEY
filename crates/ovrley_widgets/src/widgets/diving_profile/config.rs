use crate::widgets::elevation::config::PointLabelConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Presentation defaults are materialized by the host before this ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct DivingProfileConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub ceiling_line_width: f32,
    pub ceiling_line_color: String,
    pub ceiling_line_opacity: f32,
    pub ceiling_area_color: String,
    pub ceiling_area_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_depth_metric: bool,
    pub show_depth_imperial: bool,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub point_label: PointLabelConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
