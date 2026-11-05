//! Tile layout: a tile's point features become symbols, each with its shaped glyphs, its collision
//! box and its paint, all in output pixels. It runs next to tessellation on the tile workers, from the
//! same decoded features.
//!
//! A symbol carries glyph keys, not bitmaps: the atlas gets the bitmaps from the glyph service.

use super::layer::{LabelLayer, SymbolPlacement, Varying};
use super::text::{shape, Face, FontGlyph, ShapeError, ShapeStyle, BUFFER, ONE_EM};
use crate::gpu::tile::{for_each_passing, point_coordinates, DecodedFeature, GeometryKind};
use maplibre_expr::{evaluate, EvaluationContext, Expr, Value};
use std::cell::RefCell;

/// Tile units along a tile's edge, whatever extent the tile was cut with.
pub(crate) const TILE_UNITS: f64 = 4096.0;

/// A glyph in the atlas: the face that draws it and its character. The face fixes the resolution the
/// bitmap is made at.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GlyphId {
    pub face: Face,
    pub character: char,
}

/// Where layout learns how big a font's glyphs are. Tile workers hold no font files in the preview, so
/// this is not the glyph service itself.
pub trait GlyphLookup: Sync {
    /// The glyph `font` draws `character` with, or `None` when no face in its chain has it.
    fn glyph(&self, font: &str, character: char) -> Result<Option<FontGlyph>, String>;
}

/// One glyph of a symbol: which bitmap, and where it sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphQuad {
    pub(crate) glyph: GlyphId,
    /// A Regular standing in for a Bold: the shader thickens it.
    pub(crate) bold: bool,
    /// Left, top, right and bottom against the symbol's anchor, in glyph units (24 per em). It covers
    /// the whole bitmap, whose border holds the distance field's fall-off.
    pub(crate) rect: [f32; 4],
}

/// One label on one tile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LabelSymbol {
    /// The style layer, which orders placement and drawing.
    pub(crate) layer: u32,
    pub(crate) sort_key: f64,
    /// In tile units, `0..TILE_UNITS`.
    pub(crate) anchor: [f32; 2],
    pub(crate) quads: Vec<GlyphQuad>,
    /// The collision box against the anchor, in output pixels: left, top, right, bottom, with the
    /// layer's `text-padding`.
    pub(crate) bounds: [f32; 4],
    /// Output pixels per glyph unit.
    pub(crate) font_scale: f32,
    pub(crate) fill: [u8; 4],
    pub(crate) halo: [u8; 4],
    /// Output pixels.
    pub(crate) halo_width: f32,
    pub(crate) halo_blur: f32,
    pub(crate) opacity: f32,
}

