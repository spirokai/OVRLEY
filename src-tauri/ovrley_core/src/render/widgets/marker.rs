//! Marker and dot drawing helpers for route and elevation widgets.
//!
//! Owns: `draw_marker` (renders multi-layer markers at a position),
//!       `marker_layers_from_plot` (builds drawable layers from the 5
//!       plot-level marker keys).
//! Does not own: widget coordinate transforms (see [`super::transform`]).
//!
//! Allowed dependencies: `skia_safe`, `super::types`, `super::geometry`.
//! Forbidden dependencies: `crate::normalize`, `crate::activity`,
//!       `crate::encode`, `crate::commands`.
//!
//! ## Performance
//! Called once per frame for the active marker position during video rendering.
//! The layer list is pre-allocated during widget preparation — per-frame work is
//! O(layers) Skia draw calls with no heap allocation.

use super::types::MarkerLayer;
use skia_safe::{Canvas, Paint, Point};

const RING_STROKE_WIDTH: f32 = 1.5;

/// Builds marker layers from the 5 explicit plot-level marker keys.
///
/// Returns a base solid-fill circle plus an optional ring/halo variant layer.
pub(crate) fn marker_layers_from_plot(
    marker_variant: &str,
    marker_variant_diameter: f32,
    marker_size: f32,
    marker_color: &str,
    marker_opacity: f32,
) -> Vec<MarkerLayer> {
    let mut layers = vec![MarkerLayer {
        radius: marker_size.max(2.0),
        color: marker_color.to_string(),
        opacity: marker_opacity,
        solid_fill: true,
        stroke_width: 0.0,
    }];

    let variant_radius = (marker_variant_diameter * 0.5).max(0.0);
    match marker_variant {
        "ring" if variant_radius > 0.0 => {
            layers.insert(
                0,
                MarkerLayer {
                    radius: variant_radius,
                    color: marker_color.to_string(),
                    opacity: marker_opacity.clamp(0.0, 1.0),
                    solid_fill: false,
                    stroke_width: RING_STROKE_WIDTH,
                },
            );
        }
        "halo" if variant_radius > 0.0 => {
            layers.insert(
                0,
                MarkerLayer {
                    radius: variant_radius,
                    color: marker_color.to_string(),
                    opacity: (marker_opacity * 0.35).clamp(0.0, 1.0),
                    solid_fill: true,
                    stroke_width: 0.0,
                },
            );
        }
        _ => {}
    }

    layers
}

pub(crate) fn draw_marker(
    canvas: &Canvas,
    layers: &[MarkerLayer],
    x: f32,
    y: f32,
    fallback_color: &str,
    fallback_radius: f32,
    fallback_opacity: f32,
) {
    if layers.is_empty() {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);
        paint.set_color(crate::render::text::parse_color(
            fallback_color,
            fallback_opacity,
        ));
        canvas.draw_circle(Point::new(x, y), fallback_radius.max(2.0), &paint);
        return;
    }

    for layer in layers {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(crate::render::text::parse_color(
            &layer.color,
            layer.opacity,
        ));
        if layer.solid_fill {
            paint.set_style(skia_safe::paint::Style::Fill);
            canvas.draw_circle(Point::new(x, y), layer.radius, &paint);
        } else {
            paint.set_style(skia_safe::paint::Style::Stroke);
            paint.set_stroke_width(layer.stroke_width.max(1.0));
            canvas.draw_circle(Point::new(x, y), layer.radius, &paint);
        }
    }
}
