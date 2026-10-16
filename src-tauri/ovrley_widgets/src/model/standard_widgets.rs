//! Runtime typography and icon metadata from the canonical widget manifest.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetTypography {
    widget_font_weight: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetIcon {
    asset_file: Option<String>,
}

#[derive(Deserialize)]
struct WidgetDefinition {
    #[serde(rename = "type")]
    kind: String,
    icon: WidgetIcon,
}

#[derive(Deserialize)]
struct WidgetManifest {
    typography: WidgetTypography,
    definitions: Vec<WidgetDefinition>,
}

struct RuntimeWidgetMetadata {
    widget_font_weight: f32,
    time_icon_asset_file: String,
}

static WIDGET_METADATA: OnceLock<RuntimeWidgetMetadata> = OnceLock::new();

fn metadata() -> &'static RuntimeWidgetMetadata {
    WIDGET_METADATA.get_or_init(|| {
        let manifest: WidgetManifest = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/standard-widgets.json"
        )))
        .expect("shared widget manifest must be valid JSON");
        let time = manifest
            .definitions
            .into_iter()
            .find(|definition| definition.kind == "time")
            .expect("widget manifest must define time");
        RuntimeWidgetMetadata {
            widget_font_weight: manifest.typography.widget_font_weight,
            time_icon_asset_file: time
                .icon
                .asset_file
                .expect("time icon must declare assetFile"),
        }
    })
}

/// Shared weight for text outside label widgets.
pub fn widget_font_weight() -> f32 {
    metadata().widget_font_weight
}

pub fn time_icon_asset_file() -> &'static str {
    &metadata().time_icon_asset_file
}
