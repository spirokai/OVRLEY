//! The editor preview's host: a canvas over WebGL2, fed by decode workers.
//!
//! Tiles decode in workers through [`MapTileDecoder`] and reach the renderer as bytes, so the UI
//! thread only uploads. A worker lays a tile's labels out with its own glyph service, built from the
//! bundled font files the shell serves, and sends the distance fields they draw with them, so the
//! atlas fills as export's does. A view draws with what is resident, and missing tiles show the
//! style's background until they arrive, because the preview must never block. Pools start small and
//! grow when a view evicts its own tiles.
//!
//! The bytes a worker sends are plain arrays with no `wgpu` on their side: the tile's layers, then its
//! labels as fixed-size records (symbols, glyph quads, lines, points and glyph keys) followed by the
//! keys' bitmaps. Natively only the tests read them.

use super::labels::{
    Face, GlyphBitmap, GlyphId, GlyphQuad, GlyphService, LabelSymbol, LineSymbol, Paint, SymbolKind,
};
use super::pipelines::{
    FillShaderFeatureMetadata, ShaderLayerMetadata, ShaderVertex, PATTERN_COUNT,
};
use super::style::ParsedStyle;
use super::tile::{prepare_tile, ExtrusionVertex, LayerKind, PreparedLayer, PreparedTile};
use super::GpuMapConfig;
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use {
    super::pipelines::COLOR_FORMAT,
    super::{adapter_name, GpuMapRenderer, GpuPoolSizes, Targets},
    crate::{frame_tiles, MapCamera, MapLabelDraw, MapRouteDraw, MapViewport, TileId},
    std::collections::HashSet,
    wasm_bindgen::{closure::Closure, JsCast, JsValue},
};

/// Decoded layer kinds from here up are fill patterns, by image.
const FILL_PATTERN_KINDS: u32 = 3;

/// The bitmap of every glyph a tile's labels draw, once each, as its worker sent them.
pub(crate) type SentGlyphs = Vec<(GlyphId, GlyphBitmap)>;

#[cfg(target_arch = "wasm32")]
pub(super) struct Output {
    surface: wgpu::Surface<'static>,
    /// The canvas context's `bufferData` override. It must outlive every buffer the context creates.
    _static_buffers: StaticBufferHint,
    surface_config: wgpu::SurfaceConfiguration,
    /// What the pools hold. They start at the default and grow.
    pools: GpuPoolSizes,
    /// The tiles of the last asked view, to recognise when a view evicts itself.
    view: HashSet<TileId>,
    /// Every glyph bitmap the decode workers sent, which the label atlas is filled from. Every tile
    /// carries those of its labels, so one is always here.
    glyphs: HashMap<GlyphId, Arc<GlyphBitmap>>,
}

#[cfg(target_arch = "wasm32")]
impl Output {
    fn bitmap(&self, id: GlyphId) -> Result<Arc<GlyphBitmap>, String> {
        self.glyphs
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("No decode worker sent glyph {:?}", id.character))
    }
}

