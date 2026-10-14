//! Native composition supplies precomputed frame samples to the shared profile layout.
use super::super::types::{
    ElevationWidgetCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
};
use crate::{debug::RenderProfiler, error::CoreResult};
use skia_safe::Canvas;

pub(crate) fn draw_elevation_widget(
    canvas: &Canvas,
    cache: &ElevationWidgetCache,
    frame_index: usize,
    profiler: &mut RenderProfiler,
) -> CoreResult<Option<WidgetRenderReport>> {
    let Some(state) = cache
        .frame_states
        .get(frame_index.min(cache.frame_states.len().saturating_sub(1)))
    else {
        return Ok(None);
    };
    let layout = &cache.drawing;
    let frame = layout.frame(*state)?;
    profiler.measure("composite.elevation", || layout.draw(canvas, &frame));
    let config = &layout.config;
    let scale = layout.scene.scale;
    let (marker_x, marker_y) = (state.marker_x, state.marker_y);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    Ok(Some(WidgetRenderReport {
        geometry: WidgetGeometryReport {
            point_count: layout.geometry.points.len(),
            source_point_count: layout.geometry.source_point_count,
            simplification: layout.geometry.simplification.clone(),
            bbox: [0.0, 0.0, config.width * scale, config.height * scale],
            widget_width: (config.width * scale).round() as u32,
            widget_height: (config.height * scale).round() as u32,
            rotation_deg: config.rotation,
        },
        frame: WidgetFrameReport {
            progress01: state.progress01,
            marker_x,
            marker_y,
            marker_abs_x: config.x + marker_x * cos - marker_y * sin,
            marker_abs_y: config.y + marker_x * sin + marker_y * cos,
        },
    }))
}
