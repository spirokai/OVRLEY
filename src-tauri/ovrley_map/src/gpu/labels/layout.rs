//! Tile layout: a tile's named features become symbols, each with its shaped glyphs, its paint, and
//! either a collision box (a point) or the line it lies along. It runs next to tessellation on the tile
//! workers, from the same decoded features.
//!
//! Names along lines follow MapLibre GL JS (`merge_lines`, `clip_line`, `get_anchors`,
//! `check_max_angle`, and the glyph walk of `projection.ts`): merging, clipping and choosing anchors run
//! once per tile, and the walk for every camera. All of it is in tile units on the ground, in `f64`.
//!
//! A symbol carries glyph keys, not bitmaps: the atlas gets the bitmaps from the glyph service.

use super::glyphs::{GlyphId, GlyphLookup};
use super::text::{shape, ShapeError, ShapeStyle, Shaping, BUFFER, ONE_EM};
use crate::gpu::style::{LabelLayer, SymbolPlacement, Varying};
use crate::gpu::tile::{
    for_each_passing, line_coordinates, point_coordinates, DecodedFeature, GeometryKind, TILE_UNITS,
};
use maplibre_expr::{evaluate, EvaluationContext, Expr, Value};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// How far past its edge a tile's lines are kept for labels, in tile units. A label that runs off its tile
/// uses this much of the neighbouring ground, which the tile's own geometry already holds.
const CLIP_BUFFER: f64 = 256.0;

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
/// the names along its lines. `extent` is the tile's coordinate range, which is scaled to
/// [`TILE_UNITS`].
///
/// A label with a character no face covers is dropped whole. A glyph service that fails fails the tile.
pub(crate) fn lay_out(
    layer: &LabelLayer,
    features: &mut [DecodedFeature<'_>],
    extent: f32,
    context: &mut EvaluationContext,
    glyphs: &dyn GlyphLookup,
    symbols: &mut Vec<LabelSymbol>,
) -> Result<(), String> {
    let along_line = layer.placement != SymbolPlacement::Point;
    let scale = TILE_UNITS as f32 / extent;
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

/// A line with the text it carries.
pub(crate) struct TextLine<T> {
    pub(crate) text: String,
    /// Whatever the first of the merged features carries along.
    pub(crate) tag: T,
    pub(crate) points: Vec<[i64; 2]>,
}

/// Joins lines with the same text where one ends at the point another starts, so a street cut at each
/// junction is named as one street.
pub(crate) fn merge_lines<T>(lines: Vec<TextLine<T>>) -> Vec<TextLine<T>> {
    type Key = (String, [i64; 2]);
    let mut merged: Vec<Option<TextLine<T>>> = Vec::with_capacity(lines.len());
    let mut starts: HashMap<Key, usize> = HashMap::new();
    let mut ends: HashMap<Key, usize> = HashMap::new();

    for mut line in lines {
        // Tile decoding leaves every line at least two points.
        let (first, last) = (line.points[0], line.points[line.points.len() - 1]);
        let start_key = (line.text.clone(), first);
        let end_key = (line.text.clone(), last);
        match (ends.get(&start_key).copied(), starts.get(&end_key).copied()) {
            (Some(before), Some(after)) if before != after => {
                ends.remove(&start_key);
                starts.remove(&end_key);
                let Some(after_line) = merged[after].take() else {
                    unreachable!("a line in the start index is kept")
                };
                ends.remove(&(
                    after_line.text.clone(),
                    after_line.points[after_line.points.len() - 1],
                ));
                let joined = merged[before]
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("a line in the end index is kept"));
                joined.points.pop();
                joined.points.extend(line.points);
                joined.points.pop();
                joined.points.extend(after_line.points);
                let end = joined.points[joined.points.len() - 1];
                ends.insert((joined.text.clone(), end), before);
            }
            (Some(before), _) => {
                ends.remove(&start_key);
                let joined = merged[before]
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("a line in the end index is kept"));
                joined.points.pop();
                joined.points.extend(line.points);
                ends.insert(end_key, before);
            }
            (None, Some(after)) => {
                starts.remove(&end_key);
                let joined = merged[after]
                    .as_mut()
                    .unwrap_or_else(|| unreachable!("a line in the start index is kept"));
                line.points.pop();
                line.points.append(&mut joined.points);
                joined.points = line.points;
                starts.insert(start_key, after);
            }
            (None, None) => {
                starts.insert(start_key, merged.len());
                ends.insert(end_key, merged.len());
                merged.push(Some(line));
            }
        }
    }
    merged.into_iter().flatten().collect()
}

