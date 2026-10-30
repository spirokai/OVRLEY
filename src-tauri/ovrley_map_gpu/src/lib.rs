//! A native, GPU, per-frame map renderer: vector tiles and 3D buildings drawn from cached MVT.
//!
//! The rendering method is ported from maplibre-rs (`maplibre/maplibre-rs` at `96b50a09`, MIT OR
//! Apache-2.0, see `LICENSE-MIT` beside the manifest). One render pass clears, draws every visible
//! tile's mask, draws layers × tiles in style order from slices of shared buffers, then buildings.
//! Its ECS, render graph, schedule and HTTP layer are not ported: this is plain structs and calls.
//!
//! Depending on this crate is the opt-in. `ovrley_map` stays free of `wgpu` and never depends on it.

mod extrusion;
mod pattern;
mod pipelines;
mod pool;
mod route;
mod sizing;
mod style;
mod tile;

pub use pool::PoolEntries;
pub use sizing::{GpuPoolSizes, MapTileSizer, DEFAULT_POOL_ENTRIES};
pub use style::Buildings;

use extrusion::ExtrusionVertex;
use ovrley_map::{
    MapCamera, MapDataSource, MapRouteDraw, MapStyleSpec, MapViewport, TileId, ViewState,
};
use pattern::{stencil_reference_value_3d, TileShape, TileViewPattern};
use pipelines::{BackgroundLayerMetadata, Pipelines, ShaderVertex, COLOR_FORMAT, DEPTH_FORMAT};
use pool::{BufferPool, IndexEntry, LayerGeometry};
use route::RouteLayer;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;
use style::{parse_style, ParsedStyle};
use tile::{prepare_tile, LayerKind, PreparedLayer, PreparedTile};
use wgpu::util::DeviceExt;

const MASK_VERTICES: u32 = 6;
/// Marks route samples. Tile masks use at most `3 + 14 * 4`, and never run after the route.
const ROUTE_STENCIL: u32 = 0xff;

/// Everything fixed for one renderer's lifetime.
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
    /// Sizes each geometry pool. See [`GpuPoolSizes::for_views`].
    pub pools: GpuPoolSizes,
    pub buildings: Buildings,
}

/// What loading one tile cost and produced.
#[derive(Clone, Debug)]
pub struct TileLoadReport {
    pub tile: TileId,
    /// Reading from the source, off the GPU thread.
    pub fetch_ms: f64,
    /// Decoding and tessellating, off the GPU thread.
    pub prepare_ms: f64,
    /// Uploading to the pools.
    pub upload_ms: f64,
    pub ground_layers: usize,
    pub ground_vertices: usize,
    pub extrusion_vertices: usize,
    /// Why the tile did not load, when it did not. Tile data is external, so this never fails a load.
    pub error: Option<String>,
}

/// What one frame drew.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub tiles: usize,
    pub layer_draws: usize,
    pub extrusion_draws: usize,
}

/// One layer of one tile in the frame's draw order.
#[derive(Clone)]
struct LayerItem {
    index: u32,
    kind: ItemKind,
}

#[derive(Clone)]
enum ItemKind {
    Background { instance: u32 },
    Ground(GroundItem),
}

#[derive(Clone)]
struct GroundItem {
    stencil_reference: u32,
    tile_range: Range<wgpu::BufferAddress>,
    entry: IndexEntry,
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
    Layer(LayerKind),
}

