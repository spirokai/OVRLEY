//! Label placement: which symbols a camera shows.
//!
//! Placement is a function of the camera, the resident label tiles and the placement before it, in `f64`
//! on plain IEEE arithmetic. What the last placement showed is placed first and keeps its reading
//! direction, so labels don't flicker between neighbours that collide by a hair. Export frames reach the
//! map thread a little out of order, so a frame names the cameras of several placements before its own,
//! and the renderer links up with whichever of them it still holds.

use super::layout::{pose_at, LabelSymbol, LineSymbol, Pose, SymbolKind};
use super::text::ONE_EM;
use crate::gpu::style::{Alignment, LabelLayer};
use crate::gpu::tile::TILE_UNITS;
use crate::gpu::FOG_FULL;
use crate::{ClipPoint, GroundProjection, MapCamera, MapViewport, TileId, ViewState};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Cell size of the collision grid, in output pixels.
const GRID_CELL: f64 = 25.0;
/// How far outside the viewport labels are still placed, so they settle off screen first.
pub(crate) const VIEWPORT_PADDING: f64 = 100.0;
/// Labels where the perspective ratio is below this are not placed or drawn: where the haze is full,
/// since `FOG_FULL` is the map centre's share of the depth there and the ratio is `0.5 + 0.5` times it.
pub(crate) const PERSPECTIVE_CUTOFF: f64 = 0.5 + 0.5 * FOG_FULL as f64;
/// Placements the cache holds. A frame names `LABEL_PLACEMENTS`, and neighbouring frames share them.
const CACHE_CAPACITY: usize = 8;
/// How far past vertical, as the cosine of its angle to the screen's x axis, a line label that kept its
/// reading direction from the last placement may lean before it turns to read the other way.
const FLIP_MARGIN: f64 = 0.25;

/// A camera on a viewport, reduced to what projecting label anchors needs.
pub(crate) struct LabelView {
    projection: GroundProjection,
    /// Clip-space w of the map centre: how far it is from the camera.
    centre_w: f64,
    width: f64,
    height: f64,
}

/// The size a line label lies on the ground at, for one camera.
pub(crate) struct GroundLabel {
    /// Tile units in one glyph unit on the ground.
    pub(crate) units_per_glyph: f64,
    /// Output pixels in one glyph unit where the label is on the screen.
    pub(crate) screen_scale: f64,
}

impl LabelView {
    pub(crate) fn new(camera: &MapCamera, viewport: MapViewport) -> Self {
        let view = ViewState::new(camera, viewport);
        Self {
            projection: view.ground_projection(),
            centre_w: view.centre_clip_w(),
            width: f64::from(viewport.width_px()),
            height: f64::from(viewport.height_px()),
        }
    }

    /// Where a symbol's anchor lands on screen, in output pixels from the top left, with the layer's
    /// translation. Its clip-space w goes with it. `None` behind the camera.
    pub(crate) fn project(
        &self,
        tile: TileId,
        symbol: &LabelSymbol,
        layer: &LabelLayer,
    ) -> Option<([f64; 2], f64)> {
        let mut point = symbol.anchor.map(f64::from);
        // A translation along the map moves the anchor on the ground, so it follows the camera.
        if layer.translate_anchor == Alignment::Map {
            point[0] += layer.translate_units[0];
            point[1] += layer.translate_units[1];
        }
        let clip = self.clip(tile, point)?;
        let (share_x, share_y) = clip.window_share();
        let mut screen = [share_x * self.width, share_y * self.height];
        if layer.translate_anchor == Alignment::Viewport {
            screen[0] += layer.translate_px[0];
            screen[1] += layer.translate_px[1];
        }
        Some((screen, clip.w))
    }

    pub(crate) fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    /// Where a point of a tile, in tile units, lands in clip space. `None` behind the camera.
    pub(crate) fn clip(&self, tile: TileId, point: [f64; 2]) -> Option<ClipPoint> {
        let tiles = 2_f64.powi(i32::from(tile.z));
        let clip = self.projection.clip(
            (f64::from(tile.x) + point[0] / TILE_UNITS) / tiles,
            (f64::from(tile.y) + point[1] / TILE_UNITS) / tiles,
        );
        (clip.z >= 0.0).then_some(clip)
    }

