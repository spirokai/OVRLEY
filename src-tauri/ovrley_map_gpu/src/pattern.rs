//! The per-frame tile matrices, ported from maplibre-rs `render/tile_view_pattern/`.

use crate::pipelines::ShaderTileMetadata;
use cgmath::{Matrix4, Vector3};
use ovrley_map::{data_level, TileId, ViewPadding, ViewState, MAX_VIEW_TILES};
use std::collections::HashSet;
use std::mem::size_of;
use std::ops::Range;

/// MVT coordinate units along one tile edge.
pub(crate) const EXTENT: f64 = 4096.0;
/// How many tile metadata entries one frame can upload.
pub(crate) const PATTERN_CAPACITY: u64 = MAX_VIEW_TILES as u64;

/// Neighbouring tiles never share a value, and tiles from different levels never do.
pub(crate) const fn stencil_reference_value_3d(tile: TileId) -> u32 {
    const CASES: u32 = 4;
    let z = tile.z as u32;
    match (tile.x % 2 == 0, tile.y % 2 == 0) {
        (true, true) => z * CASES,
        (true, false) => 1 + z * CASES,
        (false, true) => 2 + z * CASES,
        (false, false) => 3 + z * CASES,
    }
}

/// Tile units to world pixels on this view. The z axis carries metres for extrusions; ground
/// layers sit at `z = 0` and never see it.
fn tile_transform(tile: TileId, view: &ViewState) -> Matrix4<f64> {
    let tile_scale = view.world_size() / 2_f64.powi(i32::from(tile.z));
    let translate = Matrix4::from_translation(Vector3::new(
        f64::from(tile.x) * tile_scale,
        f64::from(tile.y) * tile_scale,
        0.0,
    ));
    let normalize_and_scale = Matrix4::from_nonuniform_scale(
        tile_scale / EXTENT,
        tile_scale / EXTENT,
        view.metres_to_world(),
    );
    translate * normalize_and_scale
}

/// Where one tile is drawn this frame and which slice of the pattern buffer holds its matrix.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileShape {
    pub(crate) coords: TileId,
    pub(crate) buffer_index: u64,
}

impl TileShape {
    pub(crate) fn buffer_range(self) -> Range<wgpu::BufferAddress> {
        let stride = size_of::<ShaderTileMetadata>() as u64;
        self.buffer_index * stride..(self.buffer_index + 1) * stride
    }
}

/// The tile shapes of one frame, and their matrices on the GPU.
pub(crate) struct TileViewPattern {
    shapes: Vec<TileShape>,
    buffer: wgpu::Buffer,
    metadata: Vec<ShaderTileMetadata>,
    seen: HashSet<TileId>,
}

impl TileViewPattern {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            shapes: Vec::with_capacity(64),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile view pattern buffer"),
                size: PATTERN_CAPACITY * size_of::<ShaderTileMetadata>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            metadata: Vec::with_capacity(64),
            seen: HashSet::with_capacity(64),
        }
    }

    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub(crate) fn shapes(&self) -> &[TileShape] {
        &self.shapes
    }

    /// Chooses what to draw for each visible tile: the tile itself, or the nearest loaded parent.
    pub(crate) fn generate(&mut self, view: &ViewState, has_tile: impl Fn(TileId) -> bool) {
        self.shapes.clear();
        self.seen.clear();
        let Some(region) = view.create_view_region(data_level(view.zoom()), ViewPadding::Tight)
        else {
            return;
        };
        for coords in region.iter() {
            let mut source = Some(coords);
            while let Some(tile) = source.filter(|tile| !has_tile(*tile)) {
                source = tile.parent();
            }
            // Several tiles can fall back to one parent, which draws once.
            if let Some(tile) = source.filter(|tile| self.seen.insert(*tile)) {
                self.shapes.push(TileShape {
                    coords: tile,
                    buffer_index: self.shapes.len() as u64,
                });
            }
        }
    }

    /// Calculates each tile's matrix in `f64` and downcasts per tile, which keeps precision deep
    /// into the zoom range, then uploads them.
    pub(crate) fn upload(
        &mut self,
        queue: &wgpu::Queue,
        view: &ViewState,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<(), String> {
        if self.shapes.len() as u64 > PATTERN_CAPACITY {
            return Err(format!(
                "{} tiles in view exceed the pattern capacity of {PATTERN_CAPACITY}",
                self.shapes.len()
            ));
        }
        let view_projection = view.view_projection();
        let light_xy = view.light_xy();
        self.metadata.clear();
        for shape in &self.shapes {
            let transform = view_projection * tile_transform(shape.coords, view);
            let transform = transform
                .cast::<f32>()
                .ok_or("Tile matrix does not fit in f32")?;
            self.metadata.push(ShaderTileMetadata {
                transform: transform.into(),
                zoom_factor: 2_f64.powf(f64::from(shape.coords.z) - view.zoom()) as f32,
                viewport_width,
                viewport_height,
                light_xy,
            });
        }
        if !self.metadata.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.metadata));
        }
        Ok(())
    }
}
