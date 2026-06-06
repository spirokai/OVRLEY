//! Raw configuration types deserialized directly from frontend JSON.
//!
//! These types intentionally allow extra fields (`#[serde(flatten)]`) so
//! templates can remain forward-compatible across app versions. The
//! normalize layer consumes these types and produces validated equivalents.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::error::{CoreError, CoreResult};
use crate::types::{DisplayType, MetricKind};

pub const TEMPLATE_FILE_FORMAT: &str = "ovrley-template";
pub const TEMPLATE_FILE_VERSION: u32 = 2;

/// Global render settings shared by labels, metric values, plots, and ffmpeg.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(default)]
    pub overlay_filename: Option<String>,
    #[serde(default, alias = "updateRate")]
    pub update_rate: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_path: Option<String>,
    #[serde(default, skip_serializing)]
    pub composite_bitrate: Option<String>,
    #[serde(default, skip_serializing)]
    pub composite_sync_offset: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_video_fps_num: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_fps_den: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_duration: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_render_duration: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_video_trim_start: Option<f64>,
    #[serde(default, skip_serializing)]
    pub composite_widget_update_rate: Option<u32>,
    #[serde(default)]
    pub ffmpeg: Value,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub scale: Option<f32>,
    #[serde(default)]
    pub time_format: Option<String>,
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
    pub custom_export_range_active: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Static text label drawn onto the cached base layer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelConfig {
    #[serde(default)]
    pub text: String,
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Dynamic telemetry value configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    pub unit_color: Option<String>,
    #[serde(default)]
    pub display_unit: Option<String>,
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ValueConfig {
    pub(crate) fn to_heading_widget_config(&self) -> CoreResult<HeadingWidgetConfig> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| CoreError::Config(format!("heading value serialization: {e}")))?;
        strip_json_nulls(&mut raw);
        serde_json::from_value(raw)
            .map_err(|e| CoreError::Config(format!("heading value config: {e}")))
    }
}

