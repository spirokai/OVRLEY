//! OVRLEY's Slint UI: the markup compiled from `ui/`, the projections that write the store into
//! its globals, the callbacks that dispatch actions, and the platform glue.

slint::include_modules!();

pub mod app;
mod features;
mod logic;
pub mod platform;
mod projection;
#[cfg(debug_assertions)]
mod snapshot;

pub use logic::install_component_logic;
