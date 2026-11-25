//! Text layout and drawing: font resolution, measurement, fixed-width digit cells, shadows and
//! the widget text style.
use crate::model::catalog::widget_font_weight;
use crate::model::scene::{Numerals, ValidatedScenePresentation};
use crate::widgets::value::ValidatedValueWidget;
use crate::WidgetResult;
use skia_safe::paint::{Join, Style};
use skia_safe::{
    BlurStyle, Canvas, Color, Font, GlyphId, MaskFilter, Paint, Path, PathBuilder, Point, Rect,
    TextBlob, TextBlobBuilder, Typeface,
};
use unicode_segmentation::UnicodeSegmentation;

/// Hosts acquire font resources; widget layout and drawing only resolve prepared faces.
pub trait FontResolver {
    fn resolve_typeface(
        &self,
        name: Option<&str>,
        weight: f32,
        italic: bool,
    ) -> WidgetResult<Typeface>;
    /// Whether glyph heights and baselines snap to whole pixels. Grid fitting keeps static export
    /// text crisp; a preview re-rendered while resizing would instead grow and move in pixel steps.
    fn grid_fit(&self) -> bool;
}

/// Line box height of widget text, as a share of its font size.
pub const LINE_HEIGHT_RATIO: f32 = 0.92;

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
    /// Digit placement; static text layouts ignore it.
    pub numerals: Numerals,
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

/// Interaction bounds use character advances horizontally and glyph bounds vertically.
pub(crate) fn text_selection_bounds(measurement: &MeasuredText, x: f32, baseline: f32) -> Rect {
    let end = x + measurement.width;
    Rect::new(
        x.min(end),
        baseline + measurement.bounds_top,
        x.max(end),
        baseline + measurement.bounds_bottom,
    )
}