/// Complete template render configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    pub scene: SceneConfig,
    #[serde(default)]
    pub labels: Vec<LabelConfig>,
    #[serde(default)]
    pub values: Vec<ValueConfig>,
    #[serde(default)]
    pub plots: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Shared polyline style fragment for plot widgets.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct LineStyleConfig {
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Shared area fill style fragment for plot widgets.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct FillStyleConfig {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Label style for marker-associated point labels.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct PointLabelConfig {
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub x_offset: Option<f32>,
    #[serde(default)]
    pub y_offset: Option<f32>,
    #[serde(default)]
    pub units: Vec<String>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Course/route plot configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CoursePlotConfig {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub margin: Option<f32>,
    #[serde(default)]
    pub simplify_tolerance_px: Option<f32>,
    #[serde(default)]
    pub target_density: Option<f32>,
    #[serde(default)]
    pub completed_line_width: Option<f32>,
    #[serde(default)]
    pub completed_line_color: Option<String>,
    #[serde(default)]
    pub completed_line_opacity: Option<f32>,
    #[serde(default)]
    pub remaining_line_width: Option<f32>,
    #[serde(default)]
    pub remaining_line_color: Option<String>,
    #[serde(default)]
    pub remaining_line_opacity: Option<f32>,
    #[serde(default)]
    pub marker_variant: Option<String>,
    #[serde(default)]
    pub marker_variant_diameter: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    #[serde(default)]
    pub show_full_activity: Option<bool>,
    #[serde(default)]
    pub line: Option<LineStyleConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Elevation profile plot configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ElevationPlotConfig {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub margin: Option<f32>,
    #[serde(default)]
    pub completed_line_width: Option<f32>,
    #[serde(default)]
    pub completed_line_color: Option<String>,
    #[serde(default)]
    pub completed_line_opacity: Option<f32>,
    #[serde(default)]
    pub remaining_line_width: Option<f32>,
    #[serde(default)]
    pub remaining_line_color: Option<String>,
    #[serde(default)]
    pub remaining_line_opacity: Option<f32>,
    #[serde(default)]
    pub marker_variant: Option<String>,
    #[serde(default)]
    pub marker_variant_diameter: Option<f32>,
    #[serde(default)]
    pub marker_size: Option<f32>,
    #[serde(default)]
    pub marker_color: Option<String>,
    #[serde(default)]
    pub marker_opacity: Option<f32>,
    #[serde(default)]
    pub shadow_color: Option<String>,
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    #[serde(default)]
    pub area_completed_color: Option<String>,
    #[serde(default)]
    pub area_completed_opacity: Option<f32>,
    #[serde(default)]
    pub area_remaining_color: Option<String>,
    #[serde(default)]
    pub area_remaining_opacity: Option<f32>,
    #[serde(default)]
    pub show_full_activity: Option<bool>,
    #[serde(default)]
    pub show_elevation_metric: Option<bool>,
    #[serde(default)]
    pub show_elevation_imperial: Option<bool>,
    #[serde(default)]
    pub y_scale: Option<f32>,
    #[serde(default)]
    pub simplify_tolerance_px: Option<f32>,
    #[serde(default)]
    pub target_density: Option<f32>,
    #[serde(default)]
    pub metric_label_offset_x: Option<f32>,
    #[serde(default)]
    pub metric_label_offset_y: Option<f32>,
    #[serde(default)]
    pub imperial_label_offset_x: Option<f32>,
    #[serde(default)]
    pub imperial_label_offset_y: Option<f32>,
    #[serde(default)]
    pub line: Option<LineStyleConfig>,
    #[serde(default)]
    pub fill: Option<FillStyleConfig>,
    #[serde(default)]
    pub point_label: Option<PointLabelConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Heading compass tape widget configuration.
///
/// Required fields are concrete types — missing fields cause serde rejection.
/// Optional fields use `#[serde(default)]` — the normalize layer decides
/// whether they are required.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HeadingWidgetConfig {
    pub value: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub display_type: DisplayType,
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub opacity: Option<f32>,
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
    #[serde(default, alias = "show_numeric_labels")]
    pub show_minor_labels: Option<bool>,
    #[serde(default, alias = "show_cardinal_labels")]
    pub show_major_labels: Option<bool>,
    #[serde(default, alias = "numeric_label_color", alias = "minor_label_color")]
    pub label_color: Option<String>,
    #[serde(default, alias = "major_label_color")]
    pub cardinal_label_color: Option<String>,
    #[serde(default, alias = "label_font_family")]
    pub label_font: Option<String>,
    #[serde(default)]
    pub label_font_size: Option<f32>,
    #[serde(default)]
    pub label_offset: Option<f32>,
    #[serde(default)]
    pub indicator_style: Option<String>,
    #[serde(default)]
    pub indicator_placement: Option<String>,
    #[serde(default)]
    pub show_indicator: Option<bool>,
    #[serde(default)]
    pub indicator_color: Option<String>,
    #[serde(default)]
    pub indicator_size: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Finds a plot config by value key in either array or object-shaped template data.
pub fn find_plot_value<'a>(plots: &'a Value, value_key: &str) -> Option<&'a Value> {
    match plots {
        Value::Array(items) => items.iter().find(|item| {
            item.get("value")
                .and_then(Value::as_str)
                .map(|value| value == value_key)
                .unwrap_or(false)
        }),
        Value::Object(map) => map.get(value_key).or_else(|| {
            map.values().find(|item| {
                item.get("value")
                    .and_then(Value::as_str)
                    .map(|value| value == value_key)
                    .unwrap_or(false)
            })
        }),
        _ => None,
    }
}

/// Removes all `null` values from a JSON object tree so that `#[serde(default)]`
/// attributes on struct fields can take effect during deserialization.
pub fn strip_json_nulls(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, v| !v.is_null());
        for v in map.values_mut() {
            strip_json_nulls(v);
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing — deserialization + timing pre-checks
// ---------------------------------------------------------------------------

/// Parses and validates render configuration JSON.
///
/// Validation focuses on constraints that would otherwise break frame timing:
/// positive integer FPS, update-rate divisibility, and non-empty scene ranges.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))?;
    parse_config_value(&value)
}

/// Parses a raw render config from a pre-built JSON value.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_value(value: &Value) -> CoreResult<RenderConfig> {
    let config: RenderConfig = serde_json::from_value(value.clone())
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))?;
    if config.scene.fps <= 0.0 {
        return Err(CoreError::Config(format!(
            "scene.fps: {}",
            config.scene.fps
        )));
    }
    if (config.scene.fps.fract()).abs() > f64::EPSILON {
        return Err(CoreError::Config(format!(
            "scene.fps must be an integer for widget update rate support: {}",
            config.scene.fps
        )));
    }
    if let Some(update_rate) = config.scene.update_rate {
        if update_rate == 0 {
            return Err(CoreError::Config(
                "scene.update_rate must be at least 1".into(),
            ));
        }
        let fps = config.scene.fps.round() as u32;
        if !fps.is_multiple_of(update_rate) {
            return Err(CoreError::Config(format!(
                "scene.update_rate ({update_rate}) must cleanly divide scene.fps ({fps})"
            )));
        }
    }
    if matches!(config.scene.composite_widget_update_rate, Some(0)) {
        return Err(CoreError::Config(
            "scene.composite_widget_update_rate must be at least 1".into(),
        ));
    }
    Ok(config)
}

