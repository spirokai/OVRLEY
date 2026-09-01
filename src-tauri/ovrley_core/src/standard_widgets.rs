//! Shared standard-widget definitions loaded from the repo manifest.
//!
//! The canonical widget contract lives in `assets/standard-widgets.json` so
//! frontend and backend share one source of truth for non-metric widget
//! defaults and backdrop display-type definitions.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub struct StandardWidgetDefinition {
    pub defaults: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackdropTypeDefinition {
    pub label: String,
    pub defaults: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardWidgetDefinition {
    defaults: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBackdropTypeDefinition {
    label: String,
    defaults: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardWidgetSection {
    definitions: HashMap<String, RawStandardWidgetDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBackdropSection {
    definitions: HashMap<String, RawBackdropTypeDefinition>,
    defaults: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardWidgetManifest {
    plot: RawStandardWidgetSection,
    gradient: RawStandardWidgetSection,
    label: RawStandardWidgetSection,
    backdrops: RawBackdropSection,
}

#[derive(Clone, Debug)]
struct StandardWidgetSection {
    definitions: HashMap<String, StandardWidgetDefinition>,
}

impl StandardWidgetSection {
    fn from_raw(raw: RawStandardWidgetSection) -> Self {
        let definitions = raw
            .definitions
            .into_iter()
            .map(|(key, definition)| {
                (
                    key,
                    StandardWidgetDefinition {
                        defaults: definition.defaults,
                    },
                )
            })
            .collect();

        Self { definitions }
    }
}

#[derive(Clone, Debug)]
struct BackdropSection {
    definitions: HashMap<String, BackdropTypeDefinition>,
    defaults: Vec<String>,
}

impl BackdropSection {
    fn from_raw(raw: RawBackdropSection) -> Self {
        let definitions = raw
            .definitions
            .into_iter()
            .map(|(key, definition)| {
                (
                    key,
                    BackdropTypeDefinition {
                        label: definition.label,
                        defaults: definition.defaults,
                    },
                )
            })
            .collect();

        Self {
            definitions,
            defaults: raw.defaults,
        }
    }
}

#[derive(Clone, Debug)]
struct StandardWidgetManifest {
    plot: StandardWidgetSection,
    gradient: StandardWidgetSection,
    label: StandardWidgetSection,
    backdrops: BackdropSection,
}

static STANDARD_WIDGET_MANIFEST: OnceLock<StandardWidgetManifest> = OnceLock::new();

fn manifest() -> &'static StandardWidgetManifest {
    STANDARD_WIDGET_MANIFEST.get_or_init(load_manifest)
}

fn load_manifest() -> StandardWidgetManifest {
    let raw = serde_json::from_str::<RawStandardWidgetManifest>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/standard-widgets.json"
    )))
    .expect("shared standard widgets manifest must be valid JSON");

    StandardWidgetManifest {
        plot: StandardWidgetSection::from_raw(raw.plot),
        gradient: StandardWidgetSection::from_raw(raw.gradient),
        label: StandardWidgetSection::from_raw(raw.label),
        backdrops: BackdropSection::from_raw(raw.backdrops),
    }
}

pub fn plot_widget_definition(key: &str) -> Option<&'static StandardWidgetDefinition> {
    manifest().plot.definitions.get(key)
}

pub fn gradient_widget_definition(key: &str) -> Option<&'static StandardWidgetDefinition> {
    manifest().gradient.definitions.get(key)
}

pub fn label_widget_definition(key: &str) -> Option<&'static StandardWidgetDefinition> {
    manifest().label.definitions.get(key)
}

pub fn backdrop_type_definition(display_type: &str) -> Option<&'static BackdropTypeDefinition> {
    manifest().backdrops.definitions.get(display_type)
}

pub fn backdrop_type_label(display_type: &str) -> &str {
    manifest()
        .backdrops
        .definitions
        .get(display_type)
        .map(|definition| definition.label.as_str())
        .unwrap_or(display_type)
}

pub fn default_backdrop_display_types() -> &'static [String] {
    manifest().backdrops.defaults.as_slice()
}

pub fn is_backdrop_type_supported(display_type: &str) -> bool {
    manifest().backdrops.definitions.contains_key(display_type)
}
