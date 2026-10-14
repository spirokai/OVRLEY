//! Native font acquisition, profiling and diagnostics for the shared compass tape.
use crate::{
    debug::RenderProfiler,
    error::CoreResult,
    fonts::NativeFonts,
    normalize::ValidatedSceneConfig,
    render::widgets::types::{WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport},
};
use ovrley_widgets::widgets::heading::{layout::HeadingCache, validation::ValidatedHeading};
use skia_safe::Canvas;
use std::path::PathBuf;

pub fn prepare_heading_cache(
    scene: &ValidatedSceneConfig,
    heading: &ValidatedHeading,
    font_dirs: &[PathBuf],
    profiler: &mut RenderProfiler,
) -> CoreResult<HeadingCache> {
    profiler.measure("heading_cache.total", || {
        Ok(
            ovrley_widgets::widgets::heading::layout::prepare_heading_cache(
                heading,
                &scene.presentation,
                &NativeFonts(font_dirs),
            )?,
        )
    })
}

pub fn draw_heading_widget(
    canvas: &Canvas,
    cache: &HeadingCache,
    heading: f32,
    profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    profiler.measure("heading.draw", || {
        let frame = cache.prepare_frame(Some(heading as f64));
        frame.draw(canvas, cache);
        let bounds = cache.frame;
        let marker_x = bounds.width() * 0.5;
        let marker_y = cache.tape_body_y + cache.tape_body_height * 0.5;
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "heading_tape".into(),
                bbox: [bounds.left, bounds.top, bounds.width(), bounds.height()],
                widget_width: bounds.width().ceil() as u32,
                widget_height: bounds.height().ceil() as u32,
                rotation_deg: 0.0,
            },
            frame: WidgetFrameReport {
                progress01: heading / 360.0,
                marker_x,
                marker_y,
                marker_abs_x: bounds.left + marker_x,
                marker_abs_y: bounds.top + marker_y,
            },
        })
    })
}
