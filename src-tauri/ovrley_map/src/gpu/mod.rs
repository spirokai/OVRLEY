//! A GPU, per-frame map renderer: vector tiles and 3D buildings drawn from cached MVT.
//!
//! The rendering method is ported from maplibre-rs (`maplibre/maplibre-rs` at `96b50a09`, MIT OR
//! Apache-2.0, see `LICENSE-MIT` beside this file). One render pass clears, draws every visible
//! tile's mask, draws layers × tiles in style order from slices of shared buffers, then buildings.
//! Its ECS, render graph, schedule and HTTP layer are not ported: this is plain structs and calls.
//!
//! Both hosts share the draw path, so they draw the same map. Export loads tiles on threads and reads
//! frames back, below. Its tile data is fetched and held by the job before its first frame, so a tile
//! that fails here fails the export rather than drawing a blank patch. The editor preview compiles
//! this crate to WASM, decodes tiles in workers and presents to a canvas over WebGL2 (`web`).
//!
//! The `gpu` feature is the opt-in: without it, `ovrley_map` stays free of `wgpu`.

pub mod labels;
mod pipelines;
mod pool;
mod style;
mod tile;
#[cfg(any(target_arch = "wasm32", test))]
mod web;

pub use pool::{GpuPoolSizes, PoolEntries};
#[cfg(not(target_arch = "wasm32"))]
pub use pool::{MapTileSizer, MeasuredTile};
#[cfg(target_arch = "wasm32")]
pub use web::MapTileDecoder;

use crate::{
    data_level, MapCamera, MapLabelDraw, MapRouteDraw, MapRouteLine, MapRoutePoint,
    MapStyleBuildings, MapStyleSpec, MapViewport, TileId, ViewState, MAX_VIEW_TILES, TILE_SIZE,
};
use cgmath::{Matrix4, Vector3};
use labels::LabelLayers;
use pipelines::{
    pattern_bind_groups, BackgroundLayerMetadata, FogInstance, Pipelines, RouteInstance,
    RouteVertex, ShaderTileMetadata, ShaderVertex, ShapeMaskInstance, COLOR_FORMAT, DEPTH_FORMAT,
    PATTERN_PERIOD_PX, WATER_STENCIL_BIT,
};
use pool::{BufferPool, IndexEntry, FEATURE_METADATA, INDICES, LAYER_METADATA, VERTICES};
use std::collections::HashSet;
use std::mem::size_of;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use style::{parse_style, ParsedStyle};
use tile::{ExtrusionVertex, LayerKind, PreparedTile, DEFAULT_TOLERANCE, TILE_UNITS};
#[cfg(target_arch = "wasm32")]
use web::Output;
use wgpu::util::DeviceExt;
#[cfg(not(target_arch = "wasm32"))]
use {
    crate::MapDataSource,
    labels::{GlyphBitmap, GlyphId},
    std::collections::BTreeSet,
    std::sync::atomic::{AtomicUsize, Ordering},
    std::sync::mpsc,
    tile::prepare_tile,
};

const MASK_VERTICES: u32 = 6;
/// Marks route samples. Tile masks use at most `3 + 14 * 4`, and never run after the route.
const ROUTE_STENCIL: u32 = 0xff;
/// Source layers whose features are water. Haze over water reads as murk rather than distance.
const WATER_SOURCE_LAYERS: [&str; 2] = ["water", "waterway"];
/// Where the haze starts and where it is full, as the map centre's share of the ground's depth: 0.5
/// is twice as far as the centre.
const FOG_START: f32 = 0.8;
const FOG_FULL: f32 = 0.2;
/// The haze's strength where it is full.
const FOG_STRENGTH: f32 = 0.6;
/// Route vertices per section. Sections share their end vertex, so round caps close the seam.
const ROUTE_SECTION_POINTS: usize = 256;
/// Route units along the world's edge: the density of tile units at the deepest tile level.
const ROUTE_UNITS: f64 = TILE_UNITS * 16384.0;
/// Round joins are built from arcs, whose detail lyon sets from the tolerance against this width.
/// The shader then scales normals to the real width, so this only chooses how round they look.
const ROUTE_TESSELLATION_WIDTH: f32 = 1.0;
/// How far past the frustum's footprint a route section still draws, as a share of the footprint.
const ROUTE_BOUNDS_MARGIN: f64 = 0.1;
/// Which side of the split a route instance keeps, as the fragment shader tests it.
const KEEP_COMPLETED: f32 = -1.0;
const KEEP_REMAINING: f32 = 1.0;

/// Everything fixed for one renderer's lifetime: what it draws, and on what output.
#[derive(Clone, Debug)]
pub struct GpuMapConfig {
    pub spec: MapStyleSpec,
    /// Output size and pixel ratio. A higher ratio draws the same map sharper: world size, tile
    /// scale and line widths multiply by it, while tile levels and style values stay on the view zoom.
    pub viewport: MapViewport,
    /// MSAA samples, 1 or 4.
    pub sample_count: u32,
    /// Zoom that style values are evaluated at when tiles load, as upstream does at upload.
    pub style_zoom: f64,
    pub buildings: MapStyleBuildings,
}

