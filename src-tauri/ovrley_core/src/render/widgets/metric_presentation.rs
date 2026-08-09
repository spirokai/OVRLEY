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
use crate::render::widgets::g_force::draw_g_force_widget;
use crate::render::widgets::gauges::arc::draw_arc_gauge_widget;
use crate::render::widgets::gauges::linear::draw_linear_gauge_widget;
use crate::render::widgets::heading::draw_heading_widget;
use crate::render::widgets::lean_angle::draw_lean_angle_widget;
use crate::render::widgets::types::{
    ArcGaugeCache, GForceWidgetCache, HeadingWidgetCache, LeanAngleCache, LinearGaugeCache,
    PreparedValue, WidgetRenderReport,
};
use crate::types::DisplayType;
use skia_safe::Canvas;

/// Draws a boxed metric presentation for a value widget.
///
/// This is called for prepared boxed values. Matching on [`PreparedValue`] keeps
/// each validated widget paired with its own typed cache, so dispatch cannot
/// observe a cache belonging to another value or presentation mode.
///
/// Returns `Some(WidgetRenderReport)` if the presentation was drawn, or `None`
/// for intrinsic values and a boxed widget whose cache has not been prepared.
pub fn draw_metric_presentation(
    canvas: &Canvas,
    value: &PreparedValue,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[std::path::PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    match value {
        PreparedValue::HeadingTape(widget) => draw_tape_presentation(
            canvas,
            dense_activity,
            frame_index,
            widget.cache.as_ref(),
            frame_profiler,
        ),
        PreparedValue::LinearGauge(widget) => {
            draw_linear_presentation(canvas, widget.cache.as_ref(), frame_index, frame_profiler)
        }
        PreparedValue::ArcGauge(widget) => {
            draw_arc_presentation(canvas, widget.cache.as_ref(), frame_index, frame_profiler)
        }
        PreparedValue::LeanAngle(widget) => draw_lean_angle_presentation(
            canvas,
            widget.cache.as_ref(),
            dense_activity,
            frame_index,
            scale,
            font_dirs,
            frame_profiler,
        ),
        PreparedValue::GForce(widget) => draw_g_force_presentation(
            canvas,
            widget.cache.as_ref(),
            frame_index,
            font_dirs,
            frame_profiler,
        ),
        PreparedValue::StandardText(_)
        | PreparedValue::TimeText(_)
        | PreparedValue::Gradient(_)
        | PreparedValue::LapTimer(_) => None,
    }
}

fn draw_g_force_presentation(
    canvas: &Canvas,
    cache: Option<&GForceWidgetCache>,
    frame_index: usize,
    font_dirs: &[std::path::PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let Some(g_force_cache) = cache else {
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
    cache: Option<&LeanAngleCache>,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[std::path::PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let lean_angle_cache = cache?;
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
    cache: Option<&LinearGaugeCache>,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let gauge_cache = cache?;
    draw_linear_gauge_widget(canvas, gauge_cache, frame_index, frame_profiler)
}

/// Draws the arc gauge presentation for a single frame. The cache owns the
/// static track/unit layer and all dynamic value/fill state.
fn draw_arc_presentation(
    canvas: &Canvas,
    cache: Option<&ArcGaugeCache>,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let gauge_cache = cache?;
    draw_arc_gauge_widget(canvas, gauge_cache, frame_index, frame_profiler)
}

/// Draws the heading tape presentation for a heading metric value.
///
/// The heading tape is a boxed presentation that scrolls a 360-degree compass
/// tape based on the current heading value. The tape image is pre-rendered during
/// preparation and composited per-frame with a scroll offset and clip rect.
fn draw_tape_presentation(
    canvas: &Canvas,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    cache: Option<&HeadingWidgetCache>,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let heading_cache = cache?;

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