#[cfg(target_arch = "wasm32")]
impl GpuMapRenderer {
    /// Opens WebGL2 on `canvas`. Fails without WebGL2, which leaves the widget's placeholder showing.
    /// Other widgets are untouched. Software WebGL2 draws the same map, only slower.
    pub async fn for_canvas(
        config: GpuMapConfig,
        style: &[u8],
        canvas: web_sys::HtmlCanvasElement,
    ) -> Result<Self, String> {
        let parsed = config.read_style(style)?;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::GL;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| format!("No WebGL2 surface for the map: {error}"))?;
        // The surface made the context; the device below makes the first buffers.
        let static_buffers = hint_static_buffers(&canvas)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("No WebGL2 adapter for the map: {error}"))?;
        let adapter_name = adapter_name(&adapter);
        let limits = adapter.limits();
        let pools = GpuPoolSizes::DEFAULT;
        pools.check(limits.max_buffer_size, &adapter_name)?;
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
            _static_buffers: static_buffers,
            surface_config,
            pools,
            view: HashSet::new(),
            glyphs: HashMap::new(),
        };
        Self::assemble(config, pools, parsed, (device, queue), adapter_name, output)
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
        let (prepared, glyphs) = decode_tile(bytes)?;
        for (id, bitmap) in glyphs {
            self.output
                .glyphs
                .entry(id)
                .or_insert_with(|| Arc::new(bitmap));
        }
        let fits = self.upload_tile_layers(tile, prepared).is_ok();
        let evicted = self.drain_evicted();
        if !fits || evicted.iter().any(|tile| self.output.view.contains(tile)) {
            self.grow_pools()?;
        }
        // A tile's labels go with its roads.
        if let Some(labels) = &mut self.labels {
            let view: Vec<TileId> = self.output.view.iter().copied().collect();
            let output = &self.output;
            labels.fill_atlas(&self.queue, &self.loaded, &view, &|id| output.bitmap(id))?;
        }
        self.check_gpu()
    }

    /// Doubles the pools and empties them. Fails once the device can't hold more.
    fn grow_pools(&mut self) -> Result<(), String> {
        let pools = self.output.pools.doubled();
        pools.check(self.device.limits().max_buffer_size, &self.adapter_name)?;
        self.ground_pool = super::BufferPool::new(&self.device, "ground", pools.ground);
        self.extrusion_pool = super::BufferPool::new(&self.device, "extrusion", pools.extrusion);
        self.output.pools = pools;
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
        if let Some(layers) = self
            .labels
            .as_mut()
            .filter(|layers| layers.atlas_is_partial())
        {
            let view = frame_tiles(camera, labels, self.config.viewport).tiles;
            let output = &self.output;
            layers.fill_atlas(&self.queue, &self.loaded, &view, &|id| output.bitmap(id))?;
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

#[cfg(target_arch = "wasm32")]
type StaticBufferHint = Closure<dyn Fn(u32, JsValue, u32) -> Result<(), JsValue>>;

/// Makes `canvas`'s WebGL2 context create buffers `STATIC_DRAW` where wgpu asks for `DYNAMIC_DRAW`.
/// wgpu's GL backend creates every buffer `DYNAMIC_DRAW` (gfx-rs/wgpu#3371), and its mappable ones are
/// emulated without a GL buffer on WebGL, so that is every buffer it creates. Chrome's ANGLE on Vulkan
/// keeps dynamic buffers in host memory, which a discrete GPU reads over the bus on every draw: frames
/// with buildings drew about 3-5x slower (`benchmarks/map-labels`, `--diagnose`). The hint only places
/// the buffer; wgpu still writes it with `bufferSubData`, which works on any buffer.
#[cfg(target_arch = "wasm32")]
fn hint_static_buffers(canvas: &web_sys::HtmlCanvasElement) -> Result<StaticBufferHint, String> {
    const DYNAMIC_DRAW: u32 = 0x88E8;
    const STATIC_DRAW: u32 = 0x88E4;
    let context = canvas
        .get_context("webgl2")
        .ok()
        .flatten()
        .ok_or("The map canvas has no WebGL2 context")?;
    let name = JsValue::from_str("bufferData");
    let buffer_data: js_sys::Function = js_sys::Reflect::get(&context, &name)
        .ok()
        .and_then(|value| value.dyn_into().ok())
        .ok_or("The map canvas's WebGL2 context has no bufferData")?;
    let buffer_data = buffer_data.bind0(&context);
    let hint: StaticBufferHint = Closure::new(move |target: u32, data: JsValue, usage: u32| {
        let usage = if usage == DYNAMIC_DRAW {
            STATIC_DRAW
        } else {
            usage
        };
        buffer_data
            .call3(&JsValue::UNDEFINED, &target.into(), &data, &usage.into())
            .map(drop)
    });
    js_sys::Reflect::set(&context, &name, hint.as_ref())
        .map_err(|_| "The map canvas's WebGL2 context refused the buffer hint".to_string())?;
    Ok(hint)
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
    #[cfg(target_arch = "wasm32")]
    pub fn font_files(config: &GpuMapConfig, style: &[u8]) -> Result<Vec<&'static str>, String> {
        Ok(if config.read_style(style)?.labels.is_empty() {
            Vec::new()
        } else {
            Face::ALL.iter().map(|face| face.file_name()).collect()
        })
    }

    /// `fonts` holds the bytes of [`Self::font_files`], in order.
    pub fn new(config: &GpuMapConfig, style: &[u8], fonts: Vec<Vec<u8>>) -> Result<Self, String> {
        let style = config.read_style(style)?;
        let glyphs = if style.labels.is_empty() {
            None
        } else {
            Some(GlyphService::from_files(fonts)?)
        };
        Ok(Self { style, glyphs })
    }

    /// Bytes for [`GpuMapRenderer::upload_tile`]: the tile's roads, buildings and labels. A malformed
    /// tile is external data, so it fails this tile only.
    pub fn decode(&self, tile: &[u8]) -> Result<Vec<u8>, String> {
        let prepared = prepare_tile(
            tile,
            &self.style,
            self.glyphs.as_ref().map(|glyphs| glyphs as _),
        )?;
        let mut out = Vec::new();
        encode_layers(&prepared, &mut out);
        // Labels exist only with a glyph service, which made every glyph they draw.
        let glyphs = self.glyphs.as_ref();
        encode_labels(
            &prepared.labels,
            &|id| match glyphs {
                Some(glyphs) => glyphs.bitmap(id.face, id.character),
                None => Err("A map decoder without labels drew a glyph".to_string()),
            },
            &mut out,
        )?;
        Ok(out)
    }
}

/// Reads plain arrays back out of bytes that crossed the worker boundary.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<T: Pod>(&mut self, count: usize) -> Result<Vec<T>, String> {
        let length = count
            .checked_mul(size_of::<T>())
            .filter(|length| *length <= self.0.len())
            .ok_or_else(|| "A decoded map tile is truncated".to_string())?;
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

/// The tile's layers: the counts of ground and extrusion layers, then each layer.
fn encode_layers(tile: &PreparedTile, out: &mut Vec<u8>) {
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
    let counts = [tile.ground.len() as u32, tile.extrusions.len() as u32];
    out.extend_from_slice(bytemuck::cast_slice(&counts));
    for entry in &tile.ground {
        layer(out, entry);
    }
    for entry in &tile.extrusions {
        layer(out, entry);
    }
}

/// What [`MapTileDecoder::decode`] wrote: the tile, its labels included, and the bitmap of every glyph
/// the labels draw. Every index in it is checked, since a reader that indexed out of range would panic
/// the page.
fn decode_tile(bytes: &[u8]) -> Result<(PreparedTile, SentGlyphs), String> {
    let mut reader = Reader(bytes);
    let counts: Vec<u32> = reader.take(2)?;
    let mut tile = PreparedTile::default();
    for _ in 0..counts[0] {
        tile.ground.push(reader.layer::<ShaderVertex>()?);
    }
    for _ in 0..counts[1] {
        tile.extrusions.push(reader.layer::<ExtrusionVertex>()?);
    }
    let glyphs;
    (tile.labels, glyphs) = decode_labels(&mut reader)?;
    if !reader.0.is_empty() {
        return Err(format!(
            "{} unread bytes after a decoded map tile",
            reader.0.len()
        ));
    }
    Ok((tile, glyphs))
}

/// A symbol with its quads and line as indices into the tile's arrays.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SymbolRecord {
    sort_key: f64,
    half_height: f64,
    anchor: [f32; 2],
    /// A point label's box. Zero for a line label.
    bounds: [f32; 4],
    fill: [u8; 4],
    halo: [u8; 4],
    halo_width: f32,
    halo_blur: f32,
    opacity: f32,
    font_scale: f32,
    layer: u32,
    first_quad: u32,
    quad_count: u32,
    /// 1 for a line label, 0 for a point label.
    is_line: u32,
    /// Index of the line label's line. Zero for a point label.
    line: u32,
    segment: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadRecord {
    rect: [f32; 4],
    along: f32,
    /// Index of the quad's glyph in the tile's glyph keys.
    glyph: u32,
    bold: u32,
}

/// A line's points in the tile's points.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineRecord {
    first_point: u32,
    point_count: u32,
}

