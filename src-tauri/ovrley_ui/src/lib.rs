//! OVRLEY's Slint UI: the markup compiled from `ui/`, and the glue that binds its logic globals
//! (D2) to `ovrley_app`.

slint::include_modules!();

pub mod i18n_strings;
pub mod logic;

pub use logic::install_component_logic;
