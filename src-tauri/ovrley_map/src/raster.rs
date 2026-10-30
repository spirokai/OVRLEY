//! Rasterizes map tiles with ezu. All use of ezu's renderer stays in this module.

use crate::style::{translate, FONT_BOLD_URL, FONT_ITALIC_URL, FONT_REGULAR_URL, VECTOR_SOURCE};
use crate::{MapStyle, MapStyleSpec, TileKey, TILE_SIZE};
use ezu::renderer::{
    BindOptions, OutputFormat, RenderOptions, Renderer, SourceData, SourceKind, SourceSet,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

const FONT_REGULAR: &[u8] = include_bytes!("../../../assets/map-fonts/NotoSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../../assets/map-fonts/NotoSans-Bold.ttf");
const FONT_ITALIC: &[u8] = include_bytes!("../../../assets/map-fonts/NotoSans-Italic.ttf");

const RENDERER_CAPACITY: usize = 4;
const DECODED_TILE_CAPACITY: usize = 16;
/// Finished textures are cached by the hosts, so ezu keeps only a small intermediate cache.
const RENDER_CACHE_BYTES: usize = 4 * 1024 * 1024;

/// Where map bytes come from. The shell owns the network and disk cache.
pub trait MapDataSource: Send + Sync {
    /// The raw style JSON, without any URL rewriting.
    fn style(&self, style: MapStyle) -> Result<Vec<u8>, String>;
    fn sprite(&self, style: MapStyle) -> Result<MapSprite, String>;
    /// The vector tile at `z/x/y`, with `z` at most 14.
    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String>;
}

/// A sprite sheet: its index document and atlas image, drawn at pixel ratio 2.
pub struct MapSprite {
    pub index_json: String,
    pub atlas_png: Vec<u8>,
}

/// A finished tile texture.
pub struct RasterizedTile {
    /// Premultiplied RGBA, `size * size * 4` bytes.
    pub pixels: Vec<u8>,
    pub size: u32,
    pub opaque: bool,
}

/// Renders tiles for any spec. Shared across threads; renders run in parallel.
pub struct MapRasterizer {
    renderers: Mutex<Lru<MapStyleSpec, Arc<StyleRenderer>>>,
    decoded_tiles: Mutex<Lru<(u8, u32, u32), SourceData>>,
}

struct StyleRenderer {
    renderer: Renderer,
    neighbor_offsets: Vec<(i32, i32)>,
    opaque: bool,
}

impl MapRasterizer {
    pub fn new() -> Self {
        Self {
            renderers: Mutex::new(Lru::new(RENDERER_CAPACITY)),
            decoded_tiles: Mutex::new(Lru::new(DECODED_TILE_CAPACITY)),
        }
    }

    /// Renders one tile, fetching whatever it needs from `source`.
    pub fn render_tile(
        &self,
        key: TileKey,
        source: &dyn MapDataSource,
    ) -> Result<RasterizedTile, String> {
        self.render_tile_sized(key, TILE_SIZE, source)
    }

    /// [`Self::render_tile`] at another tile size, for the Phase 1 gate's 256 px comparison.
    #[doc(hidden)]
    pub fn render_tile_sized(
        &self,
        key: TileKey,
        tile_size: u32,
        source: &dyn MapDataSource,
    ) -> Result<RasterizedTile, String> {
        let style_renderer = self.style_renderer(key.spec(), source)?;
        let set = self.source_set(&style_renderer, key, source)?;
        let mut pixels = style_renderer
            .renderer
            .render_tile_with(
                key.z(),
                key.x(),
                key.y(),
                &set,
                RenderOptions {
                    format: OutputFormat::Rgba,
                    tile_size: Some(tile_size),
                    pixel_ratio: f32::from(key.ratio_milli()) / 1000.0,
                    ..RenderOptions::default()
                },
            )
            .map_err(|error| {
                format!(
                    "Failed to render map tile {}/{}/{}: {error}",
                    key.z(),
                    key.x(),
                    key.y()
                )
            })?;

        let size = (tile_size * u32::from(key.ratio_milli()) + 500) / 1000;
        if pixels.len() != (size * size * 4) as usize {
            return Err(format!(
                "Map tile render returned {} bytes for a {size} px tile",
                pixels.len()
            ));
        }
        premultiply(&mut pixels);
        Ok(RasterizedTile {
            pixels,
            size,
            opaque: style_renderer.opaque,
        })
    }

    fn style_renderer(
        &self,
        spec: MapStyleSpec,
        source: &dyn MapDataSource,
    ) -> Result<Arc<StyleRenderer>, String> {
        let mut renderers = lock(&self.renderers)?;
        if let Some(renderer) = renderers.get(&spec) {
            return Ok(renderer);
        }
        // Held across the build so workers starting together share one translation.
        let renderer = Arc::new(build_renderer(spec, source)?);
        renderers.insert(spec, Arc::clone(&renderer));
        Ok(renderer)
    }

    fn source_set(
        &self,
        style_renderer: &StyleRenderer,
        key: TileKey,
        source: &dyn MapDataSource,
    ) -> Result<SourceSet, String> {
        let renderer = &style_renderer.renderer;
        let (z, x, y) = (key.z(), key.x() as i32, key.y() as i32);
        let rows = 1_i32 << z;
        let mut set = SourceSet::new();
        for &(dx, dy) in std::iter::once(&(0, 0)).chain(&style_renderer.neighbor_offsets) {
            // There is no tile beyond the poles. Columns wrap in source_tile.
            if !(0..rows).contains(&(y + dy)) {
                continue;
            }
            let (data_z, data_x, data_y) = renderer
                .source_tile(VECTOR_SOURCE, z, x + dx, y + dy)
                .map_err(|error| error.to_string())?;
            let data = self.decoded_tile(renderer, data_z, data_x as u32, data_y as u32, source)?;
            set.bind(
                &data,
                &BindOptions {
                    coord: (dx, dy),
                    source_zoom: (data_z < z).then_some(data_z),
                    index: None,
                },
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(set)
    }

    fn decoded_tile(
        &self,
        renderer: &Renderer,
        z: u8,
        x: u32,
        y: u32,
        source: &dyn MapDataSource,
    ) -> Result<SourceData, String> {
        let id = (z, x, y);
        if let Some(data) = lock(&self.decoded_tiles)?.get(&id) {
            return Ok(data);
        }
        let bytes = source.tile(z, x, y)?;
        let data = renderer
            .load_source(VECTOR_SOURCE, &bytes)
            .map_err(|error| format!("Failed to decode map tile {z}/{x}/{y}: {error}"))?;
        lock(&self.decoded_tiles)?.insert(id, data.clone());
        Ok(data)
    }
}

impl Default for MapRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

fn build_renderer(spec: MapStyleSpec, source: &dyn MapDataSource) -> Result<StyleRenderer, String> {
    let style_name = spec.style.name();
    let translated = translate(&source.style(spec.style)?, spec.layers)?;
    for warning in &translated.warnings {
        log::warn!("Map style {style_name}: {warning}");
    }

    let mut renderer = Renderer::new(&translated.recipe)
        .map_err(|error| format!("Failed to build map style {style_name}: {error}"))?;
    renderer.set_cache_budget(Some(RENDER_CACHE_BYTES));
    bind_assets(&mut renderer, spec.style, source)?;

    let neighbor_offsets = renderer
        .requested_neighbor_offsets(VECTOR_SOURCE)
        .map_err(|error| error.to_string())?;
    Ok(StyleRenderer {
        renderer,
        neighbor_offsets,
        opaque: translated.opaque,
    })
}

/// Binds the fonts and sprite the recipe declares into ezu's persistent banks.
fn bind_assets(
    renderer: &mut Renderer,
    style: MapStyle,
    source: &dyn MapDataSource,
) -> Result<(), String> {
    let assets: Vec<(String, SourceKind, Option<String>)> = renderer
        .sources()
        .into_iter()
        .map(|info| {
            (
                info.name.to_string(),
                info.kind,
                info.url.map(|url| url.into_owned()),
            )
        })
        .collect();

    for (name, kind, url) in assets {
        let bound = match kind {
            SourceKind::Font => {
                let bytes = match url.as_deref() {
                    Some(FONT_REGULAR_URL) => FONT_REGULAR,
                    Some(FONT_BOLD_URL) => FONT_BOLD,
                    Some(FONT_ITALIC_URL) => FONT_ITALIC,
                    other => return Err(format!("Map style declares unknown font {other:?}")),
                };
                renderer.bind_source(&name, bytes.to_vec(), &BindOptions::default())
            }
            SourceKind::Sprite => {
                let sprite = source.sprite(style)?;
                renderer.bind_source(
                    &name,
                    sprite.atlas_png,
                    &BindOptions {
                        index: Some(sprite.index_json),
                        ..BindOptions::default()
                    },
                )
            }
            _ => continue,
        };
        bound.map_err(|error| format!("Failed to bind map asset {name}: {error}"))?;
    }
    Ok(())
}

/// ezu returns straight alpha; Skia and the WASM compositor want premultiplied.
fn premultiply(pixels: &mut [u8]) {
    let (pixels, _) = pixels.as_chunks_mut::<4>();
    for pixel in pixels {
        let alpha = u32::from(pixel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            let scaled = u32::from(*channel) * alpha + 128;
            *channel = ((scaled + (scaled >> 8)) >> 8) as u8;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    mutex.lock().map_err(|error| error.to_string())
}

/// A small least-recently-used map. Capacities here are single digits to low
/// tens, so a linear scan is cheaper than maintaining an index.
struct Lru<K, V> {
    capacity: usize,
    /// Least recently used first.
    entries: VecDeque<(K, V)>,
}

impl<K: Eq, V: Clone> Lru<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let index = self.entries.iter().position(|(entry, _)| entry == key)?;
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    fn insert(&mut self, key: K, value: V) {
        if let Some(index) = self.entries.iter().position(|(entry, _)| *entry == key) {
            self.entries.remove(index);
        } else if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
    }
}
