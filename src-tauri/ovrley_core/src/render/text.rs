//! Text styling, measurement, and drawing.
//!
//! Template text settings are resolved into concrete Skia font, color, shadow,
//! and border values here. Font lookup is cached because labels and dynamic
//! values reuse the same typefaces across many frames.

use crate::error::CoreResult;
use crate::normalize::ValidatedSceneConfig;
use ovrley_widgets::widgets::time::validation::ValidatedTimeValue;
use ovrley_widgets::widgets::value::validation::ValidatedValueWidget;
use skia_safe::{Canvas, Font};
use std::path::PathBuf;

use ovrley_widgets::render::text::{MeasuredText, ResolvedTextStyle};

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
