//! Heading widget per-frame drawing.
//!
//! Per-frame: draw the cached tape image offset by `heading × pixels_per_degree`
//! with a clip rect confining output to the widget bounds. The tape wraps by
//! drawing twice with the tape width as offset. After the tape, draw the
//! indicator (chevron or highlight bar) at the widget's horizontal center.

use super::super::types::{
    HeadingWidgetCache, WidgetFrameReport, WidgetGeometryReport, WidgetRenderReport,
};
use super::geometry::chevron_vertices;
use crate::debug::RenderProfiler;
use crate::render::text::parse_color;
use skia_safe::{Canvas, Paint, Path, Point, Rect};

/// Draws the heading widget for one frame.
///
/// The cached tape image is drawn with an X offset determined by the current
/// heading value. Two draws handle the 0°/360° wrap seamlessly. After the
/// tape, the indicator is drawn at the widget's horizontal center.
pub fn draw_heading_widget(
    canvas: &Canvas,
    heading_cache: &HeadingWidgetCache,
    heading: f32,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    frame_profiler.measure("heading.draw", || {
        let offset = heading * heading_cache.pixels_per_degree;

        // Clip to widget bounds
        canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                heading_cache.x,
                heading_cache.y,
                heading_cache.width as f32,
                heading_cache.height as f32,
            ),
            skia_safe::ClipOp::Intersect,
            false,
        );

        // Draw the tape image at the offset position.
        // Draw twice to handle the 360° wrap: once at offset, once at offset + tape_width.
        canvas.draw_image(
            &heading_cache.tape_image,
            (heading_cache.x - offset, heading_cache.y),
            None,
        );
        canvas.draw_image(
            &heading_cache.tape_image,
            (
                heading_cache.x - offset + heading_cache.tape_width,
                heading_cache.y,
            ),
            None,
        );

        // Draw the indicator on top of the tape
        draw_indicator(canvas, heading_cache);

        canvas.restore();

        // Return a minimal render report
        Some(WidgetRenderReport {
            geometry: WidgetGeometryReport {
                point_count: 0,
                source_point_count: 0,
                simplification: "heading_tape".to_string(),
                bbox: [
                    heading_cache.x,
                    heading_cache.y,
                    heading_cache.width as f32,
                    heading_cache.height as f32,
                ],
                widget_width: heading_cache.width,
                widget_height: heading_cache.height,
                rotation_deg: 0.0,
            },
            frame: WidgetFrameReport {
                progress01: heading / 360.0,
                marker_x: heading_cache.width as f32 / 2.0,
                marker_y: heading_cache.height as f32 / 2.0,
                marker_abs_x: heading_cache.x + heading_cache.width as f32 / 2.0,
                marker_abs_y: heading_cache.y + heading_cache.height as f32 / 2.0,
            },
        })
    })
}

/// Draws the indicator overlay (chevron or highlight bar) at the widget center.
fn draw_indicator(canvas: &Canvas, cache: &HeadingWidgetCache) {
    if !cache.show_indicator {
        return;
    }

    let center_x = cache.x + cache.width as f32 / 2.0;
    let top_y = cache.y;
    let bottom_y = cache.y + cache.height as f32;
    let color = parse_color(&cache.indicator_color, 1.0);

    match cache.indicator_style.as_str() {
        "chevron" => draw_chevron_indicator(canvas, cache, center_x, top_y, bottom_y, color),
        "highlight_bar" => {
            draw_highlight_bar_indicator(canvas, cache, center_x, top_y, bottom_y, color)
        }
        _ => {}
    }
}

/// Draws a chevron (filled triangle) indicator at the configured placement edges.
fn draw_chevron_indicator(
    canvas: &Canvas,
    cache: &HeadingWidgetCache,
    center_x: f32,
    top_y: f32,
    bottom_y: f32,
    color: skia_safe::Color,
) {
    let size = cache.indicator_size;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);

    let draw_chevron = |edge_y: f32, pointing_down: bool| {
        let verts = chevron_vertices(center_x - cache.x, edge_y - cache.y, size, pointing_down);
        let mut path = Path::new();
        path.move_to(Point::new(verts[0].x + cache.x, verts[0].y + cache.y));
        path.line_to(Point::new(verts[1].x + cache.x, verts[1].y + cache.y));
        path.line_to(Point::new(verts[2].x + cache.x, verts[2].y + cache.y));
        path.close();
        canvas.draw_path(&path, &paint);
    };

    match cache.indicator_placement.as_str() {
        "top" => draw_chevron(top_y, true),
        "bottom" => draw_chevron(bottom_y, false),
        "both" => {
            draw_chevron(top_y, true);
            draw_chevron(bottom_y, false);
        }
        _ => {}
    }

    // Apply shadow if configured
    if let Some(ref shadow) = cache.indicator_shadow {
        if shadow.strength > 0.0 || shadow.distance != 0.0 {
            if let Some(filter) = skia_safe::image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, 1.0),
                None,
                None,
            ) {
                let mut shadow_paint = Paint::default();
                shadow_paint.set_anti_alias(true);
                shadow_paint.set_color(color);
                shadow_paint.set_image_filter(filter);

                let draw_shadow = |edge_y: f32, pointing_down: bool| {
                    let verts =
                        chevron_vertices(center_x - cache.x, edge_y - cache.y, size, pointing_down);
                    let mut path = Path::new();
                    path.move_to(Point::new(verts[0].x + cache.x, verts[0].y + cache.y));
                    path.line_to(Point::new(verts[1].x + cache.x, verts[1].y + cache.y));
                    path.line_to(Point::new(verts[2].x + cache.x, verts[2].y + cache.y));
                    path.close();
                    canvas.draw_path(&path, &shadow_paint);
                };

                match cache.indicator_placement.as_str() {
                    "top" => draw_shadow(top_y, true),
                    "bottom" => draw_shadow(bottom_y, false),
                    "both" => {
                        draw_shadow(top_y, true);
                        draw_shadow(bottom_y, false);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Draws a highlight bar indicator: a semi-transparent vertical band spanning
/// the full tape height.
fn draw_highlight_bar_indicator(
    canvas: &Canvas,
    cache: &HeadingWidgetCache,
    center_x: f32,
    top_y: f32,
    bottom_y: f32,
    color: skia_safe::Color,
) {
    let bar_half_width = cache.indicator_size / 2.0;
    let bar_left = center_x - bar_half_width;
    let bar_width = cache.indicator_size;

    // Draw the semi-transparent bar (alpha = 0.3)
    let mut bar_paint = Paint::default();
    bar_paint.set_anti_alias(true);
    bar_paint.set_color(color);
    bar_paint.set_alpha(76); // 0.3 * 255 ≈ 76

    canvas.draw_rect(
        Rect::from_xywh(bar_left, top_y, bar_width, bottom_y - top_y),
        &bar_paint,
    );

    // Apply shadow if configured
    if let Some(ref shadow) = cache.indicator_shadow {
        if shadow.strength > 0.0 || shadow.distance != 0.0 {
            if let Some(filter) = skia_safe::image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, 1.0),
                None,
                None,
            ) {
                let mut shadow_bar_paint = Paint::default();
                shadow_bar_paint.set_anti_alias(true);
                shadow_bar_paint.set_color(color);
                shadow_bar_paint.set_alpha(76);
                shadow_bar_paint.set_image_filter(filter);

                canvas.draw_rect(
                    Rect::from_xywh(bar_left, top_y, bar_width, bottom_y - top_y),
                    &shadow_bar_paint,
                );
            }
        }
    }
}