    /// How much bigger than its flat size a label at clip-space `w` is drawn: halfway between a constant
    /// screen size and true perspective, so labels far off shrink less than the ground does.
    pub(crate) fn perspective_ratio(&self, w: f64) -> f64 {
        (0.5 + 0.5 * self.centre_w / w).clamp(0.0, 4.0)
    }

    /// Where a point of a tile lands on screen, in output pixels from the top left.
    fn screen(&self, tile: TileId, point: [f64; 2]) -> Option<[f64; 2]> {
        let (share_x, share_y) = self.clip(tile, point)?.window_share();
        Some([share_x * self.width, share_y * self.height])
    }

    /// How big a line label is on the ground where the camera sees its anchor at clip-space `w`: its
    /// size over the perspective ratio, so labels far off stay a little larger than true perspective
    /// makes them.
    pub(crate) fn ground_label(
        &self,
        w: f64,
        symbol: &LabelSymbol,
        layer: &LabelLayer,
    ) -> GroundLabel {
        let ratio = self.perspective_ratio(w);
        GroundLabel {
            units_per_glyph: layer.line.units_per_glyph / ratio,
            screen_scale: f64::from(symbol.font_scale) * self.centre_w / w / ratio,
        }
    }
}

/// Where a line label's glyph sits, `along` tile units from its anchor.
pub(crate) fn glyph_pose(
    (symbol, line, layer): (&LabelSymbol, &LineSymbol, &LabelLayer),
    along: f64,
    flip: bool,
) -> Option<Pose> {
    let anchor = symbol.anchor.map(f64::from);
    let mut pose = pose_at(&line.points, line.segment as usize, anchor, along, flip)?;
    pose.point[0] += layer.translate_units[0];
    pose.point[1] += layer.translate_units[1];
    Some(pose)
}

/// A symbol a camera shows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PlacedSymbol {
    /// The style layer, which is also the draw order.
    pub(crate) layer: u32,
    pub(crate) tile: TileId,
    /// Position in the tile's symbols.
    pub(crate) symbol: u32,
    /// A line label that reads against its line, to stay upright.
    pub(crate) flipped: bool,
}

/// A resident label tile a placement draws from: a tile the view sees, or while its tiles load the
/// nearest resident tile above one.
pub(crate) struct LabelSource<'a> {
    pub(crate) tile: TileId,
    pub(crate) symbols: &'a [LabelSymbol],
    /// The view's tiles under this one that draw from a source of their own, which already holds the
    /// labels of their ground. Anchors of this tile inside them are left out.
    covered: HashSet<TileId>,
    /// The tile level the view's tiles are at.
    level: u8,
}

impl<'a> LabelSource<'a> {
    /// A source whose anchors are all candidates.
    pub(crate) fn whole(tile: TileId, symbols: &'a [LabelSymbol]) -> Self {
        Self {
            tile,
            symbols,
            covered: HashSet::new(),
            level: tile.z,
        }
    }

    /// Whether a drawn tile under this one holds the label's ground.
    fn hides(&self, symbol: &LabelSymbol) -> bool {
        if self.covered.is_empty() {
            return false;
        }
        let tiles_across = f64::from(1_u32 << (self.level - self.tile.z));
        let [x, y] = symbol
            .anchor
            .map(|axis| (f64::from(axis) / TILE_UNITS * tiles_across).floor() as i32);
        let across = tiles_across as i32;
        self.covered.contains(&TileId::new(
            self.level,
            self.tile.x * across + x,
            self.tile.y * across + y,
        ))
    }
}

