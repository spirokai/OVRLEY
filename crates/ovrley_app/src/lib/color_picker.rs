//! Colour conversions behind `ColorPicker` (port of the helpers in
//! `app/src/components/ui/color-picker.jsx`), with the same rounding.

use std::sync::LazyLock;

use regex::Regex;

use crate::js;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
    pub a: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub h: f64,
    pub s: f64,
    pub l: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorFormat {
    Hex,
    Rgb,
    Hsl,
    Hsb,
}

/// `hexToRgb`: 3 or 6 digits with an optional `#`; anything else is black.
pub fn hex_to_rgb(hex: &str, alpha: f64) -> Rgba {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    let expanded = if digits.chars().count() == 3 {
        digits.chars().flat_map(|c| [c, c]).collect()
    } else {
        digits.to_string()
    };
    if expanded.len() == 6 && expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        let channel = |i: usize| {
            u8::from_str_radix(&expanded[i..i + 2], 16).expect("checked hex digits") as f64
        };
        Rgba {
            r: channel(0),
            g: channel(2),
            b: channel(4),
            a: alpha,
        }
    } else {
        Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: alpha,
        }
    }
}

/// `rgbToHex`.
pub fn rgb_to_hex(color: Rgba) -> String {
    let to_hex = |n: f64| format!("{:02x}", js::round(n) as i64);
    format!("#{}{}{}", to_hex(color.r), to_hex(color.g), to_hex(color.b))
}

/// `rgbToHsv`: whole degrees and whole percent.
pub fn rgb_to_hsv(color: Rgba) -> Hsv {
    let (r, g, b) = (color.r / 255.0, color.g / 255.0, color.b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let diff = max - min;
    let mut h = 0.0;
    if diff != 0.0 {
        h = if max == r {
            ((g - b) / diff) % 6.0
        } else if max == g {
            (b - r) / diff + 2.0
        } else {
            (r - g) / diff + 4.0
        };
    }
    h = js::round(h * 60.0);
    if h < 0.0 {
        h += 360.0;
    }
    let s = if max == 0.0 { 0.0 } else { diff / max };
    Hsv {
        h,
        s: js::round(s * 100.0),
        v: js::round(max * 100.0),
        a: color.a,
    }
}

/// `hsvToRgb`.
pub fn hsv_to_rgb(hsv: Hsv) -> Rgba {
    let h = hsv.h / 360.0;
    let s = hsv.s / 100.0;
    let v = hsv.v / 100.0;
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    // JavaScript `%` keeps the dividend's sign, like Rust's.
    let (r, g, b) = match (i % 6.0) as i64 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        5 => (v, p, q),
        _ => (0.0, 0.0, 0.0),
    };
    Rgba {
        r: js::round(r * 255.0),
        g: js::round(g * 255.0),
        b: js::round(b * 255.0),
        a: hsv.a,
    }
}