struct Targets {
    /// Present when multisampling. The resolve target is `color_view`.
    multisampled: Option<wgpu::TextureView>,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::TextureView,
    staging: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

/// Renders a map for one style on its own GPU device. See the crate docs for the method.
pub struct GpuMapRenderer {
    config: GpuMapConfig,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    style: ParsedStyle,
    pipelines: Pipelines,
    targets: Targets,
    pattern: TileViewPattern,
    ground_pool: BufferPool<ShaderVertex>,
    extrusion_pool: BufferPool<ExtrusionVertex>,
    background_buffer: Option<wgpu::Buffer>,
    route: Option<RouteLayer>,
    /// Tiles that finished loading. Eviction removes them again.
    loaded: HashSet<TileId>,
    pool_clears: u64,
    masks: Vec<TileShape>,
    layer_items: Vec<LayerItem>,
    extrusion_items: Vec<ExtrusionItem>,
    pixels: Vec<u8>,
    gpu_error: Arc<Mutex<Option<String>>>,
    last_stats: FrameStats,
}

impl GpuMapRenderer {
    /// Opens a GPU device and reads the style. Fails loudly on invalid configuration, without a
    /// real GPU, or when the device cannot hold the pools. Software adapters are refused: nothing
    /// has measured them, and a map frame costs far more on them than an export should wait for.
    pub fn new(config: GpuMapConfig, source: &dyn MapDataSource) -> Result<Self, String> {
        if config.sample_count != 1 && config.sample_count != 4 {
            return Err(format!(
                "Map sample count {} must be 1 or 4",
                config.sample_count
            ));
        }
        let has_empty_pool = [config.pools.ground, config.pools.extrusion]
            .iter()
            .any(|pool| pool.vertices == 0 || pool.indices == 0 || pool.layers == 0);
        if has_empty_pool {
            return Err("Map pools must not be empty".to_string());
        }
        if !config.style_zoom.is_finite() {
            return Err(format!(
                "Map style zoom {} is not finite",
                config.style_zoom
            ));
        }
        let style = parse_style(
            &source.style(config.spec.style)?,
            config.spec.layers,
            config.style_zoom,
            config.viewport.pixel_ratio(),
            &config.buildings,
        )?;

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|error| format!("No GPU adapter for the map renderer: {error}"))?;
        let info = adapter.get_info();
        let adapter_name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(format!(
                "The map renderer needs a GPU; the only adapter is software: {adapter_name}"
            ));
        }
        let max_buffer_size = adapter.limits().max_buffer_size;
        let largest_buffer = BufferPool::<ShaderVertex>::largest_buffer(config.pools.ground)
            .max(BufferPool::<ExtrusionVertex>::largest_buffer(
                config.pools.extrusion,
            ));
        if largest_buffer > max_buffer_size {
            return Err(format!(
                "This map view needs a {largest_buffer} byte GPU buffer and {adapter_name} allows \
                 {max_buffer_size}. Reduce the map's range or pitch."
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_buffer_size,
                ..wgpu::Limits::default()
            },
            ..wgpu::DeviceDescriptor::default()
        }))
        .map_err(|error| format!("No GPU device for the map renderer: {error}"))?;

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

        let pipelines = Pipelines::new(&device, config.sample_count);
        let (width, height) = (config.viewport.width_px(), config.viewport.height_px());
        let targets = Targets::new(&device, width, height, config.sample_count);
        let pattern = TileViewPattern::new(&device);
        let ground_pool = BufferPool::new(&device, "ground", config.pools.ground);
        let extrusion_pool = BufferPool::new(&device, "extrusion", config.pools.extrusion);
        let background_buffer = (!style.backgrounds.is_empty()).then(|| {
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
        });
        let pixels = vec![0; width as usize * height as usize * 4];

        let renderer = Self {
            config,
            device,
            queue,
            adapter_name,
            style,
            pipelines,
            targets,
            pattern,
            ground_pool,
            extrusion_pool,
            background_buffer,
            route: None,
            loaded: HashSet::new(),
            pool_clears: 0,
            masks: Vec::with_capacity(64),
            layer_items: Vec::with_capacity(1024),
            extrusion_items: Vec::with_capacity(64),
            pixels,
            gpu_error,
            last_stats: FrameStats::default(),
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
        self.ground_pool.index().evicted_layers() + self.extrusion_pool.index().evicted_layers()
    }

    /// What the last frame drew.
    pub const fn last_stats(&self) -> FrameStats {
        self.last_stats
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
            return Ok(());
        }
        self.load_or_fail(tiles, source)?;
        if self.has_all(tiles) {
            return Ok(());
        }
        self.ground_pool.clear();
        self.extrusion_pool.clear();
        self.loaded.clear();
        self.pool_clears += 1;
        self.load_or_fail(tiles, source)?;
        if self.has_all(tiles) {
            return Ok(());
        }
        Err(format!(
            "A view of {} map tiles does not fit its {} byte pools",
            tiles.len(),
            self.pool_bytes()
        ))
    }

    fn has_all(&self, tiles: &[TileId]) -> bool {
        tiles.iter().all(|tile| self.loaded.contains(tile))
    }

    fn load_or_fail(&mut self, tiles: &[TileId], source: &dyn MapDataSource) -> Result<(), String> {
        let reports = self.load_tiles(tiles, source)?;
        match reports
            .iter()
            .find_map(|report| report.error.as_ref().map(|error| (report, error)))
        {
            Some((report, error)) => Err(format!(
                "Map tile {}/{}/{} failed to load: {error}",
                report.tile.z, report.tile.x, report.tile.y
            )),
            None => Ok(()),
        }
    }

    /// How many times [`Self::load_view`] had to empty the pools. Each reloads one view.
    pub const fn pool_clears(&self) -> u64 {
        self.pool_clears
    }

    /// Decodes and tessellates `tiles` on worker threads and uploads them. Blocks until they are
    /// on the GPU. Tiles already loaded are skipped.
    pub fn load_tiles(
        &mut self,
        tiles: &[TileId],
        source: &dyn MapDataSource,
    ) -> Result<Vec<TileLoadReport>, String> {
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
        let style = &self.style;
        let mut reports = Vec::with_capacity(pending.len());

        std::thread::scope(|scope| {
            for _ in 0..workers {
                let (sender, next, pending) = (sender.clone(), &next, &pending);
                scope.spawn(move || {
                    while let Some(&tile) = pending.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let prepared = fetch_and_prepare(tile, style, source);
                        if sender.send(prepared).is_err() {
                            return;
                        }
                    }
                });
            }
            drop(sender);
            for (mut report, prepared) in receiver {
                if let Some(prepared) = prepared {
                    let started = Instant::now();
                    let uploaded = prepared
                        .ground
                        .iter()
                        .try_for_each(|layer| {
                            upload_layer(&mut self.ground_pool, &self.queue, report.tile, layer)
                        })
                        .and_then(|()| {
                            prepared.extrusions.iter().try_for_each(|layer| {
                                upload_layer(
                                    &mut self.extrusion_pool,
                                    &self.queue,
                                    report.tile,
                                    layer,
                                )
                            })
                        });
                    match uploaded {
                        Ok(()) => {
                            self.loaded.insert(report.tile);
                        }
                        Err(error) => {
                            log::warn!(
                                "Skipping map tile {}/{}/{}: {error}",
                                report.tile.z,
                                report.tile.x,
                                report.tile.y
                            );
                            report.error = Some(error);
                        }
                    }
                    report.upload_ms = started.elapsed().as_secs_f64() * 1000.0;
                }
                reports.push(report);
            }
        });

        // A tile that lost a layer no longer counts as loaded. Reloading it restores what is missing.
        for tile in self.ground_pool.drain_evicted_tiles() {
            self.loaded.remove(&tile);
        }
        for tile in self.extrusion_pool.drain_evicted_tiles() {
            self.loaded.remove(&tile);
        }
        self.queue.submit([]);
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("GPU upload did not finish: {error}"))?;
        self.check_gpu()?;
        Ok(reports)
    }

    /// Draws one frame and returns its RGBA pixels, with a blocking readback. A route draws above
    /// the roads and below the buildings. A new line is tessellated and uploaded on first use.
    pub fn render(
        &mut self,
        camera: &MapCamera,
        route: Option<&MapRouteDraw>,
    ) -> Result<&[u8], String> {
        self.draw_frame(camera, route)?;
        self.read_back(true)?;
        Ok(&self.pixels)
    }

    /// Draws one frame and maps its pixels without copying them out, as upstream's headless map
    /// does. Exists so gate timings compare directly with its baselines.
    pub fn render_without_copy(&mut self, camera: &MapCamera) -> Result<(), String> {
        self.draw_frame(camera, None)?;
        self.read_back(false)
    }

    fn check_gpu(&self) -> Result<(), String> {
        match lock_gpu_error(&self.gpu_error)?.clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn draw_frame(
        &mut self,
        camera: &MapCamera,
        route: Option<&MapRouteDraw>,
    ) -> Result<(), String> {
        self.check_gpu()?;
        let viewport = self.config.viewport;
        let view = ViewState::new(camera, viewport);
        let loaded = &self.loaded;
        self.pattern.generate(&view, |tile| loaded.contains(&tile));
        self.pattern.upload(
            &self.queue,
            &view,
            viewport.width_px() as f32,
            viewport.height_px() as f32,
        )?;
        self.prepare_route(&view, route)?;
        self.queue_items();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.record_main_pass(&mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.targets.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.targets.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.targets.padded_bytes_per_row),
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
                layer.clear();
            }
            return Ok(());
        };
        let device = &self.device;
        let layer = match &mut self.route {
            Some(layer) if layer.draws_line(&route.line) => layer,
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
            if let Some(entries) = self.ground_pool.index().get_layers(shape.coords) {
                for entry in entries {
                    self.layer_items.push(LayerItem {
                        index: entry.layer,
                        kind: ItemKind::Ground(GroundItem {
                            stencil_reference,
                            tile_range: tile_range.clone(),
                            entry: entry.clone(),
                        }),
                    });
                }
            }
            if let Some(entries) = self.extrusion_pool.index().get_layers(shape.coords) {
                for entry in entries {
                    self.extrusion_items.push(ExtrusionItem {
                        tile_range: tile_range.clone(),
                        entry: entry.clone(),
                    });
                }
            }
        }
        self.layer_items.sort_by_key(|item| item.index);
        self.extrusion_items.sort_by_key(|item| item.entry.layer);
        let backgrounds = self.style.backgrounds.len();
        self.last_stats = FrameStats {
            tiles: self.masks.len(),
            layer_draws: self.layer_items.len() - backgrounds,
            extrusion_draws: self.extrusion_items.len(),
        };
    }

    fn record_main_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let (view, resolve_target) = match &self.targets.multisampled {
            Some(multisampled) => (multisampled, Some(&self.targets.color_view)),
            None => (&self.targets.color_view, None),
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
            pass.set_vertex_buffer(0, self.pattern.buffer().slice(shape.buffer_range()));
            pass.draw(0..MASK_VERTICES, 0..1);
        }

        let mut bound = BoundPipeline::None;
        for item in &self.layer_items {
            match &item.kind {
                ItemKind::Background { instance } => {
                    if let Some(buffer) = &self.background_buffer {
                        if bound != BoundPipeline::Background {
                            pass.set_pipeline(&self.pipelines.background);
                            bound = BoundPipeline::Background;
                        }
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..MASK_VERTICES, *instance..*instance + 1);
                    }
                }
                ItemKind::Ground(ground) => {
                    let entry = &ground.entry;
                    if bound != BoundPipeline::Layer(entry.kind) {
                        pass.set_pipeline(self.pipelines.layer(entry.kind));
                        bound = BoundPipeline::Layer(entry.kind);
                    }
                    pass.set_stencil_reference(ground.stencil_reference);
                    pass.set_index_buffer(
                        self.ground_pool
                            .indices()
                            .slice(entry.indices_buffer_range()),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.set_vertex_buffer(
                        0,
                        self.ground_pool
                            .vertices()
                            .slice(entry.vertices_buffer_range()),
                    );
                    pass.set_vertex_buffer(
                        1,
                        self.pattern.buffer().slice(ground.tile_range.clone()),
                    );
                    pass.set_vertex_buffer(
                        2,
                        self.ground_pool
                            .metadata()
                            .slice(entry.layer_metadata_buffer_range()),
                    );
                    pass.set_vertex_buffer(
                        3,
                        self.ground_pool
                            .feature_metadata()
                            .slice(entry.feature_metadata_buffer_range()),
                    );
                    pass.draw_indexed(entry.indices_range(), 0, 0..1);
                }
            }
        }

        if let Some(route) = self.route.as_ref().filter(|route| route.has_draws()) {
            pass.set_pipeline(&self.pipelines.route);
            pass.set_stencil_reference(ROUTE_STENCIL);
            route.record(&mut pass);
        }

        if !self.extrusion_items.is_empty() {
            pass.set_pipeline(self.pipelines.layer(LayerKind::Extrusion));
            for item in &self.extrusion_items {
                let entry = &item.entry;
                pass.set_index_buffer(
                    self.extrusion_pool
                        .indices()
                        .slice(entry.indices_buffer_range()),
                    wgpu::IndexFormat::Uint32,
                );
                pass.set_vertex_buffer(
                    0,
                    self.extrusion_pool
                        .vertices()
                        .slice(entry.vertices_buffer_range()),
                );
                pass.set_vertex_buffer(1, self.pattern.buffer().slice(item.tile_range.clone()));
                pass.set_vertex_buffer(
                    2,
                    self.extrusion_pool
                        .feature_metadata()
                        .slice(entry.feature_metadata_buffer_range()),
                );
                pass.draw_indexed(entry.indices_range(), 0, 0..1);
            }
        }
    }

    /// Blocks until the frame is in the staging buffer, optionally copying it out.
    fn read_back(&mut self, copy_pixels: bool) -> Result<(), String> {
        let slice = self.targets.staging.slice(..);
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
            if copy_pixels {
                let row = self.config.viewport.width_px() as usize * 4;
                let stride = self.targets.padded_bytes_per_row as usize;
                for (to, from) in self
                    .pixels
                    .chunks_exact_mut(row)
                    .zip(mapped.chunks_exact(stride))
                {
                    to.copy_from_slice(&from[..row]);
                }
            }
        }
        self.targets.staging.unmap();
        self.check_gpu()
    }
}

