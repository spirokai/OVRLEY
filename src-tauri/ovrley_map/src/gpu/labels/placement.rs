//! Label placement: which symbols a camera shows.
//!
//! Placement is a pure function of the camera and the resident label tiles, in `f64` on plain IEEE
//! arithmetic, so a frame draws the same wherever and whenever it is asked for. Export frames reach the
//! map thread out of order, so nothing is carried from one frame to the next.

use super::layer::{Alignment, LabelLayer};
use super::layout::{LabelSymbol, LineSymbol, SymbolKind, TILE_UNITS};
use super::line::{pose_at, Pose};
use super::text::ONE_EM;
use crate::{ClipPoint, GroundProjection, TileId, ViewState};
use std::cmp::Reverse;

/// Cell size of the collision grid, in output pixels.
const GRID_CELL: f64 = 25.0;
/// How far outside the viewport labels are still placed, so they settle off screen first.
pub(crate) const VIEWPORT_PADDING: f64 = 100.0;

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
    pub(crate) fn new(view: &ViewState, width_px: u32, height_px: u32) -> Self {
        Self {
            projection: view.ground_projection(),
            centre_w: view.centre_clip_w(),
            width: f64::from(width_px),
            height: f64::from(height_px),
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
        let ratio = (0.5 + 0.5 * self.centre_w / w).clamp(0.0, 4.0);
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacedSymbol {
    /// The style layer, which is also the draw order.
    pub(crate) layer: u32,
    pub(crate) tile: TileId,
    /// Position in the tile's symbols.
    pub(crate) symbol: u32,
    /// A line label that reads against its line, to stay upright.
    pub(crate) flipped: bool,
}

/// Chooses the symbols of `tiles` the view shows. Layers place from the top of the style down,
/// then tile by tile, then by `symbol-sort-key` and layout order. A symbol that
/// collides with a placed one, or lies outside the viewport and its padding, is left out unless its
/// layer allows overlap. The result is in placement order.
pub(crate) fn place(
    view: &LabelView,
    layers: &[LabelLayer],
    tiles: &[(TileId, &[LabelSymbol])],
) -> Vec<PlacedSymbol> {
    let mut candidates: Vec<(Reverse<u32>, TileId, f64, u32)> = tiles
        .iter()
        .flat_map(|(tile, symbols)| {
            symbols.iter().enumerate().map(|(position, symbol)| {
                (
                    Reverse(symbol.layer),
                    *tile,
                    symbol.paint.sort_key,
                    position as u32,
                )
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
            .then(a.3.cmp(&b.3))
    });

    let (width, height) = view.size();
    let mut grid = GridIndex::new(
        width + 2.0 * VIEWPORT_PADDING,
        height + 2.0 * VIEWPORT_PADDING,
    );
    let symbols: std::collections::HashMap<TileId, &[LabelSymbol]> =
        tiles.iter().copied().collect();
    let mut placed = Vec::new();
    for (Reverse(layer_index), tile, _, position) in candidates {
        let Some(layer) = layers.iter().find(|layer| layer.index == layer_index) else {
            continue;
        };
        let symbol = &symbols[&tile][position as usize];
        let candidate = match &symbol.kind {
            SymbolKind::Point { bounds } => {
                place_point(view, tile, symbol, layer, *bounds).map(|shape| (vec![shape], false))
            }
            SymbolKind::Line(line) => place_line(view, tile, symbol, line, layer),
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

/// A point label's collision box on the grid, which starts `VIEWPORT_PADDING` out from the viewport.
fn place_point(
    view: &LabelView,
    tile: TileId,
    symbol: &LabelSymbol,
    layer: &LabelLayer,
    bounds: [f32; 4],
) -> Option<Shape> {
    let ([x, y], _) = view.project(tile, symbol, layer)?;
    let [left, top, right, bottom] = bounds.map(f64::from);
    Some(Shape::Box([
        x + left + VIEWPORT_PADDING,
        y + top + VIEWPORT_PADDING,
        x + right + VIEWPORT_PADDING,
        y + bottom + VIEWPORT_PADDING,
    ]))
}

/// A line label's collision circles along its path, and whether it reads against its line to stay
/// upright. `None` when it cannot be placed: its anchor is behind the camera, or the line has no room
/// for it.
fn place_line(
    view: &LabelView,
    tile: TileId,
    symbol: &LabelSymbol,
    line: &LineSymbol,
    layer: &LabelLayer,
) -> Option<(Vec<Shape>, bool)> {
    let (_, w) = view.project(tile, symbol, layer)?;
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
    let mut flip = false;
    let [start, end] = ends(false)?;
    if layer.line.keep_upright && start[0] > end[0] {
        flip = true;
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
