/// Normalizes route plot options into concrete scaled drawing settings.
///
/// Resolves legacy flat-style fields and nested styles into a single internal
/// shape, applies scene scale to dimensions and stroke/marker sizes, and
/// resolves color/opacity precedence chains. Called once per widget build.
///
/// Owns the normalization logic for route plot configuration.
use super::super::common::{
    fallback_marker_points, legacy_line_width, marker_size_from_weights, normalize_shadow_style,
    plot_base_color, resolve_style_color, scale_marker_points, shadow_with_screen_offset,
    DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER, DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
    DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX,
};
use super::super::geometry::normalize_opacity;
use super::super::marker::scaled_ring_stroke_width;
use super::super::types::NormalizedRoutePlot;
use crate::config::{CoursePlotConfig, RenderConfig};

pub(crate) fn normalize_route_plot(
    config: &RenderConfig,
    plot: &CoursePlotConfig,
) -> NormalizedRoutePlot {
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let base_color = plot_base_color(plot.color.as_deref());
    let legacy_width = legacy_line_width(
        plot.line.as_ref().and_then(|line| line.width),
        DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER,
    ) * scale;
    let marker_size = plot
        .marker_size
        .unwrap_or_else(|| marker_size_from_weights(&plot.points, 18.0, f32::sqrt))
        * scale;
    let marker_color = plot
        .marker_color
        .clone()
        .unwrap_or_else(|| base_color.clone());
    let marker_opacity = normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0);
    let marker_variant = normalize_marker_variant(plot.marker_variant.as_deref());
    let marker_variant_diameter = match plot.marker_variant_diameter {
        Some(diameter) => diameter.max(0.0) * scale,
        None => marker_size * 2.0 + 8.0,
    };
    let marker_variant_stroke_width = scaled_ring_stroke_width(scale);
    let scaled_width = ((plot.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((plot.height as f32) * scale).round().max(1.0) as u32;
    let scaled_points = scale_marker_points(&plot.points, scale);

    NormalizedRoutePlot {
        x: plot.x,
        y: plot.y,
        width: scaled_width,
        height: scaled_height,
        rotation: plot.rotation,
        simplify_tolerance_px: plot.simplify_tolerance_px.unwrap_or(
            DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX * DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
        ),
        target_density: plot.target_density.unwrap_or(1.0).clamp(0.1, 2.0),
        remaining_line_width: plot.remaining_line_width.unwrap_or(legacy_width),
        remaining_line_color: resolve_style_color(
            plot.remaining_line_color.as_ref(),
            plot.line.as_ref().and_then(|line| line.color.as_ref()),
            &base_color,
        ),
        remaining_line_opacity: normalize_opacity(
            plot.remaining_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            0.75,
        ),
        remaining_line_shadow: shadow_with_screen_offset(
            normalize_shadow_style(
                config.scene.shadow_color.as_ref(),
                config.scene.shadow_strength,
                config.scene.shadow_distance,
                scale,
            ),
            plot.rotation,
        ),
        completed_line_width: plot.completed_line_width.unwrap_or(legacy_width),
        completed_line_color: resolve_style_color(
            plot.completed_line_color.as_ref(),
            plot.line.as_ref().and_then(|line| line.color.as_ref()),
            &base_color,
        ),
        completed_line_opacity: normalize_opacity(
            plot.completed_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            1.0,
        ),
        marker_variant,
        marker_variant_diameter,
        marker_variant_stroke_width,
        marker_size,
        marker_color: marker_color.clone(),
        marker_opacity,
        marker_points: fallback_marker_points(
            &scaled_points,
            marker_size,
            &marker_color,
            marker_opacity,
        ),
    }
}

fn normalize_marker_variant(value: Option<&str>) -> String {
    match value {
        Some("ring") => "ring".to_string(),
        Some("halo") => "halo".to_string(),
        _ => "single".to_string(),
    }
}
