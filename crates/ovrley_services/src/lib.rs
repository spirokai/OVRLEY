//! The desktop app's services: everything that touches files, the network or FFmpeg on the app's
//! behalf, without any state or UI of its own. `ovrley_app` calls them.
//!
//! - [`project`]: `.oly` project archives, read, migrated, validated and written.
//! - [`rasters`]: the selected images raster widgets show, shared by history and render jobs.
//! - [`maps`]: the installed map styles, the map data renders draw from, and the editor preview's
//!   map frames; [`tiles`] fetches and caches the vector tiles they need.
//! - [`video`]: still frames of source videos, as thumbnails and timeline filmstrips.
//! - [`cache`]: the size-limited disk caches' shared limit policy, and the filmstrip cache.
//! - [`runtime`]: where the app runs from: its runtime roots, its distribution and the AppImage's
//!   menu entry.
//! - [`updater`]: checking for, downloading and installing a newer release.

pub mod cache;
pub mod maps;
pub mod project;
pub mod rasters;
pub mod runtime;
pub mod tiles;
pub mod updater;
pub mod video;
