//! Native dense samples, font resources and diagnostics around the shared renderer.
use crate::{
    debug::RenderProfiler,
    fonts::NativeFonts,
    render::widgets::types::{
        GForceWidgetCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
    },
};
use ovrley_widgets::widgets::g_force::format::GForceSample;
use skia_safe::Canvas;
use std::path::PathBuf;

pub fn draw_g_force_widget(
    canvas: &Canvas,
    cache: &GForceWidgetCache,
    frame_index: usize,
    font_dirs: &[PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("g_force.draw", || {
        let drawing = &cache.drawing;
        let frame = drawing
            .prepare_frame(
                GForceSample {
                    horizontal: cache.horizontal_values.get(frame_index).copied().flatten(),
                    vertical: cache.vertical_values.get(frame_index).copied().flatten(),
                    max_g: cache.max_g,
                },
                &NativeFonts(font_dirs),
            )
            .ok()?;
        frame.draw(canvas, drawing);
        let state = frame.state;
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: cache.horizontal_values.len(),
                simplification: "g_force".into(),
                bbox: [
                    drawing.x,
                    drawing.y,
                    drawing.layout.width,
                    drawing.layout.height,
                ],
                widget_width: drawing.layout.width.ceil() as u32,
                widget_height: drawing.layout.height.ceil() as u32,
                rotation_deg: 0.0,
            },
            frame: WidgetFrameReport {
                progress01: state
                    .magnitude
                    .filter(|_| cache.max_g > 0.0)
                    .map_or(0.0, |magnitude| (magnitude / cache.max_g).min(1.0) as f32),
                marker_x: state.marker_x,
                marker_y: state.marker_y,
                marker_abs_x: drawing.x + state.marker_x,
                marker_abs_y: drawing.y + state.marker_y,
            },
        })
    })
}
