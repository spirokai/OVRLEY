//! Raw configuration types deserialized directly from frontend JSON.
//!
//! These types intentionally allow extra fields (`#[serde(flatten)]`) so
//! templates can remain forward-compatible across app versions. The
//! normalize layer consumes these types and produces validated equivalents.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::error::{CoreError, CoreResult};
use crate::types::{BackdropType, DisplayType, MetricKind, TrackFillStyle};

pub const TEMPLATE_FILE_FORMAT: &str = "ovrley-template";
pub const TEMPLATE_FILE_VERSION: u32 = 3;

/// One typography contract for render requests and saved-document ingress.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LabelTypography {
    #[serde(deserialize_with = "deserialize_font_weight")]
    pub font_weight: f32,
    #[serde(deserialize_with = "deserialize_italic")]
    pub italic: bool,
    /// Percentage of font size between grapheme clusters; no trailing gap.
    #[serde(deserialize_with = "deserialize_letter_spacing")]
    pub letter_spacing: f32,
}

fn deserialize_font_weight<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f32, D::Error> {
    let weight = f64::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("font_weight: {error}")))?;
    // Validate the original number before narrowing to Skia's f32 coordinates.
    if !weight.is_finite() || !(1.0..=1000.0).contains(&weight) {
        return Err(serde::de::Error::custom(
            "font_weight: must be a finite number from 1 to 1000",
        ));
    }
    Ok(weight as f32)
}

fn deserialize_italic<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    bool::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("italic: {error}")))
}

fn deserialize_letter_spacing<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<f32, D::Error> {
    let spacing = f64::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("letter_spacing: {error}")))?;
    if !spacing.is_finite() || !(spacing as f32).is_finite() {
        return Err(serde::de::Error::custom(
            "letter_spacing: must be a finite 32-bit number",
        ));
    }
    Ok(spacing as f32)
}

/// Global render settings shared by labels, metric values, plots, and ffmpeg.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    #[serde(flatten)]
    pub presentation: ScenePresentationConfig,
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    #[serde(default, alias = "updateRate")]
    pub update_rate: Option<u32>,
    #[serde(default, skip_serializing)]
    pub composite_video_path: Option<String>,
    #[serde(default, rename = "qualityType", skip_serializing)]
    pub quality_type: Option<crate::encode::quality::QualityType>,
    #[serde(default, rename = "qualityValue", skip_serializing)]
    pub quality_value: Option<f64>,
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
    pub custom_export_range_active: Option<bool>,
}

/// Shared presentation ingress, independent of an activity or export window.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScenePresentationConfig {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub decimal_rounding: Option<i32>,
    #[serde(default)]
    pub overlay_filename: Option<String>,
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
    #[serde(flatten)]
    pub typography: LabelTypography,
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

