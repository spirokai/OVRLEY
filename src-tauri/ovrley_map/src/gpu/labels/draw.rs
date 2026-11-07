//! The label renderer's GPU side: the glyph atlas, and each frame's glyph quads.
//!
//! Label tiles stay on the CPU, outside the geometry pools: placement reads them every time the camera
//! changes, and the GPU only ever sees the quads placement chose. The atlas holds the distance fields of
//! every glyph the resident tiles use. The vertex shader passes positions through, so projection
//! happens here, in `f64`, once per placed symbol.
//!
//! Both hosts fill the atlas the same way, from a bitmap lookup: export's is its glyph service, and the
//! preview's the bitmaps its decode workers sent with each tile.

use super::layer::LabelLayer;
use super::layout::{GlyphId, GlyphQuad, LabelSymbol, LineSymbol, SymbolKind};
use super::placement::{
    glyph_pose, label_source_tile, label_sources, place, GroundLabel, LabelView, PlacedSymbol,
    PlacementCache, PERSPECTIVE_CUTOFF,
};
use super::text::GlyphBitmap;
use crate::gpu::pipelines::LabelVertex;
use crate::{
    view_tiles, MapCamera, MapLabelDraw, MapViewport, TileId, ViewState, LABEL_PLACEMENTS,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

/// The atlas is square, and no larger than this however much the device allows.
const ATLAS_MAX_SIZE: u32 = 4096;
/// Texels between glyphs, so linear filtering never reads a neighbour.
const GLYPH_GAP: u32 = 1;
/// Quads the buffers hold when first made.
const FIRST_QUAD_CAPACITY: usize = 1024;
/// The corners of a quad, as two triangles.
const QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 1, 3];

/// Where a glyph's bitmap sits in the atlas, in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AtlasRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

/// A row of glyphs of similar height.
struct Shelf {
    y: u32,
    height: u32,
    used: u32,
}

/// Packs rectangles into rows: each goes on the shortest shelf it fits, or opens a new one.
struct ShelfPacker {
    size: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl ShelfPacker {
    fn new(size: u32) -> Self {
        Self {
            size,
            shelves: Vec::new(),
            next_y: 0,
        }
    }

    fn clear(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
    }

    /// The corner of a free `width` x `height` area, or `None` when the atlas is full.
    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (width, height) = (width + GLYPH_GAP, height + GLYPH_GAP);
        if width > self.size {
            return None;
        }
        let shelf = self
            .shelves
            .iter_mut()
            .filter(|shelf| shelf.height >= height && self.size - shelf.used >= width)
            .min_by_key(|shelf| shelf.height);
        if let Some(shelf) = shelf {
            let corner = (shelf.used, shelf.y);
            shelf.used += width;
            return Some(corner);
        }
        if self.next_y + height > self.size {
            return None;
        }
        self.shelves.push(Shelf {
            y: self.next_y,
            height,
            used: width,
        });
        self.next_y += height;
        Some((0, self.next_y - height))
    }
}

/// One `R8Unorm` texture holding glyph distance fields.
struct GlyphAtlas {
    texture: wgpu::Texture,
    size: u32,
    packer: ShelfPacker,
    rects: HashMap<GlyphId, AtlasRect>,
}

impl GlyphAtlas {
    fn new(device: &wgpu::Device) -> Self {
        let size = ATLAS_MAX_SIZE.min(device.limits().max_texture_dimension_2d);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("label glyph atlas"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            texture,
            size,
            packer: ShelfPacker::new(size),
            rects: HashMap::new(),
        }
    }

