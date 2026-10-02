//! Text styling, measurement, and drawing.
//!
//! Template text settings are resolved into concrete Skia font, color, shadow,
//! and border values here. Font lookup is cached because labels and dynamic
//! values reuse the same typefaces across many frames.

use crate::error::CoreResult;
use crate::normalize::{
    ValidatedGradientWidget, ValidatedLabel, ValidatedLapTimer, ValidatedSceneConfig,
    ValidatedTimeValue, ValidatedValueWidget,
};
use crate::standard_widgets::widget_font_weight;
use skia_safe::{
    image_filters,
    paint::{Join, Style},
    Canvas, Color, Font, Paint, Point, Rect, TextBlob, TextBlobBuilder,
};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

/// Fully resolved text style ready for Skia drawing.
#[derive(Clone, Debug)]
pub struct ResolvedTextStyle {
    /// Left position in canvas pixels.
    pub x: f32,
    /// Top position in canvas pixels.
    pub y: f32,
    /// Font filename or family name.
    pub font_name: Option<String>,
    /// Font size in pixels after applying scene scale.
    pub font_size: f32,
    /// Explicit label weight or the shared weight for other widget text.
    pub font_weight: f32,
    /// Requested label italic state; unsupported fonts resolve upright.
    pub italic: bool,
    /// Pixels between grapheme clusters after scene scaling; no trailing gap.
    pub letter_spacing: f32,
    /// Line box height used for top-positioned text alignment.
    pub line_height: f32,
    /// Fill color with opacity applied.
    pub color: Color,
    /// Effective opacity in `0.0..=1.0`.
    pub opacity: f32,
    /// Optional shadow color.
    pub shadow_color: Option<Color>,
    /// Shadow blur radius.
    pub shadow_strength: f32,
    /// Shadow offset on both axes.
    pub shadow_distance: f32,
    /// Optional text stroke color.
    pub border_color: Option<Color>,
    pub border_thickness: f32,
}

/// Text measurement details used for manual widget layout.
#[derive(Clone, Debug)]
pub struct MeasuredText {
    pub width: f32,
    pub bounds_left: f32,
    pub bounds_top: f32,
    pub bounds_right: f32,
    pub bounds_bottom: f32,
    pub ascent: f32,
    pub descent: f32,
}

// Resolves the scene-level text shadow color with opacity applied.
fn scene_shadow_color(scene: &ValidatedSceneConfig, opacity: f32) -> Option<Color> {
    if scene.shadow_color.is_empty() {
        None
    } else {
        Some(parse_color(&scene.shadow_color, opacity))
    }
}

// Resolves the scene-level text border color with opacity applied.
fn scene_border_color(scene: &ValidatedSceneConfig, opacity: f32) -> Option<Color> {
    if scene.border_color.is_empty() {
        None
    } else {
        Some(parse_color(&scene.border_color, opacity))
    }
}

/// Resolves a text style from a validated label and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the label contract).
pub fn validated_label_style(
    validated: &ValidatedLabel,
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
        font_weight: validated.font_weight,
        italic: validated.italic,
        letter_spacing: validated.font_size * scale * validated.letter_spacing / 100.0,
        line_height: validated.font_size * scale * 0.92,
        color,
        opacity,
        shadow_color: scene_shadow_color(scene, opacity),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness * scale,
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
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness * scale,
    }
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
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness * scale,
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

/// Resolves a text style from a validated gradient widget and scene config.
///
/// All output-affecting fields are already explicit in the validated type.
/// Shadow and border come from scene config (not part of the gradient contract).
pub fn validated_gradient_style(
    validated: &ValidatedGradientWidget,
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
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness * scale,
    }
}

/// Draws text with optional drop shadow and stroke.
pub fn draw_text(
    canvas: &Canvas,
    text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    draw_text_with_vertical_metrics_text(canvas, text, text, style, font_dirs)
}

