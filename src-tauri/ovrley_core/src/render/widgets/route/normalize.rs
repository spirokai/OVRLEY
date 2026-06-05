/// Normalizes validated route plot into concrete scaled drawing settings.
///
/// Applies scene scale to dimensions and stroke/marker sizes. All values are
/// explicit — no fallbacks. Called once per widget build.
use super::super::common::{normalize_shadow_style_validated, shadow_with_screen_offset};
use super::super::types::NormalizedRoutePlot;
use crate::normalize::{ValidatedRoutePlot, ValidatedSceneConfig};

pub(crate) fn normalize_route_plot(
    validated: &ValidatedRoutePlot,
    scene: &ValidatedSceneConfig,
) -> NormalizedRoutePlot {
    let scale = scene.scale.max(0.1);
    let scaled_width = ((validated.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((validated.height as f32) * scale).round().max(1.0) as u32;

    NormalizedRoutePlot {
        x: validated.x,
        y: validated.y,
        width: scaled_width,
        height: scaled_height,
        rotation: validated.rotation,
        simplify_tolerance_px: validated.simplify_tolerance_px,
        target_density: validated.target_density.clamp(0.1, 2.0),
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
        marker_variant: validated.marker_variant.clone(),
        marker_variant_diameter: validated.marker_variant_diameter * scale,
        marker_size: validated.marker_size * scale,
        marker_color: validated.marker_color.clone(),
        marker_opacity: validated.marker_opacity,
    }
}