/// A face's index in [`Face::ALL`], a character, and the size of its bitmap.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct KeyRecord {
    face: u32,
    character: u32,
    bitmap_width: u32,
    bitmap_height: u32,
}

/// Appends a tile's labels to `out` as arrays: the counts of symbols, quads, lines, points and glyph
/// keys, then each array, then the keys' bitmaps, each from `bitmap`. Symbols of one line share it,
/// and glyphs are written once however many quads draw them.
fn encode_labels(
    symbols: &[LabelSymbol],
    bitmap: &dyn Fn(GlyphId) -> Result<Arc<GlyphBitmap>, String>,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut keys: Vec<KeyRecord> = Vec::new();
    let mut bitmaps: Vec<Arc<GlyphBitmap>> = Vec::new();
    let mut key_of: HashMap<GlyphId, u32> = HashMap::new();
    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut lines: Vec<LineRecord> = Vec::new();
    let mut line_of: HashMap<*const [f32; 2], u32> = HashMap::new();
    let mut quads: Vec<QuadRecord> = Vec::new();
    let mut records: Vec<SymbolRecord> = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let first_quad = quads.len() as u32;
        for quad in &symbol.quads {
            let glyph = match key_of.get(&quad.glyph) {
                Some(index) => *index,
                None => {
                    let bitmap = bitmap(quad.glyph)?;
                    keys.push(KeyRecord {
                        face: Face::ALL
                            .iter()
                            .position(|face| *face == quad.glyph.face)
                            .expect("every face is in Face::ALL")
                            as u32,
                        character: u32::from(quad.glyph.character),
                        bitmap_width: bitmap.width,
                        bitmap_height: bitmap.height,
                    });
                    bitmaps.push(bitmap);
                    key_of.insert(quad.glyph, keys.len() as u32 - 1);
                    keys.len() as u32 - 1
                }
            };
            quads.push(QuadRecord {
                rect: quad.rect,
                along: quad.along,
                glyph,
                bold: u32::from(quad.bold),
            });
        }
        let mut record = SymbolRecord {
            sort_key: symbol.paint.sort_key,
            half_height: 0.0,
            anchor: symbol.anchor,
            bounds: [0.0; 4],
            fill: symbol.paint.fill,
            halo: symbol.paint.halo,
            halo_width: symbol.paint.halo_width,
            halo_blur: symbol.paint.halo_blur,
            opacity: symbol.paint.opacity,
            font_scale: symbol.font_scale,
            layer: symbol.layer,
            first_quad,
            quad_count: quads.len() as u32 - first_quad,
            is_line: 0,
            line: 0,
            segment: 0,
        };
        match &symbol.kind {
            SymbolKind::Point { bounds } => record.bounds = *bounds,
            SymbolKind::Line(line) => {
                record.is_line = 1;
                record.half_height = line.half_height;
                record.segment = line.segment;
                record.line = *line_of
                    .entry(Arc::as_ptr(&line.points).cast::<[f32; 2]>())
                    .or_insert_with(|| {
                        lines.push(LineRecord {
                            first_point: points.len() as u32,
                            point_count: line.points.len() as u32,
                        });
                        points.extend_from_slice(&line.points);
                        lines.len() as u32 - 1
                    });
            }
        }
        records.push(record);
    }

    let counts = [
        records.len() as u32,
        quads.len() as u32,
        lines.len() as u32,
        points.len() as u32,
        keys.len() as u32,
    ];
    out.extend_from_slice(bytemuck::cast_slice(&counts));
    out.extend_from_slice(bytemuck::cast_slice(&records));
    out.extend_from_slice(bytemuck::cast_slice(&quads));
    out.extend_from_slice(bytemuck::cast_slice(&lines));
    out.extend_from_slice(bytemuck::cast_slice(&points));
    out.extend_from_slice(bytemuck::cast_slice(&keys));
    for bitmap in bitmaps {
        out.extend_from_slice(&bitmap.data);
    }
    Ok(())
}

