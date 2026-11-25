//! The compiled widget and display catalog: the one reader of `assets/standard-widgets.json`
//! and `assets/standard-metrics.json`, and the metric and display enums they define.
//!
//! Compiling resolves the manifests' inheritance once: widget and display defaults layered over
//! the manifest-wide defaults, data presets, mode defaults, config keys, variant keys, drawer
//! groups and widget telemetry. Template normalization, export and the editor all read it.
//! Insertion order of the manifests' objects is kept (serde_json's `preserve_order`), because
//! key lists such as `variant_keys` follow it.
use crate::model::telemetry::{ActivitySeries, WidgetTelemetry};
use crate::widgets::heart_zones::HeartZoneModel;
use ovrley_map::MapStyleId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// How a widget or display lays out: around its content, or in a frame of its own size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    Intrinsic,
    Boxed,
}

/// A widget, display, mode or drawer icon.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IconRef {
    pub source: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub asset_file: Option<String>,
}

/// One global-default binding: the data field at `path` takes the global default `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalBinding {
    pub path: Vec<String>,
    pub source: String,
}

fn compile_bindings(bindings: &BTreeMapOrdered) -> Vec<GlobalBinding> {
    bindings
        .iter()
        .map(|(field, source)| GlobalBinding {
            path: field.split('.').map(str::to_string).collect(),
            source: source.clone(),
        })
        .collect()
}

/// An object of string values in manifest order.
type BTreeMapOrdered = Vec<(String, String)>;

fn ordered_strings<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMapOrdered, D::Error> {
    let map = Map::<String, Value>::deserialize(deserializer)?;
    map.into_iter()
        .map(|(key, value)| match value {
            Value::String(text) => Ok((key, text)),
            other => Err(serde::de::Error::custom(format!(
                "{key} must name a string, found {other}"
            ))),
        })
        .collect()
}

fn ordered_string_lists<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(String, Vec<String>)>, D::Error> {
    let map = Map::<String, Value>::deserialize(deserializer)?;
    map.into_iter()
        .map(|(key, value)| {
            serde_json::from_value::<Vec<String>>(value)
                .map(|list| (key, list))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

/// A selectable display unit of a metric.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitOption {
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

/// One series an altitude widget may start from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltitudeSeries {
    pub field: String,
    pub require_value: bool,
}

/// Where an altitude widget's starting altitude comes from, and which field holds its unit.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltitudeSource {
    pub series: Vec<AltitudeSeries>,
    pub unit_field: String,
}

/// How a new course widget sizes itself from the route.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CourseSizing {
    pub max_dimension: f64,
    pub min_dimension: f64,
}

/// Editor and renderer bounds of the rolling chart's series list and time window.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChartLimits {
    pub max_series: usize,
    pub min_window_seconds: f32,
    pub max_window_seconds: f32,
}

/// A labelled choice in the manifest (time modes, elapsed origins).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestChoice {
    pub value: String,
    pub label_key: String,
    #[serde(default)]
    pub default_label: Option<String>,
    /// Extra telemetry the choice needs.
    #[serde(default)]
    pub activity: Vec<ActivitySeries>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawMode {
    value: String,
    label_key: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    icon: Option<IconRef>,
    #[serde(default)]
    activity: Vec<ActivitySeries>,
    #[serde(default)]
    defaults: Map<String, Value>,
}

/// A display mode (the lap timer's), with its compiled defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayMode {
    pub value: String,
    pub label_key: String,
    pub label: Option<String>,
    pub icon: Option<IconRef>,
    /// Extra telemetry the mode needs.
    pub activity: Vec<ActivitySeries>,
    /// The display's mode defaults, the mode's own defaults, and the fields the mode sets.
    pub defaults: Map<String, Value>,
}

/// How a standard metric formats its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricFormatter {
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
    GasMix,
    Coordinates,
    Distance,
    Elevation,
    LapTimer,
    Splits,
}

/// Whether a metric offers a unit choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitsMode {
    Selectable,
    Hidden,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDisplay {
    label_key: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    icon: Option<IconRef>,
    layout_mode: LayoutMode,
    defaults: Map<String, Value>,
    variant_defaults: Map<String, Value>,
    conditional_keys: Vec<String>,
    #[serde(deserialize_with = "ordered_strings")]
    inherited_fields: BTreeMapOrdered,
    #[serde(deserialize_with = "ordered_strings")]
    global_bindings: BTreeMapOrdered,
    #[serde(deserialize_with = "ordered_string_lists")]
    geometry_bindings: Vec<(String, Vec<String>)>,
    geometry_fields: Vec<String>,
    #[serde(deserialize_with = "ordered_strings")]
    resolved_frame: BTreeMapOrdered,
    modes: Vec<Map<String, Value>>,
    mode_defaults: Map<String, Value>,
    #[serde(deserialize_with = "ordered_strings")]
    mode_fields: BTreeMapOrdered,
    mode_selection_field: Option<String>,
    forbidden_frame_fields: Vec<String>,
    required_positive_fields: Vec<String>,
    #[serde(default)]
    zone_model: Option<HeartZoneModel>,
    #[serde(default)]
    major_tick_intervals: Option<Vec<u32>>,
}

