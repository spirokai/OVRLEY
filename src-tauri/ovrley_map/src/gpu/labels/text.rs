//! Label text: which face draws a character, and where each glyph of a label goes.
//!
//! The shaping covers what the maps draw: one font, horizontal writing, no inline images.
//! It works in glyph units, where one em is [`ONE_EM`] (the 24 px the glyph bitmaps are made at), and
//! takes glyph metrics from a caller-supplied lookup. That keeps it free of font files, so the preview's
//! decode workers can run it from the metrics the glyph service gave them.
//!
//! None of the supported scripts needs contextual shaping (Latin, Greek, Cyrillic, Hebrew, Chinese,
//! Japanese and Korean: one codepoint, one glyph), so a character is a shaping unit. Hebrew needs bidi
//! reordering only.

use std::collections::HashMap;
use unicode_bidi::{bidi_class, BidiClass, BidiInfo};
use unicode_bidi_mirroring::get_mirrored;

/// One em, in the glyph units shaping works in. Glyph bitmaps are made at this size.
pub const ONE_EM: f64 = 24.0;
/// A glyph's origin sits 17 units above the line, and the glyph metrics' `top` is relative to it.
const SHAPING_DEFAULT_OFFSET: f64 = -17.0;

/// A bundled font file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    NotoSansRegular,
    NotoSansBold,
    NotoSansItalic,
    NotoSansHebrewRegular,
    NotoSansHebrewBold,
    /// Noto Sans CJK SC Regular, cut to the characters in `scripts/build-map-cjk-pack.py`.
    CjkPack,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::NotoSansRegular,
        Face::NotoSansBold,
        Face::NotoSansItalic,
        Face::NotoSansHebrewRegular,
        Face::NotoSansHebrewBold,
        Face::CjkPack,
    ];

    /// The file's name inside `fonts/map/`.
    pub fn file_name(self) -> &'static str {
        match self {
            Face::NotoSansRegular => "NotoSans-Regular.ttf",
            Face::NotoSansBold => "NotoSans-Bold.ttf",
            Face::NotoSansItalic => "NotoSans-Italic.ttf",
            Face::NotoSansHebrewRegular => "NotoSansHebrew-Regular.ttf",
            Face::NotoSansHebrewBold => "NotoSansHebrew-Bold.ttf",
            Face::CjkPack => "NotoSansCJKsc-Pack.otf",
        }
    }

    /// How many times 24 px this face's glyph bitmaps are made at. CJK glyphs are drawn at 2x.
    pub fn texture_scale(self) -> u32 {
        match self {
            Face::CjkPack => 2,
            _ => 1,
        }
    }
}

/// One face of a font chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainLink {
    pub face: Face,
    /// The face is a Regular standing in for a Bold: the renderer lowers the SDF threshold for it.
    pub bold: bool,
}

const fn link(face: Face, bold: bool) -> ChainLink {
    ChainLink { face, bold }
}

const REGULAR_CHAIN: &[ChainLink] = &[
    link(Face::NotoSansRegular, false),
    link(Face::NotoSansHebrewRegular, false),
    link(Face::CjkPack, false),
];
const BOLD_CHAIN: &[ChainLink] = &[
    link(Face::NotoSansBold, false),
    link(Face::NotoSansHebrewBold, false),
    link(Face::CjkPack, true),
];
// Hebrew has no italic and CJK has none either.
const ITALIC_CHAIN: &[ChainLink] = &[
    link(Face::NotoSansItalic, false),
    link(Face::NotoSansHebrewRegular, false),
    link(Face::CjkPack, false),
];

/// The faces a style's `text-font` name draws with, in order. A character resolves to the first that
/// has it. `None` for a name this renderer has no fonts for.
pub fn font_chain(font: &str) -> Option<&'static [ChainLink]> {
    match font {
        "Noto Sans Regular" => Some(REGULAR_CHAIN),
        "Noto Sans Bold" => Some(BOLD_CHAIN),
        "Noto Sans Italic" => Some(ITALIC_CHAIN),
        _ => None,
    }
}

