use crate::{
    error::{WidgetError, WidgetResult},
    model::types::{BackdropEdge, BackdropType},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Static geometric backdrop drawn below other widgets.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
    pub diameter: Option<f32>,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
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
    #[serde(default)]
    pub edge_strips: Option<Vec<BackdropEdge>>,
    #[serde(default)]
    pub edge_strip_thickness: Option<f32>,
    #[serde(default)]
    pub edge_strip_color: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl BackdropConfig {
    /// Resolve saved active-variant fields at ingress. Explicit flat fields win.
    pub(super) fn with_promoted_display_variant(&self, variant_key: &str) -> WidgetResult<Self> {
        let mut raw = serde_json::to_value(self)
            .map_err(|e| WidgetError::Config(format!("backdrop config serialization: {e}")))?;
        // Option fields serialize as null; omit them before promoting saved geometry.
        raw.as_object_mut()
            .expect("serialized backdrop is an object")
            .retain(|_, value| !value.is_null());
        if let Some(variant) = self
            .display_variants
            .get(variant_key)
            .and_then(Value::as_object)
        {
            let root = raw
                .as_object_mut()
                .expect("serialized backdrop is an object");
            for (key, value) in variant {
                root.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        serde_json::from_value(raw)
            .map_err(|e| WidgetError::Config(format!("backdrop config: {e}")))
    }
}