/// `rgbToHsl`: whole degrees and whole percent.
pub fn rgb_to_hsl(color: Rgba) -> Hsl {
    let (r, g, b) = (color.r / 255.0, color.g / 255.0, color.b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let diff = max - min;
    let sum = max + min;
    let l = sum / 2.0;
    let mut h = 0.0;
    let mut s = 0.0;
    if diff != 0.0 {
        s = if l > 0.5 {
            diff / (2.0 - sum)
        } else {
            diff / sum
        };
        if max == r {
            h = (g - b) / diff + if g < b { 6.0 } else { 0.0 };
        } else if max == g {
            h = (b - r) / diff + 2.0;
        } else if max == b {
            h = (r - g) / diff + 4.0;
        }
        h /= 6.0;
    }
    Hsl {
        h: js::round(h * 360.0),
        s: js::round(s * 100.0),
        l: js::round(l * 100.0),
    }
}

/// The RGB of chroma `c`, secondary `x` and lightness offset `m` for a hue sextant, as
/// `hslToRgb` and `parseColorString` compute it. `sextant` is `None` outside [0, 6).
fn chroma_to_rgb(sextant: Option<u8>, c: f64, x: f64, m: f64, alpha: f64) -> Rgba {
    let (r, g, b) = match sextant {
        Some(0) => (c, x, 0.0),
        Some(1) => (x, c, 0.0),
        Some(2) => (0.0, c, x),
        Some(3) => (0.0, x, c),
        Some(4) => (x, 0.0, c),
        Some(5) => (c, 0.0, x),
        _ => (0.0, 0.0, 0.0),
    };
    Rgba {
        r: js::round((r + m) * 255.0),
        g: js::round((g + m) * 255.0),
        b: js::round((b + m) * 255.0),
        a: alpha,
    }
}

/// `hslToRgb`. The sextant tests compare `h / 360` against sixths, as the JavaScript does.
pub fn hsl_to_rgb(hsl: Hsl, alpha: f64) -> Rgba {
    let h = hsl.h / 360.0;
    let s = hsl.s / 100.0;
    let l = hsl.l / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h * 6.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let sextant = (0..6u8).find(|&k| h >= k as f64 / 6.0 && h < (k + 1) as f64 / 6.0);
    chroma_to_rgb(sextant, c, x, m, alpha)
}

/// `colorToString`.
pub fn color_to_string(color: Rgba, format: ColorFormat) -> String {
    let a = js::number_to_string(color.a);
    match format {
        ColorFormat::Hex => rgb_to_hex(color),
        ColorFormat::Rgb => {
            let (r, g, b) = (
                js::number_to_string(color.r),
                js::number_to_string(color.g),
                js::number_to_string(color.b),
            );
            if color.a < 1.0 {
                format!("rgba({r}, {g}, {b}, {a})")
            } else {
                format!("rgb({r}, {g}, {b})")
            }
        }
        ColorFormat::Hsl => {
            let hsl = rgb_to_hsl(color);
            if color.a < 1.0 {
                format!("hsla({}, {}%, {}%, {a})", hsl.h, hsl.s, hsl.l)
            } else {
                format!("hsl({}, {}%, {}%)", hsl.h, hsl.s, hsl.l)
            }
        }
        ColorFormat::Hsb => {
            let hsv = rgb_to_hsv(color);
            if color.a < 1.0 {
                format!("hsba({}, {}%, {}%, {a})", hsv.h, hsv.s, hsv.v)
            } else {
                format!("hsb({}, {}%, {}%)", hsv.h, hsv.s, hsv.v)
            }
        }
    }
}

static HEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#?([a-fA-F0-9]{3}|[a-fA-F0-9]{6})$").unwrap());
static RGB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^rgba?\(\s*([0-9]+)\s*,\s*([0-9]+)\s*,\s*([0-9]+)\s*(?:,\s*([0-9.]+))?\s*\)$")
        .unwrap()
});
static HSL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^hsla?\(\s*([0-9]+)\s*,\s*([0-9]+)%\s*,\s*([0-9]+)%\s*(?:,\s*([0-9.]+))?\s*\)$")
        .unwrap()
});
static HSB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^hsba?\(\s*([0-9]+)\s*,\s*([0-9]+)%\s*,\s*([0-9]+)%\s*(?:,\s*([0-9.]+))?\s*\)$")
        .unwrap()
});

/// `parseColorString`: hex, `rgb[a]()`, `hsl[a]()` or `hsb[a]()`; `None` otherwise.
pub fn parse_color_string(value: &str) -> Option<Rgba> {
    let trimmed = value.trim();
    if HEX.is_match(trimmed) {
        return Some(hex_to_rgb(trimmed, 1.0));
    }
    let int =
        |caps: &regex::Captures, i: usize| caps[i].parse::<f64>().expect("regex matched digits");
    let alpha = |caps: &regex::Captures| {
        caps.get(4)
            .map_or(Some(1.0), |m| js::parse_float_prefix(m.as_str()))
    };
    if let Some(caps) = RGB.captures(trimmed) {
        // `parseFloat` of a lone "." is NaN in JavaScript; the colour keeps it.
        return Some(Rgba {
            r: int(&caps, 1),
            g: int(&caps, 2),
            b: int(&caps, 3),
            a: alpha(&caps).unwrap_or(f64::NAN),
        });
    }
    if let Some(caps) = HSL.captures(trimmed) {
        let h = int(&caps, 1);
        let s = int(&caps, 2) / 100.0;
        let l = int(&caps, 3) / 100.0;
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
        let m = l - c / 2.0;
        let sextant = (0..6u8).find(|&k| h >= k as f64 * 60.0 && h < (k + 1) as f64 * 60.0);
        return Some(chroma_to_rgb(
            sextant,
            c,
            x,
            m,
            alpha(&caps).unwrap_or(f64::NAN),
        ));
    }
    if let Some(caps) = HSB.captures(trimmed) {
        return Some(hsv_to_rgb(Hsv {
            h: int(&caps, 1),
            s: int(&caps, 2),
            v: int(&caps, 3),
            a: alpha(&caps).unwrap_or(f64::NAN),
        }));
    }
    None
}

