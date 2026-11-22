//! Vertical altitude tape with live readout and vertical-speed trend, shared by native export
//! and WASM preview.
pub mod frame;
pub mod interaction;
pub mod layout;
pub mod preview;
pub mod validation;

#[cfg(test)]
mod tests;