impl GpuMapConfig {
    /// Reads `style` as this renderer draws it. Fails on configuration no device could use.
    fn read_style(&self, style: &[u8]) -> Result<ParsedStyle, String> {
        if self.sample_count != 1 && self.sample_count != 4 {
            return Err(format!(
                "Map sample count {} must be 1 or 4",
                self.sample_count
            ));
        }
        if !self.style_zoom.is_finite() {
            return Err(format!("Map style zoom {} is not finite", self.style_zoom));
        }
        parse_style(
            style,
            self.spec.layers,
            self.style_zoom,
            self.viewport.pixel_ratio(),
            &self.buildings,
        )
    }
}

/// One layer of one tile in the frame's draw order.
struct LayerItem {
    index: u32,
    kind: ItemKind,
}

enum ItemKind {
    Background {
        instance: u32,
    },
    Ground {
        /// Water keeps the haze off what it draws.
        water: bool,
        stencil_reference: u32,
        tile_range: Range<wgpu::BufferAddress>,
        entry: IndexEntry,
    },
}

struct ExtrusionItem {
    tile_range: Range<wgpu::BufferAddress>,
    entry: IndexEntry,
}

/// Which pipeline a render pass has bound, to skip redundant binds as upstream's tracked pass does.
#[derive(Clone, Copy, Eq, PartialEq)]
enum BoundPipeline {
    None,
    Background,
    /// A ground layer's kind, and whether it is water.
    Layer(LayerKind, bool),
}

/// Targets every output draws through.
struct Targets {
    /// Present when multisampling. It resolves to the output's colour view.
    multisampled: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

impl Targets {
    fn new(device: &wgpu::Device, width: u32, height: u32, sample_count: u32) -> Self {
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            multisampled: (sample_count > 1)
                .then(|| texture("map multisampled color", COLOR_FORMAT)),
            depth: texture("map depth stencil", DEPTH_FORMAT),
        }
    }
}

/// Renders a map for one style on its own GPU device. See the module docs for the method.
pub struct GpuMapRenderer {
    config: GpuMapConfig,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    /// Shared with tile threads while they prepare tiles.
    style: Arc<ParsedStyle>,
    pipelines: Pipelines,
    /// One per bundled fill pattern.
    pattern_bind_groups: Vec<wgpu::BindGroup>,
    targets: Targets,
    output: Output,
    pattern: TileViewPattern,
    ground_pool: BufferPool<ShaderVertex>,
    extrusion_pool: BufferPool<ExtrusionVertex>,
    background_buffer: Option<wgpu::Buffer>,
    route: Option<RouteLayer>,
    /// Present when the style has label layers.
    labels: Option<LabelLayers>,
    shape_buffer: wgpu::Buffer,
    /// Style layers that draw water, which the haze passes over.
    water_layers: HashSet<u32>,
    /// Present when the style has a background, whose colour the haze takes.
    fog_buffer: Option<wgpu::Buffer>,
    /// Whether this frame's ground is in view, so its haze draws.
    fogged: bool,
    /// Whether this frame's corners are rounded, so its shape mask draws.
    rounded: bool,
    /// Tiles whose every layer is in the pools. Eviction removes them again.
    loaded: HashSet<TileId>,
    pool_clears: u64,
    masks: Vec<TileShape>,
    layer_items: Vec<LayerItem>,
    extrusion_items: Vec<ExtrusionItem>,
    gpu_error: Arc<Mutex<Option<String>>>,
}