/// Static geometric backdrop drawn below other widgets.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackdropConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub opacity: Option<f32>,
    pub display_type: BackdropType,
    #[serde(default)]
    pub fill_color: Option<String>,
    #[serde(default)]
    pub fill_opacity: Option<f32>,
    #[serde(default)]
    pub border_thickness: Option<f32>,
    #[serde(default)]
    pub border_color: Option<String>,
    #[serde(default)]
    pub border_opacity: Option<f32>,
    #[serde(default)]
    pub display_variants: BTreeMap<String, Value>,
    #[serde(default)]
    pub diameter: Option<u32>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub corner_radius: Option<f32>,
    #[serde(default)]
    pub round_top_left: Option<bool>,
    #[serde(default)]
    pub round_top_right: Option<bool>,
    #[serde(default)]
    pub round_bottom_left: Option<bool>,
    #[serde(default)]
    pub round_bottom_right: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl BackdropConfig {
    /// Promotes nested backdrop variant fields to the top level so validators
    /// can consume them as flat fields while durable templates store geometry
    /// under `display_variants.<key>`.
    pub(crate) fn with_promoted_display_variant(
        &self,
        variant_key: &str,
    ) -> CoreResult<BackdropConfig> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| CoreError::Config(format!("backdrop config serialization: {e}")))?;
        strip_json_nulls(&mut raw);
        promote_variant_keys(&mut raw, variant_key);

        serde_json::from_value(raw).map_err(|e| CoreError::Config(format!("backdrop config: {e}")))
    }
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
    pub positive_delta_color: Option<String>,
    #[serde(default)]
    pub negative_delta_color: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub rotation: Option<f32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub arc_angle: Option<f32>,
    #[serde(default)]
    pub corner_orientation: Option<String>,
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
    pub track_fill_flat: Option<bool>,
    #[serde(default)]
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
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl ValueConfig {
    /// Promotes nested display variant fields to the top level so validators
    /// can consume them as flat fields. Used when a JS-normalized config stores
    /// variant-specific keys inside `display_variants.<key>` — Rust deserializes
    /// the raw JSON, strips nulls, and promotes the nested object up.
    pub(crate) fn with_promoted_display_variant(
        &self,
        variant_key: &str,
    ) -> CoreResult<ValueConfig> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| CoreError::Config(format!("value config serialization: {e}")))?;
        strip_json_nulls(&mut raw);
        promote_variant_keys(&mut raw, variant_key);

        serde_json::from_value(raw).map_err(|e| CoreError::Config(format!("value config: {e}")))
    }

    pub(crate) fn to_heading_widget_config(&self) -> CoreResult<HeadingWidgetConfig> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| CoreError::Config(format!("heading value serialization: {e}")))?;
        strip_json_nulls(&mut raw);

        // Promote all keys from display_variants.heading_tape to the top level
        // so HeadingWidgetConfig can deserialize. The JS durable normalization
        // stores heading-specific fields inside display_variants so they
        // survive round-tripping alongside text defaults.
        promote_variant_keys(&mut raw, "heading_tape");

        serde_json::from_value(raw)
            .map_err(|e| CoreError::Config(format!("heading value config: {e}")))
    }
}

/// Promotes all keys from a nested display variant to the top level if they
/// are not already present at the root. This ensures display-specific fields
/// (e.g. width, height, pixels_per_degree, tick_color, etc.) are visible to
/// validators that expect them at the top level.
fn promote_variant_keys(raw: &mut serde_json::Value, variant_key: &str) {
    let mut promotions: Vec<(String, serde_json::Value)> = Vec::new();

    if let Some(variants) = raw.get("display_variants") {
        if let Some(variant) = variants.get(variant_key) {
            if let Some(obj) = variant.as_object() {
                for (key, value) in obj {
                    if raw.get(key).is_none() {
                        promotions.push((key.clone(), value.clone()));
                    }
                }
            }
        }
    }

    if let Some(obj) = raw.as_object_mut() {
        for (k, v) in promotions {
            obj.insert(k, v);
        }
    }
}

/// Complete template render configuration.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RasterConfig {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub opacity: f64,
    #[serde(deserialize_with = "deserialize_raster_path")]
    pub path: Option<String>,
    #[serde(
        default,
        rename = "resourceId",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_id: Option<String>,
    #[serde(
        default,
        rename = "resourceErrorCode",
        deserialize_with = "deserialize_present_resource",
        skip_serializing_if = "Option::is_none"
    )]
    pub resource_error_code: Option<crate::raster::RasterError>,
}

