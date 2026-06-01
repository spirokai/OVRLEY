//! Render configuration schema and requirement derivation.
//!
//! The frontend template editor serializes scene, label, metric, and plot
//! settings into this module's serde types. The structs intentionally allow
//! extra fields so templates can remain forward-compatible across app versions.
//! After parsing, [`RenderConfig::render_data_requirements`] determines which
//! telemetry series must be trimmed and densified for a render.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::error::{CoreError, CoreResult};
use crate::types::MetricKind;

pub const TEMPLATE_FILE_FORMAT: &str = "ovrley-template";
pub const TEMPLATE_FILE_VERSION: u32 = 2;

/// Global render settings shared by labels, metric values, plots, and ffmpeg.
///
/// Coordinates are in canvas pixels before applying `scale`. Most style fields
/// act as defaults that individual label/value/plot items may override.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    /// Output width in pixels; defaults to 1920 when omitted by render code.
    #[serde(default)]
    pub width: Option<u32>,
    /// Output height in pixels; defaults to 1080 when omitted by render code.
    #[serde(default)]
    pub height: Option<u32>,
    /// Layout frames per second. Must be a positive integer value.
    pub fps: f64,
    /// Scene start time in source activity elapsed seconds.
    pub start: f64,
    /// Scene end time in source activity elapsed seconds.
    pub end: f64,
    /// Default font filename or system family name.
    #[serde(default)]
    pub font: Option<String>,
    /// Default text size in pixels.
    #[serde(default)]
    pub font_size: Option<f32>,
    /// Default text and plot color as hex RGB/RGBA.
    #[serde(default)]
    pub color: Option<String>,
    /// Legacy decimal rounding default used when a value omits `decimals`.
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    /// Optional requested output filename from older template flows.
    #[serde(default)]
    pub overlay_filename: Option<String>,
    /// Draw every Nth layout frame into the video container.
    #[serde(default, alias = "updateRate")]
    pub update_rate: Option<u32>,
    /// Render-time source video path for MP4 compositing mode.
    #[serde(default, skip_serializing)]
    pub composite_video_path: Option<String>,
    /// Render-time video bitrate override for MP4 compositing mode.
    #[serde(default, skip_serializing)]
    pub composite_bitrate: Option<String>,
    /// Activity timestamp where source video time zero begins.
    #[serde(default, skip_serializing)]
    pub composite_sync_offset: Option<f64>,
    /// Numerator of the source video's rational FPS.
    #[serde(default, skip_serializing)]
    pub composite_video_fps_num: Option<u32>,
    /// Denominator of the source video's rational FPS.
    #[serde(default, skip_serializing)]
    pub composite_video_fps_den: Option<u32>,
    /// Source video duration in seconds.
    #[serde(default, skip_serializing)]
    pub composite_video_duration: Option<f64>,
    /// Composite output duration in seconds.
    #[serde(default, skip_serializing)]
    pub composite_render_duration: Option<f64>,
    /// Source video trim/seek start in seconds.
    #[serde(default, skip_serializing)]
    pub composite_video_trim_start: Option<f64>,
    /// Number of source video frames per rendered overlay update.
    #[serde(default, skip_serializing)]
    pub composite_widget_update_rate: Option<u32>,
    /// Codec/container options passed through to ffmpeg settings builder.
    #[serde(default)]
    pub ffmpeg: Value,
    /// Default opacity for text and widgets.
    #[serde(default)]
    pub opacity: Option<f32>,
    /// Global render scale applied to fonts and widget dimensions.
    #[serde(default)]
    pub scale: Option<f32>,
    /// Default strftime-style override for time values.
    #[serde(default)]
    pub time_format: Option<String>,
    /// Default shadow color as hex RGB/RGBA.
    #[serde(default)]
    pub shadow_color: Option<String>,
    /// Default shadow blur radius.
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    /// Default shadow offset on both axes.
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    /// Default text border/stroke color.
    #[serde(default)]
    pub border_color: Option<String>,
    /// Default text border/stroke thickness.
    #[serde(default)]
    pub border_thickness: Option<f32>,
    /// Reserved legacy border strength option.
    #[serde(default)]
    pub border_strength: Option<f32>,
    /// Reserved legacy border distance option.
    #[serde(default)]
    pub border_distance: Option<f32>,
    /// Whether the template is rendering a custom scene subset rather than the
    /// full activity. When true, widget trim-window and progress behavior
    /// adjusts to show only the selected export range.
    #[serde(default)]
    pub custom_export_range_active: Option<bool>,
    /// Unknown scene fields preserved for compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Static text label drawn onto the cached base layer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelConfig {
    /// Text content to draw.
    #[serde(default)]
    pub text: String,
    /// Left position in canvas pixels.
    pub x: f32,
    /// Top position in canvas pixels.
    pub y: f32,
    /// Label-specific font filename or family.
    #[serde(default)]
    pub font: Option<String>,
    /// Alternate font family field used by the editor.
    #[serde(default)]
    pub font_family: Option<String>,
    /// Label-specific font size.
    #[serde(default)]
    pub font_size: Option<f32>,
    /// Label-specific color.
    #[serde(default)]
    pub color: Option<String>,
    /// Label opacity, overriding scene opacity.
    #[serde(default)]
    pub opacity: Option<f32>,
    /// Reserved per-label shadow color.
    #[serde(default)]
    pub shadow_color: Option<String>,
    /// Reserved per-label shadow blur.
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    /// Reserved per-label shadow distance.
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    /// Reserved per-label border color.
    #[serde(default)]
    pub border_color: Option<String>,
    /// Reserved per-label border thickness.
    #[serde(default)]
    pub border_thickness: Option<f32>,
    /// Reserved per-label border strength.
    #[serde(default)]
    pub border_strength: Option<f32>,
    /// Reserved per-label border distance.
    #[serde(default)]
    pub border_distance: Option<f32>,
    /// Unknown label fields preserved for compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Dynamic telemetry value configuration.
