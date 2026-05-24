//! Shared standard-metric widget definitions loaded from the repo manifest.
//!
//! The canonical standard-metric contract lives in `assets/standard-metrics.json`
//! so the frontend and backend share one source of truth for labels, display
//! units, formatter families, and icon asset bindings. `time`, `gradient`, and
//! `elevation` remain specialized render paths outside this contract.

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
    CoreTemperature,
    GForce,
    GroundContactTime,
    Torque,
    GearPosition,
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
}

#[derive(Clone, Debug, Deserialize)]
struct RawStandardMetricManifest {
    definitions: Vec<RawStandardMetricDefinition>,
}

#[derive(Clone, Debug)]
struct StandardMetricManifest {
    definitions: HashMap<MetricKind, StandardMetricDefinition>,
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
                },
            )
        })
        .collect();

    StandardMetricManifest { definitions }
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
        "core_temperature" => Some(MetricKind::CoreTemperature),
        _ => None,
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
        "widget-core-temperature.svg" => Some(MetricIconAssetKey::CoreTemperature),
        "widget-g-force.svg" => Some(MetricIconAssetKey::GForce),
        "widget-ground-contact-time.svg" => Some(MetricIconAssetKey::GroundContactTime),
        "widget-torque.svg" => Some(MetricIconAssetKey::Torque),
        "widget-gear-position.svg" => Some(MetricIconAssetKey::GearPosition),
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
        .map(|option| option.render_label.as_deref().unwrap_or(option.label.as_str()))
        .unwrap_or("")
}
