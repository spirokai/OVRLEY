//! Fixed digit cells for changing values.
//!
//! Each digit's ink is centered in a cell as wide as the face's widest digit
//! advance, so a value keeps its width while its digits change. Faces whose
//! digits already share one advance keep the plain string layout.
use skia_safe::{Font, GlyphId, Rect};
use unicode_segmentation::UnicodeSegmentation;

/// Glyphs with explicit x positions on one baseline.
pub(crate) struct PlacedGlyphs {
    pub glyphs: Vec<GlyphId>,
    pub xs: Vec<f32>,
    pub width: f32,
    pub ink: Rect,
}

/// Cell placement for `text`, or `None` when no digit would move.
pub(crate) fn place_digits(text: &str, font: &Font, letter_spacing: f32) -> Option<PlacedGlyphs> {
    if !text.bytes().any(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let digits: [GlyphId; 10] =
        std::array::from_fn(|digit| font.unichar_to_glyph(i32::from(b'0') + digit as i32));
    let mut digit_widths = [0.0; 10];
    font.get_widths(&digits, &mut digit_widths);
    let cell = digit_widths.iter().copied().fold(0.0, f32::max);
    if digit_widths.iter().all(|width| *width == cell) {
        return None;
    }

    let mut placed = PlacedGlyphs {
        glyphs: Vec::with_capacity(text.len()),
        xs: Vec::with_capacity(text.len()),
        width: 0.0,
        ink: Rect::new_empty(),
    };
    // Tracking separates extended grapheme clusters, matching the default layout.
    for (index, cluster) in text.graphemes(true).enumerate() {
        if index > 0 {
            placed.width += letter_spacing;
        }
        let glyphs = font.str_to_glyphs_vec(cluster);
        let mut advances = vec![0.0; glyphs.len()];
        let mut bounds = vec![Rect::default(); glyphs.len()];
        font.get_widths_bounds(&glyphs, Some(&mut advances), Some(&mut bounds), None);
        let digit = cluster.len() == 1 && cluster.as_bytes()[0].is_ascii_digit();
        let (mut x, advance) = if digit && !bounds[0].is_empty() {
            let ink = bounds[0];
            (placed.width + (cell - ink.width()) * 0.5 - ink.left, cell)
        } else if digit {
            (placed.width + (cell - advances[0]) * 0.5, cell)
        } else {
            (placed.width, advances.iter().sum())
        };
        for ((glyph, glyph_advance), glyph_bounds) in glyphs.into_iter().zip(advances).zip(bounds) {
            if !glyph_bounds.is_empty() {
                placed.ink.join(glyph_bounds.with_offset((x, 0.0)));
            }
            placed.glyphs.push(glyph);
            placed.xs.push(x);
            x += glyph_advance;
        }
        placed.width += advance;
    }
    if placed.ink.is_empty() {
        placed.ink = Rect::default();
    }
    Some(placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skia_safe::{Data, FontMgr, Typeface};

    fn font(file: &str) -> Font {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fonts")
            .join(file);
        let typeface: Typeface = FontMgr::default()
            .new_from_data(
                Data::new_copy(&std::fs::read(path).expect("bundled font")),
                None,
            )
            .expect("bundled font face");
        Font::from_typeface(typeface, 100.0)
    }

    #[test]
    fn proportional_digits_share_one_cell_with_centered_ink() {
        let font = font("Rajdhani.ttf");
        let ones = place_digits("11", &font, 0.0).expect("proportional digits move");
        let zeros = place_digits("00", &font, 0.0).expect("proportional digits move");
        assert_eq!(ones.width, zeros.width);
        for placed in [ones, zeros] {
            let (left, right) = (placed.ink.left, placed.width - placed.ink.right);
            assert!((left - right).abs() < 0.01, "{left} vs {right}");
        }
    }

    #[test]
    fn faces_with_uniform_digits_keep_default_layout() {
        assert!(place_digits("10", &font("JetBrainsMono-variable.ttf"), 0.0).is_none());
    }
}
