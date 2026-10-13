use super::validation::ValidatedLabel;
use crate::{model::scene::ValidatedScenePresentation, render::text::{ResolvedTextStyle, parse_color}};
use skia_safe::Color;

/// Resolves a text style from a validated label and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the label contract).
pub fn validated_label_style(
    validated: &ValidatedLabel,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    let opacity = validated.opacity;
    let color = Color::from_argb(
        validated.color[3],
        validated.color[0],
        validated.color[1],
        validated.color[2],
    );

    ResolvedTextStyle {
        x: validated.x,
        y: validated.y,
        font_name: Some(validated.font_name.clone()),
        font_size: validated.font_size * scale,
        font_weight: validated.typography.font_weight,
        italic: validated.typography.italic,
        letter_spacing: validated.font_size * scale * validated.typography.letter_spacing / 100.0,
        line_height: validated.font_size * scale * 0.92,
        color,
        opacity,
        shadow_color: (!scene.shadow_color.is_empty()).then(|| parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: (!scene.border_color.is_empty()).then(|| parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scale,
    }
}

