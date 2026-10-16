//! Widget contracts and Skia drawing shared by native export and browser WASM.
//! Hosts acquire resources; this crate performs no filesystem, IPC, or encoding work.
pub mod error;
pub mod interpolation;
pub mod rdp;
#[macro_use]
pub mod model;
pub mod activity;
pub mod preview;
pub mod preview_widget;
pub mod render;
pub mod widgets;
