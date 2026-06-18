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
//! - Boxed metric presentations (heading_tape, and future linear/bars/arc/corner)
//!   are dispatched here.
//!
//! Route and elevation remain separate true graphical widgets outside this
//! metric presentation system.

use crate::activity::schema::DenseActivityReport;
use crate::debug::RenderProfiler;
use crate::render::text::ResolvedTextStyle;
use crate::render::widgets::heading::draw_heading_widget;
use crate::render::widgets::linear_gauge::draw_linear_gauge_widget;
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
/// types like linear, bars, arc, corner).
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
        DisplayType::Bars | DisplayType::Arc | DisplayType::Corner => None,
    }
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
