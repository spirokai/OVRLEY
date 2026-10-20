use crate::error::{WidgetError, WidgetResult};
use crate::model::validation::{
    require_f32, require_non_negative_f32, require_positive_f32, require_positive_u32,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Shared presentation ingress, independent of an activity or export window.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
    /// Optional; absent scenes predate the switch and use its default, fixed-width digits.
    #[serde(default)]
    pub fixed_width_digits: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Digit placement for changing widget values; static text keeps the face's advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numerals {
    Default,
    /// Each digit's ink centered in a cell as wide as the face's widest digit.
    FixedWidth,
}

/// Shared presentation validated once before per-job timing is available.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedScenePresentation {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub font: Option<String>,
    pub font_size: Option<f32>,
    pub opacity: f32,
    pub decimal_rounding: Option<i32>,
    pub time_format: Option<String>,
    pub shadow_color: String,
    pub shadow_strength: f32,
    pub shadow_distance: f32,
    pub border_color: String,
    pub border_thickness: f32,
    pub numerals: Numerals,
    pub overlay_filename: Option<String>,
}

pub fn validate_scene_presentation(
    raw: ScenePresentationConfig,
) -> WidgetResult<ValidatedScenePresentation> {
    let width = require_positive_u32(raw.width, "scene.width")?;
    let height = require_positive_u32(raw.height, "scene.height")?;
    let scale = require_positive_f32(raw.scale, "scene.scale")?;
    let opacity = require_f32(raw.opacity, "scene.opacity")?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(WidgetError::Config(
            "scene.opacity must be between 0 and 1".into(),
        ));
    }

    let shadow_strength = require_f32(raw.shadow_strength, "scene.shadow_strength")?;
    require_non_negative_f32(shadow_strength, "scene.shadow_strength")?;
    let shadow_distance = require_f32(raw.shadow_distance, "scene.shadow_distance")?;
    require_non_negative_f32(shadow_distance, "scene.shadow_distance")?;
    let shadow_color = crate::model::validation::require_hex_color(
        raw.shadow_color.as_deref(),
        "scene.shadow_color",
    )?;
    let border_thickness = require_f32(raw.border_thickness, "scene.border_thickness")?;
    require_non_negative_f32(border_thickness, "scene.border_thickness")?;
    let border_color = crate::model::validation::require_hex_color(
        raw.border_color.as_deref(),
        "scene.border_color",
    )?;

    Ok(ValidatedScenePresentation {
        width,
        height,
        scale,
        font: raw.font,
        font_size: raw.font_size,
        opacity,
        decimal_rounding: raw.decimal_rounding,
        time_format: raw.time_format,
        shadow_color,
        shadow_strength,
        shadow_distance,
        border_color,
        border_thickness,
        numerals: if raw.fixed_width_digits == Some(false) {
            Numerals::Default
        } else {
            Numerals::FixedWidth
        },
        overlay_filename: raw.overlay_filename,
    })
}
