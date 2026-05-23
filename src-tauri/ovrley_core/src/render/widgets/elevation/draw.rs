/// Per-frame elevation widget drawing.
///
/// Composites the static remaining-area/line layer, draws the completed
/// polyline and area fill up to the frame's marker position, draws the
/// marker itself, and renders optional metric/imperial/legacy labels.

use super::super::common::{
    draw_static_layer, format_elevation_label, rotate_point_to_canvas, widget_render_report,
};
use super::super::marker::draw_marker;
use super::super::polyline::{draw_area, draw_polyline};
use super::super::transform::with_widget_transform;
use super::super::types::{ElevationWidgetCache, WidgetRenderReport};
use super::frame_state::build_elevation_completed_points;
use crate::paths::AppPaths;
use crate::config::RenderConfig;
use crate::debug::RenderProfiler;
use crate::render::text::{draw_text, parse_color, ResolvedTextStyle};
use skia_safe::Canvas;

/// Draws the elevation widget for one frame and returns preview diagnostics.
pub(crate) fn draw_elevation_widget(
    canvas: &Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    elevation_cache: &ElevationWidgetCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let scene_scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let state = elevation_cache
        .frame_states
        .get(frame_index.min(elevation_cache.frame_states.len().saturating_sub(1)))?;
    let completed_points = build_elevation_completed_points(
        &elevation_cache.geometry.points,
        &elevation_cache.geometry.progress_values,
        state.progress01,
        (state.marker_x, state.marker_y),
    );
    let baseline_y = elevation_cache.plot.height as f32;

    frame_profiler.measure("composite.elevation", || {
        with_widget_transform(
            canvas,
            elevation_cache.plot.x,
            elevation_cache.plot.y,
            elevation_cache.plot.rotation,
            |canvas| {
                draw_static_layer(canvas, elevation_cache.remaining_layer.as_ref());
                draw_area(
                    canvas,
                    &completed_points,
                    baseline_y,
                    &elevation_cache.plot.area_completed_color,
                    elevation_cache.plot.area_completed_opacity,
                );
                draw_polyline(
                    canvas,
                    &completed_points,
                    &elevation_cache.plot.completed_line_color,
                    elevation_cache.plot.completed_line_width,
                    elevation_cache.plot.completed_line_opacity,
                );
                draw_marker(
                    canvas,
                    &elevation_cache.marker_layers,
                    state.marker_x,
                    state.marker_y,
                    &elevation_cache.plot.marker_color,
                    elevation_cache.plot.marker_size,
                    elevation_cache.plot.marker_opacity,
                );
            },
        );
    });

    let (marker_abs_x, marker_abs_y) = rotate_point_to_canvas(
        state.marker_x,
        state.marker_y,
        elevation_cache.plot.x,
        elevation_cache.plot.y,
        elevation_cache.plot.width as f32,
        elevation_cache.plot.height as f32,
        elevation_cache.plot.rotation,
    );

    if elevation_cache.plot.show_elevation_metric {
        frame_profiler.measure("text.elevation_label", || {
            draw_elevation_label(
                canvas,
                paths,
                config,
                elevation_cache,
                state.elevation_m,
                "metric",
                marker_abs_x,
                marker_abs_y,
                elevation_cache.plot.metric_label_offset_x,
                elevation_cache.plot.metric_label_offset_y,
                scene_scale,
            );
        });
    }

    if elevation_cache.plot.show_elevation_imperial {
        frame_profiler.measure("text.elevation_label", || {
            draw_elevation_label(
                canvas,
                paths,
                config,
                elevation_cache,
                state.elevation_m,
                "imperial",
                marker_abs_x,
                marker_abs_y,
                elevation_cache.plot.imperial_label_offset_x,
                elevation_cache.plot.imperial_label_offset_y,
                scene_scale,
            );
        });
    }

    if !elevation_cache.plot.legacy_label_units.is_empty()
        && !elevation_cache.plot.show_elevation_metric
        && !elevation_cache.plot.show_elevation_imperial
    {
        frame_profiler.measure("text.elevation_label", || {
            let text = elevation_cache
                .plot
                .legacy_label_units
                .iter()
                .map(|unit| {
                    format_elevation_label(
                        state.elevation_m,
                        unit,
                        elevation_cache.plot.label_decimal_rounding,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let style = elevation_label_style(
                config,
                elevation_cache,
                marker_abs_x + elevation_cache.plot.metric_label_offset_x,
                marker_abs_y + elevation_cache.plot.metric_label_offset_y,
                scene_scale,
            );
            draw_text(canvas, &text, &style, &paths.font_dirs);
        });
    }

    Some(widget_render_report(
        elevation_cache.plot.x,
        elevation_cache.plot.y,
        elevation_cache.plot.width,
        elevation_cache.plot.height,
        elevation_cache.plot.rotation,
        &elevation_cache.geometry,
        state.progress01,
        state.marker_x,
        state.marker_y,
    ))
}

fn draw_elevation_label(
    canvas: &Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    elevation_cache: &ElevationWidgetCache,
    elevation_m: f64,
    unit: &str,
    base_x: f32,
    base_y: f32,
    offset_x: f32,
    offset_y: f32,
    scene_scale: f32,
) {
    let text = format_elevation_label(
        elevation_m,
        unit,
        elevation_cache.plot.label_decimal_rounding,
    );
    let style = elevation_label_style(config, elevation_cache, base_x + offset_x, base_y + offset_y, scene_scale);
    draw_text(canvas, &text, &style, &paths.font_dirs);
}

fn elevation_label_style(
    config: &RenderConfig,
    elevation_cache: &ElevationWidgetCache,
    x: f32,
    y: f32,
    scene_scale: f32,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x,
        y,
        font_name: elevation_cache
            .plot
            .label_font
            .clone()
            .or_else(|| config.scene.font.clone()),
        font_size: elevation_cache.plot.label_font_size,
        line_height: elevation_cache.plot.label_font_size * 0.92,
        color: parse_color(&elevation_cache.plot.label_color, 1.0),
        opacity: 1.0,
        shadow_color: config
            .scene
            .shadow_color
            .as_deref()
            .map(|color| parse_color(color, 1.0)),
        shadow_strength: config.scene.shadow_strength.unwrap_or(0.0) * scene_scale,
        shadow_distance: config.scene.shadow_distance.unwrap_or(0.0) * scene_scale,
        border_color: config
            .scene
            .border_color
            .as_deref()
            .map(|color| parse_color(color, 1.0)),
        border_thickness: config.scene.border_thickness.unwrap_or(0.0) * scene_scale,
        border_distance: config.scene.border_distance.unwrap_or(0.0) * scene_scale,
    }
}
