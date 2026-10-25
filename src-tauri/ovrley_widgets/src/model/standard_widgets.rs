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

/// Editor and renderer bounds of the rolling chart's series list and time window.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartLimits {
    pub max_series: usize,
    pub min_window_seconds: f32,
    pub max_window_seconds: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetDefinition {
    #[serde(rename = "type")]
    kind: String,
    icon: WidgetIcon,
    /// Declared only by `rolling_chart`.
    #[serde(default)]
    chart_limits: Option<ChartLimits>,
}

#[derive(Deserialize)]
struct WidgetManifest {
    typography: WidgetTypography,
    definitions: Vec<WidgetDefinition>,
}

struct RuntimeWidgetMetadata {
    widget_font_weight: f32,
    time_icon_asset_file: String,
    chart_limits: ChartLimits,
}

static WIDGET_METADATA: OnceLock<RuntimeWidgetMetadata> = OnceLock::new();

fn metadata() -> &'static RuntimeWidgetMetadata {
    WIDGET_METADATA.get_or_init(|| {
        let manifest: WidgetManifest = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/standard-widgets.json"
        )))
        .expect("shared widget manifest must be valid JSON");
        let definition = |kind: &str| {
            manifest
                .definitions
                .iter()
                .find(|definition| definition.kind == kind)
                .unwrap_or_else(|| panic!("widget manifest must define {kind}"))
        };
        RuntimeWidgetMetadata {
            widget_font_weight: manifest.typography.widget_font_weight,
            time_icon_asset_file: definition("time")
                .icon
                .asset_file
                .clone()
                .expect("time icon must declare assetFile"),
            chart_limits: definition("rolling_chart")
                .chart_limits
                .expect("rolling_chart must declare chartLimits"),
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

pub fn chart_limits() -> ChartLimits {
    metadata().chart_limits
}