/// The labels [`encode_labels`] wrote, and the bitmap of every glyph they draw, once each.
fn decode_labels(reader: &mut Reader) -> Result<(Vec<LabelSymbol>, SentGlyphs), String> {
    let counts: Vec<u32> = reader.take(5)?;
    let [symbols, quads, lines, points, keys] = [0, 1, 2, 3, 4].map(|at| counts[at] as usize);
    let records: Vec<SymbolRecord> = reader.take(symbols)?;
    let quads: Vec<QuadRecord> = reader.take(quads)?;
    let lines: Vec<LineRecord> = reader.take(lines)?;
    let points: Vec<[f32; 2]> = reader.take(points)?;
    let keys: Vec<KeyRecord> = reader.take(keys)?;

    let glyphs = keys
        .iter()
        .map(|key| {
            let id = GlyphId {
                face: Face::ALL.get(key.face as usize).copied().ok_or_else(|| {
                    format!("A map glyph names face {}, which does not exist", key.face)
                })?,
                character: char::from_u32(key.character)
                    .ok_or_else(|| format!("{} is not a character", key.character))?,
            };
            let length = (key.bitmap_width as usize)
                .checked_mul(key.bitmap_height as usize)
                .ok_or("A map glyph bitmap is too large")?;
            let bitmap = GlyphBitmap {
                width: key.bitmap_width,
                height: key.bitmap_height,
                data: reader.take(length)?,
            };
            Ok((id, bitmap))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let lines = lines
        .iter()
        .map(|line| {
            let start = line.first_point as usize;
            points
                .get(start..start.saturating_add(line.point_count as usize))
                .map(Arc::<[[f32; 2]]>::from)
                .ok_or_else(|| "A map label line leaves the tile's points".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;

    let symbols = records
        .iter()
        .map(|record| {
            let first = record.first_quad as usize;
            let quads = quads
                .get(first..first.saturating_add(record.quad_count as usize))
                .ok_or("A map label's quads leave the tile's quads")?
                .iter()
                .map(|quad| {
                    Ok(GlyphQuad {
                        glyph: glyphs
                            .get(quad.glyph as usize)
                            .ok_or("A map label quad names a glyph the tile lacks")?
                            .0,
                        bold: quad.bold != 0,
                        rect: quad.rect,
                        along: quad.along,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let kind = match record.is_line {
                0 => SymbolKind::Point {
                    bounds: record.bounds,
                },
                1 => {
                    let points = lines
                        .get(record.line as usize)
                        .ok_or("A map label names a line the tile lacks")?;
                    // The anchor's segment ends at the next point.
                    if record.segment as usize + 1 >= points.len() {
                        return Err("A map label's segment is not on its line".to_string());
                    }
                    SymbolKind::Line(LineSymbol {
                        points: Arc::clone(points),
                        segment: record.segment,
                        half_height: record.half_height,
                    })
                }
                other => return Err(format!("A map label has kind {other}")),
            };
            Ok(LabelSymbol {
                layer: record.layer,
                anchor: record.anchor,
                quads,
                kind,
                font_scale: record.font_scale,
                paint: Paint {
                    sort_key: record.sort_key,
                    fill: record.fill,
                    halo: record.halo,
                    halo_width: record.halo_width,
                    halo_blur: record.halo_blur,
                    opacity: record.opacity,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((symbols, glyphs))
}

/// What a preview decode worker sends for a tile, as the UI thread reads it.
#[cfg(test)]
pub(crate) fn decoded(decoder: &MapTileDecoder, tile: &[u8]) -> (PreparedTile, SentGlyphs) {
    decode_tile(&decoder.decode(tile).unwrap()).unwrap()
}