/// The tile a view tile's labels come from: the tile itself when its labels are resident, and otherwise
/// the nearest resident tile above it, as the ground does while tiles load.
pub(crate) fn label_source_tile(
    tile: TileId,
    resident: &HashMap<TileId, Vec<LabelSymbol>>,
) -> Option<TileId> {
    let mut source = Some(tile);
    while let Some(candidate) = source.filter(|candidate| !resident.contains_key(candidate)) {
        source = candidate.parent();
    }
    source
}

/// The sources a view's labels come from, each [`label_source_tile`] of its `view` tiles once. A source
/// above the view leaves out its anchors inside view tiles that have a source of their own.
pub(crate) fn label_sources<'a>(
    view: &[TileId],
    resident: &'a HashMap<TileId, Vec<LabelSymbol>>,
) -> Vec<LabelSource<'a>> {
    let drawn_from: Vec<(TileId, Option<TileId>)> = view
        .iter()
        .map(|tile| (*tile, label_source_tile(*tile, resident)))
        .collect();
    let Some(level) = view.first().map(|tile| tile.z) else {
        return Vec::new();
    };
    let mut sources: Vec<LabelSource<'a>> = Vec::new();
    for (_, source) in &drawn_from {
        if let Some(tile) = source.filter(|tile| sources.iter().all(|seen| seen.tile != *tile)) {
            sources.push(LabelSource {
                level,
                ..LabelSource::whole(tile, &resident[&tile])
            });
        }
    }
    for source in sources
        .iter_mut()
        .filter(|source| source.tile.z < source.level)
    {
        let tile = source.tile;
        source.covered = drawn_from
            .iter()
            .filter(|(drawn, from)| {
                *from != Some(tile)
                    && drawn.z >= tile.z
                    && drawn.x >> (drawn.z - tile.z) == tile.x
                    && drawn.y >> (drawn.z - tile.z) == tile.y
            })
            .map(|(drawn, _)| *drawn)
            .collect();
    }
    sources
}

/// Chooses the symbols of `tiles` the view shows. The symbols of the `previous` placement go first, in
/// its order, and keep their reading direction while they can. The rest follow: layers from the top of
/// the style down, then tile by tile, then by `symbol-sort-key` and layout order. A symbol whose anchor
/// is behind the camera, outside the viewport and its padding, or past the haze is left out. So is one
/// that collides with a placed one, unless its layer allows overlap. The result is in placement order.
pub(crate) fn place(
    view: &LabelView,
    layers: &[LabelLayer],
    tiles: &[LabelSource],
    previous: Option<&[PlacedSymbol]>,
) -> Vec<PlacedSymbol> {
    let shown: HashMap<(u32, TileId, u32), (usize, bool)> = previous
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(rank, placed)| {
            (
                (placed.layer, placed.tile, placed.symbol),
                (rank, placed.flipped),
            )
        })
        .collect();
    let mut candidates: Vec<(usize, Reverse<u32>, TileId, f64, u32)> = tiles
        .iter()
        .flat_map(|source| {
            let shown = &shown;
            let tile = source.tile;
            source
                .symbols
                .iter()
                .enumerate()
                .filter(|(_, symbol)| !source.hides(symbol))
                .map(move |(position, symbol)| {
                    let rank = shown
                        .get(&(symbol.layer, tile, position as u32))
                        .map_or(usize::MAX, |(rank, _)| *rank);
                    (
                        rank,
                        Reverse(symbol.layer),
                        tile,
                        symbol.paint.sort_key,
                        position as u32,
                    )
                })
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
            .then(a.3.total_cmp(&b.3))
            .then(a.4.cmp(&b.4))
    });

    let (width, height) = view.size();
    let mut grid = GridIndex::new(
        width + 2.0 * VIEWPORT_PADDING,
        height + 2.0 * VIEWPORT_PADDING,
    );
    let symbols: HashMap<TileId, &[LabelSymbol]> = tiles
        .iter()
        .map(|source| (source.tile, source.symbols))
        .collect();
    let mut placed = Vec::new();
    for (_, Reverse(layer_index), tile, _, position) in candidates {
        let Some(layer) = layers.iter().find(|layer| layer.index == layer_index) else {
            continue;
        };
        let symbol = &symbols[&tile][position as usize];
        let Some((anchor, w)) = view.project(tile, symbol, layer) else {
            continue;
        };
        let (x, y) = (anchor[0], anchor[1]);
        let ratio = view.perspective_ratio(w);
        if ratio < PERSPECTIVE_CUTOFF
            || !(-VIEWPORT_PADDING..=width + VIEWPORT_PADDING).contains(&x)
            || !(-VIEWPORT_PADDING..=height + VIEWPORT_PADDING).contains(&y)
        {
            continue;
        }
        let candidate = match &symbol.kind {
            SymbolKind::Point { bounds } => {
                Some((vec![place_point(ratio, anchor, *bounds)], false))
            }
            SymbolKind::Line(line) => {
                let flipped = shown
                    .get(&(layer_index, tile, position))
                    .map(|(_, flipped)| *flipped);
                place_line(view, (tile, symbol, line, layer), w, flipped)
            }
        };
        let Some((shapes, flipped)) = candidate else {
            continue;
        };
        if !shapes.iter().any(|shape| grid.contains(*shape))
            || (!layer.allow_overlap && shapes.iter().any(|shape| grid.hits(*shape)))
        {
            continue;
        }
        placed.push(PlacedSymbol {
            layer: layer_index,
            tile,
            symbol: position,
            flipped,
        });
        if !layer.ignore_placement {
            for shape in shapes {
                grid.insert(shape);
            }
        }
    }
    placed
}