/// A display type: defaults, geometry, modes and optional fields.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayDefinition {
    pub kind: String,
    pub label_key: String,
    pub label: Option<String>,
    pub icon: Option<IconRef>,
    pub layout_mode: LayoutMode,
    /// The display's defaults with its mode defaults.
    pub defaults: Map<String, Value>,
    pub variant_defaults: Map<String, Value>,
    pub conditional_keys: Vec<String>,
    pub inherited_fields: Vec<(String, String)>,
    pub global_bindings: Vec<GlobalBinding>,
    pub geometry_bindings: Vec<(String, Vec<String>)>,
    pub geometry_fields: Vec<String>,
    pub resolved_frame: Vec<(String, String)>,
    pub modes: Vec<DisplayMode>,
    pub mode_fields: Vec<(String, String)>,
    pub mode_selection_field: Option<String>,
    pub forbidden_frame_fields: Vec<String>,
    pub required_positive_fields: Vec<String>,
    /// The variant defaults' keys, then the conditional keys.
    pub variant_keys: Vec<String>,
    /// The geometry bindings' targets.
    pub geometry_keys: Vec<String>,
    /// Coggan zone shares; declared only by `heart_zones`.
    pub zone_model: Option<HeartZoneModel>,
    /// Tape units between major ticks; declared only by `altitude_tape`.
    pub major_tick_intervals: Option<Vec<u32>>,
}

impl DisplayDefinition {
    pub fn require_mode(&self, value: &str) -> Result<&DisplayMode, String> {
        self.modes
            .iter()
            .find(|mode| mode.value == value)
            .ok_or_else(|| format!("Unsupported {} mode: {value}", self.kind))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawWidget {
    #[serde(rename = "type")]
    kind: String,
    name_key: String,
    short_name_key: String,
    category: String,
    icon: IconRef,
    defaults: Map<String, Value>,
    collection: String,
    current: bool,
    drawer_group: Option<String>,
    plottable: bool,
    metric: bool,
    layout_mode: LayoutMode,
    display_types: Vec<String>,
    display_type_label_keys: BTreeMap<String, String>,
    initial_display_types: Vec<String>,
    #[serde(deserialize_with = "ordered_strings")]
    global_bindings: BTreeMapOrdered,
    name_field: Option<String>,
    sidebar_priority: i64,
    altitude_source: Option<AltitudeSource>,
    course_sizing: Option<CourseSizing>,
    reset_selection: Map<String, Value>,
    content_alignment: bool,
    time_dependent: bool,
    defaults_preset: String,
    optional_fields: Vec<String>,
    initializers: Vec<String>,
    #[serde(default)]
    supported_display_units: Vec<UnitOption>,
    #[serde(default)]
    formatter: Option<MetricFormatter>,
    #[serde(default)]
    units_mode: Option<UnitsMode>,
    #[serde(default)]
    max_decimals: Option<u32>,
    #[serde(default)]
    modes: Vec<ManifestChoice>,
    #[serde(default)]
    elapsed_origins: Vec<ManifestChoice>,
    #[serde(default)]
    chart_limits: Option<ChartLimits>,
    #[serde(default)]
    activity: Option<WidgetTelemetry>,
    /// Read by the build script's activity schema only.
    #[serde(default, rename = "interpolation")]
    _interpolation: serde::de::IgnoredAny,
    #[serde(default)]
    series_palette: Vec<String>,
    /// Read by the build script's activity schema only.
    #[serde(default, rename = "dataSource")]
    _data_source: serde::de::IgnoredAny,
}

/// A widget type: canonical defaults and capabilities.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetDefinition {
    pub kind: String,
    pub name_key: String,
    pub short_name_key: String,
    pub category: String,
    pub icon: IconRef,
    /// The data preset, the default display's defaults, the widget's own defaults, and the type
    /// field of collections that hold several types.
    pub defaults: Map<String, Value>,
    pub collection: String,
    pub current: bool,
    pub drawer_group: Option<String>,
    pub plottable: bool,
    pub metric: bool,
    pub layout_mode: LayoutMode,
    pub display_types: Vec<String>,
    pub display_type_label_keys: BTreeMap<String, String>,
    pub initial_display_types: Vec<String>,
    pub global_bindings: Vec<GlobalBinding>,
    pub name_field: Option<String>,
    pub sidebar_priority: i64,
    pub altitude_source: Option<AltitudeSource>,
    pub course_sizing: Option<CourseSizing>,
    pub reset_selection: Map<String, Value>,
    pub content_alignment: bool,
    pub time_dependent: bool,
    pub optional_fields: Vec<String>,
    pub initializers: Vec<String>,
    pub supported_display_units: Vec<UnitOption>,
    pub formatter: Option<MetricFormatter>,
    pub units_mode: Option<UnitsMode>,
    /// `id`, the defaults' keys, the optional fields, and every mode field of its displays.
    pub config_keys: Vec<String>,
    pub has_decimal_control: bool,
    pub max_decimals: u32,
    pub modes: Vec<ManifestChoice>,
    pub elapsed_origins: Vec<ManifestChoice>,
    pub chart_limits: Option<ChartLimits>,
    /// Telemetry the widget reads; absent for static widgets.
    pub activity: Option<WidgetTelemetry>,
    /// Rolling-chart series colours.
    pub series_palette: Vec<String>,
}

impl WidgetDefinition {
    /// The unit a new widget of this metric shows.
    pub fn default_display_unit(&self) -> Option<&str> {
        self.defaults.get("display_unit")?.as_str()
    }

