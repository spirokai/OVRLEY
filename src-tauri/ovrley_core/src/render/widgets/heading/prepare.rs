//! Heading widget preparation: cached tape image and per-frame offsets.
//!
//! During preparation, the full 360° tape (ticks + labels + shadows) is
//! rendered once into a cached Skia image surface. Per-frame, the tape is
//! drawn offset by `heading × pixels_per_degree`.

use super::super::common::normalize_shadow_style;
use super::super::types::HeadingWidgetCache;
use super::geometry::{visible_labels, visible_ticks};
use crate::config::{HeadingWidgetConfig, RenderConfig};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::render::surface::create_surface;
use crate::render::text::parse_color;
use skia_safe::{image_filters, Paint, Point};
use std::path::PathBuf;
use std::time::Instant;

/// Prepares the heading widget cache by rendering the full 360° tape to a
/// cached Skia image.
pub fn prepare_heading_cache(
    config: &RenderConfig,
    plot: &HeadingWidgetConfig,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<HeadingWidgetCache> {
    let prepare_started = Instant::now();

    let tape_width = (360.0 * plot.pixels_per_degree).ceil() as u32;
    let tape_height = plot.height;

    // Resolve shadow style from widget config (overrides scene defaults)
    let shadow = normalize_shadow_style(
        plot.shadow_color.as_ref(),
        plot.shadow_strength,
        plot.shadow_distance,
        1.0,
    );

    // Create the tape surface
    let mut surface = prepare_profiler.measure("heading_cache.surface", || {
        create_surface(tape_width, tape_height)
    })?;

    let canvas = surface.canvas();
    canvas.clear(skia_safe::Color::TRANSPARENT);

    // Resolve colors
    let tick_color = plot.tick_color.as_deref().unwrap_or("#ffffff");
    let cardinal_tick_color = plot.cardinal_tick_color.as_deref().unwrap_or(tick_color);
    let minor_label_color = plot.minor_label_color.as_deref().unwrap_or("#ffffff");
    let major_label_color = plot
        .major_label_color
        .as_deref()
        .unwrap_or(minor_label_color);

    // Compute tick lengths in pixels
    let major_tick_length = tape_height as f32 * plot.major_tick_length_pct / 100.0;
    let minor_tick_length = tape_height as f32 * plot.minor_tick_length_pct / 100.0;

    // Compute tick y positions based on alignment
    let center_y = tape_height as f32 / 2.0;
    let tick_bottom = if plot.tick_alignment == "centered" {
        center_y + major_tick_length / 2.0
    } else {
        // "below" alignment: ticks extend downward from centerline
        center_y + major_tick_length
    };

    // Font for labels
    let font_size = plot.label_font_size.unwrap_or(12.0);
    let label_font = plot
        .label_font
        .as_deref()
        .or(plot.label_font_family.as_deref())
        .or_else(|| first_value_font(config))
        .or(config.scene.font.as_deref());
    let font = crate::render::text::resolve_font(font_dirs, label_font, font_size);

    // Label y-position: below the ticks
    let label_y = tick_bottom + plot.label_offset.unwrap_or(4.0) + font_size;

    // Collect all ticks to draw
    let ticks = visible_ticks(
        0.0, // heading=0 for the static tape image
        plot.pixels_per_degree,
        tape_width as f32,
        plot.major_tick_interval,
        plot.minor_ticks_per_major,
        plot.show_major_ticks,
        plot.show_minor_ticks,
    );

    let labels = visible_labels(&ticks, plot.show_minor_labels, plot.show_major_labels);

    // Draw ticks
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
            plot.major_tick_thickness
        } else {
            plot.minor_tick_thickness
        });

        let length = if tick.is_major {
            major_tick_length
        } else {
            minor_tick_length
        };
        let top = if plot.tick_alignment == "centered" {
            center_y - length / 2.0
        } else {
            tick_bottom - length
        };

        canvas.draw_line(
            Point::new(tick.x, top),
            Point::new(tick.x, top + length),
            &tick_paint,
        );
    }

    // Draw labels
    if !labels.is_empty() {
        let mut label_paint = Paint::default();
        label_paint.set_anti_alias(true);

        for label in &labels {
            let color_str = if label.is_major_label {
                major_label_color
            } else {
                minor_label_color
            };
            label_paint.set_color(parse_color(color_str, 1.0));

            canvas.draw_str(
                &label.text,
                Point::new(label.x, label_y),
                &font,
                &label_paint,
            );
        }
    }

    // Apply shadow as a second pass: redraw ticks and labels with shadow filter
    if let Some(ref shadow) = shadow {
        if shadow.strength > 0.0 || shadow.distance != 0.0 {
            let shadow_filter = image_filters::drop_shadow_only(
                (shadow.offset_x, shadow.offset_y),
                (shadow.strength, shadow.strength),
                parse_color(&shadow.color, 1.0),
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
                        plot.major_tick_thickness
                    } else {
                        plot.minor_tick_thickness
                    });
                    let length = if tick.is_major {
                        major_tick_length
                    } else {
                        minor_tick_length
                    };
                    let top = if plot.tick_alignment == "centered" {
                        center_y - length / 2.0
                    } else {
                        tick_bottom - length
                    };
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
                        major_label_color
                    } else {
                        minor_label_color
                    };
                    shadow_label_paint.set_color(parse_color(color_str, 1.0));
                    canvas.draw_str(
                        &label.text,
                        Point::new(label.x, label_y),
                        &font,
                        &shadow_label_paint,
                    );
                }
            }
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
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        pixels_per_degree: plot.pixels_per_degree,
        show_indicator: plot.show_indicator,
        indicator_style: plot.indicator_style.clone(),
        indicator_placement: plot.indicator_placement.clone(),
        indicator_color: plot
            .indicator_color
            .clone()
            .unwrap_or_else(|| "#ffffff".to_string()),
        indicator_size: plot.indicator_size.unwrap_or(10.0),
        indicator_shadow: shadow,
    })
}

fn first_value_font(config: &RenderConfig) -> Option<&str> {
    config
        .values
        .iter()
        .find_map(|value| value.font.as_deref().or(value.font_family.as_deref()))
}
