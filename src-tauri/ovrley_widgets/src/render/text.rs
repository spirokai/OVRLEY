use super::shadow::shadow_paint;
use crate::error::WidgetResult;
use crate::model::standard_widgets::widget_font_weight;
use skia_safe::{
    paint::{Join, Style},
    Canvas, Color, Font, Paint, Point, Rect, TextBlob, TextBlobBuilder, Typeface,
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
}

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

/// Draws text with optional drop shadow and stroke.
pub fn draw_text(
    canvas: &Canvas,
    text: &str,
    style: &ResolvedTextStyle,
    fonts: &dyn FontResolver,
) -> WidgetResult<()> {
    draw_text_with_vertical_metrics_text(canvas, text, text, style, fonts)
}

/// Draws text while allowing baseline alignment to be measured from a stable
/// reference string instead of the rendered glyphs.
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
    let layout = layout_text(text, &font, style.letter_spacing);
    let blob = layout
        .blob
        .ok_or_else(|| crate::error::WidgetError::Render("Failed to lay out label text".into()))?;
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
    font.set_hinting(skia_safe::FontHinting::Full);
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

/// Measures text using a resolved style.
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
        measure_text_with_font(text, &font)
    } else {
        layout_text(text, &font, style.letter_spacing).measurement
    })
}

pub(crate) struct LaidOutText {
    pub blob: Option<TextBlob>,
    pub measurement: MeasuredText,
}

// Keep the existing whole-string layout at zero. Nonzero tracking uses intact
// extended grapheme clusters, with gaps only between them. One blob is reused
// for every paint layer, keeping tracked glyph positions identical across fill, border and shadow.
pub(crate) fn layout_text(text: &str, font: &Font, letter_spacing: f32) -> LaidOutText {
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

/// Canonical metric styling, independent of export timing.
pub fn validated_value_style(
    validated: &crate::widgets::value::validation::ValidatedValueWidget,
    scene: &crate::model::scene::ValidatedScenePresentation,
    scale: f32,
) -> ResolvedTextStyle {
    let opacity = validated.opacity * scene.opacity;
    let color = rgba_color(validated.color, scene.opacity);

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
        shadow_color: (!scene.shadow_color.is_empty())
            .then(|| parse_color(&scene.shadow_color, opacity)),
        shadow_strength: scene.shadow_strength * scale,
        shadow_distance: scene.shadow_distance * scale,
        border_color: (!scene.border_color.is_empty())
            .then(|| parse_color(&scene.border_color, opacity)),
        border_thickness: scene.border_thickness * scale,
    }
}