/// Lays out the point features of `features` that `layer` draws and appends their symbols.
/// `extent` is the tile's coordinate range, which `scale` turns into [`TILE_UNITS`].
///
/// A label with a character no face covers is dropped whole. A glyph service that fails fails the tile.
pub(crate) fn lay_out(
    layer: &LabelLayer,
    features: &mut [DecodedFeature<'_>],
    (extent, scale): (f32, f32),
    context: &mut EvaluationContext,
    glyphs: &dyn GlyphLookup,
    symbols: &mut Vec<LabelSymbol>,
) -> Result<(), String> {
    if layer.placement != SymbolPlacement::Point {
        return Ok(());
    }
    let style = ShapeStyle {
        max_width: Some(layer.max_width_ems),
        line_height: layer.line_height_ems,
        letter_spacing: layer.letter_spacing_ems,
        anchor: layer.anchor,
        justify: layer.justify,
        offset: layer.offset_ems,
        uppercase: layer.uppercase,
    };
    let font_scale = layer.size_px / ONE_EM;
    let failure = RefCell::new(None);
    let lookup = |character| match glyphs.glyph(&layer.font, character) {
        Ok(glyph) => glyph,
        Err(error) => {
            failure.borrow_mut().get_or_insert(error);
            None
        }
    };

    for_each_passing(
        features,
        layer.filter.as_ref(),
        context,
        |feature, context| {
            if feature.kind != GeometryKind::Point || failure.borrow().is_some() {
                return;
            }
            let anchors: Vec<[f32; 2]> = point_coordinates(feature.raw)
                .filter(|[x, y]| (0..extent as i64).contains(x) && (0..extent as i64).contains(y))
                .map(|[x, y]| [x as f32 * scale, y as f32 * scale])
                .collect();
            let Some(text) = label_text(&layer.text_field, context).filter(|text| !text.is_empty())
            else {
                return;
            };
            let Some(color) = layer.paint.color.color(context) else {
                return;
            };
            if anchors.is_empty() {
                return;
            }
            let shaping = match shape(&text, &style, &lookup) {
                Ok(shaping) => shaping,
                Err(ShapeError::MissingGlyph(character)) => {
                    log::debug!(
                        "Dropping label {text:?} of {}: no face has {character:?}",
                        layer.id
                    );
                    return;
                }
                Err(ShapeError::Empty) => return,
            };
            if failure.borrow().is_some() {
                return;
            }

            let quads: Vec<GlyphQuad> = shaping
                .lines
                .iter()
                .flatten()
                .filter(|glyph| glyph.metrics.width > 0.0 && glyph.metrics.height > 0.0)
                .map(|glyph| {
                    let border = BUFFER as f64;
                    let left = glyph.x + f64::from(glyph.metrics.left) - border
                        + layer.offset_ems[0] * ONE_EM;
                    let top = glyph.y - f64::from(glyph.metrics.top) - border
                        + layer.offset_ems[1] * ONE_EM;
                    GlyphQuad {
                        glyph: GlyphId {
                            face: glyph.face,
                            character: glyph.character,
                        },
                        bold: glyph.bold,
                        rect: [
                            left as f32,
                            top as f32,
                            (left + f64::from(glyph.metrics.width) + 2.0 * border) as f32,
                            (top + f64::from(glyph.metrics.height) + 2.0 * border) as f32,
                        ],
                    }
                })
                .collect();
            if quads.is_empty() {
                return;
            }
            let bounds = [
                shaping.left * font_scale - layer.padding_px,
                shaping.top * font_scale - layer.padding_px,
                shaping.right * font_scale + layer.padding_px,
                shaping.bottom * font_scale + layer.padding_px,
            ]
            .map(|edge| edge as f32);
            let halo = layer
                .paint
                .halo_color
                .as_ref()
                .and_then(|halo| halo.color(context))
                .unwrap_or([0.0; 4]);
            let number = |value: &Option<Varying<f64>>, default| {
                value
                    .as_ref()
                    .map_or(default, |value| value.number(context, default))
            };
            let symbol = LabelSymbol {
                layer: layer.index,
                sort_key: number(&layer.sort_key, 0.0),
                anchor: [0.0; 2],
                quads,
                bounds,
                font_scale: font_scale as f32,
                fill: color.map(unorm8),
                halo: halo.map(unorm8),
                halo_width: (number(&layer.paint.halo_width, 0.0) * layer.pixel_ratio) as f32,
                halo_blur: (number(&layer.paint.halo_blur, 0.0) * layer.pixel_ratio) as f32,
                opacity: number(&layer.paint.opacity, 1.0) as f32,
            };
            symbols.extend(anchors.into_iter().map(|anchor| LabelSymbol {
                anchor,
                ..symbol.clone()
            }));
        },
    );
    failure.into_inner().map_or(Ok(()), Err)
}

fn unorm8(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The label's text, or `None` for a feature without a name.
fn label_text(text_field: &Expr, context: &EvaluationContext) -> Option<String> {
    match evaluate(text_field, context) {
        Ok(Value::String(text)) => Some(text),
        Ok(Value::Formatted(sections)) => Some(
            sections
                .iter()
                .map(|section| section.text.as_str())
                .collect(),
        ),
        _ => None,
    }
}
