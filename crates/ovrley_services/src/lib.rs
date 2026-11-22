//! The app's desktop services.
//!
//! Owns: `.oly` project archives, template and raster file operations, installed map styles and
//!       the tile cache, runtime paths, distribution detection, the updater, raster resources,
//!       widget preview snapshots, video thumbnails and timeline filmstrips.
//! Does not own: the store, actions or any UI. `ovrley_app` calls these services.

pub mod distribution;
pub mod file_ops;
pub mod filmstrip_cache;
pub mod map_styles;
pub mod map_tile_service;
#[cfg(test)]
mod map_tile_service_tests;
pub mod project_file;
pub mod raster_resources;
pub mod runtime_paths;
pub mod updater;
pub mod video_filmstrip;
pub mod video_thumbnail;
pub mod widget_preview;