    /// A widget may name a shared display after its own metric.
    pub fn display_type_label_key<'a>(
        &'a self,
        catalog: &'a Catalog,
        display_type: &str,
    ) -> &'a str {
        self.display_type_label_keys
            .get(display_type)
            .map(String::as_str)
            .unwrap_or_else(|| &catalog.require_display(display_type).label_key)
    }
}

/// A document collection and the field that names its widgets' type, if it holds several.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Collection {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub type_field: Option<String>,
}

/// One drawer option of a group.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawerOption {
    pub value: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label_key: String,
    pub icon: IconRef,
    #[serde(default)]
    pub preset: Option<Map<String, Value>>,
}

/// A drawer entry that offers several widget types.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawerGroup {
    #[serde(skip)]
    pub name: String,
    pub category: String,
    pub short_name_key: String,
    pub icon: IconRef,
    pub options: Vec<DrawerOption>,
}

/// The compiled catalog.
#[derive(Debug)]
pub struct Catalog {
    /// Ordered document collections, keyed by name.
    pub collections: Vec<(String, Collection)>,
    /// Ordered drawer categories and their name keys.
    pub categories: Vec<(String, String)>,
    /// Unit scales from meters, for altitude input and presentation.
    pub altitude_unit_scales: Vec<(String, f64)>,
    pub widget_font_weight: f32,
    displays: Vec<DisplayDefinition>,
    widgets: Vec<WidgetDefinition>,
    pub drawer_groups: Vec<DrawerGroup>,
    /// `[metric, imperial]` unit pairs; the first pair whose target a field accepts converts it.
    pub unit_conversions: Vec<[String; 2]>,
    /// Widget and display fields that hold a unit.
    pub unit_fields: Vec<String>,
    /// `[metric, imperial]` label toggles.
    pub unit_toggles: Vec<[String; 2]>,
}

/// The manifest contents a compile needs. Each manifest object keeps its key order.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetsManifest {
    typography: Typography,
    categories: Map<String, Value>,
    drawer_groups: Map<String, Value>,
    collections: Map<String, Value>,
    altitude_unit_scales: Map<String, Value>,
    widget_defaults: Map<String, Value>,
    display_types: DisplayTypes,
    definitions: Vec<Map<String, Value>>,
    display_defaults: Map<String, Value>,
    data_defaults: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Typography {
    widget_font_weight: f32,
}

