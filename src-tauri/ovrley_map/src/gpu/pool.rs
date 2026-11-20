//! The geometry pools, and how big a job's must be.
//!
//! A pool is a ring-buffer like set of GPU buffers, ported from maplibre-rs
//! `vector/resource/buffer_pool.rs`: one large buffer each for vertices, indices, layer metadata and
//! feature metadata. Draws bind slices of them, and the oldest layers are evicted for space.
//!
//! A view that outgrows a pool would evict its own tiles and draw blank patches, and which views do
//! depends on the area and style, not on zoom or pitch alone. So export measures its tiles first.

use super::pipelines::{FillShaderFeatureMetadata, ShaderLayerMetadata};
use super::tile::{LayerKind, PreparedLayer};
use crate::TileId;
use bytemuck::Pod;
use std::collections::{btree_map, BTreeMap, HashMap, VecDeque};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::Range;
#[cfg(not(target_arch = "wasm32"))]
use {
    super::labels::{GlyphId, GlyphService},
    super::style::ParsedStyle,
    super::tile::prepare_tile,
    super::GpuMapConfig,
    crate::MapDataSource,
    std::collections::BTreeSet,
    std::sync::Arc,
};

/// Entries per geometry pool when a host has not measured: 73 MB of ground buffers. Upstream's
/// 10 million lost the device on a 512 MB integrated GPU in 3 of 8 runs.
const DEFAULT_POOL_ENTRIES: u64 = 2_000_000;
/// Tile layers per pool when a host has not measured.
const DEFAULT_POOL_LAYERS: u64 = 10 * 1024;
/// How far a worst view's pool exceeds that view. A view fills an empty pool exactly, so this only
/// buys time: new tiles fill the spare room before a tile still in view reaches the front of the
/// ring, which costs a reload of the whole view.
const HEADROOM_NUMERATOR: u64 = 3;
const HEADROOM_DENOMINATOR: u64 = 2;

/// A pool's backing buffers, by their place in [`BufferPool`]'s arrays.
pub(crate) const VERTICES: usize = 0;
pub(crate) const INDICES: usize = 1;
pub(crate) const LAYER_METADATA: usize = 2;
pub(crate) const FEATURE_METADATA: usize = 3;
const BUFFER_NAMES: [&str; 4] = ["vertices", "indices", "layer metadata", "feature metadata"];

/// What one pool holds, or what one tile asks of it. A layer of a tile uses one metadata entry and
/// a vertex uses one feature metadata entry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolEntries {
    pub vertices: u64,
    pub indices: u64,
    pub layers: u64,
}

impl PoolEntries {
    fn zip(self, other: Self, combine: impl Fn(u64, u64) -> u64) -> Self {
        Self {
            vertices: combine(self.vertices, other.vertices),
            indices: combine(self.indices, other.indices),
            layers: combine(self.layers, other.layers),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn of<V>(layers: &[PreparedLayer<V>]) -> Self {
        Self {
            vertices: layers.iter().map(|layer| layer.vertices.len() as u64).sum(),
            indices: layers.iter().map(|layer| layer.indices.len() as u64).sum(),
            layers: layers.len() as u64,
        }
    }
}

/// The two geometry pools of a renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuPoolSizes {
    pub ground: PoolEntries,
    pub extrusion: PoolEntries,
}

impl GpuPoolSizes {
    /// What a host that has not measured starts with.
    pub const DEFAULT: Self = Self::uniform(DEFAULT_POOL_ENTRIES);

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

    fn zip(self, other: Self, combine: impl Fn(u64, u64) -> u64 + Copy) -> Self {
        Self {
            ground: self.ground.zip(other.ground, combine),
            extrusion: self.extrusion.zip(other.extrusion, combine),
        }
    }

    fn sum(sizes: impl Iterator<Item = Self>) -> Self {
        sizes.fold(Self::default(), |all, size| all.zip(size, |a, b| a + b))
    }