impl GpuMapRenderer {
    /// Builds a renderer on an opened device.
    fn assemble(
        config: GpuMapConfig,
        pools: GpuPoolSizes,
        style: ParsedStyle,
        (device, queue): (wgpu::Device, wgpu::Queue),
        adapter_name: String,
        output: Output,
    ) -> Result<Self, String> {
        let gpu_error = Arc::new(Mutex::new(None));
        let lost_error = Arc::clone(&gpu_error);
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                record_gpu_error(&lost_error, format!("GPU device lost: {message}"));
            }
        });
        let uncaptured_error = Arc::clone(&gpu_error);
        device.on_uncaptured_error(Arc::new(move |error| {
            record_gpu_error(&uncaptured_error, format!("GPU error: {error}"));
        }));

        let draws_labels = !style.labels.is_empty();
        let pipelines = Pipelines::new(&device, config.sample_count, draws_labels);
        let pattern_bind_groups =
            pattern_bind_groups(&device, &queue, &pipelines.pattern_bind_group_layout);
        let (width, height) = (config.viewport.width_px(), config.viewport.height_px());
        let instance_buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let renderer = Self {
            targets: Targets::new(&device, width, height, config.sample_count),
            pattern: TileViewPattern::new(&device),
            ground_pool: BufferPool::new(&device, "ground", pools.ground),
            extrusion_pool: BufferPool::new(&device, "extrusion", pools.extrusion),
            background_buffer: (!style.backgrounds.is_empty()).then(|| {
                let metadata: Vec<_> = style
                    .backgrounds
                    .iter()
                    .map(|layer| BackgroundLayerMetadata { color: layer.color })
                    .collect();
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("background metadata buffer"),
                    contents: bytemuck::cast_slice(&metadata),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            }),
            shape_buffer: instance_buffer("shape mask buffer", size_of::<ShapeMaskInstance>()),
            fog_buffer: style
                .backgrounds
                .first()
                .map(|_| instance_buffer("fog buffer", size_of::<FogInstance>())),
            water_layers: style
                .vector_layers
                .iter()
                .filter(|layer| WATER_SOURCE_LAYERS.contains(&layer.source_layer.as_str()))
                .map(|layer| layer.index)
                .collect(),
            labels: draws_labels
                .then(|| LabelLayers::new(&device, &queue, &pipelines.pattern_bind_group_layout)),
            config,
            device,
            queue,
            adapter_name,
            style: Arc::new(style),
            pipelines,
            pattern_bind_groups,
            output,
            route: None,
            fogged: false,
            rounded: false,
            loaded: HashSet::new(),
            pool_clears: 0,
            masks: Vec::with_capacity(64),
            layer_items: Vec::with_capacity(1024),
            extrusion_items: Vec::with_capacity(64),
            gpu_error,
        };
        // An allocation the device refused is reported through the error callback.
        renderer.check_gpu()?;
        Ok(renderer)
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Bytes across both geometry pools.
    pub fn pool_bytes(&self) -> u64 {
        self.ground_pool.size_bytes() + self.extrusion_pool.size_bytes()
    }

    /// Layers evicted from the pools so far. Evictions while a view's tiles are loading mean the
    /// pools are too small for that view.
    pub fn evicted_layers(&self) -> u64 {
        self.ground_pool.evicted_layers() + self.extrusion_pool.evicted_layers()
    }

    /// How many times the pools were emptied to hold one view. Each reloads that view.
    pub const fn pool_clears(&self) -> u64 {
        self.pool_clears
    }

    /// How many times the label atlas filled up and was cleared. Many in a short export means it is too
    /// small for the route's labels.
    pub fn atlas_clears(&self) -> u64 {
        self.labels.as_ref().map_or(0, LabelLayers::atlas_clears)
    }

    fn check_gpu(&self) -> Result<(), String> {
        match self
            .gpu_error
            .lock()
            .map_err(|error| error.to_string())?
            .clone()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Puts a prepared tile on the GPU: its layers in the pools, and its labels. Fails when a layer
    /// cannot fit, and the tile is then not loaded.
    fn upload_tile_layers(&mut self, tile: TileId, prepared: PreparedTile) -> Result<(), String> {
        for layer in &prepared.ground {
            self.ground_pool.upload(&self.queue, tile, layer)?;
        }
        for layer in &prepared.extrusions {
            self.extrusion_pool.upload(&self.queue, tile, layer)?;
        }
        self.loaded.insert(tile);
        if let Some(labels) = &mut self.labels {
            labels.insert_tile(tile, prepared.labels);
        }
        Ok(())
    }

    /// Unloads the tiles that lost a layer to make room for others, and returns them. Loading them
    /// again restores what is missing.
    fn drain_evicted(&mut self) -> Vec<TileId> {
        let evicted: Vec<TileId> = self
            .ground_pool
            .drain_evicted_tiles()
            .chain(self.extrusion_pool.drain_evicted_tiles())
            .collect();
        for tile in &evicted {
            self.loaded.remove(tile);
        }
        evicted
    }

    /// Everything a frame needs before its pass is recorded. Corners round in here rather than in each
    /// host, so preview and export crop the map the same way.
    fn prepare_frame(
        &mut self,
        camera: &MapCamera,
        labels: Option<&MapLabelDraw>,
        route: Option<&MapRouteDraw>,
        corner_radius_px: f32,
    ) -> Result<(), String> {
        self.check_gpu()?;
        let viewport = self.config.viewport;
        self.rounded = corner_radius_px > 0.0;
        if self.rounded {
            self.queue.write_buffer(
                &self.shape_buffer,
                0,
                bytemuck::bytes_of(&ShapeMaskInstance {
                    half_size: [
                        viewport.width_px() as f32 / 2.0,
                        viewport.height_px() as f32 / 2.0,
                    ],
                    radius: corner_radius_px,
                }),
            );
        }
        let view = ViewState::new(camera, viewport);
        let loaded = &self.loaded;
        self.pattern.generate(&view, |tile| loaded.contains(&tile));
        self.pattern.upload(&self.queue, &view, viewport)?;
        self.fogged = false;
        if let (Some(buffer), Some(background), Some((centre_row, per_row))) = (
            &self.fog_buffer,
            self.style.backgrounds.first(),
            view.ground_depth_rows(),
        ) {
            self.queue.write_buffer(
                buffer,
                0,
                bytemuck::bytes_of(&FogInstance {
                    color: background.color,
                    depth_rows: [centre_row as f32, per_row as f32],
                    haze: [FOG_START, FOG_FULL, FOG_STRENGTH],
                }),
            );
            self.fogged = true;
        }
        self.prepare_route(&view, route)?;
        if let Some(layers) = &mut self.labels {
            layers.prepare(
                &self.device,
                &self.queue,
                &self.style.labels,
                camera,
                labels,
                viewport,
            )?;
        }
        self.queue_items();
        Ok(())
    }

    /// Uploads a line the renderer has not drawn yet, then this frame's route draws.
    fn prepare_route(
        &mut self,
        view: &ViewState,
        route: Option<&MapRouteDraw>,
    ) -> Result<(), String> {
        let Some(route) = route else {
            if let Some(layer) = &mut self.route {
                layer.draws.clear();
            }
            return Ok(());
        };
        let device = &self.device;
        let layer = match &mut self.route {
            Some(layer) if Arc::ptr_eq(&layer.line, &route.line) => layer,
            slot => slot.insert(RouteLayer::new(device, route.line.clone())),
        };
        layer.prepare(&self.queue, view, self.config.viewport, route)
    }

    /// Collects this frame's draws: every mask, then layers × tiles sorted by style layer.
    fn queue_items(&mut self) {
        self.masks.clear();
        self.layer_items.clear();
        self.extrusion_items.clear();
        for (instance, layer) in self.style.backgrounds.iter().enumerate() {
            self.layer_items.push(LayerItem {
                index: layer.index,
                kind: ItemKind::Background {
                    instance: instance as u32,
                },
            });
        }
        for shape in self.pattern.shapes() {
            self.masks.push(*shape);
            let tile_range = shape.buffer_range();
            let stencil_reference = stencil_reference_value_3d(shape.coords);
            for entry in self.ground_pool.layers(shape.coords).into_iter().flatten() {
                let water = self.water_layers.contains(&entry.layer);
                self.layer_items.push(LayerItem {
                    index: entry.layer,
                    kind: ItemKind::Ground {
                        water,
                        stencil_reference: if water {
                            stencil_reference | WATER_STENCIL_BIT
                        } else {
                            stencil_reference
                        },
                        tile_range: tile_range.clone(),
                        entry: entry.clone(),
                    },
                });
            }
            for entry in self
                .extrusion_pool
                .layers(shape.coords)
                .into_iter()
                .flatten()
            {
                self.extrusion_items.push(ExtrusionItem {
                    tile_range: tile_range.clone(),
                    entry: entry.clone(),
                });
            }
        }
        self.layer_items.sort_by_key(|item| item.index);
        self.extrusion_items.sort_by_key(|item| item.entry.layer);
    }

    fn record_main_pass(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let (view, resolve_target) = match &self.targets.multisampled {
            Some(multisampled) => (multisampled, Some(output)),
            None => (output, None),
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth,
                depth_ops: Some(wgpu::Operations {
                    // Upstream clears to 0.0, but its projection is not reversed.
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipelines.mask);
        for shape in &self.masks {
            pass.set_stencil_reference(stencil_reference_value_3d(shape.coords));
            pass.set_vertex_buffer(0, self.pattern.buffer.slice(shape.buffer_range()));
            pass.draw(0..MASK_VERTICES, 0..1);
        }

        let mut bound = BoundPipeline::None;
        for item in &self.layer_items {
            match &item.kind {
                ItemKind::Background { instance } => {
                    let Some(buffer) = &self.background_buffer else {
                        continue;
                    };
                    if bound != BoundPipeline::Background {
                        pass.set_pipeline(&self.pipelines.background);
                        bound = BoundPipeline::Background;
                    }
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..MASK_VERTICES, *instance..*instance + 1);
                }
                ItemKind::Ground {
                    water,
                    stencil_reference,
                    tile_range,
                    entry,
                } => {
                    if bound != BoundPipeline::Layer(entry.kind, *water) {
                        pass.set_pipeline(self.pipelines.layer(entry.kind, *water));
                        if let LayerKind::FillPattern(pattern) = entry.kind {
                            pass.set_bind_group(
                                0,
                                &self.pattern_bind_groups[usize::from(pattern)],
                                &[],
                            );
                        }
                        bound = BoundPipeline::Layer(entry.kind, *water);
                    }
                    let pool = &self.ground_pool;
                    pass.set_stencil_reference(*stencil_reference);
                    pass.set_index_buffer(pool.slice(entry, INDICES), wgpu::IndexFormat::Uint32);
                    pass.set_vertex_buffer(0, pool.slice(entry, VERTICES));
                    pass.set_vertex_buffer(1, self.pattern.buffer.slice(tile_range.clone()));
                    pass.set_vertex_buffer(2, pool.slice(entry, LAYER_METADATA));
                    pass.set_vertex_buffer(3, pool.slice(entry, FEATURE_METADATA));
                    pass.draw_indexed(entry.indices_range(), 0, 0..1);
                }
            }
        }

        if let Some(buffer) = self.fog_buffer.as_ref().filter(|_| self.fogged) {
            pass.set_pipeline(&self.pipelines.fog);
            pass.set_stencil_reference(0);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..MASK_VERTICES, 0..1);
        }

        // Labels lying on the ground draw under the route, and the opaque buildings drawn after cover them.
        let labels = self.labels.as_ref().zip(self.pipelines.labels.as_ref());
        if let Some((labels, pipeline)) = labels {
            labels.record(&mut pass, pipeline, true);
        }

        if let Some(route) = self.route.as_ref().filter(|route| !route.draws.is_empty()) {
            pass.set_pipeline(&self.pipelines.route);
            pass.set_stencil_reference(ROUTE_STENCIL);
            route.record(&mut pass);
        }

        if !self.extrusion_items.is_empty() {
            pass.set_pipeline(self.pipelines.layer(LayerKind::Extrusion, false));
            let pool = &self.extrusion_pool;
            for item in &self.extrusion_items {
                let entry = &item.entry;
                pass.set_index_buffer(pool.slice(entry, INDICES), wgpu::IndexFormat::Uint32);
                pass.set_vertex_buffer(0, pool.slice(entry, VERTICES));
                pass.set_vertex_buffer(1, self.pattern.buffer.slice(item.tile_range.clone()));
                pass.set_vertex_buffer(2, pool.slice(entry, FEATURE_METADATA));
                pass.draw_indexed(entry.indices_range(), 0, 0..1);
            }
        }

        // Upright labels draw over everything, the route and the buildings included.
        if let Some((labels, pipeline)) = labels {
            labels.record(&mut pass, pipeline, false);
        }

        if self.rounded {
            pass.set_pipeline(&self.pipelines.shape_mask);
            pass.set_vertex_buffer(0, self.shape_buffer.slice(..));
            pass.draw(0..MASK_VERTICES, 0..1);
        }
    }
}

