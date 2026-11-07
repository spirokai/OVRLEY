//! The editor preview's output: a canvas over WebGL2.
//!
//! Tiles decode in workers through [`MapTileDecoder`] and reach the renderer as bytes, so the UI
//! thread only uploads. A worker lays a tile's labels out with its own glyph service, built from the
//! bundled font files the shell serves, and sends the distance fields they draw with them, so the atlas
//! fills as export's does. A view draws with what is resident, and missing tiles show the style's background until they arrive, because the
//! preview must never block. Pools start small and grow when a view evicts its own tiles.

use super::extrusion::ExtrusionVertex;
use super::fill_patterns::PATTERN_COUNT;
use super::labels::transfer::{decode_labels, encode_labels, TileLabels};
use super::labels::{Face, GlyphBitmap, GlyphId, GlyphService};
use super::pipelines::{
    FillShaderFeatureMetadata, ShaderLayerMetadata, ShaderVertex, COLOR_FORMAT,
};
use super::pool::PoolEntries;
use super::style::{parse_style, ParsedStyle};
use super::tile::{prepare_tile, LayerKind, PreparedLayer, PreparedTile};
use super::transfer::Reader;
use super::{
    check_pool_sizes, upload_layer, validate_config, BufferPool, GpuMapConfig, GpuMapRenderer,
    GpuPoolSizes, Targets,
};
use crate::{frame_tiles, MapCamera, MapLabelDraw, MapRouteDraw, MapViewport, TileId};
use bytemuck::Pod;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Decoded layer kinds from here up are fill patterns, by image.
const FILL_PATTERN_KINDS: u32 = 3;

pub(crate) struct Output {
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    /// The tiles of the last asked view, to recognise when a view evicts itself.
    view: HashSet<TileId>,
    /// Every glyph bitmap the decode workers sent, which the label atlas is filled from.
    glyphs: HashMap<GlyphId, Arc<GlyphBitmap>>,
}

