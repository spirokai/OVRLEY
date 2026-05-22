//! Marker and dot drawing helpers for route and elevation widgets.
//!
//! Owns: `draw_marker` (renders multi-layer markers at a position), `marker_layers_from_points`
//!       (converts config-defined marker points into drawable layers).
//! Does not own: marker point configuration (see [`crate::config`]), widget
//!       coordinate transforms (see [`super::transform`]).
//!
//! Allowed dependencies: `skia_safe`, `super::types`, `super::geometry`, `super::common`.
//! Forbidden dependencies: `crate::config`, `crate::activity`,
//!       `crate::encode`, `crate::commands`.
//!
//! ## Performance
//! Called once per frame for the active marker position during video rendering.
//! The layer list is pre-allocated during widget preparation — per-frame work is
//! O(layers) Skia draw calls with no heap allocation.

use super::common::{DEFAULT_COLOR, DEFAULT_POINT_WEIGHT};
use super::geometry::normalize_opacity;
use super::types::MarkerLayer;
use crate::config::MarkerPointConfig;
use skia_safe::{Canvas, Paint, Point};

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
            paint.set_stroke_width((layer.radius * 0.18).round().clamp(1.0, 3.0));
            canvas.draw_circle(Point::new(x, y), layer.radius, &paint);
        }
    }
}

pub(crate) fn marker_layers_from_points(points: &[MarkerPointConfig]) -> Vec<MarkerLayer> {
    let mut layers = points
        .iter()
        .map(|point| MarkerLayer {
            radius: point
                .weight
                .unwrap_or(DEFAULT_POINT_WEIGHT)
                .max(1.0)
                .sqrt()
                .max(2.0),
            color: point
                .color
                .clone()
                .unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            opacity: normalize_opacity(point.opacity, 1.0),
            solid_fill: false,
        })
        .collect::<Vec<_>>();
    layers.sort_by(|left, right| right.radius.total_cmp(&left.radius));
    if let Some(last) = layers.last_mut() {
        last.solid_fill = true;
    }
    layers
}
