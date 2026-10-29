//! Gauge layouts and per-frame states built straight from the shared widget crate.
#![allow(dead_code)]

use ovrley_core::{activity::schema::DenseActivityReport, normalize::ValidatedRenderConfig};
use ovrley_widgets::{
    render::{preview::PreviewRasterScale, text::FontResolver},
    widgets::{
        gauges::{
            frame::GaugeLayout,
            metric::{gauge_frame_state, metric_range, GaugeFrameState},
            validation::ValidatedGauge,
        },
        value::{format::altitude_offset_m, validation::ValidatedValue},
    },
};

/// The gauge at value index 0, laid out for the scene, with its state at every dense frame.
pub fn gauge_layout(
    config: &ValidatedRenderConfig,
    dense: &DenseActivityReport,
    fonts: &dyn FontResolver,
) -> (GaugeLayout, Vec<GaugeFrameState>) {
    let gauge = match &config.values[0] {
        ValidatedValue::LinearGauge(widget) => ValidatedGauge::Linear(widget.clone()),
        ValidatedValue::ArcGauge(widget) => ValidatedGauge::Arc(widget.clone()),
        other => panic!("value 0 must be a gauge, got {:?}", other.display_type()),
    };
    let metric = gauge.metric();
    let range = metric_range(dense.full_activity_metric_ranges.get(&metric).copied());
    let offset = altitude_offset_m(gauge.starting_altitude_m(), None);
    let layout = GaugeLayout::new(&gauge, &config.scene.presentation, range, offset, fonts, PreviewRasterScale::NATIVE).unwrap();
    let states = (0..dense.frame_count)
        .map(|index| gauge_frame_state(&gauge, dense.metric_sample(metric, index, 0.0), range, offset))
        .collect();
    (layout, states)
}