/// A channel input's value as the RGB/HSL/HSB inputs accept it: `parseInt` of the text, kept
/// only when it is within `0..=max`.
pub fn parse_channel(text: &str, max: f64) -> Option<f64> {
    js::parse_int(text).filter(|value| (0.0..=max).contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        Rgba { r, g, b, a }
    }

    // Expected values were produced by running the JavaScript helpers from color-picker.jsx.

    #[test]
    fn hex_round_trips() {
        assert_eq!(hex_to_rgb("#ad5013", 1.0), rgba(173.0, 80.0, 19.0, 1.0));
        assert_eq!(hex_to_rgb("abc", 0.5), rgba(170.0, 187.0, 204.0, 0.5));
        assert_eq!(hex_to_rgb("#abcd", 1.0), rgba(0.0, 0.0, 0.0, 1.0));
        assert_eq!(rgb_to_hex(rgba(173.4, 79.5, 19.0, 1.0)), "#ad5013");
    }

    #[test]
    fn converts_rgb_to_hsv_and_back() {
        assert_eq!(
            rgb_to_hsv(rgba(173.0, 80.0, 19.0, 1.0)),
            Hsv {
                h: 24.0,
                s: 89.0,
                v: 68.0,
                a: 1.0
            }
        );
        assert_eq!(
            rgb_to_hsv(rgba(255.0, 0.0, 128.0, 1.0)),
            Hsv {
                h: 330.0,
                s: 100.0,
                v: 100.0,
                a: 1.0
            }
        );
        assert_eq!(
            rgb_to_hsv(rgba(255.0, 0.0, 1.0, 1.0)),
            Hsv {
                h: 0.0,
                s: 100.0,
                v: 100.0,
                a: 1.0
            }
        );
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 24.0,
                s: 89.0,
                v: 68.0,
                a: 1.0
            }),
            rgba(173.0, 81.0, 19.0, 1.0)
        );
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 360.0,
                s: 100.0,
                v: 100.0,
                a: 1.0
            }),
            rgba(255.0, 0.0, 0.0, 1.0)
        );
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 200.0,
                s: 50.0,
                v: 40.0,
                a: 0.5
            }),
            rgba(51.0, 85.0, 102.0, 0.5)
        );
    }

    #[test]
    fn converts_rgb_to_hsl_and_back() {
        assert_eq!(
            rgb_to_hsl(rgba(173.0, 80.0, 19.0, 1.0)),
            Hsl {
                h: 24.0,
                s: 80.0,
                l: 38.0
            }
        );
        assert_eq!(
            rgb_to_hsl(rgba(64.0, 224.0, 208.0, 1.0)),
            Hsl {
                h: 174.0,
                s: 72.0,
                l: 56.0
            }
        );
        assert_eq!(
            hsl_to_rgb(
                Hsl {
                    h: 174.0,
                    s: 72.0,
                    l: 56.0
                },
                1.0
            ),
            rgba(62.0, 224.0, 207.0, 1.0)
        );
        assert_eq!(
            hsl_to_rgb(
                Hsl {
                    h: 360.0,
                    s: 50.0,
                    l: 50.0
                },
                1.0
            ),
            rgba(64.0, 64.0, 64.0, 1.0)
        );
    }

    #[test]
    fn formats_colors() {
        let color = rgba(173.0, 80.0, 19.0, 1.0);
        assert_eq!(color_to_string(color, ColorFormat::Hex), "#ad5013");
        assert_eq!(color_to_string(color, ColorFormat::Rgb), "rgb(173, 80, 19)");
        assert_eq!(
            color_to_string(color, ColorFormat::Hsl),
            "hsl(24, 80%, 38%)"
        );
        assert_eq!(
            color_to_string(color, ColorFormat::Hsb),
            "hsb(24, 89%, 68%)"
        );
        assert_eq!(
            color_to_string(rgba(1.0, 2.0, 3.0, 0.5), ColorFormat::Rgb),
            "rgba(1, 2, 3, 0.5)"
        );
    }

    #[test]
    fn parses_color_strings() {
        assert_eq!(
            parse_color_string(" #ABC "),
            Some(rgba(170.0, 187.0, 204.0, 1.0))
        );
        assert_eq!(
            parse_color_string("ad5013"),
            Some(rgba(173.0, 80.0, 19.0, 1.0))
        );
        assert_eq!(
            parse_color_string("rgb(1, 2, 3)"),
            Some(rgba(1.0, 2.0, 3.0, 1.0))
        );
        assert_eq!(
            parse_color_string("rgba(1,2,3,0.25)"),
            Some(rgba(1.0, 2.0, 3.0, 0.25))
        );
        assert_eq!(
            parse_color_string("rgb(999, 0, 0)"),
            Some(rgba(999.0, 0.0, 0.0, 1.0))
        );
        assert_eq!(
            parse_color_string("hsl(174, 72%, 56%)"),
            Some(rgba(62.0, 224.0, 207.0, 1.0))
        );
        assert_eq!(
            parse_color_string("hsl(400, 50%, 50%)"),
            Some(rgba(64.0, 64.0, 64.0, 1.0))
        );
        assert_eq!(
            parse_color_string("hsb(24, 89%, 68%)"),
            Some(rgba(173.0, 81.0, 19.0, 1.0))
        );
        assert_eq!(parse_color_string("#abcd"), None);
        assert_eq!(parse_color_string("red"), None);
    }

    #[test]
    fn parses_channels_like_parse_int() {
        assert_eq!(parse_channel("12", 255.0), Some(12.0));
        assert_eq!(parse_channel("12px", 255.0), Some(12.0));
        assert_eq!(parse_channel("256", 255.0), None);
        assert_eq!(parse_channel("-1", 255.0), None);
        assert_eq!(parse_channel("", 100.0), None);
    }
}