    /// Puts a glyph in, unless it is already there. `false` when the atlas has no room.
    fn insert(&mut self, queue: &wgpu::Queue, id: GlyphId, bitmap: &GlyphBitmap) -> bool {
        if self.rects.contains_key(&id) {
            return true;
        }
        let Some((x, y)) = self.packer.allocate(bitmap.width, bitmap.height) else {
            return false;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bitmap.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bitmap.width),
                rows_per_image: Some(bitmap.height),
            },
            wgpu::Extent3d {
                width: bitmap.width,
                height: bitmap.height,
                depth_or_array_layers: 1,
            },
        );
        self.rects.insert(
            id,
            AtlasRect {
                x: x as u16,
                y: y as u16,
                width: bitmap.width as u16,
                height: bitmap.height as u16,
            },
        );
        true
    }

    /// Forgets every glyph and blanks the texture, so no stale texel is read between new glyphs.
    fn clear(&mut self, queue: &wgpu::Queue) {
        self.packer.clear();
        self.rects.clear();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &vec![0; (self.size * self.size) as usize],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.size),
                rows_per_image: Some(self.size),
            },
            wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// What a frame's quads were made for. A frame with the same key draws the same quads.
#[derive(Clone, Debug, PartialEq)]
struct FrameKey {
    camera: MapCamera,
    labels: MapLabelDraw,
    size: (u32, u32),
    generation: u64,
}

/// The quads of one style layer, in the vertex buffer.
struct LayerDraw {
    indices: Range<u32>,
    /// Whether any of its labels has a halo to draw first.
    halo: bool,
    /// Whether its labels lie on the ground, which buildings can hide.
    ground: bool,
}

/// The resident label tiles, their atlas, and this frame's draws.
pub(crate) struct LabelLayers {
    tiles: HashMap<TileId, Vec<LabelSymbol>>,
    atlas: GlyphAtlas,
    bind_group: wgpu::BindGroup,
    /// Whether the atlas lacks glyphs of resident tiles, because it filled and was refilled for one view.
    atlas_partial: bool,
    /// Changes whenever the tiles or the atlas do, which a frame's quads depend on.
    generation: u64,
    /// Changes whenever the resident tiles do, which a placement depends on.
    residency: u64,
    /// Times the atlas filled and was cleared.
    atlas_clears: u64,
    placements: PlacementCache,
    frame: Option<FrameKey>,
    draws: Vec<LayerDraw>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    quad_capacity: usize,
    /// The halo pass first (1), then the fill pass (0): which one a draw makes is its instance.
    passes: wgpu::Buffer,
}

