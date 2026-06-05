/// Normalizes validated elevation plot into concrete scaled drawing settings.
///
/// Applies scene scale to dimensions and stroke/marker sizes. All values are
/// explicit — no fallbacks. Called once per widget build.
use super::super::common::{normalize_shadow_style_validated, shadow_with_screen_offset};
use super::super::types::NormalizedElevationPlot;
use crate::normalize::{ValidatedElevationPlot, ValidatedSceneConfig};

pub(crate) fn normalize_elevation_plot(
    validated: &ValidatedElevationPlot,
    scene: &ValidatedSceneConfig,
) -> NormalizedElevationPlot {
    let scale = scene.scale.max(0.1);
    let scaled_width = ((validated.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((validated.height as f32) * scale).round().max(1.0) as u32;

    NormalizedElevationPlot {
        x: validated.x,
        y: validated.y,
        width: scaled_width,
        height: scaled_height,
        rotation: validated.rotation,
        y_scale: validated.y_scale,
        simplify_tolerance_px: validated.simplify_tolerance_px,
        target_density: validated.target_density,
        remaining_line_width: validated.remaining_line_width * scale,
        remaining_line_color: validated.remaining_line_color.clone(),
        remaining_line_opacity: validated.remaining_line_opacity,
        remaining_line_shadow: shadow_with_screen_offset(
            normalize_shadow_style_validated(
                &scene.shadow_color,
                scene.shadow_strength,
                scene.shadow_distance,
                scale,
            ),
            validated.rotation,
        ),
        completed_line_width: validated.completed_line_width * scale,
        completed_line_color: validated.completed_line_color.clone(),
        completed_line_opacity: validated.completed_line_opacity,
        area_remaining_color: validated.area_remaining_color.clone(),
        area_remaining_opacity: validated.area_remaining_opacity,
        area_completed_color: validated.area_completed_color.clone(),
        area_completed_opacity: validated.area_completed_opacity,
        marker_variant: validated.marker_variant.clone(),
        marker_variant_diameter: validated.marker_variant_diameter * scale,
        marker_size: validated.marker_size * scale,
        marker_color: validated.marker_color.clone(),
        marker_opacity: validated.marker_opacity,
        show_elevation_metric: validated.show_elevation_metric,
        show_elevation_imperial: validated.show_elevation_imperial,
        metric_label_offset_x: validated.metric_label_offset_x * scale,
        metric_label_offset_y: validated.metric_label_offset_y * scale,
        imperial_label_offset_x: validated.imperial_label_offset_x * scale,
        imperial_label_offset_y: validated.imperial_label_offset_y * scale,
        label_font: validated.label_font.clone(),
        label_font_size: validated.label_font_size * scale,
        label_color: validated.label_color.clone(),
    }
}
