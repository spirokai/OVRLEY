//! Sizes a renderer's pools for one job, from what its tiles actually tessellate to.
//!
//! A view that outgrows a pool would evict its own tiles and draw blank patches, and which views do
//! depends on the area and style, not on zoom or pitch alone. So a host measures its tiles first.

use super::pool::PoolEntries;
use super::style::{parse_style, Buildings, ParsedStyle};
use super::tile::{prepare_tile, PreparedLayer};
use crate::{MapDataSource, MapStyleSpec, MapViewport, TileId};
use std::collections::HashMap;

/// Entries per geometry pool when a host has not measured: 73 MB of ground buffers. Upstream's
/// 10 million lost the device on a 512 MB integrated GPU in 3 of 8 runs.
pub const DEFAULT_POOL_ENTRIES: u64 = 2_000_000;
/// Tile layers per pool when a host has not measured.
const DEFAULT_POOL_LAYERS: u64 = 10 * 1024;
/// How far a worst view's pool exceeds that view. A view fills an empty pool exactly, so this only
/// buys time: new tiles fill the spare room before a tile still in view reaches the front of the
/// ring, which costs a reload of the whole view.
const HEADROOM_NUMERATOR: u64 = 3;
const HEADROOM_DENOMINATOR: u64 = 2;

/// The two geometry pools of a renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuPoolSizes {
    pub ground: PoolEntries,
    pub extrusion: PoolEntries,
}

impl GpuPoolSizes {
    pub const fn uniform(entries: u64) -> Self {
        let pool = PoolEntries {
            vertices: entries,
            indices: entries,
            layers: DEFAULT_POOL_LAYERS,
        };
        Self {
            ground: pool,
            extrusion: pool,
        }
    }

    /// Pools that hold the largest of `views` with room to spare. A job that fits entirely gets
    /// exactly what it needs, and a longer one keeps at least the default, so tiles behind the
    /// camera stay resident while they can. Every tile of `views` must be in `tiles`.
    pub fn for_views(tiles: &HashMap<TileId, GpuPoolSizes>, views: &[Vec<TileId>]) -> Self {
        Self {
            ground: pool_for_views(tiles, views, |size| size.ground),
            extrusion: pool_for_views(tiles, views, |size| size.extrusion),
        }
    }
}

fn pool_for_views(
    tiles: &HashMap<TileId, GpuPoolSizes>,
    views: &[Vec<TileId>],
    read: impl Fn(&GpuPoolSizes) -> PoolEntries,
) -> PoolEntries {
    let sum = |sizes: &mut dyn Iterator<Item = PoolEntries>| {
        sizes.fold(PoolEntries::default(), |all, size| PoolEntries {
            vertices: all.vertices + size.vertices,
            indices: all.indices + size.indices,
            layers: all.layers + size.layers,
        })
    };
    let total = sum(&mut tiles.values().map(&read));
    let mut worst = PoolEntries::default();
    for view in views {
        let view = sum(&mut view.iter().map(|tile| read(&tiles[tile])));
        worst = PoolEntries {
            vertices: worst.vertices.max(view.vertices),
            indices: worst.indices.max(view.indices),
            layers: worst.layers.max(view.layers),
        };
    }
    let entries = |total: u64, worst: u64, default: u64| {
        let needed = worst * HEADROOM_NUMERATOR / HEADROOM_DENOMINATOR;
        needed.max(default).min(total).max(1)
    };
    PoolEntries {
        vertices: entries(total.vertices, worst.vertices, DEFAULT_POOL_ENTRIES),
        indices: entries(total.indices, worst.indices, DEFAULT_POOL_ENTRIES),
        layers: entries(total.layers, worst.layers, DEFAULT_POOL_LAYERS),
    }
}

/// Tessellates tiles to count their geometry, without touching the GPU.
pub struct MapTileSizer {
    style: ParsedStyle,
}

impl MapTileSizer {
    /// Takes what a renderer's style depends on. Fails loudly on an invalid style.
    pub fn new(
        spec: MapStyleSpec,
        viewport: MapViewport,
        style_zoom: f64,
        buildings: &Buildings,
        source: &dyn MapDataSource,
    ) -> Result<Self, String> {
        Ok(Self {
            style: parse_style(
                &source.style(spec.style)?,
                spec.layers,
                style_zoom,
                viewport.pixel_ratio(),
                buildings,
            )?,
        })
    }

    /// What one tile asks of the pools. Fails when the bytes are not a tile.
    pub fn measure(&self, bytes: &[u8]) -> Result<GpuPoolSizes, String> {
        let prepared = prepare_tile(bytes, &self.style)?;
        Ok(GpuPoolSizes {
            ground: measure_layers(&prepared.ground),
            extrusion: measure_layers(&prepared.extrusions),
        })
    }
}

fn measure_layers<V>(layers: &[PreparedLayer<V>]) -> PoolEntries {
    PoolEntries {
        vertices: layers.iter().map(|layer| layer.vertices.len() as u64).sum(),
        indices: layers.iter().map(|layer| layer.indices.len() as u64).sum(),
        layers: layers.len() as u64,
    }
}