/// A point label's collision box on the grid, which starts `VIEWPORT_PADDING` out from the viewport. The
/// box is the size the label is drawn at: scaled by the perspective `ratio` about its anchor.
fn place_point(ratio: f64, [x, y]: [f64; 2], bounds: [f32; 4]) -> Shape {
    let [left, top, right, bottom] = bounds.map(|edge| f64::from(edge) * ratio);
    Shape::Box([
        x + left + VIEWPORT_PADDING,
        y + top + VIEWPORT_PADDING,
        x + right + VIEWPORT_PADDING,
        y + bottom + VIEWPORT_PADDING,
    ])
}

/// A line label's collision circles along its path, and whether it reads against its line to stay
/// upright. `w` is its anchor's clip-space w, and `flipped` its reading direction in the last placement,
/// which it keeps until it leans `FLIP_MARGIN` past vertical. `None` when it cannot be placed: any vertex
/// it needs is behind the camera, or the line has no room for it.
fn place_line(
    view: &LabelView,
    (tile, symbol, line, layer): (TileId, &LabelSymbol, &LineSymbol, &LabelLayer),
    w: f64,
    flipped: Option<bool>,
) -> Option<(Vec<Shape>, bool)> {
    let ground = view.ground_label(w, symbol, layer);
    // Layout leaves every symbol at least one glyph.
    let first = f64::from(symbol.quads[0].along);
    let last = f64::from(symbol.quads[symbol.quads.len() - 1].along);

    // Where the first and last glyph land on screen. A lone glyph reads along its segment.
    let ends = |flip: bool| -> Option<[[f64; 2]; 2]> {
        let start = glyph_pose((symbol, line, layer), first * ground.units_per_glyph, flip)?;
        let end = if symbol.quads.len() == 1 {
            let one_em = ONE_EM * ground.units_per_glyph;
            [
                start.point[0] + start.direction[0] * one_em,
                start.point[1] + start.direction[1] * one_em,
            ]
        } else {
            glyph_pose((symbol, line, layer), last * ground.units_per_glyph, flip)?.point
        };
        Some([view.screen(tile, start.point)?, view.screen(tile, end)?])
    };
    let [start, end] = ends(false)?;
    // How far the text leans from reading left to right: 1 along the x axis, 0 vertical, -1 upside down.
    let lean = (end[0] - start[0]) / (end[0] - start[0]).hypot(end[1] - start[1]);
    let flip = layer.line.keep_upright
        && match flipped {
            None => lean < 0.0,
            Some(true) => lean < FLIP_MARGIN,
            Some(false) => lean < -FLIP_MARGIN,
        };
    if flip {
        ends(true)?;
    }

    // Circles as tall as the text, a radius apart, from the first glyph to the last.
    let radius = line.half_height;
    let count = if last - first <= radius {
        1
    } else {
        ((last - first) / radius).ceil() as usize + 1
    };
    let ground_radius = radius * ground.units_per_glyph;
    let mut circles = Vec::with_capacity(count);
    for step in 0..count {
        let along = if count == 1 {
            (first + last) / 2.0
        } else {
            first + (last - first) * step as f64 / (count - 1) as f64
        };
        let pose = glyph_pose((symbol, line, layer), along * ground.units_per_glyph, flip)?;
        let centre = view.screen(tile, pose.point)?;
        let edge = view.screen(
            tile,
            [
                pose.point[0] - pose.direction[1] * ground_radius,
                pose.point[1] + pose.direction[0] * ground_radius,
            ],
        )?;
        circles.push(Shape::Circle {
            x: centre[0] + VIEWPORT_PADDING,
            y: centre[1] + VIEWPORT_PADDING,
            r: (edge[0] - centre[0]).hypot(edge[1] - centre[1]) + layer.padding_px,
        });
    }
    Some((circles, flip))
}