///
/// `value` selects the telemetry series or synthetic value to render. The
/// remaining fields control formatting, units, icon display, and text styling.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValueConfig {
    /// Metric key, such as `speed`, `power`, `time`, or `gradient`.
    pub value: MetricKind,
    /// Left position in canvas pixels.
    pub x: f32,
    /// Top position in canvas pixels.
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
    /// Unknown value fields preserved for compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Complete template render configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    /// Global scene and ffmpeg settings.
    pub scene: SceneConfig,
    /// Static text labels drawn once into the base layer.
    #[serde(default)]
    pub labels: Vec<LabelConfig>,
    /// Dynamic metric values drawn every frame.
    #[serde(default)]
    pub values: Vec<ValueConfig>,
    /// Plot configuration as object or array for legacy template support.
    #[serde(default)]
    pub plots: Value,
    /// Unknown top-level fields preserved for compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Telemetry series needed by a template.
///
/// These booleans allow trimming/densifying to skip unused high-cardinality
/// series. Plot requirements are derived in addition to explicit `values`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderDataRequirements {
    pub speed: bool,
    pub elevation: bool,
    pub gradient: bool,
    pub heartrate: bool,
    pub cadence: bool,
    pub power: bool,
    pub temperature: bool,
    pub pace: bool,
    pub g_force: bool,
    pub air_pressure: bool,
    pub ground_contact_time: bool,
    pub left_right_balance: bool,
    pub stride_length: bool,
    pub stroke_rate: bool,
    pub torque: bool,
    pub vertical_speed: bool,
    pub gear_position: bool,
    pub vertical_ratio: bool,
    pub vertical_oscillation: bool,
    pub core_temperature: bool,
    pub heading: bool,
    pub time: bool,
    pub distance_progress: bool,
    pub course: bool,
}

/// Marker layer style used by route and elevation widgets.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct MarkerPointConfig {
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
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
///
/// Supports both newer nested `line`/`points` styles and legacy flat fields so
/// templates authored across versions continue to render.
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
    #[serde(default)]
    pub points: Vec<MarkerPointConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Elevation profile plot configuration.
///
/// Like route plots, this type accepts both legacy flat fields and newer nested
/// style fragments. Elevation labels can be controlled by explicit metric and
/// imperial toggles or the older `point_label.units` list.
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
    pub points: Vec<MarkerPointConfig>,
    #[serde(default)]
    pub point_label: Option<PointLabelConfig>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Heading compass tape widget configuration.
