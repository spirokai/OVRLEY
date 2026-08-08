//! DisplayType-driven metric presentation dispatch.
//!
//! This module is the single dispatch seam for metric widget rendering. Every
//! metric value widget routes through [`draw_metric_presentation`] based on its
//! [`DisplayType`], replacing the previous ad hoc special-case pattern where
//! boxed metric presentations escaped the normal value rendering path.
//!
//! ## Design
//!
//! - `type` (MetricKind) selects the telemetry data source.
//! - `display_type` (DisplayType) selects the visual presentation.
//! - Intrinsic text rendering stays in the value module.
//! - Boxed metric presentations (heading_tape, lean_angle, linear, arc, and corner)
//!   are dispatched here.
//!
//! Route and elevation remain separate true graphical widgets outside this
//! metric presentation system.

use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::render::text::ResolvedTextStyle;
use crate::render::widgets::g_force::draw_g_force_widget;
use crate::render::widgets::gauges::arc::draw_arc_gauge_widget;
use crate::render::widgets::gauges::linear::draw_linear_gauge_widget;
use crate::render::widgets::heading::draw_heading_widget;
use crate::render::widgets::lean_angle::draw_lean_angle_widget;
use crate::render::widgets::types::{PresentationCache, WidgetRenderReport};
use crate::types::{DisplayType, MetricKind};
use skia_safe::Canvas;
use std::collections::BTreeMap;

/// Draws a boxed metric presentation for a value widget.
///
/// This is called for non-intrinsic display types (anything other than `Text`).
/// The presentation is selected by `DisplayType`, and the metric kind determines
/// the data source.
///
/// Returns `Some(WidgetRenderReport)` if the presentation was drawn, or `None`
/// if the display type has no boxed rendering implementation yet (future display
/// types like linear, arc, and corner).
#[allow(clippy::too_many_arguments)]
pub fn draw_metric_presentation(
    canvas: &Canvas,
    metric_kind: MetricKind,
    display_type: DisplayType,
    base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[std::path::PathBuf],
    presentation_caches: &BTreeMap<usize, PresentationCache>,
    value_idx: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    match display_type {
        DisplayType::Text => None,
        DisplayType::LapTimer => None,
        DisplayType::Tape => draw_tape_presentation(
            canvas,
            metric_kind,
            base_style,
            dense_activity,
            frame_index,
            scale,
            font_dirs,
            presentation_caches.get(&value_idx),
            frame_profiler,
        ),
        DisplayType::Linear => draw_linear_presentation(
            canvas,
            presentation_caches.get(&value_idx),
            frame_index,
            frame_profiler,
        ),
        DisplayType::Arc | DisplayType::Corner => draw_arc_presentation(
            canvas,
            presentation_caches.get(&value_idx),
            frame_index,
            frame_profiler,
        ),
        DisplayType::LeanAngle => draw_lean_angle_presentation(
            canvas,
            metric_kind,
            presentation_caches.get(&value_idx),
            dense_activity,
            frame_index,
            scale,
            font_dirs,
            frame_profiler,
        ),
        DisplayType::GForce => draw_g_force_presentation(
            canvas,
            metric_kind,
            presentation_caches.get(&value_idx),
            frame_index,
            font_dirs,
            frame_profiler,
        ),
    }
}

fn draw_g_force_presentation(
    canvas: &Canvas,
    metric_kind: MetricKind,
    cache: Option<&PresentationCache>,
    frame_index: usize,
    font_dirs: &[std::path::PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    assert_eq!(
        metric_kind,
        MetricKind::GForce,
        "g_force display type requires the g_force metric"
    );
    let Some(PresentationCache::GForce(g_force_cache)) = cache else {
        panic!("g_force presentation requires its prepared GForce cache");
    };
    draw_g_force_widget(
        canvas,
        g_force_cache,
        frame_index,
        font_dirs,
        frame_profiler,
    )
}

fn draw_lean_angle_presentation(
    canvas: &Canvas,
    metric_kind: MetricKind,
    cache: Option<&PresentationCache>,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[std::path::PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    if metric_kind != MetricKind::LeanAngle {
        return None;
    }
    let PresentationCache::LeanAngle(lean_angle_cache) = cache? else {
        return None;
    };
    draw_lean_angle_widget(
        canvas,
        lean_angle_cache,
        dense_activity,
        frame_index,
        scale,
        font_dirs,
        frame_profiler,
    )
}

/// Draws the linear gauge presentation for a single frame. Delegates
/// per-frame fill-composite rendering to the shared linear gauge module.
fn draw_linear_presentation(
    canvas: &Canvas,
    cache: Option<&PresentationCache>,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let PresentationCache::LinearGauge(gauge_cache) = cache? else {
        return None;
    };
    draw_linear_gauge_widget(canvas, gauge_cache, frame_index, frame_profiler)
}

/// Draws the arc gauge presentation for a single frame. The cache owns the
/// static track/unit layer and all dynamic value/fill state.
fn draw_arc_presentation(
    canvas: &Canvas,
    cache: Option<&PresentationCache>,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let PresentationCache::ArcGauge(gauge_cache) = cache? else {
        return None;
    };
    draw_arc_gauge_widget(canvas, gauge_cache, frame_index, frame_profiler)
}

/// Draws the heading tape presentation for a heading metric value.
///
/// The heading tape is a boxed presentation that scrolls a 360-degree compass
/// tape based on the current heading value. The tape image is pre-rendered during
/// preparation and composited per-frame with a scroll offset and clip rect.
#[allow(clippy::too_many_arguments)]
fn draw_tape_presentation(
    canvas: &Canvas,
    metric_kind: MetricKind,
    _base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    _scale: f32,
    _font_dirs: &[std::path::PathBuf],
    cache: Option<&PresentationCache>,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    if metric_kind != MetricKind::Heading {
        return None;
    }

    let PresentationCache::HeadingTape(heading_cache) = cache? else {
        return None;
    };

    if heading_cache.display_type == DisplayType::Text {
        return None;
    }

    let heading = dense_activity
        .series
        .heading
        .get(frame_index)
        .and_then(|v| *v)
        .unwrap_or(0.0) as f32;

    draw_heading_widget(canvas, heading_cache, heading, frame_profiler)
}