/// The parts of `line` inside `[left, top, right, bottom]`. A line that leaves and returns comes back as
/// several parts, and a part ends exactly on the box where the line crossed it.
pub(crate) fn clip_line(line: &[[f64; 2]], [x1, y1, x2, y2]: [f64; 4]) -> Vec<Vec<[f64; 2]>> {
    let mut parts: Vec<Vec<[f64; 2]>> = Vec::new();
    for pair in line.windows(2) {
        let (mut p0, mut p1) = (pair[0], pair[1]);
        // Each edge cuts the segment at the box side where it crosses, or drops it when it is wholly outside.
        let at_x = |x: f64, p0: [f64; 2], p1: [f64; 2]| {
            [x, p0[1] + (p1[1] - p0[1]) * ((x - p0[0]) / (p1[0] - p0[0]))]
        };
        let at_y = |y: f64, p0: [f64; 2], p1: [f64; 2]| {
            [p0[0] + (p1[0] - p0[0]) * ((y - p0[1]) / (p1[1] - p0[1])), y]
        };

        if p0[0] < x1 && p1[0] < x1 {
            continue;
        } else if p0[0] < x1 {
            p0 = at_x(x1, p0, p1);
        } else if p1[0] < x1 {
            p1 = at_x(x1, p0, p1);
        }
        if p0[1] < y1 && p1[1] < y1 {
            continue;
        } else if p0[1] < y1 {
            p0 = at_y(y1, p0, p1);
        } else if p1[1] < y1 {
            p1 = at_y(y1, p0, p1);
        }
        if p0[0] >= x2 && p1[0] >= x2 {
            continue;
        } else if p0[0] >= x2 {
            p0 = at_x(x2, p0, p1);
        } else if p1[0] >= x2 {
            p1 = at_x(x2, p0, p1);
        }
        if p0[1] >= y2 && p1[1] >= y2 {
            continue;
        } else if p0[1] >= y2 {
            p0 = at_y(y2, p0, p1);
        } else if p1[1] >= y2 {
            p1 = at_y(y2, p0, p1);
        }

        match parts.last_mut() {
            Some(part) if part.last() == Some(&p0) => part.push(p1),
            _ => parts.push(vec![p0, p1]),
        }
    }
    parts
}

/// Where a label may sit on a line: a point on its segment `segment`, which starts at `line[segment]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Anchor {
    pub(crate) point: [f64; 2],
    pub(crate) segment: usize,
}

/// What anchors depend on, in tile units.
pub(crate) struct AnchorStyle {
    /// Distance between labels, widened so a long label keeps a quarter of it free either side.
    pub(crate) spacing: f64,
    /// Radians. The most a line may turn within the window under the label.
    pub(crate) max_angle: f64,
    pub(crate) label_length: f64,
    /// One em of the label's text.
    pub(crate) em: f64,
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The direction from `b` to `a`.
fn angle_to(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[1] - b[1]).atan2(a[0] - b[0])
}

/// The anchors along `line` that fit a label whole, spaced evenly. `extent` is the tile's size: a line
/// that begins outside or on its edge continues from outside, so its labels are spread rather than put at
/// its start.
pub(crate) fn get_anchors(line: &[[f64; 2]], style: &AnchorStyle, extent: f64) -> Vec<Anchor> {
    let [x, y] = line[0];
    let continued = x <= 0.0 || x >= extent || y <= 0.0 || y >= extent;
    let mut spacing = style.spacing;
    if spacing - style.label_length < spacing / 4.0 {
        spacing = style.label_length + spacing / 4.0;
    }
    // A bit of fixed extra offset keeps labels clear of the T junctions a line starts at.
    let offset = if continued {
        spacing / 2.0 % spacing
    } else {
        (style.label_length / 2.0 + style.em * 2.0) % spacing
    };
    let line_length: f64 = line.windows(2).map(|pair| distance(pair[0], pair[1])).sum();
    let anchors = resample(line, line_length, offset, spacing, style, extent);
    if anchors.is_empty() && !continued {
        // Short lines may miss every slot: try one label in the middle.
        return resample(line, line_length, line_length / 2.0, spacing, style, extent);
    }
    anchors
}

