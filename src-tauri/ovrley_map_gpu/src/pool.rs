//! A ring-buffer like pool of GPU buffers, ported from maplibre-rs
//! `vector/resource/buffer_pool.rs`: one large buffer each for vertices, indices, layer metadata
//! and feature metadata. Draws bind slices of them, and the oldest layers are evicted for space.

use crate::pattern::TileId;
use crate::pipelines::{FillShaderFeatureMetadata, ShaderLayerMetadata};
use crate::tile::LayerKind;
use bytemuck::Pod;
use std::collections::{btree_map, BTreeMap, VecDeque};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::Range;

/// One layer per tile uses one metadata entry, so this is far more than the geometry can reach.
const LAYER_METADATA_SIZE: wgpu::BufferAddress = 10 * 1024;

/// Geometry for one layer of one tile, ready to upload.
pub(crate) struct LayerGeometry<'a, V> {
    pub(crate) vertices: &'a [V],
    pub(crate) indices: &'a [u32],
    /// One entry per vertex.
    pub(crate) feature_metadata: &'a [FillShaderFeatureMetadata],
    pub(crate) layer_metadata: ShaderLayerMetadata,
}

#[derive(Clone, Copy, Debug)]
enum BackingBufferType {
    Vertices,
    Indices,
    Metadata,
    FeatureMetadata,
}

struct BackingBuffer {
    inner: wgpu::Buffer,
    inner_size: wgpu::BufferAddress,
    typ: BackingBufferType,
}

pub(crate) struct BufferPool<V> {
    vertices: BackingBuffer,
    indices: BackingBuffer,
    layer_metadata: BackingBuffer,
    feature_metadata: BackingBuffer,
    index: RingIndex,
    phantom_v: PhantomData<V>,
}

impl<V: Pod> BufferPool<V> {
    /// `entries` sizes the vertex, index and feature metadata buffers.
    pub(crate) fn new(device: &wgpu::Device, label: &str, entries: u64) -> Self {
        let names = [
            format!("{label} vertices"),
            format!("{label} indices"),
            format!("{label} layer metadata"),
            format!("{label} feature metadata"),
        ];
        let buffer = |name: &str, size, usage, typ| {
            let inner = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(name),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            BackingBuffer {
                inner,
                inner_size: size,
                typ,
            }
        };
        Self {
            vertices: buffer(
                &names[0],
                size_of::<V>() as u64 * entries,
                wgpu::BufferUsages::VERTEX,
                BackingBufferType::Vertices,
            ),
            indices: buffer(
                &names[1],
                size_of::<u32>() as u64 * entries,
                wgpu::BufferUsages::INDEX,
                BackingBufferType::Indices,
            ),
            layer_metadata: buffer(
                &names[2],
                size_of::<ShaderLayerMetadata>() as u64 * LAYER_METADATA_SIZE,
                wgpu::BufferUsages::VERTEX,
                BackingBufferType::Metadata,
            ),
            feature_metadata: buffer(
                &names[3],
                size_of::<FillShaderFeatureMetadata>() as u64 * entries,
                wgpu::BufferUsages::VERTEX,
                BackingBufferType::FeatureMetadata,
            ),
            index: RingIndex::default(),
            phantom_v: PhantomData,
        }
    }

    pub(crate) fn vertices(&self) -> &wgpu::Buffer {
        &self.vertices.inner
    }

    pub(crate) fn indices(&self) -> &wgpu::Buffer {
        &self.indices.inner
    }

    pub(crate) fn metadata(&self) -> &wgpu::Buffer {
        &self.layer_metadata.inner
    }

    pub(crate) fn feature_metadata(&self) -> &wgpu::Buffer {
        &self.feature_metadata.inner
    }

    pub(crate) fn index(&self) -> &RingIndex {
        &self.index
    }

