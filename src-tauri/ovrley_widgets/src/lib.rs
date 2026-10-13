//! Widget contracts and Skia drawing shared by native export and browser WASM.
//! Hosts acquire resources; this crate performs no filesystem, IPC, or encoding work.
pub mod activity;
pub mod error;
pub mod interpolation;
pub mod model;
pub mod preview;
pub mod render;
pub mod widgets;
pub mod preview_widget;
