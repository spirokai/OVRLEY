//! Widget telemetry contracts from the shared manifests.
//!
//! Source names describe activity columns, independently of the widgets that use them.
//! A new widget using existing columns only needs manifest metadata here.

use crate::model::catalog::{catalog, MetricKind};
use serde::Deserialize;
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
            pub(crate) fn is_numeric(self) -> bool {
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

/// The activity columns a widget reads; the build script reads the column schema beside them.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetTelemetry {
    /// Numeric source priority: use the first column containing an observation.
    pub sources: Vec<ActivitySeries>,
    /// Additional columns needed for derived values or geometry.
    pub requires: Vec<ActivitySeries>,
}

/// Required telemetry for a widget, including the numeric source priority.
pub fn widget_telemetry(key: &str) -> &'static WidgetTelemetry {
    catalog()
        .widget(key)
        .and_then(|definition| definition.activity.as_ref())
        .unwrap_or_else(|| panic!("widget {key} declares no telemetry"))
}

/// Numeric metrics with source data declared in the manifests.
pub fn numeric_metrics() -> &'static [MetricKind] {
    static METRICS: OnceLock<Vec<MetricKind>> = OnceLock::new();
    METRICS.get_or_init(|| {
        catalog()
            .widgets()
            .iter()
            .filter(|definition| {
                definition
                    .activity
                    .as_ref()
                    .is_some_and(|activity| !activity.sources.is_empty())
            })
            .map(|definition| {
                serde_json::from_value(serde_json::Value::String(definition.kind.clone()))
                    .expect("telemetry metric must have a native MetricKind")
            })
            .collect()
    })
}

/// Whether a rolling chart may plot this metric.
pub fn is_plottable_metric(kind: MetricKind) -> bool {
    catalog()
        .widget(kind.as_str())
        .is_some_and(|definition| definition.plottable)
}

/// Extra telemetry required by a selected widget or display mode.
pub fn mode_activity(key: &str, mode: &str) -> &'static [ActivitySeries] {
    let catalog = catalog();
    let widget_mode = catalog
        .widget(key)
        .and_then(|definition| definition.modes.iter().find(|choice| choice.value == mode))
        .map(|choice| choice.activity.as_slice());
    widget_mode
        .or_else(|| {
            catalog
                .display(key)?
                .modes
                .iter()
                .find(|display_mode| display_mode.value == mode)
                .map(|display_mode| display_mode.activity.as_slice())
        })
        .unwrap_or_else(|| panic!("{key} declares no mode {mode}"))
}

/// Interpolation for metric values, including specialized gradient/elevation widgets.
pub fn metric_interpolation(kind: MetricKind) -> Option<InterpolationKind> {
    widget_telemetry(kind.as_str())
        .sources
        .first()
        .copied()
        .map(ActivitySeries::interpolation)
}
