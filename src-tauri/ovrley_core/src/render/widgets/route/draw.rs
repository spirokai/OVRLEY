//! Native composition calls the shared route layout with precomputed export progress.
use super::super::types::{
    RouteWidgetCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
};
use crate::debug::RenderProfiler;
use skia_safe::Canvas;

pub(crate) fn draw_route_widget(
    canvas: &Canvas,
    cache: &RouteWidgetCache,
    frame_index: usize,
    profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let progress = *cache
        .frame_progress
        .get(frame_index.min(cache.frame_progress.len().saturating_sub(1)))?;
    let layout = &cache.drawing;
    let frame = layout.frame(progress);
    profiler.measure("composite.route", || layout.draw(canvas, &frame));
    let config = &layout.config;
    let scale = layout.scene.scale;
    let marker_x = frame.state.marker_x * scale;
    let marker_y = frame.state.marker_y * scale;
    let width = (config.width * scale).round() as u32;
    let height = (config.height * scale).round() as u32;
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let marker_abs_x = config.x + marker_x * cos - marker_y * sin;
    let marker_abs_y = config.y + marker_x * sin + marker_y * cos;
    Some(WidgetRenderReport {
        geometry: WidgetGeometryReport {
            point_count: layout.geometry.points.len(),
            source_point_count: layout.geometry.source_point_count,
            simplification: layout.geometry.simplification.clone(),
            bbox: [0.0, 0.0, config.width * scale, config.height * scale],
            widget_width: width,
            widget_height: height,
            rotation_deg: config.rotation,
        },
        frame: WidgetFrameReport {
            progress01: progress,
            marker_x,
            marker_y,
            marker_abs_x,
            marker_abs_y,
        },
    })
}