    /// Pools that hold the largest of `views` with room to spare. A job that fits entirely gets
    /// exactly what it needs, and a longer one keeps at least the default, so tiles behind the
    /// camera stay resident while they can. Every tile of `views` must be in `tiles`.
    pub fn for_views(tiles: &HashMap<TileId, GpuPoolSizes>, views: &[Vec<TileId>]) -> Self {
        let total = Self::sum(tiles.values().copied());
        let worst = views
            .iter()
            .map(|view| Self::sum(view.iter().map(|tile| tiles[tile])))
            .fold(Self::default(), |worst, view| worst.zip(view, u64::max));
        worst
            .zip(worst, |worst, _| {
                worst * HEADROOM_NUMERATOR / HEADROOM_DENOMINATOR
            })
            .zip(Self::DEFAULT, u64::max)
            .zip(total, u64::min)
            .zip(worst, |entries, _| entries.max(1))
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn doubled(self) -> Self {
        self.zip(self, |a, b| a + b)
    }

    /// Fails when a pool is empty, or needs a larger buffer than the device allows.
    pub(crate) fn check(self, max_buffer_size: u64, adapter_name: &str) -> Result<(), String> {
        let has_empty_pool = [self.ground, self.extrusion]
            .iter()
            .any(|pool| pool.vertices == 0 || pool.indices == 0 || pool.layers == 0);
        if has_empty_pool {
            return Err("Map pools must not be empty".to_string());
        }
        let largest_buffer =
            BufferPool::<super::pipelines::ShaderVertex>::buffer_sizes(self.ground)
                .into_iter()
                .chain(BufferPool::<super::tile::ExtrusionVertex>::buffer_sizes(
                    self.extrusion,
                ))
                .fold(0, u64::max);
        if largest_buffer > max_buffer_size {
            return Err(format!(
                "This map view needs a {largest_buffer} byte GPU buffer and {adapter_name} allows \
                 {max_buffer_size}. Raise the map's zoom or lower its pitch."
            ));
        }
        Ok(())
    }
}

pub(crate) struct BufferPool<V> {
    buffers: [wgpu::Buffer; 4],
    sizes: [wgpu::BufferAddress; 4],
    index: RingIndex,
    vertex: PhantomData<V>,
}

impl<V: Pod> BufferPool<V> {
    /// Bytes each backing buffer needs to hold `entries`.
    fn buffer_sizes(entries: PoolEntries) -> [u64; 4] {
        [
            size_of::<V>() as u64 * entries.vertices,
            size_of::<u32>() as u64 * entries.indices,
            size_of::<ShaderLayerMetadata>() as u64 * entries.layers,
            size_of::<FillShaderFeatureMetadata>() as u64 * entries.vertices,
        ]
    }

    pub(crate) fn new(device: &wgpu::Device, label: &str, entries: PoolEntries) -> Self {
        let sizes = Self::buffer_sizes(entries);
        let usages = [
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::INDEX,
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::VERTEX,
        ];
        Self {
            buffers: std::array::from_fn(|at| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("{label} {}", BUFFER_NAMES[at])),
                    size: sizes[at],
                    usage: usages[at] | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
            sizes,
            index: RingIndex::default(),
            vertex: PhantomData,
        }
    }

    /// The part of backing buffer `at` that holds `entry`.
    pub(crate) fn slice(&self, entry: &IndexEntry, at: usize) -> wgpu::BufferSlice<'_> {
        self.buffers[at].slice(entry.ranges[at].clone())
    }

    /// The resident layers of `tile`.
    pub(crate) fn layers(&self, tile: TileId) -> Option<&VecDeque<IndexEntry>> {
        self.index.tree.get(&tile)
    }

    /// How many layers have been evicted to make room, ever.
    pub(crate) const fn evicted_layers(&self) -> u64 {
        self.index.evicted_layers
    }

    /// Tiles that lost a layer since the last call.
    pub(crate) fn drain_evicted_tiles(&mut self) -> std::vec::Drain<'_, TileId> {
        self.index.evicted_tiles.drain(..)
    }

