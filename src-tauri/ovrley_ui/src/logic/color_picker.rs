use crate::{ColorPickerLogic, Hsl, Hsv, ParsedChannel, ParsedColor, Rgba};
use ovrley_app::lib::color_picker::{self as cp, ColorFormat};
use ovrley_app::lib::color_utils::normalize_hex_color;

fn to_app(color: Rgba) -> cp::Rgba {
    cp::Rgba {
        r: color.r as f64,
        g: color.g as f64,
        b: color.b as f64,
        a: color.a as f64,
    }
}

fn to_ui(color: cp::Rgba) -> Rgba {
    Rgba {
        r: color.r as f32,
        g: color.g as f32,
        b: color.b as f32,
        a: color.a as f32,
    }
}

fn format(format: crate::ColorFormat) -> ColorFormat {
    match format {
        crate::ColorFormat::Hex => ColorFormat::Hex,
        crate::ColorFormat::Rgb => ColorFormat::Rgb,
        crate::ColorFormat::Hsl => ColorFormat::Hsl,
        crate::ColorFormat::Hsb => ColorFormat::Hsb,
    }
}

/// A normalized `#rrggbb` string as a Slint colour. Callers pass normalized values only.
pub(crate) fn hex_to_color(hex: &str) -> slint::Color {
    let value = hex
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6)
        .and_then(|digits| u32::from_str_radix(digits, 16).ok())
        .unwrap_or_else(|| panic!("hex-to-color takes a normalized #rrggbb colour, got {hex:?}"));
    slint::Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub(super) fn bind(logic: &ColorPickerLogic) {
    logic.on_hex_to_rgb(|hex, alpha| to_ui(cp::hex_to_rgb(&hex, alpha as f64)));
    logic.on_rgb_to_hex(|color| cp::rgb_to_hex(to_app(color)).into());
    logic.on_rgb_to_hsv(|color| {
        let hsv = cp::rgb_to_hsv(to_app(color));
        Hsv {
            h: hsv.h as f32,
            s: hsv.s as f32,
            v: hsv.v as f32,
            a: hsv.a as f32,
        }
    });
    logic.on_hsv_to_rgb(|hsv| {
        to_ui(cp::hsv_to_rgb(cp::Hsv {
            h: hsv.h as f64,
            s: hsv.s as f64,
            v: hsv.v as f64,
            a: hsv.a as f64,
        }))
    });
    logic.on_rgb_to_hsl(|color| {
        let hsl = cp::rgb_to_hsl(to_app(color));
        Hsl {
            h: hsl.h as f32,
            s: hsl.s as f32,
            l: hsl.l as f32,
        }
    });
    logic.on_hsl_to_rgb(|hsl, alpha| {
        to_ui(cp::hsl_to_rgb(
            cp::Hsl {
                h: hsl.h as f64,
                s: hsl.s as f64,
                l: hsl.l as f64,
            },
            alpha as f64,
        ))
    });
    logic.on_color_to_string(|color, f| cp::color_to_string(to_app(color), format(f)).into());
    logic.on_parse_color_string(|value| match cp::parse_color_string(&value) {
        Some(color) => ParsedColor {
            valid: true,
            color: to_ui(color),
        },
        None => ParsedColor {
            valid: false,
            color: Rgba::default(),
        },
    });
    logic.on_parse_channel(|text, max| match cp::parse_channel(&text, max as f64) {
        Some(value) => ParsedChannel {
            valid: true,
            value: value as f32,
        },
        None => ParsedChannel {
            valid: false,
            value: 0.0,
        },
    });
    logic.on_normalize_hex_color(|value, fallback| normalize_hex_color(&value, &fallback).into());
    logic.on_hex_to_color(|hex| hex_to_color(&hex));
}
