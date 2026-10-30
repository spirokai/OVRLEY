//! The editor preview's output: a canvas over WebGL2.
//!
//! Tiles decode in workers through [`MapTileDecoder`] and reach the renderer as bytes, so the UI
//! thread only uploads. A view draws with what is resident, and missing tiles show the style's
//! background until they arrive, because the preview must never block. Pools start small and grow
//! when a view evicts its own tiles.

use super::extrusion::ExtrusionVertex;
use super::pipelines::{FillShaderFeatureMetadata, ShaderLayerMetadata, ShaderVertex, COLOR_FORMAT};
use super::pool::PoolEntries;
use super::style::{parse_style, ParsedStyle};
use super::tile::{prepare_tile, LayerKind, PreparedLayer, PreparedTile};
use super::{
    check_pool_sizes, upload_layer, validate_config, BufferPool, GpuMapConfig, GpuMapRenderer,
    GpuPoolSizes,
};
use bytemuck::Pod;
use crate::{view_tiles, MapCamera, MapRouteDraw, TileId};
use std::collections::HashSet;
use std::mem::size_of;

/// Renderer names of software WebGL. wgpu reports these adapters as `Other`, so a name is all there
/// is. A map drawn on the CPU would slow every other widget and show what export refuses to draw.
const SOFTWARE_RENDERERS: [&str; 5] = [
    "basic render",
    "swiftshader",
    "llvmpipe",
    "softpipe",
    "software",
];

pub(crate) struct Output {
    surface: wgpu::Surface<'static>,
    /// The tiles of the last asked view, to recognise when a view evicts itself.
    view: HashSet<TileId>,
}

impl GpuMapRenderer {
    /// Opens WebGL2 on `canvas`. Fails without a hardware GPU or WebGL2, which leaves the widget's
    /// placeholder showing. Other widgets are untouched.
    pub async fn for_canvas(
        config: GpuMapConfig,
        style: &[u8],
        canvas: web_sys::HtmlCanvasElement,
    ) -> Result<Self, String> {
        validate_config(&config)?;
        let parsed = parse_style(
            style,
            config.spec.layers,
            config.style_zoom,
            config.viewport.pixel_ratio(),
            &config.buildings,
        )?;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::GL;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| format!("No WebGL2 surface for the map: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("No WebGL2 adapter for the map: {error}"))?;
        let info = adapter.get_info();
        let name = info.name.to_lowercase();
        if SOFTWARE_RENDERERS.iter().any(|software| name.contains(software)) {
            return Err(format!(
                "The map preview needs a GPU; WebGL2 is software: {}",
                info.name
            ));
        }
        let adapter_name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let limits = adapter.limits();
        check_pool_sizes(config.pools, limits.max_buffer_size, &adapter_name)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: limits,
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(|error| format!("No WebGL2 device for the map: {error}"))?;
        let (width, height) = (config.viewport.width_px(), config.viewport.height_px());
        let mut surface_config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("The map canvas has no surface configuration")?;
        surface_config.format = COLOR_FORMAT;
        surface_config.view_formats = Vec::new();
        surface.configure(&device, &surface_config);
        let output = Output {
            surface,
            view: HashSet::new(),
        };
        Self::assemble(config, parsed, device, queue, adapter_name, output)
    }

    /// The tiles `camera` sees that are not on the GPU. Remembers the view for [`Self::upload_tile`].
    pub fn missing_tiles(&mut self, camera: &MapCamera) -> Vec<TileId> {
        let tiles = view_tiles(camera, self.config.viewport);
        self.output.view = tiles.iter().copied().collect();
        tiles
            .into_iter()
            .filter(|tile| !self.loaded.contains(tile))
            .collect()
    }

    /// Uploads a tile from [`MapTileDecoder::decode`]. Grows the pools when it can't fit or when it
    /// pushes out a tile the view still shows, and the tiles then come back through
    /// [`Self::missing_tiles`].
    pub fn upload_tile(&mut self, tile: TileId, bytes: &[u8]) -> Result<(), String> {
        self.check_gpu()?;
        let prepared = decode_prepared(bytes)?;
        let uploaded = prepared
            .ground
            .iter()
            .try_for_each(|layer| upload_layer(&mut self.ground_pool, &self.queue, tile, layer))
            .and_then(|()| {
                prepared.extrusions.iter().try_for_each(|layer| {
                    upload_layer(&mut self.extrusion_pool, &self.queue, tile, layer)
                })
            });
        if uploaded.is_ok() {
            self.loaded.insert(tile);
        }
        let mut evicted_from_view = false;
        for evicted in self
            .ground_pool
            .drain_evicted_tiles()
            .into_iter()
            .chain(self.extrusion_pool.drain_evicted_tiles())
        {
            self.loaded.remove(&evicted);
            evicted_from_view |= self.output.view.contains(&evicted);
        }
        if uploaded.is_err() || evicted_from_view {
            self.grow_pools()?;
        }
        self.check_gpu()
    }

    /// Doubles the pools and empties them. Fails once the device can't hold more.
    fn grow_pools(&mut self) -> Result<(), String> {
        let double = |pool: PoolEntries| PoolEntries {
            vertices: pool.vertices * 2,
            indices: pool.indices * 2,
            layers: pool.layers * 2,
        };
        let pools = GpuPoolSizes {
            ground: double(self.config.pools.ground),
            extrusion: double(self.config.pools.extrusion),
        };
        check_pool_sizes(
            pools,
            self.device.limits().max_buffer_size,
            &self.adapter_name,
        )?;
        self.ground_pool = BufferPool::new(&self.device, "ground", pools.ground);
        self.extrusion_pool = BufferPool::new(&self.device, "extrusion", pools.extrusion);
        self.config.pools = pools;
        self.loaded.clear();
        self.pool_clears += 1;
        Ok(())
    }

    /// Draws one frame and presents it to the canvas.
    pub fn present(
        &mut self,
        camera: &MapCamera,
        route: Option<&MapRouteDraw>,
    ) -> Result<(), String> {
        self.prepare_frame(camera, route)?;
        let frame = match self.output.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => return Err(format!("The map canvas has no texture: {other:?}")),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.record_main_pass(&mut encoder, &view);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        self.check_gpu()
    }
}

/// Turns tile bytes into uploadable geometry. Runs in a worker, with no GPU.
pub struct MapTileDecoder {
    style: ParsedStyle,
}

impl MapTileDecoder {
    pub fn new(config: &GpuMapConfig, style: &[u8]) -> Result<Self, String> {
        validate_config(config)?;
        Ok(Self {
            style: parse_style(
                style,
                config.spec.layers,
                config.style_zoom,
                config.viewport.pixel_ratio(),
                &config.buildings,
            )?,
        })
    }

