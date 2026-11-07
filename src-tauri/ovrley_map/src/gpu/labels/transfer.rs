//! A tile's labels as bytes, for the preview. Its decode workers lay labels out with their own glyph
//! service, as export does, and hand them to the UI thread beside the tile's geometry, with the distance
//! field of every glyph they draw: the renderer's atlas is filled from those, as export's is from its
//! service.
//!
//! The labels are arrays of fixed-size records ([`encode_labels`]): symbols, glyph quads, lines, points and
//! glyph keys, then the keys' bitmaps back to back.

use super::layout::{GlyphId, GlyphQuad, LabelSymbol, LineSymbol, Paint, SymbolKind};
use super::text::{Face, GlyphBitmap};
use crate::gpu::transfer::Reader;
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::sync::Arc;

/// A tile's labels as the UI thread receives them.
pub(crate) struct TileLabels {
    pub(crate) symbols: Vec<LabelSymbol>,
    /// The distance field of every glyph the symbols draw, once each.
    pub(crate) glyphs: Vec<(GlyphId, GlyphBitmap)>,
}

fn face_index(face: Face) -> u32 {
    Face::ALL
        .iter()
        .position(|candidate| *candidate == face)
        .expect("every face is in Face::ALL") as u32
}

fn face_at(index: u32) -> Result<Face, String> {
    Face::ALL
        .get(index as usize)
        .copied()
        .ok_or_else(|| format!("A map glyph names face {index}, which does not exist"))
}

/// A symbol with its quads and line as indices into the tile's arrays.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SymbolRecord {
    sort_key: f64,
    half_height: f64,
    anchor: [f32; 2],
    /// A point label's box. Zero for a line label.
    bounds: [f32; 4],
    fill: [u8; 4],
    halo: [u8; 4],
    halo_width: f32,
    halo_blur: f32,
    opacity: f32,
    font_scale: f32,
    layer: u32,
    first_quad: u32,
    quad_count: u32,
    /// 1 for a line label, 0 for a point label.
    is_line: u32,
    /// Index of the line label's line. Zero for a point label.
    line: u32,
    segment: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadRecord {
    rect: [f32; 4],
    along: f32,
    /// Index of the quad's glyph in the tile's glyph keys.
    glyph: u32,
    bold: u32,
}

/// A line's points in the tile's points.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineRecord {
    first_point: u32,
    point_count: u32,
}

/// A face's index in [`Face::ALL`], a character, and the size of its bitmap.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct KeyRecord {
    face: u32,
    character: u32,
    bitmap_width: u32,
    bitmap_height: u32,
}

/// Appends a tile's labels to `out` as arrays: the counts of symbols, quads, lines, points and glyph
/// keys, then each array, then the keys' bitmaps, each from `bitmap` (the glyph service's). Symbols of
/// one line share it, and glyphs are written once however many quads draw them.
pub(crate) fn encode_labels(
    symbols: &[LabelSymbol],
    bitmap: &dyn Fn(Face, char) -> Result<Arc<GlyphBitmap>, String>,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut keys: Vec<KeyRecord> = Vec::new();
    let mut bitmaps: Vec<Arc<GlyphBitmap>> = Vec::new();
    let mut key_of: HashMap<GlyphId, u32> = HashMap::new();
    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut lines: Vec<LineRecord> = Vec::new();
    let mut line_of: HashMap<*const [f32; 2], u32> = HashMap::new();
    let mut quads: Vec<QuadRecord> = Vec::new();
    let mut records: Vec<SymbolRecord> = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let first_quad = quads.len() as u32;
        for quad in &symbol.quads {
            let glyph = match key_of.get(&quad.glyph) {
                Some(index) => *index,
                None => {
                    let bitmap = bitmap(quad.glyph.face, quad.glyph.character)?;
                    keys.push(KeyRecord {
                        face: face_index(quad.glyph.face),
                        character: u32::from(quad.glyph.character),
                        bitmap_width: bitmap.width,
                        bitmap_height: bitmap.height,
                    });
                    bitmaps.push(bitmap);
                    key_of.insert(quad.glyph, keys.len() as u32 - 1);
                    keys.len() as u32 - 1
                }
            };
            quads.push(QuadRecord {
                rect: quad.rect,
                along: quad.along,
                glyph,
                bold: u32::from(quad.bold),
            });
        }
        let mut record = SymbolRecord {
            sort_key: symbol.paint.sort_key,
            half_height: 0.0,
            anchor: symbol.anchor,
            bounds: [0.0; 4],
            fill: symbol.paint.fill,
            halo: symbol.paint.halo,
            halo_width: symbol.paint.halo_width,
            halo_blur: symbol.paint.halo_blur,
            opacity: symbol.paint.opacity,
            font_scale: symbol.font_scale,
            layer: symbol.layer,
            first_quad,
            quad_count: quads.len() as u32 - first_quad,
            is_line: 0,
            line: 0,
            segment: 0,
        };
        match &symbol.kind {
            SymbolKind::Point { bounds } => record.bounds = *bounds,
            SymbolKind::Line(line) => {
                record.is_line = 1;
                record.half_height = line.half_height;
                record.segment = line.segment;
                record.line = *line_of
                    .entry(Arc::as_ptr(&line.points).cast::<[f32; 2]>())
                    .or_insert_with(|| {
                        lines.push(LineRecord {
                            first_point: points.len() as u32,
                            point_count: line.points.len() as u32,
                        });
                        points.extend_from_slice(&line.points);
                        lines.len() as u32 - 1
                    });
            }
        }
        records.push(record);
    }

    let counts = [
        records.len() as u32,
        quads.len() as u32,
        lines.len() as u32,
        points.len() as u32,
        keys.len() as u32,
    ];
    out.extend_from_slice(bytemuck::cast_slice(&counts));
    out.extend_from_slice(bytemuck::cast_slice(&records));
    out.extend_from_slice(bytemuck::cast_slice(&quads));
    out.extend_from_slice(bytemuck::cast_slice(&lines));
    out.extend_from_slice(bytemuck::cast_slice(&points));
    out.extend_from_slice(bytemuck::cast_slice(&keys));
    for bitmap in bitmaps {
        out.extend_from_slice(&bitmap.data);
    }
    Ok(())
}