/// A glyph's measurements in glyph units (24 px em), whatever resolution its bitmap is made at.
///
/// `top` and `left` place the bitmap against the origin [`shape`] gives each positioned glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphMetrics {
    pub width: f32,
    pub height: f32,
    pub left: f32,
    pub top: f32,
    pub advance: f32,
}

/// What shaping needs to know about a character: where it is drawn from, and how big it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontGlyph {
    pub face: Face,
    pub bold: bool,
    pub metrics: GlyphMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
    Center,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextJustify {
    /// Follows the anchor: a left anchor justifies left, a right anchor right, the rest centre.
    Auto,
    Left,
    Center,
    Right,
}

/// A label's layout properties, already evaluated at the style zoom. Lengths are in ems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeStyle {
    /// `text-max-width`. `None` never breaks for width, as for a line label; only a `\n` does.
    pub max_width: Option<f64>,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub anchor: TextAnchor,
    pub justify: TextJustify,
    pub offset: [f64; 2],
    /// `text-transform: uppercase`.
    pub uppercase: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    pub character: char,
    pub face: Face,
    pub bold: bool,
    /// The glyph's origin relative to the text's anchor, in glyph units.
    pub x: f64,
    pub y: f64,
    pub metrics: GlyphMetrics,
}

/// A laid-out label: glyph positions and the text block's bounds, in glyph units from the anchor.
/// The bounds include the `text-offset`; the glyph positions do not (the quads add it).
#[derive(Clone, Debug, PartialEq)]
pub struct Shaping {
    pub lines: Vec<Vec<PositionedGlyph>>,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeError {
    /// No face in the label's chain has this character. The label is dropped whole.
    MissingGlyph(char),
    /// The text draws nothing.
    Empty,
}

/// A character in the label: what it draws, if anything. Newlines and control characters advance
/// nothing and draw nothing; a zero-width space is one of them, kept for the line breaking it hints.
#[derive(Clone, Copy)]
struct Unit {
    character: char,
    glyph: Option<FontGlyph>,
}

impl Unit {
    fn advance(&self, spacing: f64) -> f64 {
        self.glyph
            .map_or(0.0, |glyph| f64::from(glyph.metrics.advance) + spacing)
    }
}

/// Looks characters up once each, through the caller's lookup.
struct Glyphs<'a> {
    lookup: &'a dyn Fn(char) -> Option<FontGlyph>,
    seen: HashMap<char, Unit>,
}

impl Glyphs<'_> {
    fn unit(&mut self, character: char) -> Result<Unit, ShapeError> {
        if let Some(unit) = self.seen.get(&character) {
            return Ok(*unit);
        }
        let draws_nothing = character.is_control() || bidi_class(character) == BidiClass::BN;
        let glyph = if draws_nothing {
            None
        } else {
            Some((self.lookup)(character).ok_or(ShapeError::MissingGlyph(character))?)
        };
        let unit = Unit { character, glyph };
        self.seen.insert(character, unit);
        Ok(unit)
    }
}