/// Where export's frames go: a texture, read back to pixels.
#[cfg(not(target_arch = "wasm32"))]
struct Output {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl Output {
    fn new(device: &wgpu::Device, viewport: MapViewport) -> Self {
        let (width, height) = (viewport.width_px(), viewport.height_px());
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("map color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let padded_bytes_per_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("map staging buffer"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            color_view: color.create_view(&wgpu::TextureViewDescriptor::default()),
            color,
            staging,
            padded_bytes_per_row,
        }
    }
}

/// The glyph service's bitmap of a glyph, which the atlas is filled from.
#[cfg(not(target_arch = "wasm32"))]
fn service_bitmap(
    source: &dyn MapDataSource,
) -> impl Fn(GlyphId) -> Result<Arc<GlyphBitmap>, String> + '_ {
    move |id| source.glyphs().bitmap(id.face, id.character)
}

/// Export: tile loading across threads, and frames read back to pixels.
#[cfg(not(target_arch = "wasm32"))]
impl GpuMapRenderer {
    /// Opens a GPU device and reads the style. Fails loudly on invalid configuration, without any
    /// adapter, or when the device cannot hold the pools. Without a GPU, a software rasterizer draws
    /// the same map, only far slower: WARP on Windows, lavapipe on Linux when Mesa provides it.
    pub fn new(
        config: GpuMapConfig,
        pools: GpuPoolSizes,
        source: &dyn MapDataSource,
    ) -> Result<Self, String> {
        let style = config.read_style(&source.style(&config.spec.style)?)?;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let request = |force_fallback_adapter| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter,
                ..Default::default()
            }))
        };
        // A GPU when there is one. Some backends only offer their software rasterizer when asked.
        let adapter = request(false)
            .or_else(|_| request(true))
            .map_err(|error| format!("No adapter for the map renderer: {error}"))?;
        let adapter_name = adapter_name(&adapter);
        let max_buffer_size = adapter.limits().max_buffer_size;
        pools.check(max_buffer_size, &adapter_name)?;
        let device = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_buffer_size,
                ..wgpu::Limits::default()
            },
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| format!("No GPU device for the map renderer: {error}"))?;
        let output = Output::new(&device.0, config.viewport);
        Self::assemble(config, pools, style, device, adapter_name, output)
    }

    /// Puts every tile of one view on the GPU, or fails. The pools evict oldest first, so loading can
    /// evict a tile this view still shows, which would leave a blank patch. When it does, the pools
    /// start empty and take only this view, which always fits if the view fits at all. Costs a few
    /// lookups when the tiles are already resident.
    pub fn load_view(
        &mut self,
        tiles: &[TileId],
        source: &dyn MapDataSource,
    ) -> Result<(), String> {
        if self.has_all(tiles) {
            // An atlas refilled for another view may lack the glyphs of this one's resident tiles.
            return match &mut self.labels {
                Some(labels) if labels.atlas_is_partial() => {
                    labels.fill_atlas(&self.queue, &self.loaded, tiles, &service_bitmap(source))
                }
                _ => Ok(()),
            };
        }
        self.load_tiles(tiles, source)?;
        if !self.has_all(tiles) {
            self.ground_pool.clear();
            self.extrusion_pool.clear();
            self.loaded.clear();
            self.pool_clears += 1;
            self.load_tiles(tiles, source)?;
        }
        if self.has_all(tiles) {
            Ok(())
        } else {
            Err(format!(
                "A view of {} map tiles does not fit its {} byte pools",
                tiles.len(),
                self.pool_bytes()
            ))
        }
    }

    /// Puts the glyphs a job will use in the label atlas before its first frame. Glyphs that don't fit
    /// are left for the views that need them.
    pub fn preload_glyphs(
        &mut self,
        glyphs: &BTreeSet<GlyphId>,
        source: &dyn MapDataSource,
    ) -> Result<(), String> {
        match &mut self.labels {
            Some(labels) => labels.preload_atlas(&self.queue, &service_bitmap(source), glyphs),
            None => Ok(()),
        }
    }

    fn has_all(&self, tiles: &[TileId]) -> bool {
        tiles.iter().all(|tile| self.loaded.contains(tile))
    }

    /// Decodes and tessellates the `tiles` not yet loaded on worker threads and uploads them. Blocks
    /// until they are on the GPU. Fails at the first tile that doesn't load.
    fn load_tiles(&mut self, tiles: &[TileId], source: &dyn MapDataSource) -> Result<(), String> {
        self.check_gpu()?;
        if let Some(tile) = tiles.iter().find(|tile| !tile.is_on_grid()) {
            return Err(format!(
                "Map tile {}/{}/{} is off the tile grid",
                tile.z, tile.x, tile.y
            ));
        }
        let pending: Vec<TileId> = tiles
            .iter()
            .copied()
            .filter(|tile| !self.loaded.contains(tile))
            .collect();
        let next = AtomicUsize::new(0);
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(pending.len().max(1));
        let (sender, receiver) = mpsc::channel();
        let style = self.style.clone();
        let glyphs = source.glyphs();
        let mut failure = None;
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let (sender, next, pending, style, glyphs) =
                    (sender.clone(), &next, &pending, &*style, &*glyphs);
                scope.spawn(move || {
                    while let Some(&tile) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let prepared = source
                            .tile(tile.z, tile.x as u32, tile.y as u32)
                            .and_then(|bytes| prepare_tile(&bytes, style, Some(glyphs)));
                        // The receiver stops listening at the first failure.
                        if sender.send((tile, prepared)).is_err() {
                            return;
                        }
                    }
                });
            }
            drop(sender);
            for (tile, prepared) in receiver {
                let uploaded =
                    prepared.and_then(|prepared| self.upload_tile_layers(tile, prepared));
                if let Err(error) = uploaded {
                    failure = Some(format!(
                        "Map tile {}/{}/{} failed to load: {error}",
                        tile.z, tile.x, tile.y
                    ));
                    break;
                }
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        self.drain_evicted();
        if let Some(labels) = &mut self.labels {
            labels.fill_atlas(&self.queue, &self.loaded, tiles, &service_bitmap(source))?;
        }
        self.queue.submit([]);
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("GPU upload did not finish: {error}"))?;
        self.check_gpu()
    }

    /// Draws one frame into `pixels` as RGBA, with a blocking readback, sizing it on first use so a
    /// reused buffer costs no allocation. `labels` says how its labels are placed and faded, and the
    /// view tiles of every camera in it must be loaded. A route draws above the roads and the labels,
    /// and below the buildings. A new line is tessellated and uploaded on first use. Pixels outside
    /// corners of `corner_radius_px` are transparent. Performance: staging is copied straight into
    /// `pixels`, so do not add a buffer of the renderer's own in between.
    pub fn render(
        &mut self,
        camera: &MapCamera,
        labels: Option<&MapLabelDraw>,
        route: Option<&MapRouteDraw>,
        corner_radius_px: f32,
        pixels: &mut Vec<u8>,
    ) -> Result<(), String> {
        self.prepare_frame(camera, labels, route, corner_radius_px)?;
        let viewport = self.config.viewport;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.record_main_pass(&mut encoder, &self.output.color_view);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.output.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.output.padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: viewport.width_px(),
                height: viewport.height_px(),
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        self.read_back(pixels)
    }

    /// Blocks until the frame is in the staging buffer, and copies it out.
    fn read_back(&mut self, pixels: &mut Vec<u8>) -> Result<(), String> {
        let slice = self.output.staging.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // The receiver below outlives the poll that runs this.
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("GPU frame did not finish: {error}"))?;
        receiver
            .recv()
            .map_err(|error| format!("GPU readback was dropped: {error}"))?
            .map_err(|error| format!("GPU readback failed: {error}"))?;
        {
            let mapped = slice
                .get_mapped_range()
                .map_err(|error| format!("GPU readback range failed: {error}"))?;
            let viewport = self.config.viewport;
            let row = viewport.width_px() as usize * 4;
            let stride = self.output.padded_bytes_per_row as usize;
            pixels.resize(row * viewport.height_px() as usize, 0);
            for (to, from) in pixels
                .chunks_exact_mut(row)
                .zip(mapped.chunks_exact(stride))
            {
                to.copy_from_slice(&from[..row]);
            }
        }
        self.output.staging.unmap();
        self.check_gpu()
    }
}

