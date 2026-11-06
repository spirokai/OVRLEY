//! Tile layout: a tile's named features become symbols, each with its shaped glyphs, its paint, and
//! either a collision box (a point) or the line it lies along. It runs next to tessellation on the tile
//! workers, from the same decoded features.
//!
//! A symbol carries glyph keys, not bitmaps: the atlas gets the bitmaps from the glyph service.

use super::layer::{LabelLayer, SymbolPlacement, Varying};
use super::line::{
    center_anchor, clip_line, get_anchors, merge_lines, Anchor, AnchorStyle, TextLine,
};
use super::text::{shape, Face, FontGlyph, ShapeError, ShapeStyle, Shaping, BUFFER, ONE_EM};
use crate::gpu::tile::{
    for_each_passing, line_coordinates, point_coordinates, DecodedFeature, GeometryKind,
};
use maplibre_expr::{evaluate, EvaluationContext, Expr, Value};
use std::cell::RefCell;
use std::sync::Arc;

/// Tile units along a tile's edge, whatever extent the tile was cut with.
pub(crate) const TILE_UNITS: f64 = 4096.0;
/// How far past its edge a tile's lines are kept for labels, in tile units. A label that runs off its tile
/// uses this much of the neighbouring ground, which the tile's own geometry already holds.
const CLIP_BUFFER: f64 = 256.0;

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
    /// Left, top, right and bottom in glyph units (24 per em). It covers the whole bitmap, whose border
    /// holds the distance field's fall-off. A point label's glyphs sit against its anchor, a line
    /// label's against their own centre on the line.
    pub(crate) rect: [f32; 4],
    /// A line label's glyph centre along the line from its anchor, in glyph units, positive the way the
    /// text reads. Zero for a point label.
    pub(crate) along: f32,
}

/// The line a label lies along.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineSymbol {
    /// The label's line in tile units, clipped to the tile and its buffer. Labels of one line share it.
    pub(crate) points: Arc<[[f32; 2]]>,
    /// The anchor's segment: it starts at `points[segment]`.
    pub(crate) segment: u32,
    /// Half the text block's height, in glyph units.
    pub(crate) half_height: f64,
}

/// How a symbol is laid on the map.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SymbolKind {
    /// Upright on the screen, with a collision box.
    Point {
        /// The box against the anchor, in output pixels: left, top, right, bottom, with the layer's
        /// `text-padding`.
        bounds: [f32; 4],
    },
    /// Flat on the ground along a line.
    Line(LineSymbol),
}

/// One label on one tile.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LabelSymbol {
    /// The style layer, which orders placement and drawing.
    pub(crate) layer: u32,
    /// In tile units, `0..TILE_UNITS`.
    pub(crate) anchor: [f32; 2],
    pub(crate) quads: Vec<GlyphQuad>,
    pub(crate) kind: SymbolKind,
    /// Output pixels per glyph unit.
    pub(crate) font_scale: f32,
    pub(crate) paint: Paint,
}

/// A feature's paint, evaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Paint {
    pub(crate) sort_key: f64,
    pub(crate) fill: [u8; 4],
    pub(crate) halo: [u8; 4],
    /// Output pixels.
    pub(crate) halo_width: f32,
    pub(crate) halo_blur: f32,
    pub(crate) opacity: f32,
}

impl Paint {
    /// `None` when the feature's colour is not one.
    fn evaluate(layer: &LabelLayer, context: &EvaluationContext) -> Option<Self> {
        let color = layer.paint.color.color(context)?;
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
        Some(Self {
            sort_key: number(&layer.sort_key, 0.0),
            fill: color.map(unorm8),
            halo: halo.map(unorm8),
            halo_width: (number(&layer.paint.halo_width, 0.0) * layer.pixel_ratio) as f32,
            halo_blur: (number(&layer.paint.halo_blur, 0.0) * layer.pixel_ratio) as f32,
            opacity: number(&layer.paint.opacity, 1.0) as f32,
        })
    }
}

