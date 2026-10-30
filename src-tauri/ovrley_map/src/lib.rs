//! Map vocabulary shared by native and WASM, plus the native tile rasterizer.
//!
//! Without features this crate has no Skia and no ezu, so `ovrley_widgets` can
//! depend on it from WASM. The `raster` feature adds the MapLibre style adapter
//! and the ezu renderer, which only native hosts compile.

/// Web Mercator projection helpers.
pub mod mercator;
mod source;
mod spec;

#[cfg(feature = "raster")]
pub mod raster;
#[cfg(feature = "raster")]
pub mod style;

pub use source::{MapDataSource, MapSprite};
pub use spec::{
    tile_pixels, MapLayers, MapStyle, MapStyleSpec, TileKey, MAX_RATIO_MILLI, MAX_TILE_ZOOM,
    MIN_RATIO_MILLI, TILE_SIZE, VECTOR_SOURCE,
};
