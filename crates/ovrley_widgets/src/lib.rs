//! Widget contracts and Skia drawing shared by export and the editor's preview.
//! Hosts acquire resources; this crate performs no filesystem or encoding work.
pub mod error;
pub mod interpolation;
pub mod rdp;
#[macro_use]
pub mod model;
pub mod activity;
pub mod host;
pub mod preview;
pub mod preview_instance;
pub mod preview_widget;
pub mod render;
pub mod widgets;