#[derive(Deserialize)]
struct DisplayTypes {
    definitions: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetricsManifest {
    display_types: DisplayTypes,
    unit_fields: Vec<String>,
    unit_toggles: Vec<[String; 2]>,
    unit_conversions: Vec<[String; 2]>,
    definitions: Vec<Map<String, Value>>,
    widget_defaults: Map<String, Value>,
    display_defaults: Map<String, Value>,
}

fn layered(layers: &[&Map<String, Value>]) -> Map<String, Value> {
    let mut merged = Map::new();
    for layer in layers {
        for (key, value) in layer.iter() {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

fn compile_display(kind: &str, merged: Map<String, Value>) -> Result<DisplayDefinition, String> {
    let raw: RawDisplay = serde_json::from_value(Value::Object(merged))
        .map_err(|error| format!("display {kind}: {error}"))?;
    let modes = raw
        .modes
        .iter()
        .map(|fields| {
            let mode: RawMode = serde_json::from_value(Value::Object(fields.clone()))
                .map_err(|error| format!("display {kind} mode: {error}"))?;
            let mut defaults = layered(&[&raw.mode_defaults, &mode.defaults]);
            for (target, source) in &raw.mode_fields {
                defaults.insert(
                    target.clone(),
                    fields.get(source).cloned().unwrap_or(Value::Null),
                );
            }
            Ok(DisplayMode {
                value: mode.value,
                label_key: mode.label_key,
                label: mode.label,
                icon: mode.icon,
                activity: mode.activity,
                defaults,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut variant_keys: Vec<String> = raw.variant_defaults.keys().cloned().collect();
    variant_keys.extend(raw.conditional_keys.iter().cloned());
    Ok(DisplayDefinition {
        kind: kind.to_string(),
        label_key: raw.label_key,
        label: raw.label,
        icon: raw.icon,
        layout_mode: raw.layout_mode,
        defaults: layered(&[&raw.defaults, &raw.mode_defaults]),
        variant_defaults: raw.variant_defaults,
        conditional_keys: raw.conditional_keys,
        inherited_fields: raw.inherited_fields,
        global_bindings: compile_bindings(&raw.global_bindings),
        geometry_keys: raw
            .geometry_bindings
            .iter()
            .map(|(target, _)| target.clone())
            .collect(),
        geometry_bindings: raw.geometry_bindings,
        geometry_fields: raw.geometry_fields,
        resolved_frame: raw.resolved_frame,
        modes,
        mode_fields: raw.mode_fields,
        mode_selection_field: raw.mode_selection_field,
        forbidden_frame_fields: raw.forbidden_frame_fields,
        required_positive_fields: raw.required_positive_fields,
        variant_keys,
        zone_model: raw.zone_model,
        major_tick_intervals: raw.major_tick_intervals,
    })
}

impl Catalog {
    fn compile(widgets_json: &str, metrics_json: &str) -> Result<Self, String> {
        let widgets_manifest: WidgetsManifest = serde_json::from_str(widgets_json)
            .map_err(|error| format!("standard-widgets.json: {error}"))?;
        let metrics_manifest: MetricsManifest = serde_json::from_str(metrics_json)
            .map_err(|error| format!("standard-metrics.json: {error}"))?;

        let mut displays = Vec::new();
        for (display_defaults, definitions) in [
            (
                &widgets_manifest.display_defaults,
                &widgets_manifest.display_types.definitions,
            ),
            (
                &metrics_manifest.display_defaults,
                &metrics_manifest.display_types.definitions,
            ),
        ] {
            for (kind, raw) in definitions {
                let raw = raw
                    .as_object()
                    .ok_or_else(|| format!("display {kind} must be an object"))?;
                displays.push(compile_display(
                    kind,
                    layered(&[&widgets_manifest.display_defaults, display_defaults, raw]),
                )?);
            }
        }

        let collections = widgets_manifest
            .collections
            .iter()
            .map(|(name, value)| {
                serde_json::from_value::<Collection>(value.clone())
                    .map(|collection| (name.clone(), collection))
                    .map_err(|error| format!("collection {name}: {error}"))
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut widgets = Vec::new();
        for (widget_defaults, definitions) in [
            (
                &widgets_manifest.widget_defaults,
                &widgets_manifest.definitions,
            ),
            (
                &metrics_manifest.widget_defaults,
                &metrics_manifest.definitions,
            ),
        ] {
            for raw in definitions {
                let merged = layered(&[&widgets_manifest.widget_defaults, widget_defaults, raw]);
                let kind = merged
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string();
                let raw: RawWidget = serde_json::from_value(Value::Object(merged))
                    .map_err(|error| format!("widget {kind}: {error}"))?;
                let preset = widgets_manifest
                    .data_defaults
                    .get(&raw.defaults_preset)
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        format!(
                            "widget {kind}: unknown defaults preset {}",
                            raw.defaults_preset
                        )
                    })?;
                let display_type = raw
                    .defaults
                    .get("display_type")
                    .or_else(|| preset.get("display_type"))
                    .and_then(Value::as_str);
                let empty = Map::new();
                let display_defaults = display_type
                    .and_then(|display_type| {
                        displays.iter().find(|display| display.kind == display_type)
                    })
                    .map_or(&empty, |display| &display.defaults);
                let mut defaults = layered(&[preset, display_defaults, &raw.defaults]);
                let collection = collections
                    .iter()
                    .find(|(name, _)| *name == raw.collection)
                    .map(|(_, collection)| collection)
                    .ok_or_else(|| {
                        format!("widget {kind}: unknown collection {}", raw.collection)
                    })?;
                if let Some(type_field) = &collection.type_field {
                    defaults.insert(type_field.clone(), Value::String(raw.kind.clone()));
                }
                let mut config_keys = vec!["id".to_string()];
                let mut add = |key: &str| {
                    if !config_keys.iter().any(|existing| existing == key) {
                        config_keys.push(key.to_string());
                    }
                };
                defaults.keys().for_each(|key| add(key));
                raw.optional_fields.iter().for_each(|key| add(key));
                for display_type in &raw.display_types {
                    let display = displays
                        .iter()
                        .find(|display| &display.kind == display_type)
                        .ok_or_else(|| {
                            format!("widget {kind}: unknown display type {display_type}")
                        })?;
                    display.mode_fields.iter().for_each(|(field, _)| add(field));
                }
                if let Some(activity) = &raw.activity {
                    if !activity.sources.iter().all(|source| source.is_numeric()) {
                        return Err(format!(
                            "widget {kind}: sources must be numeric activity columns"
                        ));
                    }
                    if raw.plottable && activity.sources.is_empty() {
                        return Err(format!(
                            "widget {kind}: plottable metrics need numeric sources"
                        ));
                    }
                }
                widgets.push(WidgetDefinition {
                    has_decimal_control: raw.max_decimals.is_some()
                        || matches!(
                            raw.formatter,
                            Some(MetricFormatter::Decimal | MetricFormatter::Temperature)
                        ),
                    max_decimals: raw.max_decimals.unwrap_or(1),
                    kind: raw.kind,
                    name_key: raw.name_key,
                    short_name_key: raw.short_name_key,
                    category: raw.category,
                    icon: raw.icon,
                    defaults,
                    collection: raw.collection,
                    current: raw.current,
                    drawer_group: raw.drawer_group,
                    plottable: raw.plottable,
                    metric: raw.metric,
                    layout_mode: raw.layout_mode,
                    display_types: raw.display_types,
                    display_type_label_keys: raw.display_type_label_keys,
                    initial_display_types: raw.initial_display_types,
                    global_bindings: compile_bindings(&raw.global_bindings),
                    name_field: raw.name_field,
                    sidebar_priority: raw.sidebar_priority,
                    altitude_source: raw.altitude_source,
                    course_sizing: raw.course_sizing,
                    reset_selection: raw.reset_selection,
                    content_alignment: raw.content_alignment,
                    time_dependent: raw.time_dependent,
                    optional_fields: raw.optional_fields,
                    initializers: raw.initializers,
                    supported_display_units: raw.supported_display_units,
                    formatter: raw.formatter,
                    units_mode: raw.units_mode,
                    config_keys,
                    modes: raw.modes,
                    elapsed_origins: raw.elapsed_origins,
                    chart_limits: raw.chart_limits,
                    activity: raw.activity,
                    series_palette: raw.series_palette,
                });
            }
        }

        let categories = widgets_manifest
            .categories
            .iter()
            .map(|(name, key)| {
                key.as_str()
                    .map(|key| (name.clone(), key.to_string()))
                    .ok_or_else(|| format!("category {name} must name a key"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let altitude_unit_scales = widgets_manifest
            .altitude_unit_scales
            .iter()
            .map(|(unit, scale)| {
                scale
                    .as_f64()
                    .map(|scale| (unit.clone(), scale))
                    .ok_or_else(|| format!("altitude unit {unit} must be a number"))
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut catalog = Self {
            collections,
            categories,
            altitude_unit_scales,
            widget_font_weight: widgets_manifest.typography.widget_font_weight,
            displays,
            widgets,
            drawer_groups: Vec::new(),
            unit_conversions: metrics_manifest.unit_conversions,
            unit_fields: metrics_manifest.unit_fields,
            unit_toggles: metrics_manifest.unit_toggles,
        };
        catalog.drawer_groups = catalog.compile_drawer_groups(&widgets_manifest.drawer_groups)?;
        Ok(catalog)
    }

    /// Each drawer group's options must offer its member widgets and preset only their defaults,
    /// or the manifest fails to load.
    fn compile_drawer_groups(
        &self,
        raw_groups: &Map<String, Value>,
    ) -> Result<Vec<DrawerGroup>, String> {
        let mut groups = Vec::new();
        for (name, raw) in raw_groups {
            let mut group: DrawerGroup = serde_json::from_value(raw.clone())
                .map_err(|error| format!("drawer group {name}: {error}"))?;
            group.name = name.clone();
            if !self
                .categories
                .iter()
                .any(|(category, _)| *category == group.category)
            {
                return Err(format!(
                    "Drawer group {name} has unknown category {}",
                    group.category
                ));
            }
            for option in &group.options {
                let definition = self.require_widget(&option.kind)?;
                if definition.drawer_group.as_deref() != Some(name.as_str()) {
                    return Err(format!(
                        "Drawer group {name} offers {}, which is not one of its members",
                        option.kind
                    ));
                }
                for field in option.preset.iter().flat_map(|preset| preset.keys()) {
                    if !definition.defaults.contains_key(field) {
                        return Err(format!(
                            "Drawer group {name} presets unknown {} field {field}",
                            option.kind
                        ));
                    }
                }
            }
            groups.push(group);
        }
        for definition in &self.widgets {
            let Some(group_name) = &definition.drawer_group else {
                continue;
            };
            let offered = groups
                .iter()
                .find(|group| &group.name == group_name)
                .is_some_and(|group| {
                    group
                        .options
                        .iter()
                        .any(|option| option.kind == definition.kind)
                });
            if !offered {
                return Err(format!(
                    "Widget {} names drawer group {group_name}, which does not offer it",
                    definition.kind
                ));
            }
        }
        Ok(groups)
    }

    /// Every widget definition, in manifest order.
    pub fn widgets(&self) -> &[WidgetDefinition] {
        &self.widgets
    }

    /// Every display definition, in manifest order.
    pub fn displays(&self) -> &[DisplayDefinition] {
        &self.displays
    }

    pub fn widget(&self, kind: &str) -> Option<&WidgetDefinition> {
        self.widgets
            .iter()
            .find(|definition| definition.kind == kind)
    }

    pub fn require_widget(&self, kind: &str) -> Result<&WidgetDefinition, String> {
        self.widget(kind)
            .ok_or_else(|| format!("Unknown widget type: {kind}"))
    }

    pub fn display(&self, kind: &str) -> Option<&DisplayDefinition> {
        self.displays
            .iter()
            .find(|definition| definition.kind == kind)
    }

    /// A display type that the caller already validated (a manifest reference or a normalized
    /// widget's `display_type`).
    pub fn require_display(&self, kind: &str) -> &DisplayDefinition {
        self.display(kind)
            .unwrap_or_else(|| panic!("unknown display type {kind}"))
    }

    pub fn collection(&self, name: &str) -> Option<&Collection> {
        self.collections
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, collection)| collection)
    }

    pub fn altitude_unit_scale(&self, unit: &str) -> Option<f64> {
        self.altitude_unit_scales
            .iter()
            .find(|(key, _)| key == unit)
            .map(|(_, scale)| *scale)
    }

    /// Whether a widget's active presentation has a bounded frame.
    pub fn is_framed(&self, kind: &str, display_type: Option<&str>) -> bool {
        let layout = match display_type {
            Some(display) => self.require_display(display).layout_mode,
            None => self
                .widget(kind)
                .map_or(LayoutMode::Intrinsic, |definition| definition.layout_mode),
        };
        layout == LayoutMode::Boxed
    }
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

/// The catalog compiled from the bundled manifests. A broken manifest is a build defect and
/// panics.
pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        Catalog::compile(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/standard-widgets.json"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/standard-metrics.json"
            )),
        )
        .unwrap_or_else(|error| panic!("the widget manifests do not compile: {error}"))
    })
}

include!(concat!(env!("OUT_DIR"), "/metric_kinds.rs"));

/// How a value widget presents its metric.
///
/// Templates that omit the field, or carry `null` or an unknown key, present as `Text`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayType {
    #[default]
    Text,
    Linear,
    Arc,
    Corner,
    GtGauge,
    HeadingTape,
    HeadingCompass,
    HeartZones,
    AltitudeTape,
    LeanAngle,
    GForce,
    LapTimer,
    SplitsRun,
}

impl DisplayType {
    pub const ALL: [Self; 13] = [
        Self::Text,
        Self::Linear,
        Self::Arc,
        Self::Corner,
        Self::GtGauge,
        Self::HeadingTape,
        Self::HeadingCompass,
        Self::HeartZones,
        Self::AltitudeTape,
        Self::LeanAngle,
        Self::GForce,
        Self::LapTimer,
        Self::SplitsRun,
    ];

    /// The display's manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Linear => "linear",
            Self::Arc => "arc",
            Self::Corner => "corner",
            Self::GtGauge => "gt_gauge",
            Self::HeadingTape => "heading_tape",
            Self::HeadingCompass => "heading_compass",
            Self::HeartZones => "heart_zones",
            Self::AltitudeTape => "altitude_tape",
            Self::LeanAngle => "lean_angle",
            Self::GForce => "g_force",
            Self::LapTimer => "lap_timer",
            Self::SplitsRun => "splits_run",
        }
    }

    /// Every value display is declared by the manifests.
    pub fn layout_mode(self) -> LayoutMode {
        catalog().require_display(self.as_str()).layout_mode
    }
}

impl Serialize for DisplayType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DisplayType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = Value::deserialize(deserializer)?;
        Ok(Self::ALL
            .into_iter()
            .find(|display| key.as_str() == Some(display.as_str()))
            .unwrap_or_default())
    }
}

/// Visual representation mode for a backdrop widget; unknown keys and `null` are rejected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackdropType {
    Circle,
    #[default]
    Rectangle,
}

impl BackdropType {
    /// The display's manifest key.
    pub fn as_str(self) -> &'static str {
        match self {
            BackdropType::Circle => "circle",
            BackdropType::Rectangle => "rectangle",
        }
    }
}

/// Rectangle edge that carries a coloured backdrop strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackdropEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Geometry/paint mode used by gauge tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackFillStyle {
    #[default]
    Fill,
    Bars,
}

