//! Polyline and area drawing helpers for route and elevation widgets.
//!
//! Owns: `draw_polyline` (stroked line rendering with shadow support),
//!       `draw_polyline_with_shadow` (two-pass shadow+line compositing),
//!       `draw_area` (filled area under an elevation curve).
//! Does not own: point-to-path conversion (see [`super::transform`]), color
//!       parsing (see [`crate::render::text::parse_color`]).
//!
//! Allowed dependencies: `skia_safe`, `super::transform`, `super::types`.
//! Forbidden dependencies: `crate::config`, `crate::activity`,
//!       `crate::encode`, `crate::commands`.
//!
//! ## Performance
//! Called per-frame during video rendering. `draw_polyline_with_shadow` does
//! two Skia draw calls (shadow then line). The path is rebuilt from cached
//! point arrays — no heap allocation inside the draw calls.

use super::transform::path_from_points;
use super::types::ShadowStyle;
use skia_safe::{BlurStyle, Canvas, MaskFilter, Paint, PaintCap, PaintJoin};

pub(crate) fn draw_polyline(
    canvas: &Canvas,
    points: &[(f32, f32)],
    color: &str,
    width: f32,
    opacity: f32,
) {
    draw_polyline_with_shadow(canvas, points, color, width, opacity, None);
}

pub(crate) fn draw_polyline_with_shadow(
    canvas: &Canvas,
    points: &[(f32, f32)],
    color: &str,
    width: f32,
    opacity: f32,
    shadow: Option<&ShadowStyle>,
) {
    if points.len() < 2 {
        return;
    }
    let path = path_from_points(points, false, None);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width.max(1.0));
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(crate::render::text::parse_color(color, opacity));

    if let Some(shadow) = shadow {
        if shadow.strength > 0.0 || shadow.distance != 0.0 {
            let mut shadow_paint = Paint::default();
            shadow_paint.set_anti_alias(true);
            shadow_paint.set_style(skia_safe::paint::Style::Stroke);
            shadow_paint.set_stroke_width(width.max(1.0));
            shadow_paint.set_stroke_cap(PaintCap::Round);
            shadow_paint.set_stroke_join(PaintJoin::Round);
            shadow_paint.set_color(crate::render::text::parse_color(&shadow.color, opacity));
            if shadow.strength > 0.0 {
                shadow_paint.set_mask_filter(MaskFilter::blur(
                    BlurStyle::Normal,
                    shadow.strength,
                    true,
                ));
            }
            canvas.save();
            canvas.translate((shadow.offset_x, shadow.offset_y));
            canvas.draw_path(&path, &shadow_paint);
            canvas.restore();
        }
    }

    canvas.draw_path(&path, &paint);
}

pub(crate) fn draw_area(
    canvas: &Canvas,
    points: &[(f32, f32)],
    baseline_y: f32,
    color: &str,
    opacity: f32,
) {
    if points.len() < 2 {
        return;
    }
    let path = path_from_points(points, true, Some(baseline_y));
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::paint::Style::Fill);
    paint.set_color(crate::render::text::parse_color(color, opacity));
    canvas.draw_path(&path, &paint);
}