/// A camera's latest placement, and the residency of the label tiles it was made from. Residency changes
/// whenever label tiles load or are evicted.
struct PlacementEntry {
    camera: MapCamera,
    size: (u32, u32),
    residency: u64,
    placed: Arc<Vec<PlacedSymbol>>,
}

/// The last few placements, least recently used first.
#[derive(Default)]
pub(crate) struct PlacementCache {
    entries: Vec<PlacementEntry>,
}

impl PlacementCache {
    /// The placements of `cameras`, oldest first, on a viewport of `size`. A camera placed from the current
    /// tiles keeps its placement. Any other is placed by `place` on the placement before it: the last
    /// camera's, or for the first camera its own from older tiles, or none.
    pub(crate) fn place_chain<const N: usize>(
        &mut self,
        cameras: &[MapCamera; N],
        size: (u32, u32),
        residency: u64,
        mut place: impl FnMut(&MapCamera, Option<&[PlacedSymbol]>) -> Vec<PlacedSymbol>,
    ) -> [Arc<Vec<PlacedSymbol>>; N] {
        let mut last: Option<Arc<Vec<PlacedSymbol>>> = None;
        std::array::from_fn(|index| {
            let camera = cameras[index];
            let held = self
                .entries
                .iter()
                .position(|entry| entry.camera == camera && entry.size == size)
                .map(|position| self.entries.remove(position));
            let placed = match held {
                Some(entry) if entry.residency == residency => entry.placed,
                held => {
                    let prior = last.take().or(held.map(|entry| entry.placed));
                    Arc::new(place(&camera, prior.as_deref().map(Vec::as_slice)))
                }
            };
            if self.entries.len() == CACHE_CAPACITY {
                self.entries.remove(0);
            }
            self.entries.push(PlacementEntry {
                camera,
                size,
                residency,
                placed: placed.clone(),
            });
            last = Some(placed.clone());
            placed
        })
    }
}

/// What a label takes up on the collision grid.
#[derive(Clone, Copy)]
enum Shape {
    /// Left, top, right and bottom.
    Box([f64; 4]),
    Circle {
        x: f64,
        y: f64,
        r: f64,
    },
}

impl Shape {
    fn bounds(self) -> [f64; 4] {
        match self {
            Self::Box(bounds) => bounds,
            Self::Circle { x, y, r } => [x - r, y - r, x + r, y + r],
        }
    }