///
/// Controls a horizontal compass tape that scrolls with the heading value.
/// The tape renders as a 360-degree wrapped strip with configurable ticks,
/// labels, and a center indicator.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HeadingWidgetConfig {
    /// Metric key — always `heading`.
    pub value: MetricKind,
    /// Left position in canvas pixels.
    pub x: f32,
    /// Top position in canvas pixels.
    pub y: f32,
    /// Widget width in pixels.
    pub width: u32,
    /// Widget height in pixels.
    pub height: u32,
    /// Widget rotation in degrees.
    #[serde(default)]
    pub rotation: f32,
    /// Widget opacity 0.0–1.0.
    #[serde(default = "default_one")]
    pub opacity: f32,
    /// Horizontal tape scale in pixels per degree.
    #[serde(default = "default_ppd")]
    pub pixels_per_degree: f32,
    /// Degrees between major ticks. Default 15.
    #[serde(default = "default_major_tick_interval")]
    pub major_tick_interval: u32,
    /// Subdivisions between major ticks. Default 3 (= every 5°).
    #[serde(default = "default_minor_ticks_per_major")]
    pub minor_ticks_per_major: u32,
    /// Show major ticks.
    #[serde(default = "default_true")]
    pub show_major_ticks: bool,
    /// Show minor ticks.
    #[serde(default = "default_true")]
    pub show_minor_ticks: bool,
    /// Major tick length as percentage of widget height.
    #[serde(default = "default_major_tick_length")]
    pub major_tick_length_pct: f32,
    /// Minor tick length as percentage of widget height.
    #[serde(default = "default_minor_tick_length")]
    pub minor_tick_length_pct: f32,
    /// Major tick thickness in pixels.
    #[serde(default = "default_major_tick_thickness")]
    pub major_tick_thickness: f32,
    /// Minor tick thickness in pixels.
    #[serde(default = "default_minor_tick_thickness")]
    pub minor_tick_thickness: f32,
    /// Regular (non-cardinal) tick color as hex.
    #[serde(default)]
    pub tick_color: Option<String>,
    /// Cardinal tick color (N/NE/E/SE/S/SW/W/NW) as hex.
    #[serde(default)]
    pub cardinal_tick_color: Option<String>,
    /// Tick alignment: `"below"` or `"centered"`.
    #[serde(default = "default_tick_alignment")]
    pub tick_alignment: String,
    /// Shadow distance override for all elements.
    #[serde(default)]
    pub shadow_distance: Option<f32>,
    /// Shadow strength override for all elements.
    #[serde(default)]
    pub shadow_strength: Option<f32>,
    /// Shadow color override for all elements.
    #[serde(default)]
    pub shadow_color: Option<String>,
    /// Show minor degree labels.
    #[serde(default = "default_true", alias = "show_numeric_labels")]
    pub show_minor_labels: bool,
    /// Show major labels (N/NE/E/SE/S/SW/W/NW).
    #[serde(default = "default_true", alias = "show_cardinal_labels")]
    pub show_major_labels: bool,
    /// Label color for non-cardinal numeric labels as hex.
    #[serde(default, alias = "numeric_label_color", alias = "minor_label_color")]
    pub label_color: Option<String>,
    /// Cardinal label color (N/NE/E/SE/S/SW/W/NW) as hex.
    #[serde(default, alias = "major_label_color")]
    pub cardinal_label_color: Option<String>,
    /// Label font filename or family.
    #[serde(default)]
    pub label_font: Option<String>,
    /// Alternate label font family field used by the editor.
    #[serde(default)]
    pub label_font_family: Option<String>,
    /// Label font size in pixels.
    #[serde(default)]
    pub label_font_size: Option<f32>,
    /// Distance from bottom of ticks to label baseline in pixels.
    #[serde(default)]
    pub label_offset: Option<f32>,
    /// Indicator style: `"chevron"` or `"highlight_bar"`.
    #[serde(default = "default_indicator_style")]
    pub indicator_style: String,
    /// Indicator placement: `"top"`, `"bottom"`, or `"both"`.
    #[serde(default = "default_indicator_placement")]
    pub indicator_placement: String,
    /// Show the indicator.
    #[serde(default = "default_true")]
    pub show_indicator: bool,
    /// Indicator color as hex.
    #[serde(default)]
    pub indicator_color: Option<String>,
    /// Indicator size in pixels (chevron height or bar width).
    #[serde(default)]
    pub indicator_size: Option<f32>,
    /// Unknown fields preserved for forward compatibility.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_one() -> f32 {
    1.0
}
fn default_ppd() -> f32 {
    5.0
}
fn default_major_tick_interval() -> u32 {
    15
}
fn default_minor_ticks_per_major() -> u32 {
    3
}
fn default_true() -> bool {
    true
}
fn default_major_tick_length() -> f32 {
    40.0
}
fn default_minor_tick_length() -> f32 {
    20.0
}
fn default_major_tick_thickness() -> f32 {
    2.0
}
fn default_minor_tick_thickness() -> f32 {
    2.0
}
fn default_tick_alignment() -> String {
    "below".to_string()
}
fn default_indicator_style() -> String {
    "chevron".to_string()
}
fn default_indicator_placement() -> String {
    "top".to_string()
}

