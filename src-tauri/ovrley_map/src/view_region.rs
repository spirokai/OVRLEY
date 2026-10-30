//! Tile identities and view regions, ported from maplibre-rs `coords.rs`.

use crate::camera::{MapCamera, MapViewport, ViewPadding, ViewState};

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
pub fn view_tiles(camera: &MapCamera, viewport: MapViewport, padding: ViewPadding) -> Vec<TileId> {
    let view = ViewState::new(camera, viewport);
    view.create_view_region(data_level(view.zoom()), padding)
        .map(|region| region.iter().collect())
        .unwrap_or_default()
}

/// A bounding box of tiles at one level, with a padding.
pub struct ViewRegion {
    min_tile: TileId,
    max_tile: TileId,
    padding: i32,
    max_n_tiles: usize,
}

impl ViewRegion {
    pub(crate) fn new(
        min_tile: TileId,
        max_tile: TileId,
        padding: i32,
        max_n_tiles: usize,
    ) -> Self {
        Self {
            min_tile,
            max_tile,
            padding,
            max_n_tiles,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = TileId> + '_ {
        let z = self.min_tile.z;
        (self.min_tile.x - self.padding..=self.max_tile.x + self.padding)
            .flat_map(move |x| {
                (self.min_tile.y - self.padding..=self.max_tile.y + self.padding)
                    .map(move |y| TileId::new(z, x, y))
            })
            .filter(|tile| tile.is_on_grid())
            .take(self.max_n_tiles)
    }
}
