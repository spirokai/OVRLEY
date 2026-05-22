//! Text styling, measurement, and drawing.
//!
//! Template text settings are resolved into concrete Skia font, color, shadow,
//! and border values here. Font lookup is cached because labels and dynamic
//! values reuse the same typefaces across many frames.

use crate::config::{LabelConfig, SceneConfig, ValueConfig};
use crate::MetricKind;
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
    /// Text stroke thickness.
    pub border_thickness: f32,
    /// Reserved border offset value.
    pub border_distance: f32,
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
fn scene_shadow_color(scene: &SceneConfig, opacity: f32) -> Option<Color> {
    scene
        .shadow_color
        .as_deref()
        .map(|color| parse_color(color, opacity))
}

// Resolves the scene-level text border color with opacity applied.
fn scene_border_color(scene: &SceneConfig, opacity: f32) -> Option<Color> {
    scene
        .border_color
        .as_deref()
        .map(|color| parse_color(color, opacity))
}

/// Resolves a static label style from scene defaults and label overrides.
pub fn label_style(scene: &SceneConfig, label: &LabelConfig, scale: f32) -> ResolvedTextStyle {
    let opacity = label.opacity.or(scene.opacity).unwrap_or(1.0);

    ResolvedTextStyle {
        x: label.x,
        y: label.y,
        font_name: label
            .font
            .clone()
            .or(label.font_family.clone())
            .or_else(|| scene.font.clone()),
        font_size: label.font_size.or(scene.font_size).unwrap_or(32.0) * scale,
        line_height: label.font_size.or(scene.font_size).unwrap_or(32.0) * scale * 0.92,
        color: parse_color(
            label
                .color
                .as_deref()
                .or(scene.color.as_deref())
                .unwrap_or("#ffffff"),
            opacity,
        ),
        opacity,
        shadow_color: scene_shadow_color(scene, opacity),
        shadow_strength: scene.shadow_strength.unwrap_or(0.0) * scale,
        shadow_distance: scene.shadow_distance.unwrap_or(0.0) * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness.unwrap_or(0.0) * scale,
        border_distance: scene.border_distance.unwrap_or(0.0) * scale,
    }
}

/// Resolves a dynamic value style from scene defaults and value overrides.
pub fn value_style(scene: &SceneConfig, value: &ValueConfig, scale: f32) -> ResolvedTextStyle {
    let base_y = if value.value == MetricKind::Gradient {
        value.y
    } else {
        value.y + value.value_offset.unwrap_or(0.0)
    };
    let opacity = value.opacity.or(scene.opacity).unwrap_or(1.0);

    ResolvedTextStyle {
        x: value.x,
        y: base_y,
        font_name: value
            .font
            .clone()
            .or(value.font_family.clone())
            .or_else(|| scene.font.clone()),
        font_size: value.font_size.or(scene.font_size).unwrap_or(32.0) * scale,
        line_height: value.font_size.or(scene.font_size).unwrap_or(32.0) * scale * 0.92,
        color: parse_color(
            value
                .color
                .as_deref()
                .or(scene.color.as_deref())
                .unwrap_or("#ffffff"),
            opacity,
        ),
        opacity,
        shadow_color: scene_shadow_color(scene, opacity),
        shadow_strength: scene.shadow_strength.unwrap_or(0.0) * scale,
        shadow_distance: scene.shadow_distance.unwrap_or(0.0) * scale,
        border_color: scene_border_color(scene, opacity),
        border_thickness: scene.border_thickness.unwrap_or(0.0) * scale,
        border_distance: scene.border_distance.unwrap_or(0.0) * scale,
    }
}

/// Draws text with optional drop shadow and stroke.
pub fn draw_text(canvas: &Canvas, text: &str, style: &ResolvedTextStyle, font_dirs: &[PathBuf]) {
    if text.is_empty() {
        return;
    }

    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size);
    let baseline = baseline_for_text_top_with_line_height(text, style.y, &font, style.line_height);

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
}

/// Resolves a font from configured font directories or system fonts.
pub fn resolve_font(font_dirs: &[PathBuf], name: Option<&str>, font_size: f32) -> Font {
    let typeface = resolve_typeface(font_dirs, name);
    Font::from_typeface(typeface, font_size)
}

/// Computes a baseline for text whose top edge should start at `top_y`.
pub fn baseline_for_top(top_y: f32, font: &Font) -> f32 {
    let (_, metrics) = font.metrics();
    top_y - metrics.ascent
}

/// Computes a baseline for text inside a fixed line-height box.
pub fn baseline_for_top_with_line_height(top_y: f32, font: &Font, line_height: f32) -> f32 {
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
pub fn measure_text(text: &str, style: &ResolvedTextStyle, font_dirs: &[PathBuf]) -> MeasuredText {
    let font = resolve_font(font_dirs, style.font_name.as_deref(), style.font_size);
    measure_text_with_font(text, &font)
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
fn resolve_typeface(font_dirs: &[PathBuf], name: Option<&str>) -> Typeface {
    // Cache by requested name, not resolved absolute path, because templates
    // repeatedly request the same family/file for labels and values.
    static CACHE: OnceLock<Mutex<HashMap<String, Typeface>>> = OnceLock::new();
    let key = name.unwrap_or("Arial.ttf").to_string();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock() {
        if let Some(typeface) = cache.get(&key) {
            return typeface.clone();
        }
    }

    let resolved = load_typeface(font_dirs, name)
        .or_else(|| load_typeface(font_dirs, Some("Arial.ttf")))
        .or_else(|| FontMgr::default().legacy_make_typeface(Some("Arial"), FontStyle::normal()))
        .or_else(|| FontMgr::default().legacy_make_typeface(None, FontStyle::normal()))
        .expect("failed to resolve a usable typeface");

    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, resolved.clone());
    }
    resolved
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

    if Path::new(name).extension().is_none() {
        return font_mgr.legacy_make_typeface(Some(name), FontStyle::normal());
    }

    None
}