/// Parses and validates render configuration JSON.
///
/// Validation focuses on constraints that would otherwise break frame timing:
/// positive integer FPS, update-rate divisibility, and non-empty scene ranges.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_json(input: &str) -> CoreResult<RenderConfig> {
    let config: RenderConfig = serde_json::from_str(input)
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
    if config.scene.end <= config.scene.start {
        return Err(CoreError::Config(format!(
            "scene range. scene.end ({}) must be greater than scene.start ({})",
            config.scene.end, config.scene.start
        )));
    }
    Ok(config)
}

/// Parses either a raw render config or a wrapped OVRLEY template file.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_template_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("template JSON: {error}")))?;

    let Some(format) = value.get("format").and_then(Value::as_str) else {
        return parse_config_json(input);
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

    let config_value = value
        .get("config")
        .cloned()
        .ok_or_else(|| CoreError::Config("template config missing".into()))?;
    let config_json = serde_json::to_string(&config_value)
        .map_err(|error| CoreError::Config(format!("template config JSON: {error}")))?;
    let mut config = parse_config_json(&config_json)?;
    apply_template_global_defaults(&mut config, &value);
    Ok(config)
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

impl RenderConfig {
    /// Returns the frame decimation factor used for the encoded video stream.
    pub fn widget_update_rate(&self) -> u32 {
        self.scene.update_rate.unwrap_or(1).max(1)
    }

    /// Returns the ffmpeg container FPS after applying `scene.update_rate`.
    pub fn container_fps(&self) -> f64 {
        self.scene.fps / f64::from(self.widget_update_rate())
    }

    /// Computes which telemetry series are required by this configuration.
    ///
    /// Metric values enable their direct series. Plot widgets also request
    /// distance progress and any source series required to build their geometry.
    pub fn render_data_requirements(&self) -> CoreResult<RenderDataRequirements> {
        let mut requirements = RenderDataRequirements::default();

        for value in &self.values {
            match value.value {
                MetricKind::Speed => requirements.speed = true,
                MetricKind::Elevation => requirements.elevation = true,
                MetricKind::Gradient => requirements.gradient = true,
                MetricKind::Heartrate => requirements.heartrate = true,
                MetricKind::Cadence => requirements.cadence = true,
                MetricKind::Power => requirements.power = true,
                MetricKind::Temperature => requirements.temperature = true,
                MetricKind::Pace => requirements.pace = true,
                MetricKind::GForce => requirements.g_force = true,
                MetricKind::AirPressure => requirements.air_pressure = true,
                MetricKind::GroundContactTime => requirements.ground_contact_time = true,
                MetricKind::LeftRightBalance => requirements.left_right_balance = true,
                MetricKind::StrideLength => requirements.stride_length = true,
                MetricKind::StrokeRate => requirements.stroke_rate = true,
                MetricKind::Torque => requirements.torque = true,
                MetricKind::VerticalSpeed => requirements.vertical_speed = true,
                MetricKind::GearPosition => requirements.gear_position = true,
                MetricKind::VerticalRatio => requirements.vertical_ratio = true,
                MetricKind::VerticalOscillation => requirements.vertical_oscillation = true,
                MetricKind::CoreTemperature => requirements.core_temperature = true,
                MetricKind::Heading => requirements.heading = true,
                MetricKind::Time => requirements.time = true,
            }
        }

        if self.course_plot()?.is_some() {
            requirements.distance_progress = true;
        }

        if self.elevation_plot()?.is_some() {
            requirements.elevation = true;
            requirements.distance_progress = true;
        }

        if self.heading_plot()?.is_some() {
            requirements.heading = true;
        }

        Ok(requirements)
    }

    /// Returns the course plot config if present.
    pub fn course_plot(&self) -> CoreResult<Option<CoursePlotConfig>> {
        self.parse_plot("course")
    }

    /// Returns the elevation plot config if present.
    pub fn elevation_plot(&self) -> CoreResult<Option<ElevationPlotConfig>> {
        self.parse_plot("elevation")
    }

    /// Returns the heading plot config if present.
    pub fn heading_plot(&self) -> CoreResult<Option<HeadingWidgetConfig>> {
        self.parse_plot("heading")
    }

    /// Parses one plot entry from the legacy object/array `plots` container.
    fn parse_plot<T>(&self, value_key: &str) -> CoreResult<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let Some(raw_plot) = find_plot_value(&self.plots, value_key) else {
            return Ok(None);
        };
        serde_json::from_value(raw_plot.clone())
            .map(Some)
            .map_err(|error| CoreError::Config(format!("{value_key} plot config: {error}")))
    }
}

// Finds a plot config by value key in either array or object-shaped template data.
fn find_plot_value<'a>(plots: &'a Value, value_key: &str) -> Option<&'a Value> {
    // The editor has emitted both array-based and object-based plot containers
    // over time. Search both shapes so old templates stay usable.
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
