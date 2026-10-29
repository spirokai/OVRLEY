//! Static track layers and dynamic fill rendering for linear gauges.
//!
//! Empty track, border, shadow, and min/max labels are rasterized once into a
//! cached image. Per-frame work is limited to compositing that image and
//! painting the current continuous fill or set of complete bars.

use super::super::metric::bar_fill_count;
use super::super::track_path::{translated_track_cap_path, translated_track_cap_reveal};
use super::geometry::{bordered_bar_fill_rect, segment_rect, track_cap_frame};
use super::LinearGaugeCache;
use crate::model::scene::ValidatedScenePresentation;
use crate::model::types::TrackFillStyle;
use crate::render::{shadow::shadow_paint, text::parse_color, track_fill::FillShape};
use crate::widgets::gauges::bar_geometry::ResolvedBarGeometry;
use crate::widgets::gauges::validation::{
    ValidatedLinearGaugeOrientation, ValidatedLinearGaugeWidget,
};
use skia_safe::{BlendMode, Canvas, ClipOp, Paint, PathBuilder, PathFillType, RRect, Rect};

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedScenePresentation,
    width: f32,
    height: f32,
    scale: f32,
) {
    let width_px = width;
    let height_px = height;
    let radius = gauge.track_corner_radius * scale;
    let border = gauge.track_border_thickness * scale;
    let mut source = Paint::default();
    source.set_anti_alias(true);
    let shadow = if border > 0.0 {
        shadow_paint(
            &source,
            parse_color(&scene.shadow_color, scene.opacity),
            scene.shadow_strength * scale,
        )
    } else {
        None
    };

    if let Some(bar_geometry) = gauge.bar_geometry {
        for index in 0..bar_geometry.count {
            let rect = segment_rect(
                index,
                bar_geometry.extent * scale,
                bar_geometry.gap * scale,
                width_px,
                height_px,
                gauge.orientation,
            );
            draw_static_track(
                canvas,
                gauge,
                rect,
                radius,
                border,
                scene.opacity,
                shadow
                    .as_ref()
                    .map(|paint| (paint, scene.shadow_distance * scale)),
            );
        }
    } else {
        draw_static_track(
            canvas,
            gauge,
            Rect::from_xywh(0.0, 0.0, width_px, height_px),
            radius,
            border,
            scene.opacity,
            shadow
                .as_ref()
                .map(|paint| (paint, scene.shadow_distance * scale)),
        );
    }
}

fn draw_static_track(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    rect: Rect,
    configured_radius: f32,
    border: f32,
    opacity: f32,
    shadow: Option<(&Paint, f32)>,
) {
    let radius = configured_radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    let outer_rrect = RRect::new_rect_xy(rect, radius, radius);
    let inner_rect = Rect::from_xywh(
        rect.left + border,
        rect.top + border,
        (rect.width() - border * 2.0).max(0.0),
        (rect.height() - border * 2.0).max(0.0),
    );
    let inner_radius = (radius - border).max(0.0);
    let inner_rrect = RRect::new_rect_xy(inner_rect, inner_radius, inner_radius);

    if let Some((paint, distance)) = shadow {
        canvas.save();
        canvas.translate((distance, distance));
        if border > 0.0 {
            let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
            ring_path.add_rrect(outer_rrect, None, None);
            ring_path.add_rrect(inner_rrect, None, None);
            canvas.draw_path(&ring_path.detach(), paint);
        } else {
            canvas.draw_rrect(outer_rrect, paint);
        }
        canvas.restore();
    }

    if border > 0.0 {
        let mut border_paint = Paint::default();
        border_paint.set_anti_alias(true);
        border_paint.set_color(parse_color(&gauge.track_border_color, opacity));
        canvas.draw_rrect(outer_rrect, &border_paint);

        let mut clear_paint = Paint::default();
        clear_paint.set_anti_alias(true);
        clear_paint.set_blend_mode(BlendMode::Clear);
        canvas.draw_rrect(inner_rrect, &clear_paint);
    }

    let mut empty_paint = Paint::default();
    empty_paint.set_anti_alias(true);
    empty_paint.set_color(parse_color(
        &gauge.track_empty_color,
        gauge.track_empty_opacity * opacity,
    ));
    canvas.draw_rrect(inner_rrect, &empty_paint);
}

