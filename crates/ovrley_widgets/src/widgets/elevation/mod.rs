//! Shared elevation profile preparation, drawing and editor interaction.
pub mod config;
pub mod geometry;
pub mod interaction;
pub(crate) mod label;
pub mod layout;
pub mod preview;
pub mod reduction;
pub mod sampling;
#[cfg(test)]
mod tests;
pub mod validation;