fn resample(
    line: &[[f64; 2]],
    line_length: f64,
    offset: f64,
    spacing: f64,
    style: &AnchorStyle,
    extent: f64,
) -> Vec<Anchor> {
    let half_label = style.label_length / 2.0;
    let window = 3.0 / 5.0 * style.em;
    let mut travelled = 0.0;
    let mut marked = offset - spacing;
    let mut anchors = Vec::new();

    for (segment, pair) in line.windows(2).enumerate() {
        let segment_length = distance(pair[0], pair[1]);
        while marked + spacing < travelled + segment_length {
            marked += spacing;
            let t = (marked - travelled) / segment_length;
            let point = [
                pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                pair[0][1] + (pair[1][1] - pair[0][1]) * t,
            ];
            // Inside the tile, and the label fits before the line's start and end from here.
            if (0.0..extent).contains(&point[0])
                && (0.0..extent).contains(&point[1])
                && marked - half_label >= 0.0
                && marked + half_label <= line_length
            {
                let anchor = Anchor { point, segment };
                if check_max_angle(line, anchor, style.label_length, window, style.max_angle) {
                    anchors.push(anchor);
                }
            }
        }
        travelled += segment_length;
    }

    anchors
}

/// The anchor in the middle of `line`, for `line-center` placement. `None` when the label does not fit
/// the turns there, or the middle is outside the tile.
pub(crate) fn center_anchor(line: &[[f64; 2]], style: &AnchorStyle, extent: f64) -> Option<Anchor> {
    let window = 3.0 / 5.0 * style.em;
    let centre = line
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum::<f64>()
        / 2.0;
    let mut travelled = 0.0;
    for (segment, pair) in line.windows(2).enumerate() {
        let segment_length = distance(pair[0], pair[1]);
        if travelled + segment_length > centre {
            let t = (centre - travelled) / segment_length;
            let point = [
                pair[0][0] + (pair[1][0] - pair[0][0]) * t,
                pair[0][1] + (pair[1][1] - pair[0][1]) * t,
            ];
            let anchor = Anchor { point, segment };
            let inside = (0.0..extent).contains(&point[0]) && (0.0..extent).contains(&point[1]);
            return (inside
                && check_max_angle(line, anchor, style.label_length, window, style.max_angle))
            .then_some(anchor);
        }
        travelled += segment_length;
    }
    None
}

/// Whether a label of `label_length` centred on `anchor` fits `line` without turning more than `max_angle`
/// radians within any `window` of its length.
fn check_max_angle(
    line: &[[f64; 2]],
    anchor: Anchor,
    label_length: f64,
    window: f64,
    max_angle: f64,
) -> bool {
    let mut previous = anchor.point;
    let mut index = anchor.segment + 1;
    let mut anchor_distance = 0.0;

    // Back to the first segment the label appears on.
    while anchor_distance > -label_length / 2.0 {
        let Some(earlier) = index.checked_sub(1) else {
            return false;
        };
        index = earlier;
        anchor_distance -= distance(line[index], previous);
        previous = line[index];
    }
    anchor_distance += distance(line[index], line[index + 1]);
    index += 1;

    // The corners within one window, and their angles added up.
    let mut recent: VecDeque<(f64, f64)> = VecDeque::new();
    let mut recent_angle = 0.0;
    while anchor_distance < label_length / 2.0 {
        let (Some(&before), Some(&current), Some(&next)) =
            (line.get(index - 1), line.get(index), line.get(index + 1))
        else {
            return false;
        };
        let turn = angle_to(before, current) - angle_to(current, next);
        let turn = (((turn + 3.0 * std::f64::consts::PI) % (2.0 * std::f64::consts::PI))
            - std::f64::consts::PI)
            .abs();
        recent.push_back((anchor_distance, turn));
        recent_angle += turn;
        while recent
            .front()
            .is_some_and(|(at, _)| anchor_distance - at > window)
        {
            if let Some((_, gone)) = recent.pop_front() {
                recent_angle -= gone;
            }
        }
        if recent_angle > max_angle {
            return false;
        }
        index += 1;
        anchor_distance += distance(current, next);
    }
    true
}

/// A glyph's place on the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    pub(crate) point: [f64; 2],
    /// The unit direction the text reads along there.
    pub(crate) direction: [f64; 2],
}