    fn overlaps(self, other: Self) -> bool {
        match (self, other) {
            (Self::Box(a), Self::Box(b)) => {
                a[0] <= b[2] && a[1] <= b[3] && a[2] >= b[0] && a[3] >= b[1]
            }
            (
                Self::Circle { x, y, r },
                Self::Circle {
                    x: other_x,
                    y: other_y,
                    r: other_r,
                },
            ) => (x - other_x).hypot(y - other_y) <= r + other_r,
            (Self::Box(b), Self::Circle { x, y, r }) | (Self::Circle { x, y, r }, Self::Box(b)) => {
                (x - x.clamp(b[0], b[2])).hypot(y - y.clamp(b[1], b[3])) <= r
            }
        }
    }
}

/// A grid of shapes, to find what overlaps one in a few cell lookups.
struct GridIndex {
    width: f64,
    height: f64,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
    shapes: Vec<Shape>,
}

impl GridIndex {
    fn new(width: f64, height: f64) -> Self {
        let columns = (width / GRID_CELL).ceil().max(1.0) as usize;
        let rows = (height / GRID_CELL).ceil().max(1.0) as usize;
        Self {
            width,
            height,
            columns,
            rows,
            cells: vec![Vec::new(); columns * rows],
            shapes: Vec::new(),
        }
    }

    /// Whether a shape is at least partly on the grid.
    fn contains(&self, shape: Shape) -> bool {
        let [left, top, right, bottom] = shape.bounds();
        right >= 0.0 && left < self.width && bottom >= 0.0 && top < self.height
    }

    fn cell_range(&self, shape: Shape) -> ([usize; 2], [usize; 2]) {
        let [left, top, right, bottom] = shape.bounds();
        let cell = |value: f64, count: usize| {
            ((value / GRID_CELL).floor().max(0.0) as usize).min(count - 1)
        };
        (
            [cell(left, self.columns), cell(top, self.rows)],
            [cell(right, self.columns), cell(bottom, self.rows)],
        )
    }

    fn insert(&mut self, shape: Shape) {
        let index = self.shapes.len();
        self.shapes.push(shape);
        let ([first_column, first_row], [last_column, last_row]) = self.cell_range(shape);
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                self.cells[row * self.columns + column].push(index);
            }
        }
    }

    /// Whether a shape overlaps one inserted.
    fn hits(&self, shape: Shape) -> bool {
        let ([first_column, first_row], [last_column, last_row]) = self.cell_range(shape);
        (first_row..=last_row).any(|row| {
            (first_column..=last_column).any(|column| {
                self.cells[row * self.columns + column]
                    .iter()
                    .any(|&index| self.shapes[index].overlaps(shape))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_hits_only_what_it_overlaps() {
        let mut grid = GridIndex::new(300.0, 300.0);
        grid.insert(Shape::Box([10.0, 10.0, 60.0, 30.0]));
        assert!(grid.hits(Shape::Box([50.0, 25.0, 90.0, 50.0])));
        assert!(!grid.hits(Shape::Box([61.0, 10.0, 90.0, 30.0])));
        assert!(!grid.hits(Shape::Box([10.0, 31.0, 60.0, 50.0])));
        // A circle meets a box by its nearest edge, not its bounding square.
        assert!(grid.hits(Shape::Circle {
            x: 70.0,
            y: 20.0,
            r: 11.0
        }));
        assert!(!grid.hits(Shape::Circle {
            x: 70.0,
            y: 40.0,
            r: 11.0
        }));
        grid.insert(Shape::Circle {
            x: 150.0,
            y: 150.0,
            r: 10.0,
        });
        assert!(grid.hits(Shape::Circle {
            x: 165.0,
            y: 150.0,
            r: 6.0
        }));
        assert!(!grid.hits(Shape::Circle {
            x: 165.0,
            y: 150.0,
            r: 4.0
        }));
        // Boxes past the edge land in the nearest cells rather than being lost.
        grid.insert(Shape::Box([280.0, 280.0, 400.0, 400.0]));
        assert!(grid.hits(Shape::Box([299.0, 299.0, 310.0, 310.0])));
        assert!(grid.contains(Shape::Box([-10.0, -10.0, 1.0, 1.0])));
        assert!(!grid.contains(Shape::Box([301.0, 10.0, 320.0, 20.0])));
    }
}
