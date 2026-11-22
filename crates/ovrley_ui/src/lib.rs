//! OVRLEY's Slint UI: the markup compiled from `ui/`, and the glue that binds its logic globals
//! (D2) to `ovrley_app`.

slint::include_modules!();

pub mod app;
pub mod features;
pub mod hooks;
pub mod logic;
pub mod platform;
#[cfg(debug_assertions)]
mod snapshot;

pub use logic::install_component_logic;