/// Lays out the features of `features` that `layer` draws and appends their symbols: its points, or
/// the names along its lines. `extent` is the tile's coordinate range, which `scale` turns into
/// [`TILE_UNITS`].
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
    let along_line = layer.placement != SymbolPlacement::Point;
    let style = ShapeStyle {
        // A line label is one line of text however long.
        max_width: (!along_line).then_some(layer.max_width_ems),
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
    let shape_text = |text: &str| -> Option<Shaping> {
        let shaping = match shape(text, &style, &lookup) {
            Ok(shaping) => shaping,
            Err(ShapeError::MissingGlyph(character)) => {
                log::debug!(
                    "Dropping label {text:?} of {}: no face has {character:?}",
                    layer.id
                );
                return None;
            }
            Err(ShapeError::Empty) => return None,
        };
        failure.borrow().is_none().then_some(shaping)
    };
    let symbol = |anchor, quads, kind, paint| LabelSymbol {
        layer: layer.index,
        anchor,
        quads,
        kind,
        font_scale: font_scale as f32,
        paint,
    };
    let mut lines: Vec<TextLine<Paint>> = Vec::new();

    for_each_passing(
        features,
        layer.filter.as_ref(),
        context,
        |feature, context| {
            if failure.borrow().is_some() {
                return;
            }
            // The feature's text and paint, which only a feature that is laid out evaluates.
            let named = |context: &EvaluationContext| -> Option<(String, Paint)> {
                let text =
                    label_text(&layer.text_field, context).filter(|text| !text.is_empty())?;
                Some((text, Paint::evaluate(layer, context)?))
            };
            match feature.kind {
                GeometryKind::Point if !along_line => {
                    let anchors: Vec<[f32; 2]> = point_coordinates(feature.raw)
                        .filter(|[x, y]| {
                            (0..extent as i64).contains(x) && (0..extent as i64).contains(y)
                        })
                        .map(|[x, y]| [x as f32 * scale, y as f32 * scale])
                        .collect();
                    if anchors.is_empty() {
                        return;
                    }
                    let Some((text, paint)) = named(context) else {
                        return;
                    };
                    let Some(shaping) = shape_text(&text) else {
                        return;
                    };
                    let quads = glyph_quads(layer, &shaping, false);
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
                    symbols.extend(anchors.into_iter().map(|anchor| {
                        symbol(anchor, quads.clone(), SymbolKind::Point { bounds }, paint)
                    }));
                }
                GeometryKind::Line if along_line => {
                    let Some((text, paint)) = named(context) else {
                        return;
                    };
                    lines.extend(line_coordinates(feature.raw).into_iter().map(|points| {
                        TextLine {
                            text: text.clone(),
                            tag: paint,
                            points,
                        }
                    }));
                }
                _ => {}
            }
        },
    );

    let lines = if layer.placement == SymbolPlacement::Line {
        merge_lines(lines)
    } else {
        lines
    };
    let scale = f64::from(scale);
    for line in lines {
        let Some(shaping) = shape_text(&line.text) else {
            continue;
        };
        let quads = glyph_quads(layer, &shaping, true);
        if quads.is_empty() {
            continue;
        }
        let units_per_pixel = layer.line.units_per_px;
        let anchor_style = AnchorStyle {
            spacing: layer.line.spacing_px * units_per_pixel,
            max_angle: layer.line.max_angle_deg.to_radians(),
            label_length: (shaping.right - shaping.left) * layer.line.units_per_glyph,
            em: layer.size_px * units_per_pixel,
        };
        let tile_line: Vec<[f64; 2]> = line
            .points
            .iter()
            .map(|[x, y]| [*x as f64 * scale, *y as f64 * scale])
            .collect();
        let anchored: Vec<(Vec<[f64; 2]>, Vec<Anchor>)> =
            if layer.placement == SymbolPlacement::Line {
                clip_line(
                    &tile_line,
                    [
                        -CLIP_BUFFER,
                        -CLIP_BUFFER,
                        TILE_UNITS + CLIP_BUFFER,
                        TILE_UNITS + CLIP_BUFFER,
                    ],
                )
                .into_iter()
                .map(|part| {
                    let anchors = get_anchors(&part, &anchor_style, TILE_UNITS);
                    (part, anchors)
                })
                .collect()
            } else {
                let anchors = center_anchor(&tile_line, &anchor_style, TILE_UNITS)
                    .into_iter()
                    .collect();
                vec![(tile_line, anchors)]
            };
        for (part, anchors) in anchored
            .into_iter()
            .filter(|(_, anchors)| !anchors.is_empty())
        {
            let points: Arc<[[f32; 2]]> =
                part.iter().map(|[x, y]| [*x as f32, *y as f32]).collect();
            symbols.extend(anchors.into_iter().map(|anchor| {
                symbol(
                    [anchor.point[0] as f32, anchor.point[1] as f32],
                    quads.clone(),
                    SymbolKind::Line(LineSymbol {
                        points: Arc::clone(&points),
                        segment: anchor.segment as u32,
                        half_height: (shaping.bottom - shaping.top) / 2.0,
                    }),
                    line.tag,
                )
            }));
        }
    }
    failure.into_inner().map_or(Ok(()), Err)
}

/// The quads of a shaped label's glyphs that draw anything.
fn glyph_quads(layer: &LabelLayer, shaping: &Shaping, along_line: bool) -> Vec<GlyphQuad> {
    let border = BUFFER as f64;
    let offset = [layer.offset_ems[0] * ONE_EM, layer.offset_ems[1] * ONE_EM];
    shaping
        .lines
        .iter()
        .flatten()
        .filter(|glyph| glyph.metrics.width > 0.0 && glyph.metrics.height > 0.0)
        .map(|glyph| {
            let half_advance = f64::from(glyph.metrics.advance) / 2.0;
            let (left, along) = if along_line {
                (
                    f64::from(glyph.metrics.left) - border - half_advance,
                    glyph.x + half_advance + offset[0],
                )
            } else {
                (
                    glyph.x + f64::from(glyph.metrics.left) - border + offset[0],
                    0.0,
                )
            };
            let top = glyph.y - f64::from(glyph.metrics.top) - border + offset[1];
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
                along: along as f32,
            }
        })
        .collect()
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
