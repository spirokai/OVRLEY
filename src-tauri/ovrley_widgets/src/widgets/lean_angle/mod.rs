//! Lean-angle sector rendering shared by native export and WASM preview.
pub mod format;
pub mod frame;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod validation;

#[cfg(test)]
mod tests;