/// Lays a label out: breaks it into lines, puts each in reading order, and places every glyph.
///
/// `glyph` returns what draws a character, or `None` when no face in the label's font chain has it.
/// Such a character makes the whole label [`ShapeError::MissingGlyph`].
pub fn shape(
    text: &str,
    style: &ShapeStyle,
    glyph: &dyn Fn(char) -> Option<FontGlyph>,
) -> Result<Shaping, ShapeError> {
    let text = if style.uppercase {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    let spacing = style.letter_spacing * ONE_EM;
    let line_height = style.line_height * ONE_EM;

    let mut glyphs = Glyphs {
        lookup: glyph,
        seen: HashMap::new(),
    };
    let units = text
        .chars()
        .map(|character| glyphs.unit(character))
        .collect::<Result<Vec<_>, _>>()?;

    let breaks = match style.max_width {
        Some(width) => determine_line_breaks(&units, spacing, width * ONE_EM),
        None => vec![units.len()],
    };
    let mut lines = if text.chars().any(reads_right_to_left) {
        bidi_lines(&text, &units, &breaks, &mut glyphs)?
    } else {
        break_lines(&units, &breaks)
    };
    for line in &mut lines {
        let start = line
            .iter()
            .position(|unit| !unit.character.is_whitespace())
            .unwrap_or(line.len());
        line.drain(..start);
        while line
            .last()
            .is_some_and(|unit| unit.character.is_whitespace())
        {
            line.pop();
        }
    }

    shape_lines(&lines, style, spacing, line_height)
}

/// Positions every unit of the broken lines and measures the text block.
fn shape_lines(
    lines: &[Vec<Unit>],
    style: &ShapeStyle,
    spacing: f64,
    line_height: f64,
) -> Result<Shaping, ShapeError> {
    let justify = match (style.justify, style.anchor) {
        (TextJustify::Right, _)
        | (TextJustify::Auto, TextAnchor::Right | TextAnchor::TopRight | TextAnchor::BottomRight) => {
            1.0
        }
        (TextJustify::Left, _)
        | (TextJustify::Auto, TextAnchor::Left | TextAnchor::TopLeft | TextAnchor::BottomLeft) => {
            0.0
        }
        _ => 0.5,
    };

    let mut positioned_lines = Vec::with_capacity(lines.len());
    let mut y = 0.0;
    let mut max_line_length: f64 = 0.0;
    for line in lines {
        let mut positioned = Vec::with_capacity(line.len());
        let mut x = 0.0;
        for unit in line {
            if let Some(glyph) = unit.glyph {
                positioned.push(PositionedGlyph {
                    character: unit.character,
                    face: glyph.face,
                    bold: glyph.bold,
                    x,
                    y: y + SHAPING_DEFAULT_OFFSET,
                    metrics: glyph.metrics,
                });
                x += unit.advance(spacing);
            }
        }
        if let Some(last) = positioned.last() {
            max_line_length = max_line_length.max(x - spacing);
            // Justify: shift the line so its end is at `justify` of its length.
            if justify != 0.0 {
                let indent = (last.x + f64::from(last.metrics.advance)) * justify;
                for glyph in &mut positioned {
                    glyph.x -= indent;
                }
            }
        }
        y += line_height;
        positioned_lines.push(positioned);
    }
    if positioned_lines.iter().all(Vec::is_empty) {
        return Err(ShapeError::Empty);
    }

    let (horizontal_align, vertical_align) = match style.anchor {
        TextAnchor::Center => (0.5, 0.5),
        TextAnchor::Left => (0.0, 0.5),
        TextAnchor::Right => (1.0, 0.5),
        TextAnchor::Top => (0.5, 0.0),
        TextAnchor::Bottom => (0.5, 1.0),
        TextAnchor::TopLeft => (0.0, 0.0),
        TextAnchor::TopRight => (1.0, 0.0),
        TextAnchor::BottomLeft => (0.0, 1.0),
        TextAnchor::BottomRight => (1.0, 1.0),
    };
    let shift_x = (justify - horizontal_align) * max_line_length;
    let shift_y = -vertical_align * positioned_lines.len() as f64 * line_height + 0.5 * line_height;
    for glyph in positioned_lines.iter_mut().flatten() {
        glyph.x += shift_x;
        glyph.y += shift_y;
    }

    let offset = [style.offset[0] * ONE_EM, style.offset[1] * ONE_EM];
    let top = offset[1] - vertical_align * y;
    let left = offset[0] - horizontal_align * max_line_length;
    Ok(Shaping {
        lines: positioned_lines,
        top,
        bottom: top + y,
        left,
        right: left + max_line_length,
    })
}

fn break_lines(units: &[Unit], breaks: &[usize]) -> Vec<Vec<Unit>> {
    let mut lines = Vec::with_capacity(breaks.len());
    let mut start = 0;
    for &end in breaks {
        lines.push(units[start..end].to_vec());
        start = end;
    }
    if start < units.len() {
        lines.push(units[start..].to_vec());
    }
    lines
}

/// Whether a character makes the bidirectional algorithm worth running.
fn reads_right_to_left(character: char) -> bool {
    matches!(
        bidi_class(character),
        BidiClass::R | BidiClass::AL | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI
    )
}

/// Levels are resolved over the whole label, then each line is put in visual order
/// and its brackets mirrored. A line never crosses a paragraph, whatever the breaks say.
fn bidi_lines(
    text: &str,
    units: &[Unit],
    breaks: &[usize],
    glyphs: &mut Glyphs,
) -> Result<Vec<Vec<Unit>>, ShapeError> {
    let info = BidiInfo::new(text, None);
    let byte_of: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();

    let mut bounds: Vec<usize> = breaks
        .iter()
        .copied()
        .chain(info.paragraphs.iter().map(|paragraph| {
            byte_of
                .binary_search(&paragraph.range.start)
                .expect("a paragraph starts on a character")
        }))
        .filter(|&at| at > 0 && at < units.len())
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    bounds.push(units.len());

    let mut lines = Vec::with_capacity(bounds.len());
    let mut start = 0;
    for end in bounds {
        let range = byte_of[start]..byte_of[end];
        let paragraph = info
            .paragraphs
            .iter()
            .find(|paragraph| {
                paragraph.range.start <= range.start && range.start < paragraph.range.end
            })
            .expect("every character is in a paragraph");
        let levels = info.reordered_levels_per_char(paragraph, range);
        let levels = &levels[start..end];

        let mut line = Vec::with_capacity(end - start);
        for visual in BidiInfo::reorder_visual(levels) {
            let unit = units[start + visual];
            let mirrored = if levels[visual].is_rtl() {
                get_mirrored(unit.character)
            } else {
                None
            };
            line.push(match mirrored {
                Some(character) => glyphs.unit(character)?,
                None => unit,
            });
        }
        lines.push(line);
        start = end;
    }
    Ok(lines)
}

/// Characters a line may end on.
fn is_breakable(character: char) -> bool {
    matches!(
        character,
        '\n' | '\r'
            | ' '
            | '&'
            | ')'
            | '+'
            | '-'
            | '/'
            | '\u{ad}'
            | '\u{b7}'
            | '\u{200b}'
            | '\u{2010}'
            | '\u{2013}'
            | '\u{2027}'
    )
}

/// Whether a line may break after this character: CJK ideographs, kana and their punctuation, which
/// are written without spaces. Hangul is spaced, so it is not here.
fn allows_ideographic_breaking(character: char) -> bool {
    matches!(u32::from(character),
        0x2E80..=0x2FDF // radicals
        | 0x2FF0..=0x303F // description characters, CJK symbols and punctuation
        | 0x3040..=0x312F // hiragana, katakana, bopomofo
        | 0x31A0..=0x4DBF // bopomofo extended, strokes, katakana extensions, enclosed, compatibility, extension A
        | 0x4E00..=0x9FFF // unified ideographs
        | 0xA000..=0xA4CF // yi
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xFE10..=0xFE1F // vertical forms
        | 0xFE30..=0xFE4F // compatibility forms
        | 0xFF00..=0xFFEF // halfwidth and fullwidth forms
        | 0x1AFF0..=0x1B16F // kana supplements and extensions
        | 0x1F200..=0x1F2FF // enclosed ideographic supplement
        | 0x20000..=0x323AF // ideograph extensions B to H, and the compatibility supplement
    )
}

struct Break {
    index: usize,
    x: f64,
    prior: Option<usize>,
    badness: f64,
}

fn calculate_badness(line_width: f64, target_width: f64, penalty: f64, is_last: bool) -> f64 {
    let raggedness = (line_width - target_width).powi(2);
    if is_last {
        // Favour a last line shorter than average over a longer one.
        if line_width < target_width {
            raggedness / 2.0
        } else {
            raggedness * 2.0
        }
    } else {
        raggedness + penalty.abs() * penalty
    }
}

/// Scores a candidate break, lower being better: a newline is one the text asked for.
fn calculate_penalty(character: char, next: char, penalizable_ideographic_break: bool) -> f64 {
    let mut penalty = 0.0;
    if character == '\n' || character == '\r' {
        penalty -= 10000.0;
    }
    if penalizable_ideographic_break {
        penalty += 150.0;
    }
    if character == '(' || character == '\u{ff08}' {
        penalty += 50.0;
    }
    if next == ')' || next == '\u{ff09}' {
        penalty += 50.0;
    }
    penalty
}

fn evaluate_break(
    breaks: &[Break],
    index: usize,
    x: f64,
    target_width: f64,
    penalty: f64,
    is_last: bool,
) -> Break {
    let mut prior = None;
    let mut best = calculate_badness(x, target_width, penalty, is_last);
    for (candidate_index, candidate) in breaks.iter().enumerate() {
        let badness =
            calculate_badness(x - candidate.x, target_width, penalty, is_last) + candidate.badness;
        if badness <= best {
            prior = Some(candidate_index);
            best = badness;
        }
    }
    Break {
        index,
        x,
        prior,
        badness: best,
    }
}

/// The unit indices to break at for lines of roughly `max_width`,
/// weighing each candidate by how ragged it leaves the lines.
fn determine_line_breaks(units: &[Unit], spacing: f64, max_width: f64) -> Vec<usize> {
    let total_width: f64 = units.iter().map(|unit| unit.advance(spacing)).sum();
    let asks_for_newline = units
        .iter()
        .any(|unit| unit.character == '\n' || unit.character == '\r');
    if spacing >= 0.0 && total_width > 0.0 && total_width <= max_width && !asks_for_newline {
        return vec![units.len()];
    }

    let target_width = total_width / (total_width / max_width).ceil().max(1.0);
    let has_zero_width_spaces = units.iter().any(|unit| unit.character == '\u{200b}');

    let mut potential: Vec<Break> = Vec::new();
    let mut current_x = 0.0;
    for (i, unit) in units.iter().enumerate() {
        if i > 0 {
            let previous = units[i - 1].character;
            let ideographic = allows_ideographic_breaking(previous);
            if is_breakable(previous)
                || ideographic
                || (i + 1 < units.len() && unit.character == '(')
            {
                let penalty = calculate_penalty(
                    previous,
                    unit.character,
                    ideographic && has_zero_width_spaces,
                );
                let found = evaluate_break(&potential, i, current_x, target_width, penalty, false);
                potential.push(found);
            }
        }
        if !unit.character.is_whitespace() {
            current_x += unit.advance(spacing);
        }
    }

    let last = evaluate_break(&potential, units.len(), current_x, target_width, 0.0, true);
    let mut indices = vec![last.index];
    let mut prior = last.prior;
    while let Some(at) = prior {
        indices.push(potential[at].index);
        prior = potential[at].prior;
    }
    indices.reverse();
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every printable character is 10 units wide, in the same face.
    fn ten_wide(character: char) -> Option<FontGlyph> {
        (character != '\u{1f600}').then_some(FontGlyph {
            face: Face::NotoSansRegular,
            bold: false,
            metrics: GlyphMetrics {
                width: 8.0,
                height: 12.0,
                left: 1.0,
                top: 0.0,
                advance: 10.0,
            },
        })
    }

    fn style() -> ShapeStyle {
        ShapeStyle {
            max_width: Some(100.0 / ONE_EM),
            line_height: 1.2,
            letter_spacing: 0.0,
            anchor: TextAnchor::Center,
            justify: TextJustify::Center,
            offset: [0.0, 0.0],
            uppercase: false,
        }
    }

    fn lines(text: &str, style: &ShapeStyle) -> Vec<String> {
        shape(text, style, &ten_wide)
            .unwrap()
            .lines
            .iter()
            .map(|line| line.iter().map(|glyph| glyph.character).collect())
            .collect()
    }

    #[test]
    fn a_label_that_fits_stays_on_one_line() {
        assert_eq!(lines("Praha 1", &style()), ["Praha 1"]);
    }

    #[test]
    fn long_labels_break_into_balanced_lines_at_spaces() {
        // 40 characters is 400 units: four lines of about 100, broken at spaces.
        let text = "aaaaaaaa bbbbbbbb cccccccc dddddddd eeee";
        let broken = lines(text, &style());
        assert_eq!(broken.len(), 4);
        assert_eq!(broken.join(" "), text);
    }

    #[test]
    fn explicit_newlines_break_even_a_label_that_fits() {
        assert_eq!(lines("Praha\nPrague", &style()), ["Praha", "Prague"]);
    }

    #[test]
    fn ideographs_break_between_any_two_characters() {
        let text = "北京市海淀区中关村大街东路北段";
        let broken = lines(text, &style());
        assert!(broken.len() > 1);
        assert_eq!(broken.concat(), text);
    }

    #[test]
    fn line_labels_do_not_break_for_width() {
        let style = ShapeStyle {
            max_width: None,
            ..style()
        };
        assert_eq!(
            lines("aaaaaaaa bbbbbbbb cccccccc dddddddd", &style).len(),
            1
        );
        assert_eq!(lines("aaa
bbb", &style).len(), 1);
    }

    #[test]
    fn hebrew_is_laid_out_right_to_left_and_brackets_mirror() {
        let shaping = shape("אב (גד)", &style(), &ten_wide).unwrap();
        let drawn: String = shaping.lines[0]
            .iter()
            .map(|glyph| glyph.character)
            .collect();
        // Visual order, left to right: the bracketed word first, its brackets mirrored, then the first word.
        assert_eq!(drawn, "(דג) בא");
        let xs: Vec<f64> = shaping.lines[0].iter().map(|glyph| glyph.x).collect();
        assert!(xs.windows(2).all(|pair| pair[1] > pair[0]));
    }

    #[test]
    fn uppercase_transforms_before_layout() {
        assert_eq!(
            lines(
                "praha",
                &ShapeStyle {
                    uppercase: true,
                    ..style()
                }
            ),
            ["PRAHA"]
        );
    }

    #[test]
    fn anchor_and_justification_place_the_text_block() {
        let centred = shape("abcd", &style(), &ten_wide).unwrap();
        assert_eq!((centred.left, centred.right), (-20.0, 20.0));
        let first = centred.lines[0][0];
        // The glyph origin is its left edge: four 10-wide glyphs centred on the anchor.
        assert_eq!(first.x, -20.0);

        let top_left = shape(
            "abcd",
            &ShapeStyle {
                anchor: TextAnchor::TopLeft,
                offset: [1.0, 0.5],
                ..style()
            },
            &ten_wide,
        )
        .unwrap();
        assert_eq!(top_left.lines[0][0].x, 0.0);
        assert_eq!(top_left.left, ONE_EM);
        assert_eq!(top_left.top, 0.5 * ONE_EM);
    }

    #[test]
    fn letter_spacing_widens_each_gap() {
        let spaced = shape(
            "abc",
            &ShapeStyle {
                letter_spacing: 0.5,
                justify: TextJustify::Left,
                anchor: TextAnchor::Left,
                ..style()
            },
            &ten_wide,
        )
        .unwrap();
        let xs: Vec<f64> = spaced.lines[0].iter().map(|glyph| glyph.x).collect();
        assert_eq!(xs, [0.0, 22.0, 44.0]);
    }

    #[test]
    fn a_character_no_face_has_drops_the_whole_label() {
        assert_eq!(
            shape("Praha \u{1f600}", &style(), &ten_wide),
            Err(ShapeError::MissingGlyph('\u{1f600}'))
        );
    }

    #[test]
    fn text_that_draws_nothing_is_empty() {
        assert_eq!(shape(" \n ", &style(), &ten_wide), Err(ShapeError::Empty));
    }

    #[test]
    fn every_bundled_font_name_has_a_chain_ending_in_the_cjk_pack() {
        for font in ["Noto Sans Regular", "Noto Sans Bold", "Noto Sans Italic"] {
            let chain = font_chain(font).unwrap();
            assert_eq!(chain.last().unwrap().face, Face::CjkPack);
        }
        assert!(font_chain("Open Sans Regular").is_none());
    }
}
