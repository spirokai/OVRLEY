//! Shared standard-metric widget definitions loaded from the repo manifest.
//!
//! The canonical standard-metric contract lives in `assets/standard-metrics.json`
//! so the frontend and backend share one source of truth for labels, display
//! units, formatter families, and icon asset bindings. `time`, `gradient`, and
//! `elevation` remain specialized render paths outside this contract.

use crate::DisplayType;
use crate::MetricKind;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandardMetricInterpolationKind {
    Linear,
    Hold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandardMetricUnitsMode {
    Selectable,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricIconAssetKey {
    Speed,
    Heartrate,
    Cadence,
    Power,
    Time,
    Temperature,
    Pace,
    AirPressure,
    LeftRightBalance,
    StrideLength,
    StrokeRate,
    VerticalSpeed,
    VerticalRatio,
    VerticalOscillation,
    CoreTemperature,
    GForce,
    GroundContactTime,
    Torque,
    GearPosition,
    Heading,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardMetricUnitOption {
    pub value: String,
    pub label: String,
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
    pub kind: MetricKind,
    pub key: String,
    pub current: bool,
    pub label: String,
    pub default_display_unit: String,
    pub supported_display_units: Vec<StandardMetricUnitOption>,
    pub show_units_by_default: bool,
    pub formatter: StandardMetricFormatterKind,
    pub icon: StandardMetricIconDefinition,
    pub interpolation: StandardMetricInterpolationKind,
    pub units_mode: StandardMetricUnitsMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardMetricDefinition {
    #[serde(rename = "type")]
    key: String,
    current: bool,
    label: String,
    default_display_unit: String,
    supported_display_units: Vec<StandardMetricUnitOption>,
    show_units_by_default: bool,
    formatter: StandardMetricFormatterKind,
    icon: StandardMetricIconDefinition,
    #[serde(default = "default_interpolation")]
    interpolation: StandardMetricInterpolationKind,
    #[serde(default = "default_units_mode")]
    units_mode: StandardMetricUnitsMode,
}

fn default_interpolation() -> StandardMetricInterpolationKind {
    StandardMetricInterpolationKind::Linear
}

fn default_units_mode() -> StandardMetricUnitsMode {
    StandardMetricUnitsMode::Selectable
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDisplayTypeDefinition {
    label: String,
    layout_mode: DisplayTypeLayoutMode,
    #[serde(default)]
    default_frame_width: Option<u32>,
    #[serde(default)]
    default_frame_height: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDisplayTypeManifest {
    definitions: HashMap<String, RawDisplayTypeDefinition>,
    defaults: Vec<String>,
    #[serde(default)]
    overrides: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStandardMetricManifest {
    display_types: RawDisplayTypeManifest,
    definitions: Vec<RawStandardMetricDefinition>,
}

#[derive(Clone, Debug)]
struct DisplayTypeManifest {
    definitions: HashMap<String, DisplayTypeDefinition>,
    defaults: Vec<String>,
    overrides: HashMap<String, Vec<String>>,
}

impl DisplayTypeManifest {
    fn from_raw(raw: RawDisplayTypeManifest) -> Self {
        let definitions = raw
            .definitions
            .into_iter()
            .map(|(key, raw_def)| {
                (
                    key,
                    DisplayTypeDefinition {
                        label: raw_def.label,
                        layout_mode: raw_def.layout_mode,
                        default_frame_width: raw_def.default_frame_width,
                        default_frame_height: raw_def.default_frame_height,
                    },
                )
            })
            .collect();

        DisplayTypeManifest {
            definitions,
            defaults: raw.defaults,
            overrides: raw.overrides,
        }
    }
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

fn load_manifest() -> StandardMetricManifest {
    let raw = serde_json::from_str::<RawStandardMetricManifest>(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/standard-metrics.json"
    )))
    .expect("shared standard metrics manifest must be valid JSON");

    let definitions = raw
        .definitions
        .into_iter()
        .map(|definition| {
            let kind = metric_kind_from_key(&definition.key).unwrap_or_else(|| {
                panic!(
                    "shared standard metrics manifest contains unknown metric type: {}",
                    definition.key
                )
            });

            (
                kind,
                StandardMetricDefinition {
                    kind,
                    key: definition.key,
                    current: definition.current,
                    label: definition.label,
                    default_display_unit: definition.default_display_unit,
                    supported_display_units: definition.supported_display_units,
                    show_units_by_default: definition.show_units_by_default,
                    formatter: definition.formatter,
                    icon: definition.icon,
                    interpolation: definition.interpolation,
                    units_mode: definition.units_mode,
                },
            )
        })
        .collect();

    StandardMetricManifest {
        definitions,
        display_types: DisplayTypeManifest::from_raw(raw.display_types),
    }
}

fn metric_kind_from_key(key: &str) -> Option<MetricKind> {
    match key {
        "speed" => Some(MetricKind::Speed),
        "heartrate" => Some(MetricKind::Heartrate),
        "cadence" => Some(MetricKind::Cadence),
        "power" => Some(MetricKind::Power),
        "temperature" => Some(MetricKind::Temperature),
        "pace" => Some(MetricKind::Pace),
        "g_force" => Some(MetricKind::GForce),
        "air_pressure" => Some(MetricKind::AirPressure),
        "ground_contact_time" => Some(MetricKind::GroundContactTime),
        "left_right_balance" => Some(MetricKind::LeftRightBalance),
        "stride_length" => Some(MetricKind::StrideLength),
        "stroke_rate" => Some(MetricKind::StrokeRate),
        "torque" => Some(MetricKind::Torque),
        "vertical_speed" => Some(MetricKind::VerticalSpeed),
        "gear_position" => Some(MetricKind::GearPosition),
        "vertical_ratio" => Some(MetricKind::VerticalRatio),
        "vertical_oscillation" => Some(MetricKind::VerticalOscillation),
        "core_temperature" => Some(MetricKind::CoreTemperature),
        "heading" => Some(MetricKind::Heading),
        "altitude" => Some(MetricKind::Altitude),
        "iso" => Some(MetricKind::Iso),
        "aperture" => Some(MetricKind::Aperture),
        "shutter_speed" => Some(MetricKind::ShutterSpeed),
        "focal_length" => Some(MetricKind::FocalLength),
        "ev" => Some(MetricKind::Ev),
        "color_temperature" => Some(MetricKind::ColorTemperature),
        _ => None,
    }
}

fn metric_kind_to_key(kind: MetricKind) -> &'static str {
    match kind {
        MetricKind::Speed => "speed",
        MetricKind::Heartrate => "heartrate",
        MetricKind::Elevation => "elevation",
        MetricKind::Time => "time",
        MetricKind::Gradient => "gradient",
        MetricKind::Cadence => "cadence",
        MetricKind::Power => "power",
        MetricKind::Temperature => "temperature",
        MetricKind::Pace => "pace",
        MetricKind::GForce => "g_force",
        MetricKind::AirPressure => "air_pressure",
        MetricKind::GroundContactTime => "ground_contact_time",
        MetricKind::LeftRightBalance => "left_right_balance",
        MetricKind::StrideLength => "stride_length",
        MetricKind::StrokeRate => "stroke_rate",
        MetricKind::Torque => "torque",
        MetricKind::VerticalSpeed => "vertical_speed",
        MetricKind::GearPosition => "gear_position",
        MetricKind::VerticalRatio => "vertical_ratio",
        MetricKind::VerticalOscillation => "vertical_oscillation",
        MetricKind::CoreTemperature => "core_temperature",
        MetricKind::Heading => "heading",
        MetricKind::Altitude => "altitude",
        MetricKind::Iso => "iso",
        MetricKind::Aperture => "aperture",
        MetricKind::ShutterSpeed => "shutter_speed",
        MetricKind::FocalLength => "focal_length",
        MetricKind::Ev => "ev",
        MetricKind::ColorTemperature => "color_temperature",
    }
}

pub fn standard_metric_definition(kind: MetricKind) -> Option<&'static StandardMetricDefinition> {
    manifest().definitions.get(&kind)
}

pub fn is_standard_metric(kind: MetricKind) -> bool {
    standard_metric_definition(kind).is_some()
}

pub fn standard_metric_default_display_unit(kind: MetricKind) -> Option<&'static str> {
    standard_metric_definition(kind).map(|definition| definition.default_display_unit.as_str())
}

pub fn standard_metric_show_units(kind: MetricKind, configured: Option<bool>) -> bool {
    configured.unwrap_or_else(|| {
        standard_metric_definition(kind)
            .map(|definition| definition.show_units_by_default)
            .unwrap_or(false)
    })
}

pub fn standard_metric_formatter(kind: MetricKind) -> Option<StandardMetricFormatterKind> {
    standard_metric_definition(kind).map(|definition| definition.formatter)
}

pub fn standard_metric_interpolation(kind: MetricKind) -> Option<StandardMetricInterpolationKind> {
    standard_metric_definition(kind).map(|definition| definition.interpolation)
}

pub fn standard_metric_units_mode(kind: MetricKind) -> Option<StandardMetricUnitsMode> {
    standard_metric_definition(kind).map(|definition| definition.units_mode)
}

pub fn metric_icon_asset_key(kind: MetricKind) -> Option<MetricIconAssetKey> {
    if kind == MetricKind::Time {
        return Some(MetricIconAssetKey::Time);
    }

    match standard_metric_definition(kind)?.icon.asset_file.as_str() {
        "widget-speed.svg" => Some(MetricIconAssetKey::Speed),
        "widget-heartrate.svg" => Some(MetricIconAssetKey::Heartrate),
        "widget-cadence.svg" => Some(MetricIconAssetKey::Cadence),
        "widget-power.svg" => Some(MetricIconAssetKey::Power),
        "widget-temperature.svg" => Some(MetricIconAssetKey::Temperature),
        "widget-pace.svg" => Some(MetricIconAssetKey::Pace),
        "widget-air-pressure.svg" => Some(MetricIconAssetKey::AirPressure),
        "widget-left-right-balance.svg" => Some(MetricIconAssetKey::LeftRightBalance),
        "widget-stride-length.svg" => Some(MetricIconAssetKey::StrideLength),
        "widget-stroke-rate.svg" => Some(MetricIconAssetKey::StrokeRate),
        "widget-vertical-speed.svg" => Some(MetricIconAssetKey::VerticalSpeed),
        "widget-vertical-ratio.svg" => Some(MetricIconAssetKey::VerticalRatio),
        "widget-vertical-oscillation.svg" => Some(MetricIconAssetKey::VerticalOscillation),
        "widget-core-temperature.svg" => Some(MetricIconAssetKey::CoreTemperature),
        "widget-g-force.svg" => Some(MetricIconAssetKey::GForce),
        "widget-ground-contact-time.svg" => Some(MetricIconAssetKey::GroundContactTime),
        "widget-torque.svg" => Some(MetricIconAssetKey::Torque),
        "widget-gear-position.svg" => Some(MetricIconAssetKey::GearPosition),
        "widget-heading.svg" => Some(MetricIconAssetKey::Heading),
        _ => None,
    }
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
                .unwrap_or(option.label.as_str())
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

/// Formal definition of a display type from the shared manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayTypeDefinition {
    pub label: String,
    pub layout_mode: DisplayTypeLayoutMode,
    pub default_frame_width: Option<u32>,
    pub default_frame_height: Option<u32>,
}

/// Look up the full definition for a `display_type` value.
pub fn display_type_definition(display_type: &str) -> Option<&'static DisplayTypeDefinition> {
    manifest().display_types.definitions.get(display_type)
}

/// Look up the human-readable label for a `display_type` value.
pub fn display_type_label(display_type: &str) -> &str {
    manifest()
        .display_types
        .definitions
        .get(display_type)
        .map(|def| def.label.as_str())
        .unwrap_or(display_type)
}

/// Check whether a display type uses boxed (framed) layout.
pub fn is_boxed_display_type(display_type: &str) -> bool {
    manifest()
        .display_types
        .definitions
        .get(display_type)
        .is_some_and(|def| def.layout_mode == DisplayTypeLayoutMode::Boxed)
}

/// Canonical layout mode for a [`DisplayType`] variant, sourced from the
/// shared manifest. This is the single adapter that every Rust path should
/// use when deciding boxed-vs-intrinsic behaviour.
pub fn display_type_layout_mode(display_type: DisplayType) -> DisplayTypeLayoutMode {
    if is_boxed_display_type(display_type.as_str()) {
        DisplayTypeLayoutMode::Boxed
    } else {
        DisplayTypeLayoutMode::Intrinsic
    }
}

/// Return the default frame dimensions for a boxed display type, if available.
pub fn default_frame_dimensions(display_type: &str) -> Option<(u32, u32)> {
    let def = manifest().display_types.definitions.get(display_type)?;
    if def.layout_mode != DisplayTypeLayoutMode::Boxed {
        return None;
    }
    let w = def.default_frame_width?;
    let h = def.default_frame_height?;
    Some((w, h))
}

/// Return the permitted display types for a given metric kind.
///
/// If the manifest contains an override for the metric, that list is returned;
/// otherwise the global defaults are returned.
pub fn supported_display_types(kind: MetricKind) -> &'static [String] {
    let m = manifest();
    let key = metric_kind_to_key(kind);
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
