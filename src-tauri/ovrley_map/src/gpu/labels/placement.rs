//! Label placement: which symbols a camera shows.
//!
//! Placement is a pure function of the camera and the resident label tiles, in `f64` on plain IEEE
//! arithmetic, so a frame draws the same wherever and whenever it is asked for. Export frames reach the
//! map thread out of order, so nothing is carried from one frame to the next.

use super::layer::{Alignment, LabelLayer};
use super::layout::{LabelSymbol, TILE_UNITS};
use crate::{GroundProjection, TileId, ViewState};
use std::cmp::Reverse;

/// Cell size of the collision grid, in output pixels.
const GRID_CELL: f64 = 25.0;
/// How far outside the viewport labels are still placed, so they settle off screen first.
pub(crate) const VIEWPORT_PADDING: f64 = 100.0;

/// A camera on a viewport, reduced to what projecting label anchors needs.
pub(crate) struct LabelView {
    projection: GroundProjection,
    world_size: f64,
    width: f64,
    height: f64,
}

impl LabelView {
    pub(crate) fn new(view: &ViewState, width_px: u32, height_px: u32) -> Self {
        Self {
            projection: view.ground_projection(),
            world_size: view.world_size(),
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
        let tiles = 2_f64.powi(i32::from(tile.z));
        let mut world = [
            (f64::from(tile.x) + f64::from(symbol.anchor[0]) / TILE_UNITS) / tiles,
            (f64::from(tile.y) + f64::from(symbol.anchor[1]) / TILE_UNITS) / tiles,
        ];
        // A translation along the map moves the anchor on the ground, so it follows the camera.
        if layer.translate_anchor == Alignment::Map {
            world[0] += layer.translate_px[0] / self.world_size;
            world[1] += layer.translate_px[1] / self.world_size;
        }
        let clip = self.projection.clip(world[0], world[1]);
        if clip.z < 0.0 {
            return None;
        }
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
}

/// A symbol a camera shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacedSymbol {
    /// The style layer, which is also the draw order.
    pub(crate) layer: u32,
    pub(crate) tile: TileId,
    /// Position in the tile's symbols.
    pub(crate) symbol: u32,
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
                    symbol.sort_key,
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
        let Some(([x, y], _)) = view.project(tile, symbol, layer) else {
            continue;
        };
        let [left, top, right, bottom] = symbol.bounds.map(f64::from);
        let boxed = [
            x + left + VIEWPORT_PADDING,
            y + top + VIEWPORT_PADDING,
            x + right + VIEWPORT_PADDING,
            y + bottom + VIEWPORT_PADDING,
        ];
        if !grid.contains(boxed) || (!layer.allow_overlap && grid.hits(boxed)) {
            continue;
        }
        placed.push(PlacedSymbol {
            layer: layer_index,
            tile,
            symbol: position,
        });
        if !layer.ignore_placement {
            grid.insert(boxed);
        }
    }
    placed
}

/// A grid of boxes, to find what overlaps one in a few cell lookups.
struct GridIndex {
    width: f64,
    height: f64,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
    boxes: Vec<[f64; 4]>,
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
            boxes: Vec::new(),
        }
    }

    /// Whether a box is at least partly on the grid.
    fn contains(&self, [left, top, right, bottom]: [f64; 4]) -> bool {
        right >= 0.0 && left < self.width && bottom >= 0.0 && top < self.height
    }

    fn cell_range(&self, [left, top, right, bottom]: [f64; 4]) -> ([usize; 2], [usize; 2]) {
        let cell = |value: f64, count: usize| {
            ((value / GRID_CELL).floor().max(0.0) as usize).min(count - 1)
        };
        (
            [cell(left, self.columns), cell(top, self.rows)],
            [cell(right, self.columns), cell(bottom, self.rows)],
        )
    }

    fn insert(&mut self, boxed: [f64; 4]) {
        let index = self.boxes.len();
        self.boxes.push(boxed);
        let ([first_column, first_row], [last_column, last_row]) = self.cell_range(boxed);
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                self.cells[row * self.columns + column].push(index);
            }
        }
    }

    /// Whether a box overlaps one inserted.
    fn hits(&self, boxed: [f64; 4]) -> bool {
        let ([first_column, first_row], [last_column, last_row]) = self.cell_range(boxed);
        (first_row..=last_row).any(|row| {
            (first_column..=last_column).any(|column| {
                self.cells[row * self.columns + column]
                    .iter()
                    .any(|&index| {
                        let other = self.boxes[index];
                        other[0] <= boxed[2]
                            && other[1] <= boxed[3]
                            && other[2] >= boxed[0]
                            && other[3] >= boxed[1]
                    })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_hits_only_what_it_overlaps() {
        let mut grid = GridIndex::new(300.0, 300.0);
        grid.insert([10.0, 10.0, 60.0, 30.0]);
        assert!(grid.hits([50.0, 25.0, 90.0, 50.0]));
        assert!(!grid.hits([61.0, 10.0, 90.0, 30.0]));
        assert!(!grid.hits([10.0, 31.0, 60.0, 50.0]));
        // Boxes past the edge land in the nearest cells rather than being lost.
        grid.insert([280.0, 280.0, 400.0, 400.0]);
        assert!(grid.hits([299.0, 299.0, 310.0, 310.0]));
        assert!(grid.contains([-10.0, -10.0, 1.0, 1.0]));
        assert!(!grid.contains([301.0, 10.0, 320.0, 20.0]));
    }
}