/// The pose `along` tile units from `anchor`, which sits on segment `segment` of `points`. Positive is
/// the way the label reads: along the line, or against it when `flip`. `None` when the line ends first.
pub(crate) fn pose_at(
    points: &[[f32; 2]],
    segment: usize,
    anchor: [f64; 2],
    along: f64,
    flip: bool,
) -> Option<Pose> {
    let forward = (along > 0.0) != flip;
    let reading = if along > 0.0 { 1.0 } else { -1.0 };
    let mut index = if forward { segment } else { segment + 1 };
    let (mut previous, mut current) = (anchor, anchor);
    let (mut behind, mut segment_length) = (0.0, 0.0);
    let wanted = along.abs();

    while behind + segment_length <= wanted {
        index = if forward {
            index + 1
        } else {
            index.checked_sub(1)?
        };
        let vertex = points.get(index)?;
        previous = current;
        current = [f64::from(vertex[0]), f64::from(vertex[1])];
        behind += segment_length;
        segment_length = distance(previous, current);
    }
    let t = (wanted - behind) / segment_length;
    Some(Pose {
        point: [
            previous[0] + (current[0] - previous[0]) * t,
            previous[1] + (current[1] - previous[1]) * t,
        ],
        direction: [
            (current[0] - previous[0]) / segment_length * reading,
            (current[1] - previous[1]) / segment_length * reading,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, points: &[[i64; 2]]) -> TextLine<()> {
        TextLine {
            text: text.to_string(),
            tag: (),
            points: points.to_vec(),
        }
    }

    #[test]
    fn lines_with_the_same_text_join_where_they_touch() {
        let merged = merge_lines(vec![
            line("Main", &[[0, 0], [10, 0]]),
            line("Other", &[[10, 0], [20, 0]]),
            line("Main", &[[20, 0], [30, 0]]),
            line("Main", &[[10, 0], [20, 0]]),
        ]);
        let main: Vec<_> = merged.iter().filter(|line| line.text == "Main").collect();
        assert_eq!(main.len(), 1);
        assert_eq!(main[0].points, [[0, 0], [10, 0], [20, 0], [30, 0]]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn a_line_is_cut_where_it_leaves_the_box() {
        let parts = clip_line(
            &[
                [-10.0, 5.0],
                [10.0, 5.0],
                [10.0, 30.0],
                [-5.0, 30.0],
                [-5.0, 8.0],
                [5.0, 8.0],
            ],
            [0.0, 0.0, 20.0, 20.0],
        );
        assert_eq!(
            parts,
            [
                vec![[0.0, 5.0], [10.0, 5.0], [10.0, 20.0]],
                vec![[0.0, 8.0], [5.0, 8.0]]
            ]
        );
    }

    #[test]
    fn anchors_keep_a_label_whole_on_its_line_and_off_sharp_turns() {
        let style = AnchorStyle {
            spacing: 100.0,
            max_angle: 45.0_f64.to_radians(),
            label_length: 40.0,
            em: 10.0,
        };
        let straight: Vec<_> = (0..=10)
            .map(|step| [10.0 + f64::from(step) * 40.0, 50.0])
            .collect();
        let anchors = get_anchors(&straight, &style, 4096.0);
        assert!(anchors.len() >= 3);
        assert!(anchors
            .iter()
            .all(|anchor| anchor.point[0] >= 20.0 && anchor.point[0] <= 390.0));

        // A right angle in the middle of the only place a label would fit.
        let corner = [[0.0, 0.0], [50.0, 0.0], [50.0, 50.0]];
        assert!(get_anchors(&corner, &style, 4096.0).is_empty());
        // A line shorter than its label gets none.
        assert!(get_anchors(&[[0.0, 0.0], [30.0, 0.0]], &style, 4096.0).is_empty());
    }

    #[test]
    fn a_glyph_walks_round_the_corner_and_a_flipped_label_reads_backwards() {
        let points = [[0.0_f32, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let anchor = [5.0, 0.0];
        let ahead = pose_at(&points, 0, anchor, 10.0, false).unwrap();
        assert_eq!(ahead.point, [10.0, 5.0]);
        assert_eq!(ahead.direction, [0.0, 1.0]);

        let behind = pose_at(&points, 0, anchor, -3.0, false).unwrap();
        assert_eq!(behind.point, [2.0, 0.0]);
        assert_eq!(behind.direction, [1.0, 0.0]);

        // Flipped, positive is back along the line, and the text reads against it.
        let flipped = pose_at(&points, 0, anchor, 3.0, true).unwrap();
        assert_eq!(flipped.point, [2.0, 0.0]);
        assert_eq!(flipped.direction, [-1.0, 0.0]);
        assert!(pose_at(&points, 0, anchor, 30.0, false).is_none());
    }
}