    /// Tiles that lost a layer since the last call.
    pub(crate) fn drain_evicted_tiles(&mut self) -> std::vec::Drain<'_, TileId> {
        self.index.evicted_tiles.drain(..)
    }

    /// Total bytes across this pool's buffers.
    pub(crate) fn size_bytes(&self) -> u64 {
        self.vertices.inner_size
            + self.indices.inner_size
            + self.layer_metadata.inner_size
            + self.feature_metadata.inner_size
    }

    /// Allocates one layer, evicting the oldest layers if there is not enough space.
    pub(crate) fn allocate_layer_geometry(
        &mut self,
        queue: &wgpu::Queue,
        coords: TileId,
        layer: u32,
        kind: LayerKind,
        geometry: &LayerGeometry<'_, V>,
    ) -> Result<(), String> {
        let vertices = bytemuck::cast_slice::<V, u8>(geometry.vertices);
        let indices = bytemuck::cast_slice::<u32, u8>(geometry.indices);
        let layer_metadata = bytemuck::bytes_of(&geometry.layer_metadata);
        let feature_metadata = bytemuck::cast_slice::<_, u8>(geometry.feature_metadata);

        let entry = IndexEntry {
            coords,
            layer,
            kind,
            buffer_vertices: self.index.make_room(
                vertices.len() as u64,
                self.vertices.typ,
                self.vertices.inner_size,
            )?,
            buffer_indices: self.index.make_room(
                indices.len() as u64,
                self.indices.typ,
                self.indices.inner_size,
            )?,
            buffer_layer_metadata: self.index.make_room(
                layer_metadata.len() as u64,
                self.layer_metadata.typ,
                self.layer_metadata.inner_size,
            )?,
            buffer_feature_metadata: self.index.make_room(
                feature_metadata.len() as u64,
                self.feature_metadata.typ,
                self.feature_metadata.inner_size,
            )?,
            usable_indices: geometry.indices.len() as u32,
        };

        queue.write_buffer(&self.vertices.inner, entry.buffer_vertices.start, vertices);
        queue.write_buffer(&self.indices.inner, entry.buffer_indices.start, indices);
        queue.write_buffer(
            &self.layer_metadata.inner,
            entry.buffer_layer_metadata.start,
            layer_metadata,
        );
        queue.write_buffer(
            &self.feature_metadata.inner,
            entry.buffer_feature_metadata.start,
            feature_metadata,
        );
        self.index.push_back(entry);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IndexEntry {
    pub(crate) coords: TileId,
    /// The style layer's draw order.
    pub(crate) layer: u32,
    pub(crate) kind: LayerKind,
    buffer_vertices: Range<wgpu::BufferAddress>,
    buffer_indices: Range<wgpu::BufferAddress>,
    buffer_layer_metadata: Range<wgpu::BufferAddress>,
    buffer_feature_metadata: Range<wgpu::BufferAddress>,
    usable_indices: u32,
}

impl IndexEntry {
    pub(crate) fn indices_range(&self) -> Range<u32> {
        0..self.usable_indices
    }

    pub(crate) fn indices_buffer_range(&self) -> Range<wgpu::BufferAddress> {
        self.buffer_indices.clone()
    }

    pub(crate) fn vertices_buffer_range(&self) -> Range<wgpu::BufferAddress> {
        self.buffer_vertices.clone()
    }

    pub(crate) fn layer_metadata_buffer_range(&self) -> Range<wgpu::BufferAddress> {
        self.buffer_layer_metadata.clone()
    }

    pub(crate) fn feature_metadata_buffer_range(&self) -> Range<wgpu::BufferAddress> {
        self.buffer_feature_metadata.clone()
    }
}

#[derive(Default)]
pub(crate) struct RingIndex {
    tree_index: BTreeMap<TileId, VecDeque<IndexEntry>>,
    linear_index: VecDeque<TileId>,
    /// Tiles that lost a layer since the last drain, oldest first.
    evicted_tiles: Vec<TileId>,
    evicted_layers: u64,
}

impl RingIndex {
    pub(crate) fn get_layers(&self, coords: TileId) -> Option<&VecDeque<IndexEntry>> {
        self.tree_index.get(&coords)
    }

    /// How many layers have been evicted to make room, ever.
    pub(crate) const fn evicted_layers(&self) -> u64 {
        self.evicted_layers
    }

    fn front(&self) -> Option<&IndexEntry> {
        let key = self.linear_index.front()?;
        self.tree_index.get(key)?.front()
    }

    fn back(&self) -> Option<&IndexEntry> {
        let key = self.linear_index.back()?;
        self.tree_index.get(key)?.back()
    }

    fn pop_front(&mut self) -> Option<IndexEntry> {
        let key = self.linear_index.pop_front()?;
        let btree_map::Entry::Occupied(mut layers) = self.tree_index.entry(key) else {
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
        self.linear_index.push_back(entry.coords);
        self.tree_index
            .entry(entry.coords)
            .or_default()
            .push_back(entry);
    }

    fn make_room(
        &mut self,
        new_data: wgpu::BufferAddress,
        typ: BackingBufferType,
        inner_size: wgpu::BufferAddress,
    ) -> Result<Range<wgpu::BufferAddress>, String> {
        if new_data > inner_size {
            return Err(format!(
                "{new_data} bytes do not fit in the {inner_size} byte {typ:?} buffer"
            ));
        }
        let mut available_gap = self.find_largest_gap(typ, inner_size);
        while new_data > available_gap.end - available_gap.start {
            // No more space, so evict the oldest layer.
            if self.pop_front().is_none() {
                return Err(format!("{typ:?} buffer is out of space and has no layers"));
            }
            available_gap = self.find_largest_gap(typ, inner_size);
        }
        Ok(available_gap.start..available_gap.start + new_data)
    }

    fn find_largest_gap(
        &self,
        typ: BackingBufferType,
        inner_size: wgpu::BufferAddress,
    ) -> Range<wgpu::BufferAddress> {
        let start = self.front().map(|first| match typ {
            BackingBufferType::Vertices => first.buffer_vertices.start,
            BackingBufferType::Indices => first.buffer_indices.start,
            BackingBufferType::Metadata => first.buffer_layer_metadata.start,
            BackingBufferType::FeatureMetadata => first.buffer_feature_metadata.start,
        });
        let end = self.back().map(|last| match typ {
            BackingBufferType::Vertices => last.buffer_vertices.end,
            BackingBufferType::Indices => last.buffer_indices.end,
            BackingBufferType::Metadata => last.buffer_layer_metadata.end,
            BackingBufferType::FeatureMetadata => last.buffer_feature_metadata.end,
        });
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