    /// Bytes for [`GpuMapRenderer::upload_tile`]. A malformed tile is external data, so it fails
    /// this tile only.
    pub fn decode(&self, tile: &[u8]) -> Result<Vec<u8>, String> {
        Ok(encode_prepared(&prepare_tile(tile, &self.style)?))
    }
}

fn encode_prepared(tile: &PreparedTile) -> Vec<u8> {
    fn layer<V: Pod>(out: &mut Vec<u8>, layer: &PreparedLayer<V>) {
        let kind: u32 = match layer.kind {
            LayerKind::Fill => 0,
            LayerKind::Line => 1,
            LayerKind::Extrusion => 2,
        };
        let header = [
            layer.index,
            kind,
            layer.vertices.len() as u32,
            layer.indices.len() as u32,
            layer.feature_metadata.len() as u32,
        ];
        out.extend_from_slice(bytemuck::cast_slice(&header));
        out.extend_from_slice(bytemuck::bytes_of(&layer.layer_metadata));
        out.extend_from_slice(bytemuck::cast_slice(&layer.vertices));
        out.extend_from_slice(bytemuck::cast_slice(&layer.indices));
        out.extend_from_slice(bytemuck::cast_slice(&layer.feature_metadata));
    }
    let mut out = Vec::new();
    let counts = [tile.ground.len() as u32, tile.extrusions.len() as u32];
    out.extend_from_slice(bytemuck::cast_slice(&counts));
    for entry in &tile.ground {
        layer(&mut out, entry);
    }
    for entry in &tile.extrusions {
        layer(&mut out, entry);
    }
    out
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<T: Pod>(&mut self, count: usize) -> Result<Vec<T>, String> {
        let length = count * size_of::<T>();
        if length > self.0.len() {
            return Err("A decoded map tile is truncated".to_string());
        }
        let (bytes, rest) = self.0.split_at(length);
        self.0 = rest;
        // The bytes came from JavaScript, so they may not be aligned for `T`.
        Ok(bytemuck::pod_collect_to_vec(bytes))
    }

    fn layer<V: Pod>(&mut self) -> Result<PreparedLayer<V>, String> {
        let header: Vec<u32> = self.take(5)?;
        let kind = match header[1] {
            0 => LayerKind::Fill,
            1 => LayerKind::Line,
            2 => LayerKind::Extrusion,
            other => return Err(format!("A decoded map tile has layer kind {other}")),
        };
        let metadata: Vec<ShaderLayerMetadata> = self.take(1)?;
        Ok(PreparedLayer {
            index: header[0],
            kind,
            layer_metadata: metadata[0],
            vertices: self.take(header[2] as usize)?,
            indices: self.take(header[3] as usize)?,
            feature_metadata: self.take::<FillShaderFeatureMetadata>(header[4] as usize)?,
        })
    }
}

fn decode_prepared(bytes: &[u8]) -> Result<PreparedTile, String> {
    let mut reader = Reader(bytes);
    let counts: Vec<u32> = reader.take(2)?;
    let mut tile = PreparedTile::default();
    for _ in 0..counts[0] {
        tile.ground.push(reader.layer::<ShaderVertex>()?);
    }
    for _ in 0..counts[1] {
        tile.extrusions.push(reader.layer::<ExtrusionVertex>()?);
    }
    Ok(tile)
}