/// Parses either a raw render config or a wrapped OVRLEY template file.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_template_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("template JSON: {error}")))?;
    parse_template_value(&value)
}

/// Parses either a raw render config or a wrapped OVRLEY template value.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_template_value(value: &Value) -> CoreResult<RenderConfig> {
    let Some(format) = value.get("format").and_then(Value::as_str) else {
        return parse_config_value(value);
    };

    if format != TEMPLATE_FILE_FORMAT {
        return Err(CoreError::Config(format!(
            "template format: expected {TEMPLATE_FILE_FORMAT}, got {format}"
        )));
    }

    let Some(version) = value.get("version").and_then(Value::as_u64) else {
        return Err(CoreError::Config("template version missing".into()));
    };

    if version != u64::from(TEMPLATE_FILE_VERSION) {
        return Err(CoreError::Config(format!(
            "unsupported template version: {version}. expected {TEMPLATE_FILE_VERSION}"
        )));
    }

    let mut config_value = value
        .get("config")
        .cloned()
        .ok_or_else(|| CoreError::Config("template config missing".into()))?;
    materialize_template_scene_defaults(&mut config_value, value);
    let mut config = parse_config_value(&config_value)?;
    apply_template_global_defaults(&mut config, &value);
    Ok(config)
}

fn materialize_template_scene_defaults(config: &mut Value, template: &Value) {
    let Some(scene) = config.get_mut("scene").and_then(Value::as_object_mut) else {
        return;
    };

    if !scene.contains_key("start") {
        scene.insert("start".to_string(), Value::from(0.0));
    }
    if !scene.contains_key("end") {
        scene.insert("end".to_string(), Value::from(1.0));
    }

    let Some(globals) = template
        .get("settings")
        .and_then(|settings| settings.get("globalDefaults"))
        .and_then(Value::as_object)
    else {
        return;
    };

    copy_scene_default_if_missing(scene, globals, "scale", "scale");
    copy_scene_default_if_missing(scene, globals, "font", "font_values");
    copy_scene_default_if_missing(scene, globals, "font_size", "font_size");
    copy_scene_default_if_missing(scene, globals, "opacity", "opacity");
    copy_scene_default_if_missing(scene, globals, "shadow_color", "shadow_color");
    copy_scene_default_if_missing(scene, globals, "shadow_strength", "shadow_strength");
    copy_scene_default_if_missing(scene, globals, "shadow_distance", "shadow_distance");
    copy_scene_default_if_missing(scene, globals, "border_color", "border_color");
    copy_scene_default_if_missing(scene, globals, "border_thickness", "border_thickness");
}

fn copy_scene_default_if_missing(
    scene: &mut serde_json::Map<String, Value>,
    globals: &serde_json::Map<String, Value>,
    scene_key: &str,
    globals_key: &str,
) {
    if scene.contains_key(scene_key) {
        return;
    }

    if let Some(value) = globals.get(globals_key) {
        scene.insert(scene_key.to_string(), value.clone());
    }
}

fn apply_template_global_defaults(config: &mut RenderConfig, template: &Value) {
    let Some(globals) = template
        .get("settings")
        .and_then(|settings| settings.get("globalDefaults"))
    else {
        return;
    };

    let font_values = globals
        .get("font_values")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(font_values) = font_values {
        for value in &mut config.values {
            if value.font.is_none() {
                value.font = Some(font_values.clone());
            }
            if value.font_family.is_none() {
                value.font_family = Some(font_values.clone());
            }
        }
        apply_heading_label_font_default(&mut config.plots, &font_values);
    }
}

fn apply_heading_label_font_default(plots: &mut Value, font_values: &str) {
    match plots {
        Value::Array(items) => {
            for item in items {
                apply_heading_label_font_default_to_plot(item, font_values);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                apply_heading_label_font_default_to_plot(item, font_values);
            }
        }
        _ => {}
    }
}

fn apply_heading_label_font_default_to_plot(plot: &mut Value, font_values: &str) {
    let Some(object) = plot.as_object_mut() else {
        return;
    };
    let is_heading = object
        .get("value")
        .and_then(Value::as_str)
        .map(|value| value == "heading")
        .unwrap_or(false);

    if is_heading && !object.contains_key("label_font") {
        object.insert(
            "label_font".to_string(),
            Value::String(font_values.to_string()),
        );
    }
}