/// `{name} ({backend}, {device type})`, as the logs and benchmarks name an adapter.
fn adapter_name(adapter: &wgpu::Adapter) -> String {
    let info = adapter.get_info();
    format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
}

fn record_gpu_error(slot: &Mutex<Option<String>>, message: String) {
    if let Ok(mut slot) = slot.lock() {
        slot.get_or_insert(message);
    }
}

/// Neighbouring tiles never share a value, and tiles from different levels never do.
const fn stencil_reference_value_3d(tile: TileId) -> u32 {
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
        tile_scale / TILE_UNITS,
        tile_scale / TILE_UNITS,
        view.metres_to_world(),
    );
    translate * normalize_and_scale
}

/// A tile's corner in style pixels at the view zoom, wrapped where every fill pattern repeats.
fn pattern_origin(tile: TileId, view: &ViewState) -> [f32; 2] {
    let tile_pixels = f64::from(TILE_SIZE) * 2_f64.powf(view.zoom() - f64::from(tile.z));
    [tile.x, tile.y]
        .map(|index| (f64::from(index) * tile_pixels).rem_euclid(PATTERN_PERIOD_PX) as f32)
}

/// Where one tile is drawn this frame and which slice of the pattern buffer holds its matrix.
#[derive(Clone, Copy, Debug)]
struct TileShape {
    coords: TileId,
    buffer_index: u64,
}