impl GpuMapRenderer {
    /// Opens WebGL2 on `canvas`. Fails without WebGL2, which leaves the widget's placeholder showing.
    /// Other widgets are untouched. Software WebGL2 draws the same map, only slower.
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
            surface_config,
            view: HashSet::new(),
            glyphs: HashMap::new(),
        };
        Self::assemble(config, parsed, device, queue, adapter_name, output)
    }

    /// Draws at a new size from the next frame. Only the output changes: tiles stay resident, and the
    /// map keeps its scale, so a size edit shows more or less of it.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.config.viewport = MapViewport::new(width, height, self.config.viewport.pixel_ratio())?;
        self.targets = Targets::new(&self.device, width, height, self.config.sample_count);
        self.output.surface_config.width = width;
        self.output.surface_config.height = height;
        self.output
            .surface
            .configure(&self.device, &self.output.surface_config);
        self.check_gpu()
    }

    /// The tiles a frame at `camera` with `labels` needs that are not on the GPU: those its camera
    /// sees, and with labels those of their placements' cameras. Remembers the view for
    /// [`Self::upload_tile`].
    pub fn missing_tiles(
        &mut self,
        camera: &MapCamera,
        labels: Option<&MapLabelDraw>,
    ) -> Vec<TileId> {
        let tiles = frame_tiles(camera, labels, self.config.viewport).tiles;
        self.output.view = tiles.iter().copied().collect();
        tiles
            .into_iter()
            .filter(|tile| !self.loaded.contains(tile))
            .collect()
    }

    /// Uploads a tile from [`MapTileDecoder::decode`], with its labels. Grows the pools when it can't
    /// fit or when it pushes out a tile the view still shows, and the tiles then come back through
    /// [`Self::missing_tiles`].
    pub fn upload_tile(&mut self, tile: TileId, bytes: &[u8]) -> Result<(), String> {
        self.check_gpu()?;
        let (prepared, labels) = decode_prepared(bytes)?;
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
        // A tile's labels go with its roads.
        if let Some(layers) = &mut self.labels {
            let loaded = &self.loaded;
            layers.retain_tiles(|tile| loaded.contains(&tile));
            if loaded.contains(&tile) {
                for (id, bitmap) in labels.glyphs {
                    self.output.glyphs.entry(id).or_insert_with(|| Arc::new(bitmap));
                }
                layers.insert_tile(tile, labels.symbols);
            }
            let view: Vec<TileId> = self.output.view.iter().copied().collect();
            let glyphs = &self.output.glyphs;
            layers.fill_atlas(&self.queue, &|id| sent_bitmap(glyphs, id), &view)?;
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

    /// Draws one frame and presents it to the canvas. `labels` says how its labels are placed and
    /// faded; the tiles of every camera in it should be loaded.
    pub fn present(
        &mut self,
        camera: &MapCamera,
        labels: Option<&MapLabelDraw>,
        route: Option<&MapRouteDraw>,
        corner_radius_px: f32,
    ) -> Result<(), String> {
        // An atlas refilled for another view may lack this one's glyphs, as in export.
        if let Some(layers) = self.labels.as_mut().filter(|layers| layers.atlas_is_partial()) {
            let view = frame_tiles(camera, labels, self.config.viewport).tiles;
            let glyphs = &self.output.glyphs;
            layers.fill_atlas(&self.queue, &|id| sent_bitmap(glyphs, id), &view)?;
        }
        self.prepare_frame(camera, labels, route, corner_radius_px)?;
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

/// A bitmap a decode worker sent. Every tile carries those of its labels, so one is always there.
fn sent_bitmap(
    glyphs: &HashMap<GlyphId, Arc<GlyphBitmap>>,
    id: GlyphId,
) -> Result<Arc<GlyphBitmap>, String> {
    glyphs
        .get(&id)
        .cloned()
        .ok_or_else(|| format!("No decode worker sent glyph {:?}", id.character))
}

/// Turns tile bytes into uploadable geometry and laid-out labels. Runs in a worker, with no GPU.
pub struct MapTileDecoder {
    style: ParsedStyle,
    /// `None` when the style draws no labels, so the font files are not needed.
    glyphs: Option<GlyphService>,
}

impl MapTileDecoder {
    /// The font files a decoder for this style takes, in order, which the shell serves by these names:
    /// every bundled face when the style draws labels, and none when it doesn't.
    pub fn font_files(config: &GpuMapConfig, style: &[u8]) -> Result<Vec<&'static str>, String> {
        Ok(if Self::parse(config, style)?.labels.is_empty() {
            Vec::new()
        } else {
            Face::ALL.iter().map(|face| face.file_name()).collect()
        })
    }

    /// `fonts` holds the bytes of [`Self::font_files`], in order.
    pub fn new(config: &GpuMapConfig, style: &[u8], fonts: Vec<Vec<u8>>) -> Result<Self, String> {
        let style = Self::parse(config, style)?;
        let glyphs = if style.labels.is_empty() {
            None
        } else {
            Some(GlyphService::from_files(fonts)?)
        };
        Ok(Self { style, glyphs })
    }

    fn parse(config: &GpuMapConfig, style: &[u8]) -> Result<ParsedStyle, String> {
        validate_config(config)?;
        parse_style(
            style,
            config.spec.layers,
            config.style_zoom,
            config.viewport.pixel_ratio(),
            &config.buildings,
        )
    }

    /// Bytes for [`GpuMapRenderer::upload_tile`]: the tile's roads, buildings and labels. A malformed
    /// tile is external data, so it fails this tile only.
    pub fn decode(&self, tile: &[u8]) -> Result<Vec<u8>, String> {
        let prepared = prepare_tile(
            tile,
            &self.style,
            self.glyphs.as_ref().map(|glyphs| glyphs as _),
        )?;
        let bitmap = |face, character| match &self.glyphs {
            Some(glyphs) => glyphs.bitmap(face, character),
            None => Err("A map decoder without labels drew a glyph".to_string()),
        };
        let mut out = encode_prepared(&prepared);
        encode_labels(&prepared.labels, &bitmap, &mut out)?;
        Ok(out)
    }
}

/// The tile's geometry; [`MapTileDecoder::decode`] appends its labels after it.
fn encode_prepared(tile: &PreparedTile) -> Vec<u8> {
    fn layer<V: Pod>(out: &mut Vec<u8>, layer: &PreparedLayer<V>) {
        let kind: u32 = match layer.kind {
            LayerKind::Fill => 0,
            LayerKind::Line => 1,
            LayerKind::Extrusion => 2,
            LayerKind::FillPattern(pattern) => FILL_PATTERN_KINDS + u32::from(pattern),
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

impl Reader<'_> {
    fn layer<V: Pod>(&mut self) -> Result<PreparedLayer<V>, String> {
        let header: Vec<u32> = self.take(5)?;
        let kind = match header[1] {
            0 => LayerKind::Fill,
            1 => LayerKind::Line,
            2 => LayerKind::Extrusion,
            other => LayerKind::FillPattern(
                other
                    .checked_sub(FILL_PATTERN_KINDS)
                    .and_then(|pattern| u8::try_from(pattern).ok())
                    .filter(|pattern| usize::from(*pattern) < PATTERN_COUNT)
                    .ok_or_else(|| format!("A decoded map tile has layer kind {other}"))?,
            ),
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

fn decode_prepared(bytes: &[u8]) -> Result<(PreparedTile, TileLabels), String> {
    let mut reader = Reader(bytes);
    let counts: Vec<u32> = reader.take(2)?;
    let mut tile = PreparedTile::default();
    for _ in 0..counts[0] {
        tile.ground.push(reader.layer::<ShaderVertex>()?);
    }
    for _ in 0..counts[1] {
        tile.extrusions.push(reader.layer::<ExtrusionVertex>()?);
    }
    let labels = decode_labels(&mut reader)?;
    reader.finish()?;
    Ok((tile, labels))
}
