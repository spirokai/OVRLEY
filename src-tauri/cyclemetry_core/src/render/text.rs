use crate::config::{LabelConfig, SceneConfig, ValueConfig};
use skia_safe::{Canvas, Color, Font, FontMgr, FontStyle, Paint, Point, Typeface};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct ResolvedTextStyle {
    pub x: f32,
    pub y: f32,
    pub font_name: Option<String>,
    pub font_size: f32,
    pub color: Color,
    pub opacity: f32,
    pub shadow_color: Option<Color>,
    pub shadow_strength: f32,
    pub shadow_distance: f32,
    pub border_color: Option<Color>,
    pub border_thickness: f32,
    pub border_distance: f32,
}

pub fn label_style(scene: &SceneConfig, label: &LabelConfig, scale: f32) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: label.x,
        y: label.y,
        font_name: label
            .font
            .clone()
            .or(label.font_family.clone())
            .or_else(|| scene.font.clone()),
        font_size: label.font_size.or(scene.font_size).unwrap_or(32.0) * scale,
        color: parse_color(
            label
                .color
                .as_deref()
                .or(scene.color.as_deref())
                .unwrap_or("#ffffff"),
            label.opacity.or(scene.opacity).unwrap_or(1.0),
        ),
        opacity: label.opacity.or(scene.opacity).unwrap_or(1.0),
        shadow_color: label
            .shadow_color
            .as_deref()
            .map(|color| parse_color(color, label.opacity.or(scene.opacity).unwrap_or(1.0))),
        shadow_strength: label.shadow_strength.unwrap_or(0.0) * scale,
        shadow_distance: label.shadow_distance.unwrap_or(0.0) * scale,
        border_color: label
            .border_color
            .as_deref()
            .map(|color| parse_color(color, label.opacity.or(scene.opacity).unwrap_or(1.0))),
        border_thickness: label.border_thickness.unwrap_or(0.0) * scale,
        border_distance: label.border_distance.unwrap_or(1.0).max(1.0) * scale,
    }
}

pub fn value_style(scene: &SceneConfig, value: &ValueConfig, scale: f32) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: value.x,
        y: value.y + value.value_offset.unwrap_or(0.0),
        font_name: value
            .font
            .clone()
            .or(value.font_family.clone())
            .or_else(|| scene.font.clone()),
        font_size: value.font_size.or(scene.font_size).unwrap_or(32.0) * scale,
        color: parse_color(
            value
                .color
                .as_deref()
                .or(scene.color.as_deref())
                .unwrap_or("#ffffff"),
            value.opacity.or(scene.opacity).unwrap_or(1.0),
        ),
        opacity: value.opacity.or(scene.opacity).unwrap_or(1.0),
        shadow_color: value
            .shadow_color
            .as_deref()
            .map(|color| parse_color(color, value.opacity.or(scene.opacity).unwrap_or(1.0))),
        shadow_strength: value.shadow_strength.unwrap_or(0.0) * scale,
        shadow_distance: value.shadow_distance.unwrap_or(0.0) * scale,
        border_color: value
            .border_color
            .as_deref()
            .map(|color| parse_color(color, value.opacity.or(scene.opacity).unwrap_or(1.0))),
        border_thickness: value.border_thickness.unwrap_or(0.0) * scale,
        border_distance: value.border_distance.unwrap_or(1.0).max(1.0) * scale,
    }
}

pub fn draw_text(canvas: &Canvas, text: &str, style: &ResolvedTextStyle, font_dirs: &[PathBuf]) {
    if text.is_empty() {
        return;
    }

    let typeface = resolve_typeface(font_dirs, style.font_name.as_deref());
    let font = Font::from_typeface(typeface, style.font_size);
    let (_, metrics) = font.metrics();
    let baseline = style.y - metrics.ascent;

    if let Some(border_color) = style.border_color {
        let steps = style.border_thickness.round().max(0.0) as i32;
        if steps > 0 {
            let paint = text_paint(border_color);
            for step in 1..=steps {
                let offset = step as f32 * style.border_distance.max(1.0);
                for (dx, dy) in [
                    (-offset, 0.0),
                    (offset, 0.0),
                    (0.0, -offset),
                    (0.0, offset),
                    (-offset, -offset),
                    (offset, -offset),
                    (-offset, offset),
                    (offset, offset),
                ] {
                    canvas.draw_str(text, Point::new(style.x + dx, baseline + dy), &font, &paint);
                }
            }
        }
    }

    if let Some(shadow_color) = style.shadow_color {
        let paint = text_paint(shadow_color);
        let passes = style.shadow_strength.round().max(1.0) as i32;
        let shadow_offset = style.shadow_distance;
        for blur_step in 0..passes {
            let spread = blur_step as f32 * 0.4;
            for (dx, dy) in [
                (shadow_offset, shadow_offset),
                (shadow_offset + spread, shadow_offset),
                (shadow_offset, shadow_offset + spread),
            ] {
                canvas.draw_str(text, Point::new(style.x + dx, baseline + dy), &font, &paint);
            }
        }
    }

    let paint = text_paint(style.color);
    canvas.draw_str(text, Point::new(style.x, baseline), &font, &paint);
}

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

fn text_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint
}

fn resolve_typeface(font_dirs: &[PathBuf], name: Option<&str>) -> Typeface {
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

fn load_typeface(font_dirs: &[PathBuf], name: Option<&str>) -> Option<Typeface> {
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

    if !Path::new(name).extension().is_some() {
        return font_mgr.legacy_make_typeface(Some(name), FontStyle::normal());
    }

    None
}
