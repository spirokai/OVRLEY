use super::frame_state::g_force_frame_state;
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::render::text::{draw_text, measure_text, parse_color, ResolvedTextStyle};
use crate::render::widgets::types::{
    GForceWidgetCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
};
use crate::standard_widgets::widget_font_weight;
use skia_safe::{Canvas, Paint, Point};
use std::path::PathBuf;

pub fn draw_g_force_widget(
    canvas: &Canvas,
    cache: &GForceWidgetCache,
    frame_index: usize,
    font_dirs: &[PathBuf],
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("g_force.draw", || {
        let horizontal = cache.horizontal_values.get(frame_index).copied().flatten();
        let vertical = cache.vertical_values.get(frame_index).copied().flatten();
        let state = g_force_frame_state(
            horizontal,
            vertical,
            cache.max_g,
            cache.center_x,
            cache.center_y,
            cache.radius,
            cache.label_decimals,
        );

        canvas.save();
        canvas.translate((
            cache.x + cache.width as f32 * 0.5,
            cache.y + cache.height as f32 * 0.5,
        ));
        canvas.translate((-(cache.width as f32) * 0.5, -(cache.height as f32) * 0.5));
        canvas.draw_image(
            &cache.parent_circle_image,
            (cache.parent_circle_image_x, cache.parent_circle_image_y),
            None,
        );
        draw_marker(canvas, cache, state.marker_x, state.marker_y);
        if draw_label(
            canvas,
            cache,
            &state.label,
            state.magnitude.is_some(),
            font_dirs,
        )
        .is_err()
        {
            canvas.restore();
            return None;
        }
        canvas.restore();

        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: cache.horizontal_values.len(),
                simplification: "g_force".to_string(),
                bbox: [cache.x, cache.y, cache.width as f32, cache.height as f32],
                widget_width: cache.width,
                widget_height: cache.height,
                rotation_deg: 0.0,
            },
            frame: WidgetFrameReport {
                progress01: state
                    .magnitude
                    .filter(|_| cache.max_g > 0.0)
                    .map_or(0.0, |magnitude| (magnitude / cache.max_g).min(1.0) as f32),
                marker_x: state.marker_x,
                marker_y: state.marker_y,
                marker_abs_x: cache.x + state.marker_x,
                marker_abs_y: cache.y + state.marker_y,
            },
        })
    })
}

fn draw_marker(canvas: &Canvas, cache: &GForceWidgetCache, x: f32, y: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(
        &cache.marker_color,
        cache.marker_opacity * cache.opacity,
    ));
    canvas.draw_circle(Point::new(x, y), cache.marker_radius, &paint);
}

fn draw_label(
    canvas: &Canvas,
    cache: &GForceWidgetCache,
    text: &str,
    has_value: bool,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    let value_text = if has_value {
        text.strip_suffix(" G").expect("formatted G-force text")
    } else {
        text
    };
    let unit_text = if has_value {
        cache.label_unit.as_str()
    } else {
        ""
    };
    let line_height = cache.label_font_size * 0.92;
    let shadow_color = cache
        .shadow
        .as_ref()
        .map(|shadow| parse_color(&shadow.color, cache.opacity));
    let mut value_style = ResolvedTextStyle {
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        x: 0.0,
        y: 0.0,
        font_name: Some(cache.label_font.clone()),
        font_size: cache.label_font_size,
        line_height,
        color: parse_color(&cache.label_color, cache.opacity),
        opacity: cache.opacity,
        shadow_color,
        shadow_strength: cache.shadow.as_ref().map_or(0.0, |shadow| shadow.strength),
        shadow_distance: cache.shadow.as_ref().map_or(0.0, |shadow| shadow.distance),
        border_color: None,
        border_thickness: 0.0,
    };
    let value_width = measure_text(value_text, &value_style, font_dirs)?.width;
    let unit_gap = if unit_text.is_empty() { 0.0 } else { 3.0 };
    let margin = cache.label_font_size * 0.05;
    let top = cache.center_y + cache.radius - line_height - margin + cache.label_offset_y;
    value_style.x = cache.center_x + cache.radius + margin + cache.label_offset_x;
    value_style.y = top;
    draw_text(canvas, value_text, &value_style, font_dirs)?;

    if !unit_text.is_empty() {
        let unit_style = ResolvedTextStyle {
            x: value_style.x + value_width + unit_gap,
            color: parse_color(&cache.label_unit_color, cache.opacity),
            ..value_style
        };
        draw_text(canvas, unit_text, &unit_style, font_dirs)?;
    }
    Ok(())
}
