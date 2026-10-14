//! Native dense-frame preparation, fonts, profiling and diagnostics for shared gauges.
use crate::{
    activity::schema::DenseActivityReport,
    debug::RenderProfiler,
    error::CoreResult,
    fonts::NativeFonts,
    normalize::ValidatedSceneConfig,
    render::{
        format::{format_metric_presentation_parts, resolve_metric_display_value},
        widgets::types::{GaugeCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport},
    },
};
use ovrley_widgets::{
    model::types::MetricKind,
    widgets::gauges::{
        arc::{arc_point, prepare_arc_gauge_cache as prepare_arc_layout},
        frame::GaugeLayout,
        linear::prepare_linear_gauge_cache as prepare_linear_layout,
        metric::{fill_percentage, metric_range, GaugeFrameState},
        validation::{ValidatedArcGaugeWidget, ValidatedLinearGaugeWidget},
    },
    widgets::value::format::format_metric_parts,
};
use skia_safe::Canvas;
use std::path::PathBuf;

fn range(activity: &DenseActivityReport, metric: MetricKind) -> (f64, f64) {
    metric_range(activity.full_activity_metric_ranges.get(&metric).copied())
}

fn fill_at(
    activity: &DenseActivityReport,
    metric: MetricKind,
    index: usize,
    range: (f64, f64),
) -> f32 {
    let raw = activity
        .series
        .numeric_series_for(metric)
        .and_then(|series| series.get(index).copied().flatten());
    let value = resolve_metric_display_value(metric, raw, activity).unwrap_or(range.0);
    fill_percentage(value, range.0, range.1)
}

pub fn prepare_linear_gauge_cache(
    gauge: &ValidatedLinearGaugeWidget,
    altitude_offset_m: f64,
    activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    font_dirs: &[PathBuf],
    profiler: &mut RenderProfiler,
) -> CoreResult<GaugeCache> {
    profiler.measure("gauge.linear.prepare", || {
        let range = range(activity, gauge.metric);
        Ok(GaugeCache {
            layout: GaugeLayout::Linear(prepare_linear_layout(
                gauge,
                &scene.presentation,
                range,
                altitude_offset_m,
                &NativeFonts(font_dirs),
            )?),
            frame_states: (0..activity.frame_count)
                .map(|index| GaugeFrameState {
                    fill01: fill_at(activity, gauge.metric, index, range),
                    value_text: String::new(),
                })
                .collect(),
        })
    })
}

pub fn prepare_arc_gauge_cache(
    gauge: &ValidatedArcGaugeWidget,
    altitude_offset_m: f64,
    activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    font_dirs: &[PathBuf],
    profiler: &mut RenderProfiler,
) -> CoreResult<GaugeCache> {
    profiler.measure("gauge.arc.prepare", || {
        let range = range(activity, gauge.metric);
        let parts = format_metric_parts(&gauge.inner_value, &Default::default());
        let unit = parts.standard_text().1;
        Ok(GaugeCache {
            layout: GaugeLayout::Arc(prepare_arc_layout(
                gauge,
                &scene.presentation,
                range,
                altitude_offset_m,
                unit,
                &NativeFonts(font_dirs),
            )?),
            frame_states: (0..activity.frame_count)
                .map(|index| {
                    let parts = format_metric_presentation_parts(
                        &gauge.inner_value,
                        activity,
                        index,
                        altitude_offset_m,
                    )
                    .expect("validated gauge formatter");
                    GaugeFrameState {
                        fill01: fill_at(activity, gauge.metric, index, range),
                        value_text: parts.standard_text().0.to_owned(),
                    }
                })
                .collect(),
        })
    })
}

pub fn draw_gauge_widget(
    canvas: &Canvas,
    cache: &GaugeCache,
    index: usize,
    font_dirs: &[PathBuf],
    profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    profiler.measure("gauge.draw", || {
        let state = cache
            .frame_states
            .get(index)
            .or_else(|| cache.frame_states.last())?
            .clone();
        let frame = cache
            .layout
            .prepare_frame(state, &NativeFonts(font_dirs))
            .expect("prepared gauge frame");
        frame.draw(canvas, &cache.layout);
        let (x, y, width, height, rotation, simplification, marker) = match &cache.layout {
            GaugeLayout::Linear(layout) => (
                layout.x,
                layout.y,
                layout.width,
                layout.height,
                layout.rotation,
                "linear_gauge",
                skia_safe::Point::new(
                    layout.x + layout.width * frame.state.fill01,
                    layout.y + layout.height * (1.0 - frame.state.fill01),
                ),
            ),
            GaugeLayout::Arc(layout) => {
                let geometry = layout.geometry;
                (
                    layout.x,
                    layout.y,
                    layout.width,
                    layout.height,
                    layout.rotation,
                    "arc_gauge",
                    arc_point(
                        geometry.center_x,
                        geometry.center_y,
                        geometry.radius,
                        geometry.start_angle + geometry.sweep_angle * frame.state.fill01,
                    ),
                )
            }
        };
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: simplification.into(),
                bbox: [x, y, width, height],
                widget_width: width.round() as u32,
                widget_height: height.round() as u32,
                rotation_deg: rotation,
            },
            frame: WidgetFrameReport {
                progress01: frame.state.fill01,
                marker_x: marker.x - x,
                marker_y: marker.y - y,
                marker_abs_x: marker.x,
                marker_abs_y: marker.y,
            },
        })
    })
}
