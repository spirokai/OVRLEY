//! Heading widget preparation: cached tape image and per-frame offsets.
//!
//! During preparation, the full 360° tape (ticks + labels + shadows) is
//! rendered once into a cached Skia image surface. Per-frame, the tape is
//! drawn offset by `heading × pixels_per_degree`.

use super::super::common::normalize_shadow_style_validated;
use super::super::types::HeadingWidgetCache;
use super::geometry::{
    heading_label_baseline, heading_tape_layout, heading_tick_position, visible_labels,
    visible_ticks, CHEVRON_GAP_PX,
};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::{ValidatedHeading, ValidatedSceneConfig};
use crate::render::surface::create_surface;
use crate::render::text::{origin_x_for_centered_text, parse_color};
use skia_safe::{image_filters, Paint, Point};
use std::path::PathBuf;
use std::time::Instant;

/// Prepares the heading widget cache by rendering the full 360° tape to a
/// cached Skia image.
pub fn prepare_heading_cache(
    scene: &ValidatedSceneConfig,
    heading: &ValidatedHeading,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<HeadingWidgetCache> {
    let prepare_started = Instant::now();

    let scale = scene.scale.max(0.1);
    let scaled_ppd = heading.pixels_per_degree * scale;
    let tape_width = (360.0 * scaled_ppd).ceil() as u32;
    let scaled_tick_scale_height = (heading.height as f32) * scale;
    let scaled_width = ((heading.width as f32) * scale).round().max(1.0) as u32;
    let scaled_indicator_size = heading.indicator_size * scale;
    let scaled_chevron_gap = CHEVRON_GAP_PX * scale;
    let label_offset = heading.label_offset * scale;
    let font_size = heading.label_font_size * scale;
    let layout = heading_tape_layout(
        scaled_tick_scale_height,
        heading.show_indicator,
        &heading.indicator_style,
        &heading.indicator_placement,
        scaled_indicator_size,
        scaled_chevron_gap,
        heading.major_tick_length_pct,
        label_offset,
        font_size,
    );
    let tape_height = layout.body_height.ceil().max(1.0) as u32;
    let scaled_height = layout.total_height.ceil().max(1.0) as u32;

    // Resolve shadow style from scene defaults (shadow is not part of heading contract)
    let shadow = normalize_shadow_style_validated(
        &scene.shadow_color,
        scene.shadow_strength,
        scene.shadow_distance,
        scale,
    );

    // Create the tape surface
    let mut surface = prepare_profiler.measure("heading_cache.surface", || {
        create_surface(tape_width, tape_height)
    })?;

    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);

    // Colors are already resolved in the validated heading
    let tick_color = heading.tick_color.as_str();
    let cardinal_tick_color = heading.cardinal_tick_color.as_str();
    let label_color = heading.label_color.as_str();
    let cardinal_label_color = heading.cardinal_label_color.as_str();

    let major_tick_thickness = heading.major_tick_thickness * scale;
    let minor_tick_thickness = heading.minor_tick_thickness * scale;

    // Font for labels
    let font =
        crate::render::text::resolve_font(font_dirs, heading.label_font.as_deref(), font_size)?;

    // Label y-position: below the ticks
    let label_y = heading_label_baseline(
        layout.tick_scale_height,
        heading.major_tick_length_pct,
        label_offset,
        font_size,
    );

    // Collect all ticks to draw
    let ticks = visible_ticks(
        0.0, // heading=0 for the static tape image
        scaled_ppd,
        tape_width as f32,
        heading.major_tick_interval,
        heading.minor_ticks_per_major,
        heading.show_major_ticks,
        heading.show_minor_ticks,
    );

    let labels = visible_labels(&ticks, heading.show_minor_labels, heading.show_major_labels);

    // Draw shadow pass first so shadows sit behind the main content
    if let Some(ref shadow) = shadow {
        if shadow.strength > 0.0 {
            let shadow_filter = image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, 1.0),
                None,
                None,
                None,
            );

            if let Some(filter) = shadow_filter {
                // Shadow pass for ticks
                let mut shadow_tick_paint = Paint::default();
                shadow_tick_paint.set_anti_alias(true);
                shadow_tick_paint.set_image_filter(filter.clone());

                for tick in &ticks {
                    shadow_tick_paint.set_color(parse_color(tick_color, 1.0));
                    shadow_tick_paint.set_stroke_width(if tick.is_major {
                        major_tick_thickness
                    } else {
                        minor_tick_thickness
                    });
                    let (top, length) = heading_tick_position(
                        layout.tick_scale_height,
                        heading.major_tick_length_pct,
                        heading.minor_tick_length_pct,
                        &heading.tick_alignment,
                        tick.is_major,
                    );
                    canvas.draw_line(
                        Point::new(tick.x, top),
                        Point::new(tick.x, top + length),
                        &shadow_tick_paint,
                    );
                }

                // Shadow pass for labels
                let mut shadow_label_paint = Paint::default();
                shadow_label_paint.set_anti_alias(true);
                shadow_label_paint.set_image_filter(filter);

                for label in &labels {
                    let color_str = if label.is_major_label {
                        cardinal_label_color
                    } else {
                        label_color
                    };
                    let label_x = origin_x_for_centered_text(&label.text, label.x, &font);
                    shadow_label_paint.set_color(parse_color(color_str, 1.0));
                    canvas.draw_str(
                        &label.text,
                        Point::new(label_x, label_y),
                        &font,
                        &shadow_label_paint,
                    );
                }
            }
        }
    }

    // Draw ticks on top of shadows
    let mut tick_paint = Paint::default();
    tick_paint.set_anti_alias(true);

    for tick in &ticks {
        let color_str = if tick.is_cardinal {
            cardinal_tick_color
        } else {
            tick_color
        };
        tick_paint.set_color(parse_color(color_str, 1.0));
        tick_paint.set_stroke_width(if tick.is_major {
            major_tick_thickness
        } else {
            minor_tick_thickness
        });

        let (top, length) = heading_tick_position(
            layout.tick_scale_height,
            heading.major_tick_length_pct,
            heading.minor_tick_length_pct,
            &heading.tick_alignment,
            tick.is_major,
        );

        canvas.draw_line(
            Point::new(tick.x, top),
            Point::new(tick.x, top + length),
            &tick_paint,
        );
    }

    // Draw labels on top of shadows
    if !labels.is_empty() {
        let mut label_paint = Paint::default();
        label_paint.set_anti_alias(true);

        for label in &labels {
            let color_str = if label.is_major_label {
                cardinal_label_color
            } else {
                label_color
            };
            let label_x = origin_x_for_centered_text(&label.text, label.x, &font);
            label_paint.set_color(parse_color(color_str, 1.0));

            canvas.draw_str(
                &label.text,
                Point::new(label_x, label_y),
                &font,
                &label_paint,
            );
        }
    }

    let tape_image = surface.image_snapshot();

    prepare_profiler.record_ms(
        "heading_cache.total",
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    );

    Ok(HeadingWidgetCache {
        tape_image,
        tape_width: tape_width as f32,
        tape_body_y: layout.body_y,
        tape_body_height: layout.body_height,
        x: heading.x,
        y: heading.y,
        width: scaled_width,
        height: scaled_height,
        pixels_per_degree: scaled_ppd,
        show_indicator: heading.show_indicator,
        indicator_style: heading.indicator_style.clone(),
        indicator_placement: heading.indicator_placement.clone(),
        indicator_color: heading.indicator_color.clone(),
        indicator_size: scaled_indicator_size,
        indicator_shadow: shadow,
        display_type: crate::types::DisplayType::Tape,
    })
}
