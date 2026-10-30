//! Map vocabulary shared by native and WASM, plus the renderer.
//!
//! Without features this crate has no wgpu, so `ovrley_widgets` can depend on it from
//! WASM. The `gpu` feature adds the renderer that export and the preview's map module
//! draw with.

mod camera;
/// Web Mercator projection helpers.
pub mod mercator;
mod route;
mod source;
mod spec;
mod view_region;

#[cfg(feature = "gpu")]
pub mod gpu;

pub use camera::{ClipPoint, GroundProjection, MapCamera, MapViewport, ViewState, MAX_VIEW_TILES};
pub use route::{MapRouteDraw, MapRouteLine, MapRoutePoint, MapRouteStyle};
pub use source::MapDataSource;
pub use spec::{MapLayers, MapStyle, MapStyleSpec, MAX_TILE_ZOOM, TILE_SIZE, VECTOR_SOURCE};
pub use view_region::{data_level, view_tiles, TileId, ViewRegion};
