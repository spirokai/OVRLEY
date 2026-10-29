//! Gauge progress and boundary formatting, independent of either host's activity storage.
use crate::{model::types::MetricKind, widgets::value::format::convert_standard_metric_value};

/// Missing or constant external telemetry uses the existing neutral gauge range.
pub fn metric_range(range: Option<(f64, f64)>) -> (f64, f64) {
    range.filter(|(min, max)| max > min).unwrap_or((0.0, 100.0))
}

pub fn fill_percentage(value: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

pub fn bar_fill_count(fill01: f32, count: u32) -> usize {
    (fill01.clamp(0.0, 1.0) * count as f32).floor() as usize
}

pub(crate) fn format_gauge_label(
    kind: MetricKind,
    display_unit: Option<&str>,
    value: f64,
) -> String {
    (convert_standard_metric_value(kind, display_unit, value).round() as i64).to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct GaugeFrameState {
    pub fill01: f32,
    pub value_text: String,
}

pub fn sample_gauge(
    widget: &super::validation::ValidatedGauge,
    activity: Option<&crate::activity::ActivitySampler>,
    second: f64,
) -> GaugeFrameState {
    let range = activity.map_or((0.0, 100.0), |activity| {
        activity.gauge_range(widget.metric())
    });
    let sample = activity
        .map(|activity| activity.gauge_sample(widget.metric(), second))
        .unwrap_or_default();
    let offset = activity.map_or(0.0, |activity| {
        activity.gauge_altitude_offset(widget.starting_altitude_m())
    });
    gauge_frame_state(widget, sample, range, offset)
}

/// Both hosts derive gauge fill and inner text from the same canonical sample.
pub fn gauge_frame_state(
    widget: &super::validation::ValidatedGauge,
    mut sample: crate::widgets::value::format::MetricSample<'_>,
    range: (f64, f64),
    altitude_offset_m: f64,
) -> GaugeFrameState {
    use crate::widgets::value::format::{format_metric_parts, resolve_metric_display_value};
    let value = resolve_metric_display_value(widget.metric(), sample.numeric, sample.available)
        .unwrap_or(range.0);
    let value_text = match widget {
        super::validation::ValidatedGauge::Linear(_) => String::new(),
        super::validation::ValidatedGauge::Arc(config) => {
            sample.numeric = sample.numeric.map(|value| value + altitude_offset_m);
            format_metric_parts(&config.inner_value, &sample)
                .standard_text()
                .0
                .to_owned()
        }
    };
    GaugeFrameState {
        fill01: fill_percentage(value, range.0, range.1),
        value_text,
    }
}