impl TileShape {
    fn buffer_range(self) -> Range<wgpu::BufferAddress> {
        let stride = size_of::<ShaderTileMetadata>() as u64;
        self.buffer_index * stride..(self.buffer_index + 1) * stride
    }
}

/// The tile shapes of one frame and their matrices on the GPU, ported from maplibre-rs
/// `render/tile_view_pattern/`.
struct TileViewPattern {
    shapes: Vec<TileShape>,
    buffer: wgpu::Buffer,
    metadata: Vec<ShaderTileMetadata>,
    seen: HashSet<TileId>,
}

impl TileViewPattern {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            shapes: Vec::with_capacity(64),
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("tile view pattern buffer"),
                size: MAX_VIEW_TILES as u64 * size_of::<ShaderTileMetadata>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            metadata: Vec::with_capacity(64),
            seen: HashSet::with_capacity(64),
        }
    }

    fn shapes(&self) -> &[TileShape] {
        &self.shapes
    }

    /// Chooses what to draw for each visible tile: the tile itself, or the nearest loaded parent.
    fn generate(&mut self, view: &ViewState, has_tile: impl Fn(TileId) -> bool) {
        self.shapes.clear();
        self.seen.clear();
        let Some(region) = view.create_view_region(data_level(view.zoom())) else {
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
    /// into the zoom range, then uploads them. A view region holds at most the buffer's tiles.
    fn upload(
        &mut self,
        queue: &wgpu::Queue,
        view: &ViewState,
        viewport: MapViewport,
    ) -> Result<(), String> {
        let view_projection = view.view_projection();
        let light_xy = view.light_xy();
        let centre_w = view.centre_clip_w() as f32;
        self.metadata.clear();
        for shape in &self.shapes {
            let transform = (view_projection * tile_transform(shape.coords, view))
                .cast::<f32>()
                .ok_or("Tile matrix does not fit in f32")?;
            self.metadata.push(ShaderTileMetadata {
                transform: transform.into(),
                zoom_factor: 2_f64.powf(f64::from(shape.coords.z) - view.zoom()) as f32,
                viewport_width: viewport.width_px() as f32,
                viewport_height: viewport.height_px() as f32,
                light_xy,
                pattern_origin: pattern_origin(shape.coords, view),
                centre_w,
            });
        }
        if !self.metadata.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.metadata));
        }
        Ok(())
    }
}

