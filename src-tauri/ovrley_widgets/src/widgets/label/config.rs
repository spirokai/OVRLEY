use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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

pub(crate) fn deserialize_font_weight<'de, D: serde::Deserializer<'de>>(
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
