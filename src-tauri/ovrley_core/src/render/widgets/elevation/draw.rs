/// Per-frame elevation widget drawing.
///
/// # Five-phase composition
///
/// 1. **Fetch** — retrieve precomputed frame state and build completed points.
/// 2. **Static layer** — draw the cached remaining area/line (unchanging per frame).
/// 3. **Completed overlay** — draw the area fill, polyline, and marker for the
///    portion of the profile the marker has already passed.
/// 4. **Labels** — draw metric, imperial, or legacy elevation labels outside the
///    widget transform so text stays upright regardless of rotation.
/// 5. **Report** — build preview diagnostics from widget geometry and frame state.
use super::super::common::{
    draw_static_layer, format_elevation_label, rotate_point_to_canvas, widget_render_report,
};
use super::super::marker::draw_marker;
use super::super::polyline::{draw_area, draw_polyline};
use super::super::transform::with_widget_transform;
use super::super::types::{ElevationWidgetCache, WidgetRenderReport};
use super::frame_state::build_elevation_completed_points;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::paths::AppPaths;
use crate::render::text::{draw_text, parse_color, ResolvedTextStyle};
use skia_safe::Canvas;

/// Draws the elevation widget for one frame and returns preview diagnostics.
pub(crate) fn draw_elevation_widget(
    canvas: &Canvas,
    paths: &AppPaths,
    elevation_cache: &ElevationWidgetCache,
    frame_index: usize,
    scene: &crate::normalize::ValidatedSceneConfig,
    frame_profiler: &mut RenderProfiler,
) -> CoreResult<Option<WidgetRenderReport>> {
    // Phase 1: fetch precomputed frame state and build the completed-points prefix.
    let scene_scale = scene.scale.max(0.1);
    let Some(state) = elevation_cache
        .frame_states
        .get(frame_index.min(elevation_cache.frame_states.len().saturating_sub(1)))
    else {
        return Ok(None);
    };
    let completed_points = build_elevation_completed_points(
        &elevation_cache.geometry.points,
        &elevation_cache.geometry.elapsed_fractions,
        state.frame_elapsed_fraction,
        (state.marker_x, state.marker_y),
    );
    let baseline_y = elevation_cache.plot.height as f32;

    // Phase 2–3: draw the static remaining layer, then the completed area/polyline/marker.
    frame_profiler.measure("composite.elevation", || {
        with_widget_transform(
            canvas,
            elevation_cache.plot.x,
            elevation_cache.plot.y,
            elevation_cache.plot.rotation,
            |canvas| {
                // Phase 2: static remaining layer (drawn first so completed content sits on top).
                draw_static_layer(canvas, elevation_cache.remaining_layer.as_ref());
                // Phase 3: completed overlay — area, polyline, and marker for the traversed portion.
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

    // Phase 4: draw elevation labels outside the widget transform so text orientation
    // stays upright even when the widget itself is rotated.
    for (enabled, unit, offset_x, offset_y) in [
        (
            elevation_cache.plot.show_elevation_metric,
            "metric",
            elevation_cache.plot.metric_label_offset_x,
            elevation_cache.plot.metric_label_offset_y,
        ),
        (
            elevation_cache.plot.show_elevation_imperial,
            "imperial",
            elevation_cache.plot.imperial_label_offset_x,
            elevation_cache.plot.imperial_label_offset_y,
        ),
    ] {
        if enabled {
            frame_profiler.measure("text.elevation_label", || -> CoreResult<()> {
                draw_elevation_label(
                    canvas,
                    paths,
                    elevation_cache,
                    state.elevation_m,
                    unit,
                    marker_abs_x,
                    marker_abs_y,
                    offset_x,
                    offset_y,
                    scene,
                    scene_scale,
                )
            })?;
        }
    }

    // Phase 5: build preview diagnostics from widget geometry and current frame state.
    Ok(Some(widget_render_report(
        elevation_cache.plot.x,
        elevation_cache.plot.y,
        elevation_cache.plot.width,
        elevation_cache.plot.height,
        elevation_cache.plot.rotation,
        &elevation_cache.geometry,
        state.progress01,
        state.marker_x,
        state.marker_y,
    )))
}

fn draw_elevation_label(
    canvas: &Canvas,
    paths: &AppPaths,
    elevation_cache: &ElevationWidgetCache,
    elevation_m: f64,
    unit: &str,
    base_x: f32,
    base_y: f32,
    offset_x: f32,
    offset_y: f32,
    scene: &crate::normalize::ValidatedSceneConfig,
    scene_scale: f32,
) -> CoreResult<()> {
    let text = format_elevation_label(elevation_m, unit, None);
    let style = elevation_label_style(
        elevation_cache,
        base_x + offset_x,
        base_y + offset_y,
        scene,
        scene_scale,
    );
    draw_text(canvas, &text, &style, &paths.font_dirs)?;
    Ok(())
}

fn elevation_label_style(
    elevation_cache: &ElevationWidgetCache,
    x: f32,
    y: f32,
    scene: &crate::normalize::ValidatedSceneConfig,
    scene_scale: f32,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x,
        y,
        font_name: elevation_cache
            .plot
            .label_font
            .clone()
            .or_else(|| scene.font.clone()),
        font_size: elevation_cache.plot.label_font_size,
        line_height: elevation_cache.plot.label_font_size * 0.92,
        color: parse_color(&elevation_cache.plot.label_color, 1.0),
        opacity: 1.0,
        shadow_color: if scene.shadow_color.is_empty() {
            None
        } else {
            Some(parse_color(&scene.shadow_color, 1.0))
        },
        shadow_strength: scene.shadow_strength * scene_scale,
        shadow_distance: scene.shadow_distance * scene_scale,
        border_color: if scene.border_color.is_empty() {
            None
        } else {
            Some(parse_color(&scene.border_color, 1.0))
        },
        border_thickness: scene.border_thickness * scene_scale,
    }
}