impl LabelLayers {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let atlas = GlyphAtlas::new(device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("label glyph sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view = atlas
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("label glyph atlas"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let passes = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("label pass buffer"),
            contents: bytemuck::cast_slice(&[1.0_f32, 0.0]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let (vertices, indices) = quad_buffers(device, queue, FIRST_QUAD_CAPACITY);
        Self {
            tiles: HashMap::new(),
            atlas,
            bind_group,
            atlas_partial: false,
            generation: 0,
            residency: 0,
            atlas_clears: 0,
            placements: PlacementCache::default(),
            frame: None,
            draws: Vec::new(),
            vertices,
            indices,
            quad_capacity: FIRST_QUAD_CAPACITY,
            passes,
        }
    }

    /// Keeps a tile's symbols. Their glyphs reach the atlas through [`Self::fill_atlas`].
    pub(crate) fn insert_tile(&mut self, tile: TileId, symbols: Vec<LabelSymbol>) {
        self.generation += 1;
        self.residency += 1;
        self.tiles.insert(tile, symbols);
    }

    /// Drops the tiles `keep` refuses.
    pub(crate) fn retain_tiles(&mut self, keep: impl Fn(TileId) -> bool) {
        let before = self.tiles.len();
        self.tiles.retain(|tile, _| keep(*tile));
        if self.tiles.len() != before {
            self.generation += 1;
            self.residency += 1;
        }
    }

    /// Puts the glyphs `view`'s labels draw in the atlas: those of its tiles, or of the resident tile
    /// above one that stands in while it loads. When it fills it is cleared and refilled from the
    /// resident tiles, the view's first. Fails only when one view's glyphs do not fit.
    pub(crate) fn fill_atlas(
        &mut self,
        queue: &wgpu::Queue,
        bitmap: &dyn Fn(GlyphId) -> Result<Arc<GlyphBitmap>, String>,
        view: &[TileId],
    ) -> Result<(), String> {
        let sources: BTreeSet<TileId> = view
            .iter()
            .filter_map(|tile| label_source_tile(*tile, &self.tiles))
            .collect();
        let needed = glyphs_of(sources.iter().map(|tile| &self.tiles[tile]));
        let mut inserted = false;
        let mut fits = true;
        for id in &needed {
            if self.atlas.rects.contains_key(id) {
                continue;
            }
            fits = self.atlas.insert(queue, *id, &*bitmap(*id)?);
            if !fits {
                break;
            }
            inserted = true;
        }
        if fits {
            self.generation += u64::from(inserted);
            return Ok(());
        }

        self.atlas.clear(queue);
        self.atlas_clears += 1;
        self.generation += 1;
        for id in &needed {
            if !self.atlas.insert(queue, *id, &*bitmap(*id)?) {
                return Err(format!(
                    "One map view's labels need more than the {0}x{0} glyph atlas holds",
                    self.atlas.size
                ));
            }
        }
        let resident = glyphs_of(self.tiles.values());
        self.atlas_partial = false;
        for id in resident.difference(&needed) {
            if !self.atlas.insert(queue, *id, &*bitmap(*id)?) {
                self.atlas_partial = true;
                break;
            }
        }
        Ok(())
    }

    /// Times the atlas filled up and was cleared and refilled.
    pub(crate) const fn atlas_clears(&self) -> u64 {
        self.atlas_clears
    }

    /// Whether the atlas may lack glyphs of tiles already resident, which a view of them must restore.
    pub(crate) const fn atlas_is_partial(&self) -> bool {
        self.atlas_partial
    }

    /// Puts as many of `glyphs` in the atlas as fit, without clearing it. Export knows every glyph
    /// it will use before its first frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn preload_atlas(
        &mut self,
        queue: &wgpu::Queue,
        bitmap: &dyn Fn(GlyphId) -> Result<Arc<GlyphBitmap>, String>,
        glyphs: &BTreeSet<GlyphId>,
    ) -> Result<(), String> {
        for id in glyphs {
            if self.atlas.rects.contains_key(id) {
                continue;
            }
            if !self.atlas.insert(queue, *id, &*bitmap(*id)?) {
                self.atlas_partial = true;
                break;
            }
        }
        self.generation += 1;
        Ok(())
    }

    /// Chooses this frame's symbols and writes their quads. Labels are placed at each camera of `labels` in
    /// turn, each placement on the one before it, and the frame fades between the last two by its blend: in both placements it is fully drawn, in one only
    /// it is drawn by the blend or its rest. Positions and sizes follow `camera`, so labels move with the
    /// map between placements. Without `labels` nothing is drawn. A frame like the last one writes nothing.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layers: &[LabelLayer],
        camera: &MapCamera,
        labels: Option<&MapLabelDraw>,
        viewport: MapViewport,
    ) -> Result<(), String> {
        let Some(labels) = labels else {
            self.draws.clear();
            self.frame = None;
            return Ok(());
        };
        let key = FrameKey {
            camera: *camera,
            labels: *labels,
            size: (viewport.width_px(), viewport.height_px()),
            generation: self.generation,
        };
        if self.frame.as_ref() == Some(&key) {
            return Ok(());
        }

        let [.., before, after] = self.placements(layers, &labels.cameras(), viewport);
        let blend = labels.blend();
        let in_before: HashSet<PlacedSymbol> = before.iter().copied().collect();
        let in_after: HashSet<PlacedSymbol> = after.iter().copied().collect();
        let mut shown: Vec<(PlacedSymbol, f32)> = Vec::with_capacity(before.len().max(after.len()));
        shown.extend(after.iter().map(|placed| {
            (
                *placed,
                if in_before.contains(placed) {
                    1.0
                } else {
                    blend
                },
            )
        }));
        shown.extend(
            before
                .iter()
                .filter(|placed| !in_after.contains(placed))
                .map(|placed| (*placed, 1.0 - blend)),
        );
        shown.retain(|(_, fade)| *fade > 0.0);
        // Placement runs from the top layer down, and drawing from the bottom up.
        shown.sort_by_key(|(placed, _)| placed.layer);

        let view = LabelView::new(
            &ViewState::new(camera, viewport),
            viewport.width_px(),
            viewport.height_px(),
        );
        let mut vertices: Vec<LabelVertex> = Vec::new();
        self.draws.clear();
        let mut layer_start = 0;
        let mut layer_halo = false;
        for (position, (placed, fade)) in shown.iter().enumerate() {
            let next_layer = shown.get(position + 1).map(|(next, _)| next.layer);
            let symbol = self.symbol(placed);
            let layer = layers
                .iter()
                .find(|layer| layer.index == placed.layer)
                .ok_or("A placed label has no layer")?;
            // A label placed for another camera can be out of reach of this one, and is left out.
            if self.push_symbol(&mut vertices, &view, (placed, symbol, layer), *fade)? {
                layer_halo |= symbol.paint.halo[3] > 0 && symbol.paint.halo_width > 0.0;
            }
            // A layer's symbols are all points or all lines.
            let ground = matches!(symbol.kind, SymbolKind::Line(_));
            if next_layer != Some(placed.layer) {
                let end = vertices.len() as u32 / 4 * QUAD_INDICES.len() as u32;
                if end > layer_start {
                    self.draws.push(LayerDraw {
                        indices: layer_start..end,
                        halo: layer_halo,
                        ground,
                    });
                }
                layer_start = end;
                layer_halo = false;
            }
        }

        self.reserve(device, queue, vertices.len() / 4);
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&vertices));
        }
        self.frame = Some(key);
        Ok(())
    }

    /// The symbols each of `cameras` places, oldest first, each on the placement before it, from the cache
    /// when it has them. Only the labels of the tiles a camera sees take part, and a renderer is asked to
    /// hold all of those.
    fn placements(
        &mut self,
        layers: &[LabelLayer],
        cameras: &[MapCamera; LABEL_PLACEMENTS],
        viewport: MapViewport,
    ) -> [Arc<Vec<PlacedSymbol>>; LABEL_PLACEMENTS] {
        let tiles = &self.tiles;
        self.placements.place_chain(
            cameras,
            (viewport.width_px(), viewport.height_px()),
            self.residency,
            |camera, previous| {
                let seen = label_sources(&view_tiles(camera, viewport), tiles);
                let view = LabelView::new(
                    &ViewState::new(camera, viewport),
                    viewport.width_px(),
                    viewport.height_px(),
                );
                place(&view, layers, &seen, previous)
            },
        )
    }

    /// Appends a placed symbol's quads as `view` sees it, at `fade` of its opacity. `false`, with
    /// nothing appended, when it cannot be drawn here: its anchor or a glyph is behind the camera, past
    /// the haze, or its line no longer holds it.
    fn push_symbol(
        &self,
        vertices: &mut Vec<LabelVertex>,
        view: &LabelView,
        (placed, symbol, layer): (&PlacedSymbol, &LabelSymbol, &LabelLayer),
        fade: f32,
    ) -> Result<bool, String> {
        let Some(([x, y], w)) = view.project(placed.tile, symbol, layer) else {
            return Ok(false);
        };
        let ratio = view.perspective_ratio(w);
        if ratio < PERSPECTIVE_CUTOFF {
            return Ok(false);
        }
        let start = vertices.len();
        let ground = match &symbol.kind {
            SymbolKind::Point { .. } => None,
            SymbolKind::Line(line) => Some((line, view.ground_label(w, symbol, layer))),
        };
        for quad in &symbol.quads {
            let rect = self.atlas.rects.get(&quad.glyph).ok_or_else(|| {
                format!("Glyph {:?} is not in the label atlas", quad.glyph.character)
            })?;
            let drawn = match &ground {
                None => {
                    push_quad(
                        vertices,
                        view,
                        symbol,
                        quad,
                        (x, y, w),
                        (ratio, fade),
                        *rect,
                    );
                    true
                }
                Some((line, label)) => push_ground_quad(
                    vertices,
                    view,
                    (placed, symbol, line, layer),
                    quad,
                    (label, fade),
                    *rect,
                ),
            };
            if !drawn {
                vertices.truncate(start);
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn symbol(&self, placed: &PlacedSymbol) -> &LabelSymbol {
        &self.tiles[&placed.tile][placed.symbol as usize]
    }

    /// Grows the buffers to hold `quads`.
    fn reserve(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, quads: usize) {
        if quads <= self.quad_capacity {
            return;
        }
        self.quad_capacity = quads.next_power_of_two();
        (self.vertices, self.indices) = quad_buffers(device, queue, self.quad_capacity);
    }

    /// Whether this frame draws labels lying on the ground (`ground`) or upright ones (`!ground`).
    pub(crate) fn has_draws(&self, ground: bool) -> bool {
        self.draws.iter().any(|draw| draw.ground == ground)
    }

    /// Records the frame's draws of labels lying on the ground (`ground`) or upright ones (`!ground`)
    /// with `pipeline`, layer by layer from the bottom of the style up, a layer's halos first. The two
    /// kinds are recorded at different points of the pass: ground labels under the route, upright ones
    /// over the buildings.
    pub(crate) fn record(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        ground: bool,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.passes.slice(..));
        for draw in self.draws.iter().filter(|draw| draw.ground == ground) {
            if draw.halo {
                pass.draw_indexed(draw.indices.clone(), 0, 0..1);
            }
            pass.draw_indexed(draw.indices.clone(), 0, 1..2);
        }
    }
}

/// The distinct glyphs of these tiles' symbols, in a fixed order.
fn glyphs_of<'a>(tiles: impl Iterator<Item = &'a Vec<LabelSymbol>>) -> BTreeSet<GlyphId> {
    tiles
        .flat_map(|symbols| symbols.iter().flat_map(|symbol| &symbol.quads))
        .map(|quad| quad.glyph)
        .collect()
}

