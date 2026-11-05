//! Where map bytes come from. Shared by every renderer, so it needs no features,
//! except the glyph accessor, which needs the native `gpu` feature.

use crate::{MapStyleCatalog, MapStyleId};
use std::sync::Arc;

/// Where map bytes come from. The shell owns the installed styles, the network and the disk cache.
pub trait MapDataSource: Send + Sync {
    /// An installed style's JSON as its file holds it, without any URL rewriting.
    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String>;
    /// The vector tile at `z/x/y`, with `z` at most 14.
    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String>;
    /// The bundled map fonts' glyphs, opened once by the shell. Tile threads call it directly.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    fn glyphs(&self) -> Arc<crate::gpu::labels::GlyphService>;
}

/// What a render's maps draw from: the installed styles, and the bytes behind them.
#[derive(Clone)]
pub struct MapSource {
    pub styles: MapStyleCatalog,
    pub data: Arc<dyn MapDataSource>,
}
