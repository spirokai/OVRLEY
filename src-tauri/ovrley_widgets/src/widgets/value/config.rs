use crate::model::types::{DisplayType, MetricKind, TrackFillStyle};
use crate::widgets::heart_zones::zones::HeartZoneRange;
use crate::render::track_fill::GradientStop;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Dynamic telemetry value configuration.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ValueConfig {
    pub value: MetricKind,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub hours_offset: Option<i32>,
    #[serde(default)]
    pub time_format: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub time_mode: Option<String>,
    #[serde(default)]
    pub elapsed_origin: Option<String>,
    #[serde(default)]
    pub show_hundredths: Option<bool>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(default)]
    pub decimals: Option<usize>,
    #[serde(default)]
    pub show_icon: Option<bool>,
    #[serde(default)]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub icon_size: Option<f32>,
    #[serde(default)]
    pub icon_offset_x: Option<f32>,
    #[serde(default)]
    pub icon_offset_y: Option<f32>,
    #[serde(default)]
    pub show_units: Option<bool>,
    #[serde(default)]
    pub show_full_distance: Option<bool>,
    #[serde(default)]
    pub show_full_ascent: Option<bool>,
    #[serde(default)]
    pub unit_color: Option<String>,
    #[serde(default)]
    pub display_unit: Option<String>,
    #[serde(default)]
    pub starting_altitude: Option<f64>,
    #[serde(default)]
    pub coordinate_format: Option<String>,
    #[serde(default)]
    pub balance_format: Option<String>,
    #[serde(default)]
    pub value_offset: Option<f32>,
    #[serde(default)]
    pub triangle_positive_color: Option<String>,
    #[serde(default)]
    pub triangle_negative_color: Option<String>,
    #[serde(default)]
    pub show_sign: Option<bool>,
    #[serde(default)]
    pub show_triangle: Option<bool>,
    #[serde(default)]
    pub triangle_width: Option<f32>,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_thickness: Option<f32>,
    #[serde(default)]
    pub border_strength: Option<f32>,
    #[serde(default)]
    pub border_distance: Option<f32>,
    #[serde(default)]
    pub display_type: DisplayType,
    #[serde(default)]
    pub content_alignment: Option<String>,
    #[serde(default)]
    pub lap_timer_mode: Option<String>,
    #[serde(default)]
    pub show_label: Option<bool>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub split_distance: Option<f64>,
    #[serde(default)]
    pub split_unit: Option<String>,
    #[serde(default)]
    pub max_splits: Option<u32>,
    #[serde(default)]
    pub visible_columns: Option<Vec<String>>,
    #[serde(default)]
    pub row_gap: Option<f32>,
    #[serde(default)]
    pub bar_width: Option<f32>,
    #[serde(default)]
    pub track_border_opacity: Option<f32>,
    #[serde(default)]
    pub positive_delta_color: Option<String>,
    #[serde(default)]
    pub negative_delta_color: Option<String>,
    #[serde(default)]
    pub renumber_laps: Option<bool>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub rotation: Option<f32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub arc_angle: Option<f32>,
    #[serde(default)]
    pub corner_orientation: Option<String>,
    #[serde(default)]
    pub tick_count: Option<u32>,
    #[serde(default)]
    pub tick_length: Option<f32>,
    #[serde(default)]
    pub glow: Option<f32>,
    #[serde(default)]
    pub inner_widget_offset_x: Option<f32>,
    #[serde(default)]
    pub inner_widget_offset_y: Option<f32>,
    #[serde(default)]
    pub value_offset_x: Option<f32>,
    #[serde(default)]
    pub value_offset_y: Option<f32>,
    #[serde(default)]
    pub track_thickness: Option<f32>,
    #[serde(default)]
    pub track_corner_radius: Option<f32>,
    #[serde(default)]
    pub track_border_thickness: Option<f32>,
    #[serde(default)]
    pub track_border_color: Option<String>,
    #[serde(default)]
    pub track_empty_color: Option<String>,
    #[serde(default)]
    pub track_empty_opacity: Option<f32>,
    #[serde(default)]
    pub track_filled_color: Option<String>,
    #[serde(default)]
    pub track_filled_opacity: Option<f32>,
    #[serde(default)]
    pub track_filled_gradient: Option<bool>,
    #[serde(default)]
    pub track_filled_gradient_stops: Option<Vec<GradientStop>>,
    #[serde(default)]
    pub track_fill_flat: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "deserialize_track_fill_style")]
    pub track_fill_style: Option<TrackFillStyle>,
    #[serde(default)]
    pub bar_count: Option<u32>,
    #[serde(default)]
    pub bar_gap: Option<f32>,
    #[serde(default)]
    pub show_min_max_labels: Option<bool>,
    #[serde(default)]
    pub min_max_label_font: Option<String>,
    #[serde(default)]
    pub min_max_label_font_size: Option<f32>,
    #[serde(default)]
    pub min_max_label_position: Option<String>,
    #[serde(default)]
    pub min_max_label_color: Option<String>,
    #[serde(default)]
    pub diameter: Option<f32>,
    #[serde(default)]
    pub fill_color: Option<String>,
    #[serde(default)]
    pub fill_opacity: Option<f32>,
    #[serde(default)]
    pub border_opacity: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub axis_horizontal: Option<String>,
    #[serde(default)]
    pub axis_vertical: Option<String>,
    #[serde(default)]
    pub invert_horizontal: Option<bool>,
    #[serde(default)]
    pub invert_vertical: Option<bool>,
    #[serde(default)]
    pub clip_percentile: Option<f32>,
    #[serde(default)]
    pub label_font: Option<String>,
    #[serde(default)]
    pub label_font_size: Option<f32>,
    #[serde(default)]
    pub label_color: Option<String>,
    #[serde(default)]
    pub label_decimals: Option<usize>,
    #[serde(default)]
    pub label_unit: Option<String>,
    #[serde(default)]
    pub label_unit_color: Option<String>,
    #[serde(default)]
    pub label_offset_x: Option<f32>,
    #[serde(default)]
    pub label_offset_y: Option<f32>,
    #[serde(default)]
    pub pixels_per_degree: Option<f32>,
    #[serde(default)]
    pub major_tick_interval: Option<u32>,
    #[serde(default)]
    pub minor_ticks_per_major: Option<u32>,
    #[serde(default)]
    pub show_major_ticks: Option<bool>,
    #[serde(default)]
    pub show_minor_ticks: Option<bool>,
    #[serde(default)]
    pub major_tick_length_pct: Option<f32>,
    #[serde(default)]
    pub minor_tick_length_pct: Option<f32>,
    #[serde(default)]
    pub major_tick_thickness: Option<f32>,
    #[serde(default)]
    pub minor_tick_thickness: Option<f32>,
    #[serde(default)]
    pub tick_color: Option<String>,
    #[serde(default)]
    pub cardinal_tick_color: Option<String>,
    #[serde(default)]
    pub tick_alignment: Option<String>,
    #[serde(default)]
    pub show_minor_labels: Option<bool>,
    #[serde(default)]
    pub show_major_labels: Option<bool>,
    #[serde(default)]
    pub cardinal_label_color: Option<String>,
    #[serde(default)]
    pub label_offset: Option<f32>,
    #[serde(default)]
    pub show_indicator: Option<bool>,
    #[serde(default)]
    pub indicator_style: Option<String>,
    #[serde(default)]
    pub indicator_placement: Option<String>,
    #[serde(default)]
    pub indicator_color: Option<String>,
    #[serde(default)]
    pub indicator_size: Option<f32>,
    #[serde(default)]
    pub indicator_corner_radius: Option<f32>,
    #[serde(default)]
    pub rotation_mode: Option<String>,
    #[serde(default)]
    pub track_fill: Option<String>,
    #[serde(default)]
    pub show_cardinal_labels: Option<bool>,
    #[serde(default)]
    pub cardinal_font_size: Option<f32>,
    #[serde(default)]
    pub show_value: Option<bool>,
    #[serde(default)]
    pub zone_calculation: Option<String>,
    #[serde(default)]
    pub lthr: Option<f32>,
    #[serde(default)]
    pub max_heartrate: Option<f32>,
    #[serde(default)]
    pub manual_zones: Option<Vec<HeartZoneRange>>,
    #[serde(default)]
    pub zone_colors: Option<Vec<String>>,
    #[serde(default)]
    pub zone_gap: Option<f32>,
    #[serde(default)]
    pub label_font_weight: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// Omitted track_fill_style retains the documented continuous-fill default;
// malformed present values, including null, must fail at ingress.
fn deserialize_track_fill_style<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<TrackFillStyle>, D::Error> {
    TrackFillStyle::deserialize(deserializer).map(Some)
}
