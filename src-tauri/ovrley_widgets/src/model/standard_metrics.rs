//! Shared standard-metric widget definitions loaded from the repo manifest.
//!
//! The canonical standard-metric contract lives in `assets/standard-metrics.json`
//! so the frontend and backend share one source of truth for labels, display
//! units, formatter families, and icon asset bindings. `time`, `gradient`, and
//! `elevation` remain specialized render paths outside this contract.

use crate::model::types::DisplayType;
use crate::model::types::MetricKind;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandardMetricFormatterKind {
    Speed,
    Temperature,
    Pace,
    Integer,
    Decimal,
    Balance,
    Shutter,
    Aperture,
    Ev,
    Gear,
    Coordinates,
    Distance,
    Elevation,
    LapTimer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandardMetricUnitsMode {
    Selectable,
    Hidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardMetricUnitOption {
    pub value: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub label_key: Option<String>,
    #[serde(default)]
    pub default_label: Option<String>,
    #[serde(default)]
    pub render_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardMetricIconDefinition {
    pub source: String,
    #[serde(default)]
    pub name: Option<String>,
    pub asset_file: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardMetricDefinition {
    pub default_display_unit: String,
    pub supported_display_units: Vec<StandardMetricUnitOption>,
    pub formatter: StandardMetricFormatterKind,
    pub icon: StandardMetricIconDefinition,
    pub units_mode: StandardMetricUnitsMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardMetricDefinition {
    #[serde(rename = "type")]
    kind: MetricKind,
    current: bool,
    defaults: RawMetricDefaults,
    supported_display_units: Vec<StandardMetricUnitOption>,
    formatter: StandardMetricFormatterKind,
    icon: StandardMetricIconDefinition,
    units_mode: StandardMetricUnitsMode,
    #[serde(default)]
    display_types: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct RawMetricDefaults {
    display_unit: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDisplayTypeDefinition {
    layout_mode: DisplayTypeLayoutMode,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDisplayTypeManifest {
    definitions: HashMap<String, RawDisplayTypeDefinition>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetricWidgetDefaults {
    display_types: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardMetricManifest {
    display_types: RawDisplayTypeManifest,
    widget_defaults: RawMetricWidgetDefaults,
    definitions: Vec<RawStandardMetricDefinition>,
}

#[derive(Clone, Debug)]
struct DisplayTypeManifest {
    definitions: HashMap<String, RawDisplayTypeDefinition>,
    defaults: Vec<String>,
    overrides: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
struct StandardMetricManifest {
    definitions: HashMap<MetricKind, StandardMetricDefinition>,
    display_types: DisplayTypeManifest,
}

static STANDARD_METRIC_MANIFEST: OnceLock<StandardMetricManifest> = OnceLock::new();

fn manifest() -> &'static StandardMetricManifest {
    STANDARD_METRIC_MANIFEST.get_or_init(load_manifest)
}

#[derive(Deserialize)]
struct RawWidgetDefaults {
    display_type: Option<String>,
}

#[derive(Deserialize)]
struct RawWidgetDefinition {
    #[serde(rename = "type")]
    key: String,
    defaults: RawWidgetDefaults,
}

#[derive(Deserialize)]
struct RawWidgetManifest {
    definitions: Vec<RawWidgetDefinition>,
}

fn load_manifest() -> StandardMetricManifest {
    let raw: RawStandardMetricManifest =
        serde_json::from_str(include_str!("../../../../assets/standard-metrics.json"))
            .expect("shared standard metrics manifest must be valid JSON");
    let widgets: RawWidgetManifest =
        serde_json::from_str(include_str!("../../../../assets/standard-widgets.json"))
            .expect("shared widget manifest must be valid JSON");
    let mut overrides = HashMap::new();
    for definition in widgets.definitions {
        if let Some(display_type) = definition.defaults.display_type {
            overrides.insert(definition.key, vec![display_type]);
        }
    }
    let mut definitions = HashMap::new();
    for definition in raw.definitions {
        if let Some(display_types) = definition.display_types {
            overrides.insert(definition.kind.as_str().to_owned(), display_types);
        }
        if definition.current {
            definitions.insert(
                definition.kind,
                StandardMetricDefinition {
                    default_display_unit: definition.defaults.display_unit,
                    supported_display_units: definition.supported_display_units,
                    formatter: definition.formatter,
                    icon: definition.icon,
                    units_mode: definition.units_mode,
                },
            );
        }
    }
    StandardMetricManifest {
        definitions,
        display_types: DisplayTypeManifest {
            definitions: raw.display_types.definitions,
            defaults: raw.widget_defaults.display_types,
            overrides,
        },
    }
}

pub fn standard_metric_definition(kind: MetricKind) -> Option<&'static StandardMetricDefinition> {
    manifest().definitions.get(&kind)
}

pub fn is_standard_metric(kind: MetricKind) -> bool {
    standard_metric_definition(kind).is_some()
}

pub fn standard_metric_formatter(kind: MetricKind) -> Option<StandardMetricFormatterKind> {
    standard_metric_definition(kind).map(|definition| definition.formatter)
}

pub fn standard_metric_units_mode(kind: MetricKind) -> Option<StandardMetricUnitsMode> {
    standard_metric_definition(kind).map(|definition| definition.units_mode)
}

pub fn standard_metric_unit_label(kind: MetricKind, display_unit: Option<&str>) -> &'static str {
    let definition = match standard_metric_definition(kind) {
        Some(definition) => definition,
        None => return "",
    };
    let resolved = display_unit.unwrap_or(definition.default_display_unit.as_str());

    definition
        .supported_display_units
        .iter()
        .find(|option| option.value == resolved)
        .map(|option| {
            option
                .render_label
                .as_deref()
                .or(option.label.as_deref())
                .or(option.default_label.as_deref())
                .unwrap_or("")
        })
        .unwrap_or("")
}

// ---------------------------------------------------------------------------
// Display type helpers (sourced from assets/standard-metrics.json)
// ---------------------------------------------------------------------------

/// Whether a display type uses intrinsic (text) or boxed (framed) layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayTypeLayoutMode {
    Intrinsic,
    Boxed,
}

/// Layout mode for a validated display type. Every native display must be declared.
pub fn display_type_layout_mode(display_type: DisplayType) -> DisplayTypeLayoutMode {
    manifest().display_types.definitions[display_type.as_str()].layout_mode
}

/// Return the permitted display types for a given metric kind.
///
/// If the manifest contains an override for the metric, that list is returned;
/// otherwise the global defaults are returned.
pub fn supported_display_types(kind: MetricKind) -> &'static [String] {
    let m = manifest();
    let key = kind.as_str();
    if let Some(override_list) = m.display_types.overrides.get(key) {
        override_list.as_slice()
    } else {
        m.display_types.defaults.as_slice()
    }
}

/// Check whether a given `display_type` value is permitted for a metric kind.
pub fn is_display_type_supported(kind: MetricKind, display_type: &str) -> bool {
    supported_display_types(kind)
        .iter()
        .any(|dt| dt == display_type)
}