fn deserialize_present_resource<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn deserialize_raster_path<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig<S = SceneConfig> {
    pub scene: S,
    #[serde(default)]
    pub backdrops: Vec<BackdropConfig>,
    #[serde(default, deserialize_with = "deserialize_render_rasters")]
    pub rasters: Vec<RasterConfig>,
    #[serde(default)]
    pub labels: Vec<LabelConfig>,
    #[serde(default)]
    pub values: Vec<ValueConfig>,
    #[serde(default)]
    pub plots: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Materialized presentation requires explicit collections at JSON ingress.
/// Saved/single config optional collections retain their existing contract.
pub(crate) fn deserialize_render_presentation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<RenderConfig<ScenePresentationConfig>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    for field in ["scene", "backdrops", "rasters", "labels", "values", "plots"] {
        if value.get(field).is_none() {
            return Err(serde::de::Error::missing_field(field));
        }
    }
    if !value["plots"].is_array() {
        return Err(serde::de::Error::custom(
            "batch template.plots must be an array",
        ));
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

impl<S> RenderConfig<S> {
    pub(crate) fn with_scene<T>(self, scene: T) -> RenderConfig<T> {
        RenderConfig {
            scene,
            backdrops: self.backdrops,
            rasters: self.rasters,
            labels: self.labels,
            values: self.values,
            plots: self.plots,
            extra: self.extra,
        }
    }
}

fn deserialize_render_rasters<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RasterConfig>, D::Error> {
    use serde::de::Error;
    Vec::<RasterConfig>::deserialize(deserializer).map_err(|error| {
        D::Error::custom(format!(
            "rasters: {error} [raster_error:invalid_config:configuration]"
        ))
    })
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
    pub starting_altitude: Option<f64>,
    #[serde(default)]
    pub starting_altitude_unit: Option<String>,
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
    #[serde(default)]
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

/// Finds every plot config with the requested value key, preserving array order.
pub fn find_plot_values<'a>(plots: &'a Value, value_key: &str) -> Vec<(usize, &'a Value)> {
    match plots {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.get("value").and_then(Value::as_str) == Some(value_key))
            .collect(),
        Value::Object(_) => find_plot_value(plots, value_key)
            .map(|plot| vec![(0, plot)])
            .unwrap_or_default(),
        _ => Vec::new(),
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
// Parsing — deserialization; normalization owns semantic validation
// ---------------------------------------------------------------------------

/// Deserializes render configuration JSON; submission owns normalization.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_json(input: &str) -> CoreResult<RenderConfig> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))?;
    parse_config_value(&value)
}

/// Parses a raw render config from a pre-built JSON value.
#[must_use = "parsed config must be consumed for rendering"]
pub fn parse_config_value(value: &Value) -> CoreResult<RenderConfig> {
    serde_json::from_value(value.clone())
        .map_err(|error| CoreError::Config(format!("config JSON: {error}")))
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

    if version != 2 && version != u64::from(TEMPLATE_FILE_VERSION) {
        return Err(CoreError::Config(format!(
            "unsupported template version: {version}. expected {TEMPLATE_FILE_VERSION}"
        )));
    }

    let mut migrated = value.clone();
    migrate_saved_font_input(&mut migrated);
    let mut config_value = migrated
        .get("config")
        .cloned()
        .ok_or_else(|| CoreError::Config("template config missing".into()))?;
    if version == 2 {
        let config = config_value
            .as_object_mut()
            .ok_or_else(|| CoreError::Config("template config must be an object".into()))?;
        if config.contains_key("rasters") {
            return Err(CoreError::Config(
                "version 2 template cannot contain rasters".into(),
            ));
        }
        config.insert("rasters".into(), Value::Array(Vec::new()));
    }
    config_value
        .get("rasters")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Config("template config.rasters must be an array".into()))?;
    materialize_template_scene_defaults(&mut config_value, &migrated);
    let mut config = parse_config_value(&config_value)?;
    apply_template_global_defaults(&mut config, &migrated);
    Ok(config)
}

/// Upgrades legacy saved documents in memory, including development v3 files.
/// Present typography values are preserved; validation belongs to the load boundary.
pub fn migrate_saved_font_input(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if let Some(labels) = object.get_mut("labels").and_then(Value::as_array_mut) {
                for label in labels {
                    if let Some(label) = label.as_object_mut() {
                        label.entry("font_weight").or_insert(Value::from(400));
                        label.entry("italic").or_insert(Value::Bool(false));
                        label.entry("letter_spacing").or_insert(Value::from(0));
                    }
                }
            }
            for (key, value) in object {
                match (key.as_str(), value.as_str()) {
                    (
                        "font" | "label_font" | "min_max_label_font" | "font_text" | "font_values",
                        Some("Inter ExtraBold.ttf" | "Inter ExtraBold"),
                    ) => *value = Value::from("Inter.ttf"),
                    ("font_family", Some("Inter ExtraBold")) => *value = Value::from("Inter"),
                    _ => migrate_saved_font_input(value),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                migrate_saved_font_input(item);
            }
        }
        _ => {}
    }
}

/// Validates project-owned label typography once at its JSON load boundary.
pub fn validate_saved_label_typography(config: &Value) -> CoreResult<()> {
    let labels = config
        .get("labels")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Config("labels must be an array".into()))?;
    for (index, label) in labels.iter().enumerate() {
        serde_json::from_value::<LabelTypography>(label.clone())
            .map_err(|error| CoreError::Config(format!("labels[{index}].{error}")))?;
    }
    Ok(())
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