/// The labels [`encode_labels`] wrote, read from `reader`. Every index in them is checked, since a
/// reader that indexed out of range would panic the page.
pub(crate) fn decode_labels(reader: &mut Reader) -> Result<TileLabels, String> {
    let counts: Vec<u32> = reader.take(5)?;
    let [symbols, quads, lines, points, keys] = [0, 1, 2, 3, 4].map(|at| counts[at] as usize);
    let records: Vec<SymbolRecord> = reader.take(symbols)?;
    let quads: Vec<QuadRecord> = reader.take(quads)?;
    let lines: Vec<LineRecord> = reader.take(lines)?;
    let points: Vec<[f32; 2]> = reader.take(points)?;
    let keys: Vec<KeyRecord> = reader.take(keys)?;

    let glyphs = keys
        .iter()
        .map(|key| {
            let id = GlyphId {
                face: face_at(key.face)?,
                character: char::from_u32(key.character)
                    .ok_or_else(|| format!("{} is not a character", key.character))?,
            };
            let length = (key.bitmap_width as usize)
                .checked_mul(key.bitmap_height as usize)
                .ok_or("A map glyph bitmap is too large")?;
            let bitmap = GlyphBitmap {
                width: key.bitmap_width,
                height: key.bitmap_height,
                data: reader.take(length)?,
            };
            Ok((id, bitmap))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let lines = lines
        .iter()
        .map(|line| {
            let start = line.first_point as usize;
            points
                .get(start..start.saturating_add(line.point_count as usize))
                .map(Arc::<[[f32; 2]]>::from)
                .ok_or_else(|| "A map label line leaves the tile's points".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;

    let symbols = records
        .iter()
        .map(|record| {
            let first = record.first_quad as usize;
            let quad_records = quads
                .get(first..first.saturating_add(record.quad_count as usize))
                .ok_or("A map label's quads leave the tile's quads")?;
            let quads = quad_records
                .iter()
                .map(|quad| {
                    Ok(GlyphQuad {
                        glyph: glyphs
                            .get(quad.glyph as usize)
                            .ok_or("A map label quad names a glyph the tile lacks")?
                            .0,
                        bold: quad.bold != 0,
                        rect: quad.rect,
                        along: quad.along,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let kind = match record.is_line {
                0 => SymbolKind::Point {
                    bounds: record.bounds,
                },
                1 => {
                    let points = lines
                        .get(record.line as usize)
                        .ok_or("A map label names a line the tile lacks")?;
                    // The anchor's segment ends at the next point.
                    if record.segment as usize + 1 >= points.len() {
                        return Err("A map label's segment is not on its line".to_string());
                    }
                    SymbolKind::Line(LineSymbol {
                        points: Arc::clone(points),
                        segment: record.segment,
                        half_height: record.half_height,
                    })
                }
                other => return Err(format!("A map label has kind {other}")),
            };
            Ok(LabelSymbol {
                layer: record.layer,
                anchor: record.anchor,
                quads,
                kind,
                font_scale: record.font_scale,
                paint: Paint {
                    sort_key: record.sort_key,
                    fill: record.fill,
                    halo: record.halo,
                    halo_width: record.halo_width,
                    halo_blur: record.halo_blur,
                    opacity: record.opacity,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(TileLabels { symbols, glyphs })
}