/// Draws text while allowing baseline alignment to be measured from a stable
/// reference string instead of the rendered glyphs.
pub fn draw_text_with_vertical_metrics_text(
    canvas: &Canvas,
    text: &str,
    vertical_metrics_text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<()> {
    if text.is_empty() {
        return Ok(());
    }

    let font = resolve_styled_font(
        font_dirs,
        style.font_name.as_deref(),
        style.font_size,
        style.font_weight,
        style.italic,
    )?;
    let metrics_text = if vertical_metrics_text.is_empty() {
        text
    } else {
        vertical_metrics_text
    };
    let layout = layout_text(text, &font, style.letter_spacing);
    let blob = layout
        .blob
        .ok_or_else(|| crate::error::CoreError::Render("Failed to lay out label text".into()))?;
    let vertical_measurement = if metrics_text == text {
        layout.measurement
    } else {
        measure_text_with_font(metrics_text, &font)
    };
    let glyph_height = vertical_measurement.bounds_bottom - vertical_measurement.bounds_top;
    let baseline = if glyph_height <= f32::EPSILON {
        baseline_for_top_with_line_height(style.y, &font, style.line_height)
    } else {
        style.y + (style.line_height - glyph_height) * 0.5 - vertical_measurement.bounds_top
    };

    if let Some(shadow_color) = style.shadow_color {
        if style.shadow_strength > 0.0 {
            if let Some(shadow_filter) = image_filters::drop_shadow_only(
                (style.shadow_distance, style.shadow_distance),
                (style.shadow_strength, style.shadow_strength),
                shadow_color,
                None,
                None,
                None,
            ) {
                let mut paint = text_paint(style.color);
                paint.set_image_filter(shadow_filter);
                canvas.draw_text_blob(&blob, Point::new(style.x, baseline), &paint);
            }
        }
    }

    if let Some(border_color) = style.border_color {
        if style.border_thickness > 0.0 {
            let mut paint = text_paint(border_color);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(style.border_thickness);
            paint.set_stroke_join(Join::Round);
            canvas.draw_text_blob(&blob, Point::new(style.x, baseline), &paint);
        }
    }

    let paint = text_paint(style.color);
    canvas.draw_text_blob(&blob, Point::new(style.x, baseline), &paint);
    Ok(())
}

/// Resolves a font from configured font directories or system fonts.
pub fn resolve_font(font_dirs: &[PathBuf], name: Option<&str>, font_size: f32) -> CoreResult<Font> {
    resolve_styled_font(font_dirs, name, font_size, widget_font_weight(), false)
}

/// Resolves the same genuine style and weight for drawing and measurement.
pub fn resolve_styled_font(
    font_dirs: &[PathBuf],
    name: Option<&str>,
    font_size: f32,
    font_weight: f32,
    italic: bool,
) -> CoreResult<Font> {
    let typeface = match name {
        Some(name) => crate::fonts::resolve_typeface(font_dirs, name, font_weight, italic)?,
        None => skia_safe::FontMgr::default()
            .legacy_make_typeface(
                None,
                skia_safe::FontStyle::new(
                    (font_weight.round() as i32).into(),
                    skia_safe::font_style::Width::NORMAL,
                    skia_safe::font_style::Slant::Upright,
                ),
            )
            .ok_or_else(|| {
                crate::error::CoreError::Render("System default font unavailable".into())
            })?,
    };
    let mut font = Font::from_typeface(typeface, font_size);
    font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
    font.set_subpixel(true);
    font.set_hinting(skia_safe::FontHinting::Full);
    Ok(font)
}

/// Computes a baseline for text inside a fixed line-height box.
pub(crate) fn baseline_for_top_with_line_height(top_y: f32, font: &Font, line_height: f32) -> f32 {
    let (_, metrics) = font.metrics();
    let leading_offset = (line_height - font.size()) * 0.5;
    top_y + leading_offset - metrics.ascent
}

/// Computes a baseline using glyph bounds for tighter visual top alignment.
pub fn baseline_for_text_top_with_line_height(
    text: &str,
    top_y: f32,
    font: &Font,
    line_height: f32,
) -> f32 {
    let (_, bounds) = font.measure_str(text, None);
    let glyph_height = (bounds.bottom - bounds.top).abs();

    if glyph_height <= f32::EPSILON {
        return baseline_for_top_with_line_height(top_y, font, line_height);
    }

    let linebox_offset = (line_height - glyph_height) * 0.5;
    top_y + linebox_offset - bounds.top
}

/// Measures text using a resolved style.
pub fn measure_text(
    text: &str,
    style: &ResolvedTextStyle,
    font_dirs: &[PathBuf],
) -> CoreResult<MeasuredText> {
    let font = resolve_styled_font(
        font_dirs,
        style.font_name.as_deref(),
        style.font_size,
        style.font_weight,
        style.italic,
    )?;
    Ok(if style.letter_spacing == 0.0 {
        measure_text_with_font(text, &font)
    } else {
        layout_text(text, &font, style.letter_spacing).measurement
    })
}

struct LaidOutText {
    blob: Option<TextBlob>,
    measurement: MeasuredText,
}

// Keep the existing whole-string layout at zero. Nonzero tracking uses intact
// extended grapheme clusters, with gaps only between them. One blob is reused
// for every paint layer, including shadows where overlapping runs must blur together.
fn layout_text(text: &str, font: &Font, letter_spacing: f32) -> LaidOutText {
    if letter_spacing == 0.0 || text.is_empty() {
        return LaidOutText {
            blob: TextBlob::from_str(text, font),
            measurement: measure_text_with_font(text, font),
        };
    }
    let mut builder = TextBlobBuilder::new();
    let mut width = 0.0;
    let mut ink: Option<Rect> = None;
    for (index, cluster) in text.graphemes(true).enumerate() {
        if index > 0 {
            width += letter_spacing;
        }
        let glyphs = font.str_to_glyphs_vec(cluster);
        builder
            .alloc_run(font, glyphs.len(), (width, 0.0), None)
            .copy_from_slice(&glyphs);
        let (advance, bounds) = font.measure_str(cluster, None);
        if !bounds.is_empty() {
            let positioned = Rect::new(
                bounds.left + width,
                bounds.top,
                bounds.right + width,
                bounds.bottom,
            );
            ink = Some(match ink {
                Some(previous) => Rect::new(
                    previous.left.min(positioned.left),
                    previous.top.min(positioned.top),
                    previous.right.max(positioned.right),
                    previous.bottom.max(positioned.bottom),
                ),
                None => positioned,
            });
        }
        width += advance;
    }
    let bounds = ink.unwrap_or_default();
    let (_, metrics) = font.metrics();
    LaidOutText {
        blob: builder.make(),
        measurement: MeasuredText {
            width,
            bounds_left: bounds.left,
            bounds_top: bounds.top,
            bounds_right: bounds.right,
            bounds_bottom: bounds.bottom,
            ascent: metrics.ascent,
            descent: metrics.descent,
        },
    }
}

/// Measures text using an already-resolved Skia font.
pub fn measure_text_with_font(text: &str, font: &Font) -> MeasuredText {
    let (width, bounds) = font.measure_str(text, None);
    let (_, metrics) = font.metrics();
    MeasuredText {
        width,
        bounds_left: bounds.left,
        bounds_top: bounds.top,
        bounds_right: bounds.right,
        bounds_bottom: bounds.bottom,
        ascent: metrics.ascent,
        descent: metrics.descent,
    }
}

/// Converts a visual text center into the Skia text origin used by `draw_str`.
pub fn origin_x_for_centered_text(text: &str, center_x: f32, font: &Font) -> f32 {
    let (_, bounds) = font.measure_str(text, None);
    center_x - (bounds.left + bounds.right) * 0.5
}

/// Parses `#RRGGBB` or `#RRGGBBAA` text into a Skia ARGB color.
pub fn parse_color(input: &str, opacity: f32) -> Color {
    let hex = input.trim().trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(255),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(255),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(255),
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(255),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(255),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(255),
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(255),
        ),
        _ => (255, 255, 255, 255),
    };
    let scaled_alpha = ((a as f32) * opacity.clamp(0.0, 1.0)).round() as u8;
    Color::from_argb(scaled_alpha, r, g, b)
}

// Creates the anti-aliased Skia paint used for text fills and strokes.
fn text_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint
}
