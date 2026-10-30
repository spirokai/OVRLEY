//! Tile identities, view regions and the per-frame tile matrices, ported from maplibre-rs
//! `coords.rs` and `render/tile_view_pattern/`.

use crate::camera::{MapCamera, ViewPadding, ViewState};
use crate::pipelines::ShaderTileMetadata;
use cgmath::{Matrix4, Vector3};
use ovrley_map::TILE_SIZE;
use std::collections::HashSet;
use std::mem::size_of;
use std::ops::Range;

/// MVT coordinate units along one tile edge.
pub(crate) const EXTENT: f64 = 4096.0;
/// Tiles stop at this level upstream. Deeper views draw these tiles scaled.
const MAX_DATA_LEVEL: u8 = 14;
/// How many tile metadata entries one frame can upload.
pub(crate) const PATTERN_CAPACITY: u64 = 512;

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
    pub(crate) const fn is_on_grid(self) -> bool {
        let bounds = 1_i32 << self.z;
        self.x >= 0 && self.y >= 0 && self.x < bounds && self.y < bounds
    }

    fn parent(self) -> Option<Self> {
        (self.z > 0).then(|| Self::new(self.z - 1, self.x >> 1, self.y >> 1))
    }

    /// Neighbouring tiles never share a value, and tiles from different levels never do.
    pub(crate) const fn stencil_reference_value_3d(self) -> u32 {
        const CASES: u32 = 4;
        let z = self.z as u32;
        match (self.x % 2 == 0, self.y % 2 == 0) {
            (true, true) => z * CASES,
            (true, false) => 1 + z * CASES,
            (false, true) => 2 + z * CASES,
            (false, false) => 3 + z * CASES,
        }
    }

    /// Tile units to world pixels at `zoom`. The z axis carries metres for extrusions; ground
    /// layers sit at `z = 0` and never see it.
    fn transform_for_zoom(self, zoom: f64, metres_to_world: f64) -> Matrix4<f64> {
        let tile_scale = f64::from(TILE_SIZE) * 2_f64.powf(zoom - f64::from(self.z));
        let translate = Matrix4::from_translation(Vector3::new(
            f64::from(self.x) * tile_scale,
            f64::from(self.y) * tile_scale,
            0.0,
        ));
        let normalize_and_scale = Matrix4::from_nonuniform_scale(
            tile_scale / EXTENT,
            tile_scale / EXTENT,
            metres_to_world,
        );
        translate * normalize_and_scale
    }
}

/// The tile level drawn at a view zoom, capped so deeper views overzoom.
fn data_level(zoom: f64) -> u8 {
    (zoom.floor() as u8).min(MAX_DATA_LEVEL)
}

/// The on-grid tiles a camera sees on a viewport, for `GpuMapRenderer::load_tiles`.
pub fn view_tiles(
    camera: &MapCamera,
    width: u32,
    height: u32,
    padding: ViewPadding,
) -> Vec<TileId> {
    let view = ViewState::new(camera, width, height);
    view.create_view_region(data_level(view.zoom()), padding)
        .map(|region| region.iter().collect())
        .unwrap_or_default()
}

/// A bounding box of tiles at one level, with a padding.
pub(crate) struct ViewRegion {
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

    fn iter(&self) -> impl Iterator<Item = TileId> + '_ {
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
        let metres_to_world = view.metres_to_world();
        let light_xy = view.light_xy();
        self.metadata.clear();
        for shape in &self.shapes {
            let transform = view_projection
                * shape
                    .coords
                    .transform_for_zoom(view.zoom(), metres_to_world);
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
