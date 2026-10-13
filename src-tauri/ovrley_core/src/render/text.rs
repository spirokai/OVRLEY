//! Text styling, measurement, and drawing.
//!
//! Template text settings are resolved into concrete Skia font, color, shadow,
//! and border values here. Font lookup is cached because labels and dynamic
//! values reuse the same typefaces across many frames.

use crate::error::CoreResult;
use crate::normalize::{
    ValidatedLapTimer, ValidatedSceneConfig,
};
use ovrley_widgets::model::standard_widgets::widget_font_weight;
use ovrley_widgets::widgets::time::validation::ValidatedTimeValue;
use ovrley_widgets::widgets::value::validation::ValidatedValueWidget;
use skia_safe::{Canvas, Color, Font};
use std::path::PathBuf;

use ovrley_widgets::render::text::{parse_color, MeasuredText, ResolvedTextStyle};

// Resolves the scene-level text shadow color with opacity applied.
fn scene_shadow_color(scene: &ValidatedSceneConfig, opacity: f32) -> Option<Color> {
    if scene.presentation.shadow_color.is_empty() {
        None
    } else {
        Some(parse_color(&scene.presentation.shadow_color, opacity))
    }
}

// Resolves the scene-level text border color with opacity applied.
fn scene_border_color(scene: &ValidatedSceneConfig, opacity: f32) -> Option<Color> {
    if scene.presentation.border_color.is_empty() {
        None
    } else {
        Some(parse_color(&scene.presentation.border_color, opacity))
    }
}

/// Resolves a text style from a validated value widget and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the value contract).
pub fn validated_value_style(
    validated: &ValidatedValueWidget,
    scene: &ValidatedSceneConfig,
    scale: f32,
) -> ResolvedTextStyle {
    ovrley_widgets::render::text::validated_value_style(validated, &scene.presentation, scale)
}

/// Resolves a text style from a validated time widget and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the time contract).
pub fn validated_time_style(
    validated: &ValidatedTimeValue,
    scene: &ValidatedSceneConfig,
    scale: f32,
) -> ResolvedTextStyle {
    validated_value_style(&validated.base, scene, scale)
}

/// Resolves the text style shared by current and best lap widgets.
pub fn validated_lap_timer_style(
    validated: &ValidatedLapTimer,
    scene: &ValidatedSceneConfig,
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
        font_weight: widget_font_weight(),
        italic: false,
        letter_spacing: 0.0,
        line_height: validated.font_size * scale * 0.92,
        color,
        opacity,
        shadow_color: scene_shadow_color(scene, opacity),
        shadow_strength: scene.presentation.shadow_strength * scale,
        shadow_distance: scene.presentation.shadow_distance * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.presentation.border_thickness * scale,
    }
}

/// Resolves the independently configured label style for a lap timer.
pub fn validated_lap_timer_label_style(
    validated: &ValidatedLapTimer,
    scene: &ValidatedSceneConfig,
    scale: f32,
) -> ResolvedTextStyle {
    let mut style = validated_lap_timer_style(validated, scene, scale);
    style.font_name = Some(validated.label_font_name.clone());
    style.font_size = validated.label_font_size * scale;
    style.line_height = style.font_size * 0.92;
    style.color = Color::from_argb(
        validated.label_color[3],
        validated.label_color[0],
        validated.label_color[1],
        validated.label_color[2],
    );
    style
}

/// Native font acquisition boundary for shared text rendering.
pub fn draw_text(
    canvas: &Canvas,
    text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    Ok(ovrley_widgets::render::text::draw_text(
        canvas,
        text,
        style,
        &crate::fonts::NativeFonts(font_dirs),
    )?)
}
pub fn draw_text_with_vertical_metrics_text(
    canvas: &Canvas,
    text: &str,
    vertical_metrics_text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    Ok(
        ovrley_widgets::render::text::draw_text_with_vertical_metrics_text(
            canvas,
            text,
            vertical_metrics_text,
            style,
            &crate::fonts::NativeFonts(font_dirs),
        )?,
    )
}
pub fn resolve_font(font_dirs: &[PathBuf], name: Option<&str>, font_size: f32) -> CoreResult<Font> {
    Ok(ovrley_widgets::render::text::resolve_font(
        &crate::fonts::NativeFonts(font_dirs),
        name,
        font_size,
    )?)
}
pub fn resolve_styled_font(
    font_dirs: &[PathBuf],
    name: Option<&str>,
    font_size: f32,
    font_weight: f32,
    italic: bool,
) -> CoreResult<Font> {
    Ok(ovrley_widgets::render::text::resolve_styled_font(
        &crate::fonts::NativeFonts(font_dirs),
        name,
        font_size,
        font_weight,
        italic,
    )?)
}
pub fn measure_text(
    text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<MeasuredText> {
    Ok(ovrley_widgets::render::text::measure_text(
        text,
        style,
        &crate::fonts::NativeFonts(font_dirs),
    )?)
}