/// Inner rounded rect of one bar, inset by the border, relative to `origin`.
fn bar_fill_rrect(
    index: u32,
    bars: ResolvedBarGeometry,
    cache: &LinearGaugeCache,
    origin: (f32, f32),
) -> RRect {
    let outer = segment_rect(
        index,
        bars.extent,
        bars.gap,
        cache.width,
        cache.height,
        cache.orientation,
    );
    let border = cache.track_border_thickness;
    let inner = Rect::from_xywh(
        origin.0 + outer.left + border,
        origin.1 + outer.top + border,
        (outer.width() - border * 2.0).max(0.0),
        (outer.height() - border * 2.0).max(0.0),
    );
    let radius = (cache.track_corner_radius - border)
        .max(0.0)
        .min(inner.width() * 0.5)
        .min(inner.height() * 0.5);
    RRect::new_rect_xy(inner, radius, radius)
}

/// The border-inset continuous track relative to `origin`.
fn inner_track_rrect(cache: &LinearGaugeCache, origin: (f32, f32)) -> RRect {
    let inset = cache.track_border_thickness;
    let inner_rect = Rect::from_xywh(
        origin.0 + inset,
        origin.1 + inset,
        (cache.width - inset * 2.0).max(0.0),
        (cache.height - inset * 2.0).max(0.0),
    );
    let radius = (cache.track_corner_radius - inset).max(0.0);
    RRect::new_rect_xy(inner_rect, radius, radius)
}

/// Paints the completely filled track relative to the gauge origin; per-frame clips
/// reveal this cached gradient raster instead of shading the gradient every frame.
pub(super) fn draw_full_fill_layer(canvas: &Canvas, cache: &LinearGaugeCache, paint: &Paint) {
    match (cache.track_fill_style, cache.bar_geometry) {
        (TrackFillStyle::Bars, Some(bars)) => {
            for index in 0..bars.count {
                canvas.draw_rrect(bar_fill_rrect(index, bars, cache, (0.0, 0.0)), paint);
            }
        }
        _ => {
            canvas.draw_rrect(inner_track_rrect(cache, (0.0, 0.0)), paint);
        }
    }
}

/// Draws only complete bars whose discrete buckets are active at `fill01`.
pub(super) fn draw_segmented_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let bars = cache
        .bar_geometry
        .expect("bars fill style must carry resolved bar geometry");
    let filled_count = bar_fill_count(fill01, bars.count) as u32;
    if filled_count == 0 {
        return;
    }
    // Gaps are empty in the full-fill raster, so one rect ending mid-gap reveals whole bars.
    let segment = |index| {
        segment_rect(
            index,
            bars.extent,
            bars.gap,
            cache.width,
            cache.height,
            cache.orientation,
        )
    };
    let mut reveal = segment(0);
    reveal.join(segment(filled_count - 1));
    reveal.outset((bars.gap * 0.5, bars.gap * 0.5));
    reveal.offset((cache.x, cache.y));
    cache.fill.draw(canvas, &FillShape::Rect(reveal), |paint| {
        for index in 0..filled_count {
            canvas.draw_rrect(
                bar_fill_rrect(index, bars, cache, (cache.x, cache.y)),
                paint,
            );
        }
    });
}

/// Reveals the continuous source track while preserving its configured caps.
pub(super) fn draw_continuous_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let (x, y, width, height) = bordered_bar_fill_rect(
        cache.x,
        cache.y,
        cache.width,
        cache.height,
        fill01,
        cache.orientation,
        cache.track_border_thickness,
    );
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let fill_rect = Rect::from_xywh(x, y, width, height);
    let radius = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    let inner_rrect = inner_track_rrect(cache, (cache.x, cache.y));
    if radius == 0.0 {
        let reveal = FillShape::Rect(fill_rect);
        cache
            .fill
            .draw(canvas, &reveal, |paint| reveal.draw(canvas, paint));
        return;
    }
    if cache.track_fill_flat {
        cache
            .fill
            .draw(canvas, &FillShape::Rect(fill_rect), |paint| {
                canvas.save();
                canvas.clip_rect(fill_rect, ClipOp::Intersect, true);
                canvas.draw_rrect(inner_rrect, paint);
                canvas.restore();
            });
        return;
    }
    let revealed_length = match cache.orientation {
        ValidatedLinearGaugeOrientation::Horizontal => width,
        ValidatedLinearGaugeOrientation::Vertical => height,
    };
    let reveal = match translated_track_cap_reveal(revealed_length, radius) {
        Some(cap) => {
            let (frame, track_thickness) =
                track_cap_frame(*inner_rrect.rect(), radius, cache.orientation);
            FillShape::Path(translated_track_cap_path(frame, track_thickness, cap))
        }
        None => FillShape::RRect(RRect::new_rect_xy(fill_rect, radius, radius)),
    };
    cache.fill.draw(canvas, &reveal, |paint| {
        canvas.save();
        canvas.clip_rrect(inner_rrect, ClipOp::Intersect, true);
        reveal.draw(canvas, paint);
        canvas.restore();
    });
}
