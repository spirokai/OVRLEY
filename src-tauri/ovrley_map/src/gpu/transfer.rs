//! Bytes that cross a thread boundary: the preview's decode workers hand tiles to the UI thread as
//! plain arrays, with no `wgpu` on their side.
//!
//! Only the preview's WASM module reads them, and tests natively.
use bytemuck::Pod;
use std::mem::size_of;

/// Reads plain arrays back out of bytes that crossed a boundary.
pub(crate) struct Reader<'a>(pub(crate) &'a [u8]);

impl Reader<'_> {
    pub(crate) fn take<T: Pod>(&mut self, count: usize) -> Result<Vec<T>, String> {
        let length = count
            .checked_mul(size_of::<T>())
            .filter(|length| *length <= self.0.len())
            .ok_or_else(|| "A decoded map tile is truncated".to_string())?;
        let (bytes, rest) = self.0.split_at(length);
        self.0 = rest;
        // The bytes came from JavaScript, so they may not be aligned for `T`.
        Ok(bytemuck::pod_collect_to_vec(bytes))
    }

    /// Fails when bytes are left over, which would mean the writer and reader disagree on the layout.
    pub(crate) fn finish(self) -> Result<(), String> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} unread bytes after a decoded map tile",
                self.0.len()
            ))
        }
    }
}
