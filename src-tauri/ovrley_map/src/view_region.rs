//! Tile identities and view regions, ported from maplibre-rs `coords.rs`.

use crate::camera::{MapCamera, MapViewport, ViewState};

/// Tiles stop at this level upstream. Deeper views draw these tiles scaled.
const MAX_DATA_LEVEL: u8 = 14;

/// A position in the tile pyramid.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TileId {
    pub z: u8,
    pub x: i32,
    pub y: i32,
}

impl TileId {
    pub const fn new(z: u8, x: i32, y: i32) -> Self {
        Self { z, x, y }
    }

    /// Whether this tile exists on the grid.
    pub const fn is_on_grid(self) -> bool {
        let bounds = 1_i32 << self.z;
        self.x >= 0 && self.y >= 0 && self.x < bounds && self.y < bounds
    }

    pub fn parent(self) -> Option<Self> {
        (self.z > 0).then(|| Self::new(self.z - 1, self.x >> 1, self.y >> 1))
    }
}

/// The tile level drawn at a view zoom, capped so deeper views overzoom.
pub fn data_level(zoom: f64) -> u8 {
    (zoom.floor() as u8).min(MAX_DATA_LEVEL)
}

/// The on-grid tiles a camera sees on a viewport.
pub fn view_tiles(camera: &MapCamera, viewport: MapViewport) -> Vec<TileId> {
    let view = ViewState::new(camera, viewport);
    view.create_view_region(data_level(view.zoom()))
        .map(|region| region.iter().collect())
        .unwrap_or_default()
}

/// A bounding box of tiles at one level, cut to the frustum's footprint.
pub struct ViewRegion {
    min_tile: TileId,
    max_tile: TileId,
    /// The footprint's corners in order, in tile units at this level. A pitched, rotated view's
    /// footprint fills well under half of its bounding box.
    footprint: [(f64, f64); 4],
    max_n_tiles: usize,
}

impl ViewRegion {
    pub(crate) fn new(
        min_tile: TileId,
        max_tile: TileId,
        footprint: [(f64, f64); 4],
        max_n_tiles: usize,
    ) -> Self {
        Self {
            min_tile,
            max_tile,
            footprint,
            max_n_tiles,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = TileId> + '_ {
        let z = self.min_tile.z;
        (self.min_tile.x..=self.max_tile.x)
            .flat_map(move |x| (self.min_tile.y..=self.max_tile.y).map(move |y| TileId::new(z, x, y)))
            .filter(|tile| tile.is_on_grid())
            .filter(|tile| footprint_touches(&self.footprint, *tile))
            .take(self.max_n_tiles)
    }
}

/// Whether a tile's square meets the footprint. The bounding box already covers the square's own
/// axes, so only the footprint's edges can separate the two.
fn footprint_touches(footprint: &[(f64, f64); 4], tile: TileId) -> bool {
    let (left, top) = (f64::from(tile.x), f64::from(tile.y));
    let corners = [
        (left, top),
        (left + 1.0, top),
        (left + 1.0, top + 1.0),
        (left, top + 1.0),
    ];
    let edges = [0, 1, 2, 3].map(|index| (footprint[index], footprint[(index + 1) % 4]));
    // Twice the signed area: its sign says which side of each edge is inside.
    let winding: f64 = edges.iter().map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
    edges.iter().all(|(a, b)| {
        corners.iter().any(|corner| {
            let side = (b.0 - a.0) * (corner.1 - a.1) - (b.1 - a.1) * (corner.0 - a.0);
            side * winding >= 0.0
        })
    })
}
