/// Per-frame route widget drawing.
///
/// Composites the static remaining-route layer, draws the completed-route
/// polyline up to the frame's marker position, and draws the marker itself.

use super::super::common::{draw_static_layer, widget_render_report};
use super::super::marker::draw_marker;
use super::super::polyline::draw_polyline;
use super::super::transform::with_widget_transform;
use super::super::types::{RouteWidgetCache, WidgetRenderReport};
use crate::debug::RenderProfiler;
use skia_safe::Canvas;

/// Draws the route widget for one frame and returns preview diagnostics.
pub(crate) fn draw_route_widget(
    canvas: &Canvas,
    route_cache: &RouteWidgetCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    use super::frame_state::build_route_prefix_points;

    let state = route_cache
        .frame_states
        .get(frame_index.min(route_cache.frame_states.len().saturating_sub(1)))?;
    let prefix_points = build_route_prefix_points(&route_cache.geometry, state);

    frame_profiler.measure("composite.route", || {
        with_widget_transform(
            canvas,
            route_cache.plot.x,
            route_cache.plot.y,
            route_cache.plot.rotation,
            |canvas| {
                draw_static_layer(canvas, route_cache.remaining_layer.as_ref());
                draw_polyline(
                    canvas,
                    &prefix_points,
                    &route_cache.plot.completed_line_color,
                    route_cache.plot.completed_line_width,
                    route_cache.plot.completed_line_opacity,
                );
                draw_marker(
                    canvas,
                    &route_cache.marker_layers,
                    state.marker_x,
                    state.marker_y,
                    &route_cache.plot.marker_color,
                    route_cache.plot.marker_size,
                    route_cache.plot.marker_opacity,
                );
            },
        );
    });

    Some(widget_render_report(
        route_cache.plot.x,
        route_cache.plot.y,
        route_cache.plot.width,
        route_cache.plot.height,
        route_cache.plot.rotation,
        &route_cache.geometry,
        state.progress01,
        state.marker_x,
        state.marker_y,
    ))
}