/// Draws a metric readout with optional drop shadow and stroke in the style's numerals.
pub fn draw_text(
    canvas: &Canvas,
    text: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<()> {
    draw_text_with_vertical_metrics_text(canvas, text, text, style, fonts)
}

/// Draws a metric readout while allowing baseline alignment to be measured from
/// a stable reference string instead of the rendered glyphs, in the style's numerals.
pub fn draw_text_with_vertical_metrics_text(
    canvas: &Canvas,
    text: &str,
    vertical_metrics_text: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<()> {
    if text.is_empty() {
        return Ok(());
    }

    let font = resolve_styled_font(
        fonts,
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
    let layout = layout_text(text, &font, style.letter_spacing, style.numerals);
    let blob = layout
        .blob
        .ok_or_else(|| crate::WidgetError::Render("Failed to lay out label text".into()))?;
    let vertical_measurement = if metrics_text == text {
        layout.measurement
    } else {
        measure_text_with_font(metrics_text, &font, style.numerals)
    };
    let glyph_height = vertical_measurement.bounds_bottom - vertical_measurement.bounds_top;
    let baseline = if glyph_height <= f32::EPSILON {
        baseline_for_top_with_line_height(style.y, &font, style.line_height)
    } else {
        style.y + (style.line_height - glyph_height) * 0.5 - vertical_measurement.bounds_top
    };

    draw_text_blob(canvas, &blob, baseline, style);
    Ok(())
}

/// Paint an already prepared text run, shared by static label frames and text helpers.
pub fn draw_text_blob(canvas: &Canvas, blob: &TextBlob, baseline: f32, style: &ResolvedTextStyle) {
    TextPaints::new(style).draw(canvas, blob, (style.x, baseline));
}

/// Prepared fill, stroke, and shadow resources reusable while text moves or changes.
#[derive(Clone, Debug)]
pub struct TextPaints {
    shadow: Option<Paint>,
    shadow_distance: f32,
    border: Option<Paint>,
    fill: Paint,
}

impl TextPaints {
    pub fn new(style: &ResolvedTextStyle) -> Self {
        let fill = text_paint(style.color);
        let shadow = style
            .shadow_color
            .and_then(|color| shadow_paint(&fill, color, style.shadow_strength));
        let border = style
            .border_color
            .filter(|_| style.border_thickness > 0.0)
            .map(|color| {
                let mut paint = text_paint(color);
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(style.border_thickness);
                paint.set_stroke_join(Join::Round);
                paint
            });
        Self {
            shadow,
            shadow_distance: style.shadow_distance,
            border,
            fill,
        }
    }

    pub fn draw(&self, canvas: &Canvas, blob: &TextBlob, position: (f32, f32)) {
        if let Some(shadow) = &self.shadow {
            canvas.draw_text_blob(
                blob,
                (
                    position.0 + self.shadow_distance,
                    position.1 + self.shadow_distance,
                ),
                shadow,
            );
        }
        for paint in [self.border.as_ref(), Some(&self.fill)]
            .into_iter()
            .flatten()
        {
            canvas.draw_text_blob(blob, Point::from(position), paint);
        }
    }

    /// Paints placed glyph outlines, which keep fractional positions the glyph cache would round.
    pub fn draw_path(&self, canvas: &Canvas, path: &Path) {
        if let Some(shadow) = &self.shadow {
            let offset = (self.shadow_distance, self.shadow_distance);
            canvas.draw_path(&path.with_offset(offset), shadow);
        }
        for paint in [self.border.as_ref(), Some(&self.fill)]
            .into_iter()
            .flatten()
        {
            canvas.draw_path(path, paint);
        }
    }
}

/// Resolves a font from configured font directories or system fonts.
pub fn resolve_font(
    fonts: &dyn FontResolver,
    name: Option<&str>,
    font_size: f32,
) -> WidgetResult<Font> {
    resolve_styled_font(fonts, name, font_size, widget_font_weight(), false)
}

/// Resolves the same genuine style and weight for drawing and measurement.
pub fn resolve_styled_font(
    fonts: &dyn FontResolver,
    name: Option<&str>,
    font_size: f32,
    font_weight: f32,
    italic: bool,
) -> WidgetResult<Font> {
    let typeface = fonts.resolve_typeface(name, font_weight, italic)?;
    let mut font = Font::from_typeface(typeface, font_size);
    font.set_edging(skia_safe::font::Edging::SubpixelAntiAlias);
    font.set_subpixel(true);
    let grid_fit = fonts.grid_fit();
    font.set_hinting(if grid_fit {
        skia_safe::FontHinting::Full
    } else {
        skia_safe::FontHinting::None
    });
    font.set_baseline_snap(grid_fit);
    Ok(font)
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

/// Measures a metric readout using a resolved style and its numerals.
pub fn measure_text(
    text: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<MeasuredText> {
    let font = resolve_styled_font(
        fonts,
        style.font_name.as_deref(),
        style.font_size,
        style.font_weight,
        style.italic,
    )?;
    Ok(if style.letter_spacing == 0.0 {
        measure_text_with_font(text, &font, style.numerals)
    } else {
        layout_text(text, &font, style.letter_spacing, style.numerals).measurement
    })
}

pub(crate) struct LaidOutText {
    pub blob: Option<TextBlob>,
    pub measurement: MeasuredText,
}

/// Glyph outlines placed as `layout_text` places untracked text, origin on the baseline, and their advance width.
pub(crate) fn layout_text_path(text: &str, font: &Font, numerals: Numerals) -> (Path, f32) {
    let (glyphs, xs, width) = match fixed_width_digits(text, font, 0.0, numerals) {
        Some(placed) => (placed.glyphs, placed.xs, placed.width),
        None => {
            let glyphs = font.str_to_glyphs_vec(text);
            let mut xs = vec![0.0; glyphs.len()];
            font.get_x_pos(&glyphs, &mut xs, None);
            (glyphs, xs, font.measure_str(text, None).0)
        }
    };
    let mut builder = PathBuilder::new();
    for (glyph, x) in glyphs.into_iter().zip(xs) {
        if let Some(outline) = font.get_path(glyph) {
            builder.add_path_with_offset(&outline, (x, 0.0), None);
        }
    }
    (builder.detach(), width)
}

// Keep the existing whole-string layout at zero. Nonzero tracking uses intact
// extended grapheme clusters, with gaps only between them. One blob is reused
// for every paint layer, keeping tracked glyph positions identical across fill, border and shadow.
pub(crate) fn layout_text(
    text: &str,
    font: &Font,
    letter_spacing: f32,
    numerals: Numerals,
) -> LaidOutText {
    if let Some(placed) = fixed_width_digits(text, font, letter_spacing, numerals) {
        let mut builder = TextBlobBuilder::new();
        let (glyphs, xs) = builder.alloc_run_pos_h(font, placed.glyphs.len(), 0.0, None);
        glyphs.copy_from_slice(&placed.glyphs);
        xs.copy_from_slice(&placed.xs);
        return LaidOutText {
            blob: builder.make(),
            measurement: measurement(font, (placed.width, placed.ink)),
        };
    }
    if letter_spacing == 0.0 || text.is_empty() {
        return LaidOutText {
            blob: TextBlob::from_str(text, font),
            measurement: measurement(font, font.measure_str(text, None)),
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

/// Measures text using an already-resolved Skia font, as `layout_text` places it.
pub fn measure_text_with_font(text: &str, font: &Font, numerals: Numerals) -> MeasuredText {
    match fixed_width_digits(text, font, 0.0, numerals) {
        Some(placed) => measurement(font, (placed.width, placed.ink)),
        None => measurement(font, font.measure_str(text, None)),
    }
}

/// Digit-cell placement when the numerals ask for it and any digit would move.
fn fixed_width_digits(
    text: &str,
    font: &Font,
    letter_spacing: f32,
    numerals: Numerals,
) -> Option<PlacedGlyphs> {
    match numerals {
        Numerals::Default => None,
        Numerals::FixedWidth => place_digits(text, font, letter_spacing),
    }
}

fn measurement(font: &Font, (width, bounds): (f32, Rect)) -> MeasuredText {
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

/// Applies an additional opacity to an RGBA color whose widget opacity is already resolved.
pub fn rgba_color(color: [u8; 4], opacity: f32) -> Color {
    Color::from_argb(
        (f32::from(color[3]) * opacity).round() as u8,
        color[0],
        color[1],
        color[2],
    )
}

// Creates the anti-aliased Skia paint used for text fills and strokes.
fn text_paint(color: Color) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint
}

impl ResolvedTextStyle {
    /// Widget text at the origin: `font_size` and the scene's shadow and border scaled by
    /// `scale`, the shared weight and line box, and the scene's numerals. Callers place it.
    pub(crate) fn widget(
        scene: &ValidatedScenePresentation,
        scale: f32,
        font_name: Option<String>,
        font_size: f32,
        color: Color,
        opacity: f32,
    ) -> Self {
        let font_size = font_size * scale;
        Self {
            x: 0.0,
            y: 0.0,
            font_name,
            font_size,
            font_weight: widget_font_weight(),
            italic: false,
            letter_spacing: 0.0,
            line_height: font_size * LINE_HEIGHT_RATIO,
            color,
            opacity,
            shadow_color: Some(parse_color(&scene.shadow_color, opacity)),
            shadow_strength: scene.shadow_strength * scale,
            shadow_distance: scene.shadow_distance * scale,
            border_color: Some(parse_color(&scene.border_color, opacity)),
            border_thickness: scene.border_thickness * scale,
            numerals: scene.numerals,
        }
    }
}

/// A metric value widget's text style.
pub fn validated_value_style(
    validated: &ValidatedValueWidget,
    scene: &ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    ResolvedTextStyle {
        x: validated.x,
        y: validated.y,
        ..ResolvedTextStyle::widget(
            scene,
            scale,
            Some(validated.font_name.clone()),
            validated.font_size,
            rgba_color(validated.color, scene.opacity),
            validated.opacity * scene.opacity,
        )
    }
}

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

/// Retains the source geometry/style and its alpha, replacing its color with the shadow color.
/// Callers draw at the shadow offset before drawing the original foreground.
pub fn shadow_paint(source: &Paint, color: Color, sigma: f32) -> Option<Paint> {
    if sigma <= 0.0 {
        return None;
    }
    let mut paint = source.clone();
    paint.set_color(color);
    // The previous image filter colored the source's alpha, including its paint opacity.
    paint.set_alpha_f(source.alpha_f() * f32::from(color.a()) / 255.0);
    paint.set_mask_filter(
        MaskFilter::blur(BlurStyle::Normal, sigma, true).expect("positive shadow sigma"),
    );
    Some(paint)
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
