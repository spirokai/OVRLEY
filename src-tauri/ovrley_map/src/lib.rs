//! Map vocabulary shared by native and WASM, plus the renderer and the style validator.
//!
//! Without features this crate has no wgpu, so `ovrley_widgets` can depend on it from
//! WASM. The `gpu` feature adds the renderer that export and the preview's map module
//! draw with. The `validate` feature adds what the shell registers installed styles with.

mod camera;
/// Web Mercator projection helpers.
pub mod mercator;
mod route;
mod source;
mod spec;
#[cfg(feature = "validate")]
mod style_file;
#[cfg(feature = "validate")]
pub mod style_spec;
mod view_region;

#[cfg(feature = "gpu")]
pub mod gpu;

pub use camera::{
    ClipPoint, GroundProjection, MapCamera, MapViewport, ViewState, LIGHT_XY, MAX_VIEW_TILES,
};
pub use route::{MapRouteDraw, MapRouteLine, MapRoutePoint, MapRouteStyle};
pub use source::{MapDataSource, MapSource};
pub use spec::{
    MapLayers, MapStyleBuildings, MapStyleCatalog, MapStyleId, MapStyleInfo, MapStyleSpec,
    BUILDING_COLOR_KEY, MAX_TILE_ZOOM, TILE_SIZE, VECTOR_SOURCE,
};
#[cfg(feature = "validate")]
pub use style_file::read_style_file;
pub use view_region::{data_level, view_tiles, TileId, ViewRegion};