impl TrackFillStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Bars => "bars",
        }
    }
}

impl Catalog {
    /// A current standard metric: one with a formatter, display units and an icon. Specialized
    /// values (time, gradient) and plots have none.
    pub fn standard_metric(&self, kind: MetricKind) -> Option<&WidgetDefinition> {
        self.widget(kind.as_str())
            .filter(|definition| definition.metric && definition.current)
    }

    /// The rendered label of a standard metric's display unit (its default when `None`).
    pub fn unit_label(&self, kind: MetricKind, display_unit: Option<&str>) -> &str {
        let Some(definition) = self.standard_metric(kind) else {
            return "";
        };
        let unit = display_unit.or_else(|| definition.default_display_unit());
        definition
            .supported_display_units
            .iter()
            .find(|option| Some(option.value.as_str()) == unit)
            .and_then(|option| {
                option
                    .render_label
                    .as_deref()
                    .or(option.label.as_deref())
                    .or(option.default_label.as_deref())
            })
            .unwrap_or("")
    }

    /// Whether a metric may present as `display`: one of its displays, or its only default.
    pub fn supports_display(&self, kind: MetricKind, display: DisplayType) -> bool {
        self.widget(kind.as_str()).is_some_and(|definition| {
            if definition.display_types.is_empty() {
                definition
                    .defaults
                    .get("display_type")
                    .and_then(Value::as_str)
                    == Some(display.as_str())
            } else {
                definition
                    .display_types
                    .iter()
                    .any(|key| key == display.as_str())
            }
        })
    }
}