struct RouteSection {
    indices: Range<u32>,
    origin: (f64, f64),
    min: (f64, f64),
    max: (f64, f64),
}

/// One route on the GPU, with this frame's draws. It draws after every other ground layer and before
/// the buildings, so buildings hide it exactly as they hide roads. Unlike a road it comes from the
/// activity, not from tiles, and its colour changes with the split. So it keeps its own buffers: the
/// line is cut into sections, each with its own origin and matrix, which keeps `f32` precision on long
/// routes and lets far sections be culled per frame.
struct RouteLayer {
    line: Arc<MapRouteLine>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    sections: Vec<RouteSection>,
    frame_instances: Vec<RouteInstance>,
    /// Index range and instance of each draw, completed side first so it stays on top.
    draws: Vec<(Range<u32>, u64)>,
}

impl RouteLayer {
    /// Tessellates and uploads a line. GPS is external data, so a section that cannot tessellate
    /// is logged and skipped.
    fn new(device: &wgpu::Device, line: Arc<MapRouteLine>) -> Self {
        use lyon::tessellation::{
            BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
            VertexBuffers,
        };
        let mut buffers: VertexBuffers<RouteVertex, u32> = VertexBuffers::new();
        let mut tessellator = StrokeTessellator::new();
        let options = StrokeOptions::tolerance(DEFAULT_TOLERANCE)
            .with_line_width(ROUTE_TESSELLATION_WIDTH)
            .with_line_join(LineJoin::Round)
            .with_line_cap(LineCap::Round);
        let mut sections = Vec::new();
        let points = line.points();
        let last = points.len() - 1;
        let mut next_start = 0;
        while next_start < last {
            // Sections share their end vertex, so the line stays connected.
            let start = next_start;
            next_start = (start + ROUTE_SECTION_POINTS - 1).min(last);
            let section_points = &points[start..=next_start];
            let origin = (section_points[0].world_x, section_points[0].world_y);
            let first_index = buffers.indices.len() as u32;
            let result = tessellator.tessellate_path(
                &route_section_path(section_points, origin),
                &options,
                &mut BuffersBuilder::new(&mut buffers, |mut vertex: StrokeVertex| {
                    let position = vertex.position_on_path();
                    let normal = vertex.normal();
                    RouteVertex {
                        position: [position.x, position.y],
                        normal: [normal.x, normal.y],
                        along_metres: vertex.interpolated_attributes()[0],
                    }
                }),
            );
            if let Err(error) = result {
                log::warn!("Skipping a map route section: {error:?}");
                buffers.indices.truncate(first_index as usize);
                continue;
            }
            let (mut min, mut max) = (origin, origin);
            for point in section_points {
                min = (min.0.min(point.world_x), min.1.min(point.world_y));
                max = (max.0.max(point.world_x), max.1.max(point.world_y));
            }
            sections.push(RouteSection {
                indices: first_index..buffers.indices.len() as u32,
                origin,
                min,
                max,
            });
        }

        let init = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Self {
            vertices: init(
                "route vertex buffer",
                bytemuck::cast_slice(&buffers.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: init(
                "route index buffer",
                bytemuck::cast_slice(&buffers.indices),
                wgpu::BufferUsages::INDEX,
            ),
            instances: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("route instance buffer"),
                size: (sections.len().max(1) * 2 * size_of::<RouteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            line,
            frame_instances: Vec::with_capacity(sections.len() * 2),
            draws: Vec::with_capacity(sections.len() * 2),
            sections,
        }
    }

    /// Chooses this frame's sections and uploads their matrices, computed in `f64` per section.
    fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        view: &ViewState,
        viewport: MapViewport,
        route: &MapRouteDraw,
    ) -> Result<(), String> {
        self.draws.clear();
        self.frame_instances.clear();
        let Some((min, max)) = view.ground_bounds() else {
            return Ok(());
        };
        let margin = (
            (max.0 - min.0) * ROUTE_BOUNDS_MARGIN,
            (max.1 - min.1) * ROUTE_BOUNDS_MARGIN,
        );
        let view_projection = view.view_projection();
        let world_size = view.world_size();
        let visible: Vec<usize> = self
            .sections
            .iter()
            .enumerate()
            .filter(|(_, section)| {
                section.max.0 >= min.0 - margin.0
                    && section.min.0 <= max.0 + margin.0
                    && section.max.1 >= min.1 - margin.1
                    && section.min.1 <= max.1 + margin.1
            })
            .map(|(index, _)| index)
            .collect();
        let style = route.style;
        for (color, side) in [
            (style.completed_color, KEEP_COMPLETED),
            (style.remaining_color, KEEP_REMAINING),
        ] {
            for &index in &visible {
                let section = &self.sections[index];
                let transform = (view_projection
                    * Matrix4::from_translation(Vector3::new(
                        section.origin.0 * world_size,
                        section.origin.1 * world_size,
                        0.0,
                    ))
                    * Matrix4::from_scale(world_size / ROUTE_UNITS))
                .cast::<f32>()
                .ok_or("Route matrix does not fit in f32")?;
                self.draws
                    .push((section.indices.clone(), self.frame_instances.len() as u64));
                self.frame_instances.push(RouteInstance {
                    transform: transform.into(),
                    color,
                    viewport_width: viewport.width_px() as f32,
                    viewport_height: viewport.height_px() as f32,
                    line_width: style.width_px,
                    travelled_metres: route.travelled_metres,
                    keep_side: side,
                });
            }
        }
        if !self.frame_instances.is_empty() {
            queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(&self.frame_instances),
            );
        }
        Ok(())
    }

    /// Records the frame's draws. The pipeline must already be bound.
    fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        let stride = size_of::<RouteInstance>() as u64;
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        for (indices, instance) in &self.draws {
            pass.set_vertex_buffer(
                1,
                self.instances
                    .slice(instance * stride..(instance + 1) * stride),
            );
            pass.draw_indexed(indices.clone(), 0, 0..1);
        }
    }
}

/// One route section's stroke path relative to its origin, carrying travelled distance as an
/// attribute.
fn route_section_path(points: &[MapRoutePoint], origin: (f64, f64)) -> lyon::path::Path {
    let position = |point: &MapRoutePoint| {
        lyon::geom::point(
            ((point.world_x - origin.0) * ROUTE_UNITS) as f32,
            ((point.world_y - origin.1) * ROUTE_UNITS) as f32,
        )
    };
    let mut builder = lyon::path::Path::builder_with_attributes(1);
    builder.begin(position(&points[0]), &[points[0].along_metres]);
    for point in &points[1..] {
        builder.line_to(position(point), &[point.along_metres]);
    }
    builder.end(false);
    builder.build()
}
