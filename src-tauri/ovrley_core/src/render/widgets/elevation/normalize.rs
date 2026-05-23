/// Elevation plot normalization: option resolution and label-style defaults.
///
/// Merges legacy flat-style fields and newer nested-style fields, applies the
/// scene scale to all dimension/stroke/marker/label values, and resolves color
/// precedence chains so draw code receives only concrete values. Runs once per
/// widget build, not per-frame.
use super::super::common::{
    fallback_marker_points, legacy_line_width, marker_size_from_weights, normalize_shadow_style,
    plot_base_color, resolve_style_color, scale_marker_points, shadow_with_screen_offset,
    DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER, DEFAULT_ELEVATION_MARKER_SCALE,
};
use super::super::geometry::normalize_opacity;
use super::super::types::NormalizedElevationPlot;
use crate::config::{ElevationPlotConfig, RenderConfig};

/// Normalizes elevation plot options into concrete scaled drawing settings.
pub(crate) fn normalize_elevation_plot(
    config: &RenderConfig,
    plot: &ElevationPlotConfig,
) -> NormalizedElevationPlot {
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let base_color = plot_base_color(plot.color.as_deref());
    let legacy_width = legacy_line_width(
        plot.line.as_ref().and_then(|line| line.width),
        DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER,
    ) * scale;
    let marker_size = plot.marker_size.unwrap_or_else(|| {
        marker_size_from_weights(&plot.points, 16.0, |weight| {
            weight.sqrt() * DEFAULT_ELEVATION_MARKER_SCALE.sqrt()
        })
    }) * scale;
    let point_label = plot.point_label.clone().unwrap_or_default();
    let marker_color = plot
        .marker_color
        .clone()
        .unwrap_or_else(|| base_color.clone());
    let marker_opacity = normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0);
    let scaled_width = ((plot.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((plot.height as f32) * scale).round().max(1.0) as u32;
    let scaled_points = scale_marker_points(&plot.points, scale);

    NormalizedElevationPlot {
        x: plot.x,
        y: plot.y,
        width: scaled_width,
        height: scaled_height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(0.0),
        y_scale: plot.y_scale.unwrap_or(1.0).clamp(0.2, 4.0),
        simplify_tolerance_px: plot.simplify_tolerance_px.unwrap_or(1.0).clamp(0.0, 8.0),
        target_density: plot.target_density.unwrap_or(0.75).clamp(0.1, 2.0),
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
            1.0,
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
        area_remaining_color: resolve_style_color(
            plot.area_remaining_color.as_ref(),
            plot.fill.as_ref().and_then(|fill| fill.color.as_ref()),
            &base_color,
        ),
        area_remaining_opacity: normalize_opacity(
            plot.area_remaining_opacity.or_else(|| {
                plot.fill
                    .as_ref()
                    .and_then(|fill| fill.opacity)
                    .map(|opacity| opacity * 0.35)
            }),
            0.12,
        ),
        area_completed_color: resolve_style_color(
            plot.area_completed_color.as_ref(),
            plot.fill.as_ref().and_then(|fill| fill.color.as_ref()),
            &base_color,
        ),
        area_completed_opacity: normalize_opacity(
            plot.area_completed_opacity
                .or_else(|| plot.fill.as_ref().and_then(|fill| fill.opacity)),
            0.24,
        ),
        marker_size,
        marker_color: marker_color.clone(),
        marker_opacity,
        marker_points: fallback_marker_points(
            &scaled_points,
            marker_size,
            &marker_color,
            marker_opacity,
        ),
        show_elevation_metric: plot.show_elevation_metric.unwrap_or(false),
        show_elevation_imperial: plot.show_elevation_imperial.unwrap_or(false),
        metric_label_offset_x: plot
            .metric_label_offset_x
            .or(point_label.x_offset)
            .unwrap_or(0.0)
            * scale,
        metric_label_offset_y: plot
            .metric_label_offset_y
            .or(point_label.y_offset)
            .unwrap_or(-28.0)
            * scale,
        imperial_label_offset_x: plot.imperial_label_offset_x.unwrap_or(0.0) * scale,
        imperial_label_offset_y: plot.imperial_label_offset_y.unwrap_or(6.0) * scale,
        label_font: point_label
            .font
            .or_else(|| first_value_font(config))
            .or_else(|| config.scene.font.clone()),
        label_font_size: point_label
            .font_size
            .or(config.scene.font_size)
            .unwrap_or(12.5)
            * scale,
        label_color: point_label.color.unwrap_or_else(|| base_color.clone()),
        label_decimal_rounding: point_label
            .decimal_rounding
            .or(config.scene.decimal_rounding),
        legacy_label_units: point_label.units,
    }
}

/// Returns the first configured value font for legacy elevation labels.
fn first_value_font(config: &RenderConfig) -> Option<String> {
    config
        .values
        .iter()
        .find_map(|value| value.font.clone().or_else(|| value.font_family.clone()))
}
