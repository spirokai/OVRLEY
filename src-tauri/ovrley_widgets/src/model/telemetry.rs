//! Widget telemetry contracts from the shared manifests.
//!
//! Source names describe activity columns, independently of the widgets that use them.
//! A new widget using existing columns only needs manifest metadata here.

use super::types::MetricKind;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Interpolation policy for canonical activity columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationKind {
    Angular,
    Linear,
    Hold,
    Preserve,
}

include!(concat!(env!("OUT_DIR"), "/activity_series.rs"));

macro_rules! define_activity_series {
    (numeric { $($numeric:ident: $number:ident),* } other { $($other:ident: $variant:ident),* }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ActivitySeries {
            $($number,)*
            $($variant,)*
        }

        impl ActivitySeries {
            fn is_numeric(self) -> bool {
                matches!(self, $(Self::$number)|*)
            }
        }
    };
}

activity_series!(define_activity_series);

/// Typed host access to numeric columns, with shared manifest source selection.
pub trait NumericActivity {
    fn numeric_series(&self, series: ActivitySeries) -> &[Option<f64>];

    /// Missing external observations select the next declared source. Empty sources mean
    /// a nonnumeric widget; an empty selected column means unavailable telemetry.
    fn numeric_series_for(&self, metric: MetricKind) -> Option<&[Option<f64>]> {
        let sources = &widget_telemetry(metric.as_str()).sources;
        let (last, preferred) = sources.split_last()?;
        for source in preferred {
            let values = self.numeric_series(*source);
            if values.iter().any(Option::is_some) {
                return Some(values);
            }
        }
        Some(self.numeric_series(*last))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetTelemetry {
    /// Numeric source priority: use the first column containing an observation.
    pub sources: Vec<ActivitySeries>,
    /// Additional columns needed for derived values or geometry.
    pub requires: Vec<ActivitySeries>,
}

#[derive(Deserialize)]
struct RawMode {
    value: String,
    activity: Vec<ActivitySeries>,
}

#[derive(Deserialize)]
struct RawDefinition {
    #[serde(rename = "type")]
    key: String,
    activity: Option<WidgetTelemetry>,
    #[serde(default)]
    modes: Vec<RawMode>,
}

#[derive(Deserialize)]
struct RawDisplayDefinition {
    #[serde(default)]
    modes: Vec<RawMode>,
}

#[derive(Deserialize)]
struct RawDisplayTypes {
    definitions: HashMap<String, RawDisplayDefinition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    definitions: Vec<RawDefinition>,
    display_types: RawDisplayTypes,
}

struct TelemetryManifest {
    definitions: HashMap<String, WidgetTelemetry>,
    numeric_metrics: Vec<MetricKind>,
    modes: HashMap<String, HashMap<String, Vec<ActivitySeries>>>,
}

static TELEMETRY_MANIFEST: OnceLock<TelemetryManifest> = OnceLock::new();

fn load_manifest() -> TelemetryManifest {
    let mut manifest = TelemetryManifest {
        definitions: HashMap::new(),
        numeric_metrics: Vec::new(),
        modes: HashMap::new(),
    };
    for json in [
        include_str!("../../../../assets/standard-widgets.json"),
        include_str!("../../../../assets/standard-metrics.json"),
    ] {
        let raw: RawManifest =
            serde_json::from_str(json).expect("widget telemetry manifest must be valid");
        for definition in raw.definitions {
            if let Some(activity) = definition.activity {
                for source in &activity.sources {
                    assert!(
                        source.is_numeric(),
                        "numeric widget source must be a numeric activity column"
                    );
                }
                if !activity.sources.is_empty() {
                    manifest
                        .numeric_metrics
                        .push(parse_metric_kind(&definition.key));
                }
                assert!(
                    manifest
                        .definitions
                        .insert(definition.key.clone(), activity)
                        .is_none(),
                    "duplicate widget telemetry definition"
                );
            }
            if !definition.modes.is_empty() {
                manifest
                    .modes
                    .insert(definition.key, collect_modes(definition.modes));
            }
        }
        for (key, display) in raw.display_types.definitions {
            if !display.modes.is_empty() {
                manifest.modes.insert(key, collect_modes(display.modes));
            }
        }
    }
    manifest
}

fn parse_metric_kind(key: &str) -> MetricKind {
    serde_json::from_value(serde_json::Value::String(key.to_owned()))
        .expect("telemetry metric must have a native MetricKind")
}

fn collect_modes(modes: Vec<RawMode>) -> HashMap<String, Vec<ActivitySeries>> {
    let mut activity = HashMap::new();
    for mode in modes {
        assert!(
            activity.insert(mode.value, mode.activity).is_none(),
            "duplicate widget mode"
        );
    }
    activity
}

/// Required telemetry for a widget, including the numeric source priority.
pub fn widget_telemetry(key: &str) -> &'static WidgetTelemetry {
    &TELEMETRY_MANIFEST.get_or_init(load_manifest).definitions[key]
}

/// Numeric metrics with source data declared in the manifests.
pub fn numeric_metrics() -> &'static [MetricKind] {
    &TELEMETRY_MANIFEST
        .get_or_init(load_manifest)
        .numeric_metrics
}

/// Extra telemetry required by a selected widget/display mode.
pub fn mode_activity(key: &str, mode: &str) -> &'static [ActivitySeries] {
    &TELEMETRY_MANIFEST.get_or_init(load_manifest).modes[key][mode]
}

/// Interpolation for metric values, including specialized gradient/elevation widgets.
pub fn metric_interpolation(kind: MetricKind) -> Option<InterpolationKind> {
    widget_telemetry(kind.as_str())
        .sources
        .first()
        .copied()
        .map(ActivitySeries::interpolation)
}
