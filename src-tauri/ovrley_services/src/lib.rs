//! Desktop services shared by the Tauri shell and the native app.
//!
//! Owns: `.oly` project archives, template and raster file operations, installed map styles and
//!       the tile cache, runtime paths, distribution detection, raster resources, widget
//!       preview snapshots, video thumbnails and timeline filmstrips.
//! Does not own: any host glue. Tauri commands, `tauri::State` and event emission stay in the
//!       Tauri crate; the native app reaches these services through `ovrley_app`.

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
pub mod video_filmstrip;
pub mod video_thumbnail;
pub mod widget_preview;
