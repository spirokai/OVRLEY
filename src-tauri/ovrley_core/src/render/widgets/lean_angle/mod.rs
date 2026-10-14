//! Native activity, font resources and diagnostics around the shared lean-angle renderer.
use crate::{
    activity::schema::DenseActivityReport,
    debug::RenderProfiler,
    fonts::NativeFonts,
    render::widgets::types::{WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport},
};
use ovrley_widgets::widgets::lean_angle::layout::{LeanAngleCache, LEAN_ANGLE_MAX_FILL_SWEEP};
use skia_safe::Canvas;
use std::path::PathBuf;

pub fn draw_lean_angle_widget(
    canvas: &Canvas,
    cache: &LeanAngleCache,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    font_dirs: &[PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("lean_angle.draw", || {
        let raw = dense_activity
            .series
            .lean_angle
            .get(frame_index)
            .copied()
            .flatten();
        let frame = cache.prepare_frame(raw, &NativeFonts(font_dirs)).ok()?;
        frame.draw(canvas, cache);
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "lean_angle".into(),
                bbox: [cache.x, cache.y, cache.layout.width, cache.layout.height],
                widget_width: cache.width,
                widget_height: cache.height,
                rotation_deg: cache.rotation,
            },
            frame: WidgetFrameReport {
                progress01: frame.fill_sweep.abs() / LEAN_ANGLE_MAX_FILL_SWEEP,
                marker_x: frame.marker.x - cache.x,
                marker_y: frame.marker.y - cache.y,
                marker_abs_x: frame.marker.x,
                marker_abs_y: frame.marker.y,
            },
        })
    })
}
