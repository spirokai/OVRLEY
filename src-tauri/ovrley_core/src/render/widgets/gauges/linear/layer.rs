//! Static track layers and dynamic fill rendering for linear gauges.
//!
//! Empty track, border, shadow, and min/max labels are rasterized once into a
//! cached image. Per-frame work is limited to compositing that image and
//! painting the current continuous fill or set of complete bars.

use super::super::metric::bar_fill_count;
use super::super::track_path::{translated_track_cap_path, translated_track_cap_reveal};
use super::geometry::{bordered_bar_fill_rect, segment_rect, track_cap_frame};
use super::labels::label_layout;
use crate::error::CoreResult;
use crate::normalize::{
    ValidatedLinearGaugeOrientation, ValidatedLinearGaugeWidget, ValidatedSceneConfig,
};
use crate::render::text::{parse_color, resolve_font};
use crate::render::widgets::common::normalize_shadow_style_validated;
use crate::render::widgets::types::LinearGaugeCache;
use skia_safe::{image_filters, BlendMode, Canvas, Paint, PathBuilder, PathFillType, RRect, Rect};
use std::path::PathBuf;

/// Builds the padded, cached image containing every frame-invariant element.
pub(super) fn draw_static_layer(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    scene: &ValidatedSceneConfig,
    width: u32,
    height: u32,
    scale: f32,
    font_dirs: &[PathBuf],
    min_value: f64,
    max_value: f64,
    altitude_offset_m: f64,
) -> CoreResult<()> {
    let width_px = width as f32;
    let height_px = height as f32;
    let radius = gauge.track_corner_radius * scale;
    let border = gauge.track_border_thickness * scale;
    let shadow_filter = if border > 0.0 {
        normalize_shadow_style_validated(
            &scene.shadow_color,
            scene.shadow_strength,
            scene.shadow_distance,
            scale,
        )
        .and_then(|shadow| {
            image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, 1.0),
                None,
                None,
                None,
            )
        })
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
            draw_static_track(canvas, gauge, rect, radius, border, shadow_filter.as_ref());
        }
    } else {
        draw_static_track(
            canvas,
            gauge,
            Rect::from_xywh(0.0, 0.0, width_px, height_px),
            radius,
            border,
            shadow_filter.as_ref(),
        );
    }

    if gauge.show_min_max_labels {
        let font_size = gauge.min_max_label_font_size * scale;
        let font = resolve_font(font_dirs, Some(&gauge.min_max_label_font), font_size)?;
        let layout = label_layout(
            gauge,
            width,
            height,
            scale,
            &font,
            min_value,
            max_value,
            altitude_offset_m,
        );
        let mut text_paint = Paint::default();
        text_paint.set_anti_alias(true);
        text_paint.set_color(parse_color(&gauge.min_max_label_color, 1.0));
        canvas.draw_str(&layout.min_label, layout.min_origin, &font, &text_paint);
        canvas.draw_str(&layout.max_label, layout.max_origin, &font, &text_paint);
    }

    Ok(())
}

fn draw_static_track(
    canvas: &Canvas,
    gauge: &ValidatedLinearGaugeWidget,
    rect: Rect,
    configured_radius: f32,
    border: f32,
    shadow_filter: Option<&skia_safe::ImageFilter>,
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

    if let Some(filter) = shadow_filter {
        let mut shadow_paint = Paint::default();
        shadow_paint.set_anti_alias(true);
        shadow_paint.set_color(skia_safe::Color::BLACK);
        shadow_paint.set_image_filter(filter.clone());
        if border > 0.0 {
            let mut ring_path = PathBuilder::new_with_fill_type(PathFillType::EvenOdd);
            ring_path.add_rrect(outer_rrect, None, None);
            ring_path.add_rrect(inner_rrect, None, None);
            canvas.draw_path(&ring_path.detach(), &shadow_paint);
        } else {
            canvas.draw_rrect(outer_rrect, &shadow_paint);
        }
    }

    if border > 0.0 {
        let mut border_paint = Paint::default();
        border_paint.set_anti_alias(true);
        border_paint.set_color(parse_color(&gauge.track_border_color, 1.0));
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
        gauge.track_empty_opacity,
    ));
    canvas.draw_rrect(inner_rrect, &empty_paint);
}

/// Draws only complete bars whose discrete buckets are active at `fill01`.
pub(super) fn draw_segmented_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let bar_geometry = cache
        .bar_geometry
        .expect("bars fill style must carry resolved bar geometry");
    let filled_count = bar_fill_count(fill01, bar_geometry.count);
    let paint = fill_paint(cache);

    for index in 0..filled_count as u32 {
        let outer = segment_rect(
            index,
            bar_geometry.extent,
            bar_geometry.gap,
            cache.width as f32,
            cache.height as f32,
            cache.orientation,
        );
        let border = cache.track_border_thickness;
        let inner = Rect::from_xywh(
            cache.x + outer.left + border,
            cache.y + outer.top + border,
            (outer.width() - border * 2.0).max(0.0),
            (outer.height() - border * 2.0).max(0.0),
        );
        let radius = (cache.track_corner_radius - border)
            .max(0.0)
            .min(inner.width() * 0.5)
            .min(inner.height() * 0.5);
        canvas.draw_rrect(RRect::new_rect_xy(inner, radius, radius), &paint);
    }
}

/// Reveals the continuous source track while preserving its configured caps.
pub(super) fn draw_continuous_fill(canvas: &Canvas, cache: &LinearGaugeCache, fill01: f32) {
    let (x, y, width, height) = bordered_bar_fill_rect(
        cache.x,
        cache.y,
        cache.width as f32,
        cache.height as f32,
        fill01,
        cache.orientation,
        cache.track_border_thickness,
    );
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let fill_paint = fill_paint(cache);
    let fill_rect = Rect::from_xywh(x, y, width, height);
    let radius = (cache.track_corner_radius - cache.track_border_thickness).max(0.0);
    if radius == 0.0 {
        canvas.draw_rrect(RRect::new_rect_xy(fill_rect, radius, radius), &fill_paint);
        return;
    }

    let inset = cache.track_border_thickness;
    let inner_rect = Rect::from_xywh(
        cache.x + inset,
        cache.y + inset,
        (cache.width as f32 - inset * 2.0).max(0.0),
        (cache.height as f32 - inset * 2.0).max(0.0),
    );
    let inner_rrect = RRect::new_rect_xy(inner_rect, radius, radius);
    canvas.save();
    if cache.track_fill_flat {
        canvas.clip_rect(fill_rect, skia_safe::ClipOp::Intersect, true);
        canvas.draw_rrect(inner_rrect, &fill_paint);
    } else {
        canvas.clip_rrect(inner_rrect, skia_safe::ClipOp::Intersect, true);
        let revealed_length = match cache.orientation {
            ValidatedLinearGaugeOrientation::Horizontal => width,
            ValidatedLinearGaugeOrientation::Vertical => height,
        };
        if let Some(cap) = translated_track_cap_reveal(revealed_length, radius) {
            let (frame, track_thickness) = track_cap_frame(inner_rect, radius, cache.orientation);
            let cap_path = translated_track_cap_path(frame, track_thickness, cap);
            canvas.draw_path(&cap_path, &fill_paint);
        } else {
            canvas.draw_rrect(RRect::new_rect_xy(fill_rect, radius, radius), &fill_paint);
        }
    }
    canvas.restore();
}

fn fill_paint(cache: &LinearGaugeCache) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(parse_color(
        &cache.track_filled_color,
        cache.track_filled_opacity,
    ));
    paint
}