/// Coggan zone shares of the heart-zones display.
pub fn heart_zone_model() -> &'static HeartZoneModel {
    catalog()
        .require_display(DisplayType::HeartZones.as_str())
        .zone_model
        .as_ref()
        .expect("heart_zones display must declare zoneModel")
}

/// Tape units between major ticks the altitude tape offers.
pub fn altitude_tape_major_tick_intervals() -> &'static [u32] {
    catalog()
        .require_display(DisplayType::AltitudeTape.as_str())
        .major_tick_intervals
        .as_deref()
        .expect("altitude_tape display must declare majorTickIntervals")
}

/// Shared weight for text outside label widgets.
pub fn widget_font_weight() -> f32 {
    catalog().widget_font_weight
}

pub fn time_icon_asset_file() -> &'static str {
    catalog()
        .require_widget("time")
        .expect("the catalog defines time")
        .icon
        .asset_file
        .as_deref()
        .expect("time icon must declare assetFile")
}

pub fn chart_limits() -> ChartLimits {
    catalog()
        .require_widget("rolling_chart")
        .expect("the catalog defines rolling_chart")
        .chart_limits
        .expect("rolling_chart must declare chartLimits")
}

/// The style a new map widget draws, which every build must install.
pub fn default_map_style() -> &'static MapStyleId {
    static STYLE: OnceLock<MapStyleId> = OnceLock::new();
    STYLE.get_or_init(|| {
        let map = catalog()
            .require_widget("map")
            .expect("the catalog defines map");
        serde_json::from_value(map.defaults["map_style"].clone())
            .expect("map defaults must name a map style")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WIDGETS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/standard-widgets.json"
    ));
    const METRICS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/standard-metrics.json"
    ));

    fn compile_with(edit: impl FnOnce(&mut Value)) -> Result<Catalog, String> {
        let mut widgets: Value = serde_json::from_str(WIDGETS).unwrap();
        edit(&mut widgets);
        Catalog::compile(&widgets.to_string(), METRICS)
    }

    #[test]
    fn compiles_the_bundled_manifests() {
        let catalog = catalog();
        assert!(catalog.widgets().len() > 50);
        assert_eq!(
            catalog
                .collections
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["backdrops", "rasters", "labels", "values", "plots"]
        );
    }

    #[test]
    fn value_widgets_inherit_the_value_preset_and_their_display_defaults() {
        let speed = catalog().require_widget("speed").unwrap();
        assert_eq!(speed.defaults["value"], "speed");
        assert_eq!(speed.defaults["display_type"], "text");
        assert_eq!(speed.defaults["display_unit"], "kmh");
        assert_eq!(speed.defaults["show_units"], true);
        assert_eq!(speed.defaults["font_size"], 100);
        assert!(speed.config_keys.starts_with(&["id".to_string()]));
        assert!(speed.config_keys.contains(&"display_variants".to_string()));
        assert!(speed.has_decimal_control);
        assert_eq!(speed.max_decimals, 1);
        assert_eq!(speed.collection, "values");
    }

    #[test]
    fn mode_fields_join_config_keys_and_mode_defaults() {
        let catalog = catalog();
        let lap_timer = catalog.require_widget("lap_timer").unwrap();
        assert!(lap_timer
            .config_keys
            .contains(&"lap_timer_mode".to_string()));
        let display = catalog.require_display("lap_timer");
        let log = display.require_mode("lap_log").unwrap();
        assert_eq!(log.defaults["font_size"], 30);
        assert_eq!(log.defaults["label_font_size"], 15);
        assert_eq!(log.defaults["lap_timer_mode"], "lap_log");
        assert_eq!(
            display.require_mode("current_lap").unwrap().defaults["font_size"],
            50
        );
        assert_eq!(
            display.require_mode("nope").unwrap_err(),
            "Unsupported lap_timer mode: nope"
        );
        assert_eq!(
            display.defaults["label_font_size"], 17.5,
            "mode defaults join the display defaults"
        );
    }

    #[test]
    fn displays_compile_variant_and_geometry_keys() {
        let linear = catalog().require_display("linear");
        assert_eq!(linear.layout_mode, LayoutMode::Boxed);
        assert!(linear.variant_keys.contains(&"width".to_string()));
        assert_eq!(linear.geometry_keys, ["width", "height", "rotation"]);
        assert_eq!(
            catalog().require_display("text").layout_mode,
            LayoutMode::Intrinsic
        );
    }

    #[test]
    fn global_bindings_split_nested_paths() {
        let elevation = catalog().require_widget("elevation").unwrap();
        assert!(elevation.global_bindings.contains(&GlobalBinding {
            path: vec!["point_label".into(), "font".into()],
            source: "font_values".into()
        }));
    }

    #[test]
    fn unknown_widgets_fail_with_their_type() {
        assert_eq!(
            catalog().require_widget("nope").unwrap_err(),
            "Unknown widget type: nope"
        );
    }

    #[test]
    fn drawer_groups_reject_unknown_categories_members_and_presets() {
        let error =
            compile_with(|manifest| manifest["drawerGroups"]["map"]["category"] = json!("nowhere"))
                .unwrap_err();
        assert_eq!(error, "Drawer group map has unknown category nowhere");
        let error = compile_with(|manifest| {
            manifest["drawerGroups"]["map"]["options"][0]["type"] = json!("backdrop")
        })
        .unwrap_err();
        assert_eq!(
            error,
            "Drawer group map offers backdrop, which is not one of its members"
        );
        let error = compile_with(|manifest| {
            manifest["drawerGroups"]["map"]["options"][1]["preset"] = json!({"nope": 1})
        })
        .unwrap_err();
        assert_eq!(error, "Drawer group map presets unknown map field nope");
        let error = compile_with(|manifest| {
            let options = manifest["drawerGroups"]["map"]["options"]
                .as_array_mut()
                .unwrap();
            options.retain(|option| option["type"] != "course");
        })
        .unwrap_err();
        assert_eq!(
            error,
            "Widget course names drawer group map, which does not offer it"
        );
    }

    #[test]
    fn framed_widgets_follow_their_display_layout() {
        let catalog = catalog();
        assert!(catalog.is_framed("speed", Some("linear")));
        assert!(!catalog.is_framed("speed", Some("text")));
        assert!(catalog.is_framed("raster", None));
    }

    #[test]
    fn manifest_modes_have_native_renderers() {
        use crate::widgets::lap_timer::LapTimerMode;
        use crate::widgets::time::{ElapsedTimeOrigin, TimeMode};
        let time = catalog().require_widget("time").unwrap();
        assert!(time
            .modes
            .iter()
            .all(|mode| TimeMode::parse(&mode.value).is_some()));
        assert!(time
            .elapsed_origins
            .iter()
            .all(|origin| ElapsedTimeOrigin::parse(&origin.value).is_some()));
        assert!(catalog()
            .require_display("lap_timer")
            .modes
            .iter()
            .all(|mode| LapTimerMode::parse(&mode.value).is_some()));
    }

    #[test]
    fn metadata_matches_the_manifest() {
        assert_eq!(widget_font_weight(), 700.0);
        assert_eq!(time_icon_asset_file(), "widget-time.svg");
        let limits = chart_limits();
        assert_eq!(
            (
                limits.max_series,
                limits.min_window_seconds,
                limits.max_window_seconds
            ),
            (5, 5.0, 300.0)
        );
        assert_eq!(default_map_style(), &MapStyleId::new("liberty").unwrap());
    }
}