impl Targets {
    fn new(device: &wgpu::Device, width: u32, height: u32, sample_count: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "map color",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let multisampled = (sample_count > 1).then(|| {
            texture(
                "map multisampled color",
                COLOR_FORMAT,
                sample_count,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let depth = texture(
            "map depth stencil",
            DEPTH_FORMAT,
            sample_count,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&wgpu::TextureViewDescriptor::default());
        let padded_bytes_per_row = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("map staging buffer"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            multisampled,
            color,
            color_view,
            depth,
            staging,
            padded_bytes_per_row,
        }
    }
}

/// A worker's job: one tile from source bytes to tessellated layers.
fn fetch_and_prepare(
    tile: TileId,
    style: &ParsedStyle,
    source: &dyn MapDataSource,
) -> (TileLoadReport, Option<PreparedTile>) {
    let mut report = TileLoadReport {
        tile,
        fetch_ms: 0.0,
        prepare_ms: 0.0,
        upload_ms: 0.0,
        ground_layers: 0,
        ground_vertices: 0,
        extrusion_vertices: 0,
        error: None,
    };
    let started = Instant::now();
    let bytes = source.tile(tile.z, tile.x as u32, tile.y as u32);
    report.fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let prepared = bytes.and_then(|bytes| prepare_tile(&bytes, style));
    report.prepare_ms = started.elapsed().as_secs_f64() * 1000.0;
    match prepared {
        Ok(prepared) => {
            report.ground_layers = prepared.ground.len();
            report.ground_vertices = prepared
                .ground
                .iter()
                .map(|layer| layer.vertices.len())
                .sum();
            report.extrusion_vertices = prepared
                .extrusions
                .iter()
                .map(|layer| layer.vertices.len())
                .sum();
            (report, Some(prepared))
        }
        Err(error) => {
            log::warn!(
                "Skipping map tile {}/{}/{}: {error}",
                tile.z,
                tile.x,
                tile.y
            );
            report.error = Some(error);
            (report, None)
        }
    }
}

/// Uploads one layer, unless it survived an earlier eviction of its tile. Fails when it cannot fit.
fn upload_layer<V: bytemuck::Pod>(
    pool: &mut BufferPool<V>,
    queue: &wgpu::Queue,
    tile: TileId,
    layer: &PreparedLayer<V>,
) -> Result<(), String> {
    let is_resident = pool
        .index()
        .get_layers(tile)
        .is_some_and(|entries| entries.iter().any(|entry| entry.layer == layer.index));
    if is_resident {
        return Ok(());
    }
    let geometry = LayerGeometry {
        vertices: &layer.vertices,
        indices: &layer.indices,
        feature_metadata: &layer.feature_metadata,
        layer_metadata: layer.layer_metadata,
    };
    pool.allocate_layer_geometry(queue, tile, layer.index, layer.kind, &geometry)
}

fn record_gpu_error(slot: &Mutex<Option<String>>, message: String) {
    if let Ok(mut slot) = slot.lock() {
        slot.get_or_insert(message);
    }
}

fn lock_gpu_error(
    slot: &Mutex<Option<String>>,
) -> Result<std::sync::MutexGuard<'_, Option<String>>, String> {
    slot.lock().map_err(|error| error.to_string())
}
