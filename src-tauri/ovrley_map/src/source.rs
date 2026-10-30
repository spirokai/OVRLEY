//! Where map bytes come from. Shared by every renderer, so it needs no features.

use crate::MapStyle;

/// Where map bytes come from. The shell owns the network and disk cache.
pub trait MapDataSource: Send + Sync {
    /// The raw style JSON, without any URL rewriting.
    fn style(&self, style: MapStyle) -> Result<Vec<u8>, String>;
    /// The vector tile at `z/x/y`, with `z` at most 14.
    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String>;
}
