//! Text styling, measurement, and drawing.
//!
//! Template text settings are resolved into concrete Skia font, color, shadow,
//! and border values here. Font lookup is cached because labels and dynamic
//! values reuse the same typefaces across many frames.

use crate::error::{CoreError, CoreResult};
use crate::normalize::{
    ValidatedGradientWidget, ValidatedLabel, ValidatedSceneConfig, ValidatedTimeValue,
    ValidatedValueWidget,
};
use skia_safe::{
    image_filters,
    paint::{Join, Style},
    Canvas, Color, Font, FontMgr, FontStyle, Paint, Point, Typeface,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

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

    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size)?;
    let metrics_text = if vertical_metrics_text.is_empty() {
        text
    } else {
        vertical_metrics_text
    };
    let baseline =
        baseline_for_text_top_with_line_height(metrics_text, style.y, &font, style.line_height);

    if let Some(shadow_color) = style.shadow_color {
        if style.shadow_strength > 0.0 || style.shadow_distance != 0.0 {
            if let Some(shadow_filter) = image_filters::drop_shadow_only(
                (style.shadow_distance, style.shadow_distance),
                (style.shadow_strength, style.shadow_strength),
                shadow_color,
                None,
                None,
            ) {
                let mut paint = text_paint(style.color);
                paint.set_image_filter(shadow_filter);
                canvas.draw_str(text, Point::new(style.x, baseline), &font, &paint);
            }
        }
    }

    if let Some(border_color) = style.border_color {
        if style.border_thickness > 0.0 {
            let mut paint = text_paint(border_color);
            paint.set_style(Style::Stroke);
            paint.set_stroke_width(style.border_thickness);
            paint.set_stroke_join(Join::Round);
            canvas.draw_str(text, Point::new(style.x, baseline), &font, &paint);
        }
    }

    let paint = text_paint(style.color);
    canvas.draw_str(text, Point::new(style.x, baseline), &font, &paint);
    Ok(())
}

/// Resolves a font from configured font directories or system fonts.
pub fn resolve_font(font_dirs: &[PathBuf], name: Option<&str>, font_size: f32) -> CoreResult<Font> {
    let typeface = resolve_typeface(font_dirs, name)?;
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
    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size)?;
    Ok(measure_text_with_font(text, &font))
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

// Resolves and caches a Skia typeface for a font name or file.
//
// The cache is a global `OnceLock<Mutex<HashMap<…>>>` keyed by font name. Fonts
// are loaded from disk once and never invalidated — the font set is fixed and
// small (system fonts + bundled fonts, ~10–20 entries max). Entries are
// immutable after load, so stale reads are harmless. This cache is accessed on
// the hot path (every text-drawing call), but the `Mutex` is only locked on
// first insertion; subsequent lookups hit the cached `Typeface` directly.
fn resolve_typeface(font_dirs: &[PathBuf], name: Option<&str>) -> CoreResult<Typeface> {
    static CACHE: OnceLock<Mutex<HashMap<String, Typeface>>> = OnceLock::new();
    let key = name.unwrap_or("__default_font__").to_string();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(typeface) = cache.get(&key) {
            return Ok(typeface.clone());
        }
    }

    let resolved = load_typeface(font_dirs, name)
        .or_else(|| load_first_bundled_typeface(font_dirs))
        .or_else(|| FontMgr::default().legacy_make_typeface(Some("Arial"), FontStyle::normal()))
        .or_else(|| FontMgr::default().legacy_make_typeface(None, FontStyle::normal()))
        .ok_or_else(|| CoreError::Render("failed to resolve a usable typeface".into()))?;

    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, resolved.clone());
    }
    Ok(resolved)
}

// Attempts to load a typeface from an explicit path, bundled dir, or system family.
fn load_typeface(font_dirs: &[PathBuf], name: Option<&str>) -> Option<Typeface> {
    // Prefer explicit files, then bundled font directories, then system family
    // names. That lets packaged templates pin a bundled font when needed.
    let name = name?;
    let font_mgr = FontMgr::default();
    let direct = PathBuf::from(name);
    if direct.is_file() {
        let bytes = fs::read(&direct).ok()?;
        return font_mgr.new_from_data(&bytes, None);
    }

    for dir in font_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            let bytes = fs::read(&candidate).ok()?;
            return font_mgr.new_from_data(&bytes, None);
        }
    }

    let family_name = strip_supported_font_extension(name).unwrap_or(name);
    font_mgr
        .match_family_style(family_name, FontStyle::normal())
        .or_else(|| font_mgr.legacy_make_typeface(Some(family_name), FontStyle::normal()))
}

fn load_first_bundled_typeface(font_dirs: &[PathBuf]) -> Option<Typeface> {
    let font_mgr = FontMgr::default();
    let mut candidates = Vec::new();

    for dir in font_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_supported_font = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|extension| {
                        extension.eq_ignore_ascii_case("ttf")
                            || extension.eq_ignore_ascii_case("otf")
                            || extension.eq_ignore_ascii_case("ttc")
                    })
                    .unwrap_or(false);

                if is_supported_font {
                    candidates.push(path);
                }
            }
        }
    }

    candidates.sort();

    for candidate in candidates {
        if let Ok(bytes) = fs::read(&candidate) {
            if let Some(typeface) = font_mgr.new_from_data(&bytes, None) {
                return Some(typeface);
            }
        }
    }

    None
}

fn strip_supported_font_extension(name: &str) -> Option<&str> {
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            value.eq_ignore_ascii_case("ttf")
                || value.eq_ignore_ascii_case("otf")
                || value.eq_ignore_ascii_case("ttc")
                || value.eq_ignore_ascii_case("woff")
                || value.eq_ignore_ascii_case("woff2")
                || value.eq_ignore_ascii_case("fon")
        })?;
    let end = name.len().saturating_sub(extension.len() + 1);
    Some(name[..end].trim_end_matches('.'))
}
