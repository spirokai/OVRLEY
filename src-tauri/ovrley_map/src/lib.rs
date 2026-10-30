//! Map vocabulary shared by native and WASM, plus the native tile rasterizer.
//!
//! Without features this crate has no Skia and no ezu, so `ovrley_widgets` can
//! depend on it from WASM. The `raster` feature adds the MapLibre style adapter
//! and the ezu renderer, which only native hosts compile.

mod camera;
/// Web Mercator projection helpers.
pub mod mercator;
mod route;
mod source;
mod spec;
mod view_region;

#[cfg(feature = "raster")]
pub mod raster;
#[cfg(feature = "raster")]
pub mod style;

pub use camera::{
    ClipPoint, GroundProjection, MapCamera, MapViewport, ViewPadding, ViewState, MAX_VIEW_TILES,
};
pub use route::{MapRouteDraw, MapRouteLine, MapRoutePoint, MapRouteStyle};
pub use source::{MapDataSource, MapSprite};
pub use spec::{
    tile_pixels, MapLayers, MapStyle, MapStyleSpec, TileKey, MAX_RATIO_MILLI, MAX_TILE_ZOOM,
    MIN_RATIO_MILLI, TILE_SIZE, VECTOR_SOURCE,
};
pub use view_region::{data_level, view_tiles, TileId, ViewRegion};
