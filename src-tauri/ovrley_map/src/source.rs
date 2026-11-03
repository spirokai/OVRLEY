//! Where map bytes come from. Shared by every renderer, so it needs no features.

use crate::{MapStyleCatalog, MapStyleId};

/// Where map bytes come from. The shell owns the installed styles, the network and the disk cache.
pub trait MapDataSource: Send + Sync {
    /// The installed styles.
    fn styles(&self) -> &MapStyleCatalog;
    /// An installed style's JSON as its file holds it, without any URL rewriting.
    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String>;
    /// The vector tile at `z/x/y`, with `z` at most 14.
    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String>;
}