    /// Forgets every layer, so the next ones fill the buffers from the start. Frames recorded after
    /// this draw only what is uploaded after it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn clear(&mut self) {
        self.index = RingIndex::default();
    }

    /// Total bytes across this pool's buffers.
    pub(crate) fn size_bytes(&self) -> u64 {
        self.sizes.iter().sum()
    }

    /// Uploads one layer of `tile`, unless it survived an earlier eviction of its tile, evicting the
    /// oldest layers when there is not enough space. Fails when it cannot fit at all.
    pub(crate) fn upload(
        &mut self,
        queue: &wgpu::Queue,
        tile: TileId,
        layer: &PreparedLayer<V>,
    ) -> Result<(), String> {
        let is_resident = self
            .layers(tile)
            .is_some_and(|entries| entries.iter().any(|entry| entry.layer == layer.index));
        if is_resident {
            return Ok(());
        }
        let data: [&[u8]; 4] = [
            bytemuck::cast_slice(&layer.vertices),
            bytemuck::cast_slice(&layer.indices),
            bytemuck::bytes_of(&layer.layer_metadata),
            bytemuck::cast_slice(&layer.feature_metadata),
        ];
        let mut ranges: [Range<wgpu::BufferAddress>; 4] = Default::default();
        for at in 0..data.len() {
            ranges[at] = self
                .index
                .make_room(data[at].len() as u64, at, self.sizes[at])?;
        }
        for (at, bytes) in data.iter().enumerate() {
            queue.write_buffer(&self.buffers[at], ranges[at].start, bytes);
        }
        self.index.push_back(IndexEntry {
            coords: tile,
            layer: layer.index,
            kind: layer.kind,
            ranges,
            usable_indices: layer.indices.len() as u32,
        });
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IndexEntry {
    pub(crate) coords: TileId,
    /// The style layer's draw order.
    pub(crate) layer: u32,
    pub(crate) kind: LayerKind,
    /// Where the layer sits in each backing buffer.
    ranges: [Range<wgpu::BufferAddress>; 4],
    usable_indices: u32,
}

impl IndexEntry {
    pub(crate) fn indices_range(&self) -> Range<u32> {
        0..self.usable_indices
    }
}

#[derive(Default)]
struct RingIndex {
    tree: BTreeMap<TileId, VecDeque<IndexEntry>>,
    linear: VecDeque<TileId>,
    /// Tiles that lost a layer since the last drain, oldest first.
    evicted_tiles: Vec<TileId>,
    evicted_layers: u64,
}

impl RingIndex {
    fn front(&self) -> Option<&IndexEntry> {
        self.tree.get(self.linear.front()?)?.front()
    }

    fn back(&self) -> Option<&IndexEntry> {
        self.tree.get(self.linear.back()?)?.back()
    }

    fn pop_front(&mut self) -> Option<IndexEntry> {
        let key = self.linear.pop_front()?;
        let btree_map::Entry::Occupied(mut layers) = self.tree.entry(key) else {
            return None;
        };
        let entry = layers.get_mut().pop_front();
        if layers.get().is_empty() {
            layers.remove();
        }
        self.evicted_tiles.push(key);
        self.evicted_layers += 1;
        entry
    }

    fn push_back(&mut self, entry: IndexEntry) {
        self.linear.push_back(entry.coords);
        self.tree.entry(entry.coords).or_default().push_back(entry);
    }

    /// Room for `new_data` bytes in backing buffer `at`, evicting the oldest layers until it fits.
    fn make_room(
        &mut self,
        new_data: wgpu::BufferAddress,
        at: usize,
        inner_size: wgpu::BufferAddress,
    ) -> Result<Range<wgpu::BufferAddress>, String> {
        let name = BUFFER_NAMES[at];
        if new_data > inner_size {
            return Err(format!(
                "{new_data} bytes do not fit in the {inner_size} byte {name} buffer"
            ));
        }
        let mut available_gap = self.find_largest_gap(at, inner_size);
        while new_data > available_gap.end - available_gap.start {
            // No more space, so evict the oldest layer.
            if self.pop_front().is_none() {
                return Err(format!(
                    "The {name} buffer is out of space and has no layers"
                ));
            }
            available_gap = self.find_largest_gap(at, inner_size);
        }
        Ok(available_gap.start..available_gap.start + new_data)
    }

    fn find_largest_gap(
        &self,
        at: usize,
        inner_size: wgpu::BufferAddress,
    ) -> Range<wgpu::BufferAddress> {
        let start = self.front().map(|first| first.ranges[at].start);
        let end = self.back().map(|last| last.ranges[at].end);
        match (start, end) {
            (Some(start), Some(end)) if end > start => {
                // Not wrapped yet in the ring.
                let gap_from_start = 0..start;
                let gap_to_end = end..inner_size;
                if gap_to_end.end - gap_to_end.start > gap_from_start.end - gap_from_start.start {
                    gap_to_end
                } else {
                    gap_from_start
                }
            }
            // Wrapped, so the gap lies between the two.
            (Some(start), Some(end)) => end..start,
            _ => 0..inner_size,
        }
    }
}

/// What one tile asks of a renderer: room in the geometry pools, and the glyphs of its labels.
#[cfg(not(target_arch = "wasm32"))]
pub struct MeasuredTile {
    pub pools: GpuPoolSizes,
    pub glyphs: BTreeSet<GlyphId>,
}

/// Tessellates tiles to count their geometry, and lays out their labels, without touching the GPU.
#[cfg(not(target_arch = "wasm32"))]
pub struct MapTileSizer {
    style: ParsedStyle,
    glyphs: Arc<GlyphService>,
}

#[cfg(not(target_arch = "wasm32"))]
impl MapTileSizer {
    /// Reads the style a renderer of `config` draws. Fails loudly on an invalid style.
    pub fn new(config: &GpuMapConfig, source: &dyn MapDataSource) -> Result<Self, String> {
        Ok(Self {
            style: config.read_style(&source.style(&config.spec.style)?)?,
            glyphs: source.glyphs(),
        })
    }

    /// What one tile asks for. Fails when the bytes are not a tile. Laying its labels out makes every
    /// glyph they use in the glyph service, so a job that measures all its tiles has them all before
    /// its first frame.
    pub fn measure(&self, bytes: &[u8]) -> Result<MeasuredTile, String> {
        let prepared = prepare_tile(bytes, &self.style, Some(&*self.glyphs))?;
        Ok(MeasuredTile {
            pools: GpuPoolSizes {
                ground: PoolEntries::of(&prepared.ground),
                extrusion: PoolEntries::of(&prepared.extrusions),
            },
            glyphs: prepared
                .labels
                .iter()
                .flat_map(|symbol| symbol.quads.iter().map(|quad| quad.glyph))
                .collect(),
        })
    }
}
