//! Elevation profile widget.
//!
//! The elevation widget maps distance progress to x coordinates and elevation
//! to y coordinates, pre-renders the remaining profile/area, then draws the
//! completed profile, marker, and optional labels on each frame.
//!
//! Module ownership:
//! - `normalize` — plot option normalization and label-style defaults.
//! - `prepare` — sample extraction, geometry fitting, and static-layer caching.
//! - `frame_state` — per-frame marker coordinate and elevation value computation.
//! - `reduction` — smoothing, downsampling, projection, and RDP simplification.
//! - `draw` — per-frame canvas composition of layers, area, polyline, marker,
//!   and elevation labels.

mod normalize;
mod prepare;
mod frame_state;
mod reduction;
mod draw;

pub(crate) use draw::draw_elevation_widget;
pub(crate) use prepare::prepare_elevation_cache;
#[allow(unused_imports)]
pub(crate) use reduction::{
    ElevationSample, simplify_elevation_samples, simplify_elevation_samples_segment,
};