/// Vertex and index buffers for `quads` quads, the indices written once.
fn quad_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    quads: usize,
) -> (wgpu::Buffer, wgpu::Buffer) {
    let vertices = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("label vertex buffer"),
        size: (quads * 4 * size_of::<LabelVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let indices = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("label index buffer"),
        size: (quads * QUAD_INDICES.len() * size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let data: Vec<u32> = (0..quads as u32)
        .flat_map(|quad| QUAD_INDICES.map(|index| quad * 4 + index))
        .collect();
    queue.write_buffer(&indices, 0, bytemuck::cast_slice(&data));
    (vertices, indices)
}

/// Appends an upright glyph's four corners. The anchor is `(x, y)` pixels from the top left, `w` its clip
/// w: corners are offset on screen from it, grown by the perspective `ratio`, then written in clip space
/// with the anchor's real w.
fn push_quad(
    vertices: &mut Vec<LabelVertex>,
    view: &LabelView,
    symbol: &LabelSymbol,
    quad: &GlyphQuad,
    (x, y, w): (f64, f64, f64),
    (ratio, fade): (f64, f32),
    atlas: AtlasRect,
) {
    let (width, height) = view.size();
    let scale = f64::from(symbol.font_scale) * ratio;
    let corner = |dx: f32, dy: f32| {
        let ndc_x = (x + f64::from(dx) * scale) / width * 2.0 - 1.0;
        let ndc_y = 1.0 - (y + f64::from(dy) * scale) / height * 2.0;
        [(ndc_x * w) as f32, (ndc_y * w) as f32, 0.0, w as f32]
    };
    let [left, top, right, bottom] = quad.rect;
    push_corners(
        vertices,
        symbol,
        (quad.bold, scale as f32, fade),
        [
            corner(left, top),
            corner(right, top),
            corner(left, bottom),
            corner(right, bottom),
        ],
        atlas,
    );
}

/// Appends a glyph of a line label, lying on the ground along the line. Its centre is walked out from the
/// anchor, and its corners are laid out on the ground there and projected, so the glyph foreshortens with
/// the map. `false`, with nothing appended, when the line no longer holds the label or a corner is behind
/// the camera, as it can be for a label placed at another camera.
fn push_ground_quad(
    vertices: &mut Vec<LabelVertex>,
    view: &LabelView,
    (placed, symbol, line, layer): (&PlacedSymbol, &LabelSymbol, &LineSymbol, &LabelLayer),
    quad: &GlyphQuad,
    (label, fade): (&GroundLabel, f32),
    atlas: AtlasRect,
) -> bool {
    let Some(pose) = glyph_pose(
        (symbol, line, layer),
        f64::from(quad.along) * label.units_per_glyph,
        placed.flipped,
    ) else {
        return false;
    };
    // The glyph's x runs along the text, and its y turns a quarter clockwise from it.
    let corner = |dx: f32, dy: f32| -> Option<[f32; 4]> {
        let (dx, dy) = (
            f64::from(dx) * label.units_per_glyph,
            f64::from(dy) * label.units_per_glyph,
        );
        let ground = [
            pose.point[0] + pose.direction[0] * dx - pose.direction[1] * dy,
            pose.point[1] + pose.direction[1] * dx + pose.direction[0] * dy,
        ];
        let clip = view.clip(placed.tile, ground)?;
        Some([clip.x as f32, clip.y as f32, clip.z as f32, clip.w as f32])
    };
    let [left, top, right, bottom] = quad.rect;
    let (Some(top_left), Some(top_right), Some(bottom_left), Some(bottom_right)) = (
        corner(left, top),
        corner(right, top),
        corner(left, bottom),
        corner(right, bottom),
    ) else {
        return false;
    };
    push_corners(
        vertices,
        symbol,
        (quad.bold, label.screen_scale as f32, fade),
        [top_left, top_right, bottom_left, bottom_right],
        atlas,
    );
    true
}

/// Appends a glyph's quad: four clip-space corners in the order top left, top right, bottom left and
/// bottom right, with the atlas texels they sample. `(bold, font_scale, fade)` is the glyph's thickening,
/// its size in output pixels per glyph unit on screen, and the share of the label's opacity it draws at.
fn push_corners(
    vertices: &mut Vec<LabelVertex>,
    symbol: &LabelSymbol,
    (bold, font_scale, fade): (bool, f32, f32),
    positions: [[f32; 4]; 4],
    atlas: AtlasRect,
) {
    let texel = |u: u16, v: u16| [atlas.x + u, atlas.y + v];
    let corners = [
        (positions[0], texel(0, 0)),
        (positions[1], texel(atlas.width, 0)),
        (positions[2], texel(0, atlas.height)),
        (positions[3], texel(atlas.width, atlas.height)),
    ];
    vertices.extend(corners.map(|(position, texel)| LabelVertex {
        position,
        texel,
        fill: symbol.paint.fill,
        halo: symbol.paint.halo,
        font_scale,
        halo_width: symbol.paint.halo_width,
        halo_blur: symbol.paint.halo_blur,
        opacity: symbol.paint.opacity * fade,
        bold: f32::from(u8::from(bold)),
    }));
}
