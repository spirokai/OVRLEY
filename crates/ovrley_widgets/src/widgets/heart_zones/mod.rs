//! Five-zone heart-rate track shared by native export and WASM preview.
pub mod frame;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod validation;
pub mod zones;

#[cfg(test)]
mod tests;
