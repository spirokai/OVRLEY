use crate::model::types::MetricKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// One plotted metric. `label` is the legend text the editor localized when the metric was picked.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ChartSeriesConfig {
    pub metric: MetricKind,
    pub label: String,
    pub color: String,
}

/// Presentation defaults are materialized by the host before this ingress.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct RollingChartConfig {
    pub value: String,
    pub x: f32,
    pub y: f32,
    /// Plot area; the legend row sits below it.
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub window_seconds: f32,
    pub line_width: f32,
    pub line_opacity: f32,
    pub area_fill: bool,
    pub area_gradient: bool,
    pub area_opacity: f32,
    /// Drawn back to front.
    pub series: Vec<ChartSeriesConfig>,
    pub show_marker: bool,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_opacity: f32,
    pub show_legend: bool,
    pub legend_font: String,
    pub legend_font_size: f32,
    pub legend_color: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
