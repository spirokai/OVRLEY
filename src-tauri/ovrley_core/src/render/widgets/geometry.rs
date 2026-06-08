//! Point/rect/math and layout-fitting helpers for overlay widgets.
//!
//! Owns: `distance` (Euclidean distance between 2D points).
//! Does not own: RDP simplification (see [`crate::rdp`]), polyline/area drawing
//!       (see [`super::polyline`]), marker drawing (see [`super::marker`]).
//!
//! Allowed dependencies: `std`.
//! Forbidden dependencies: `skia_safe`, `crate::normalize`, `crate::activity`.
//!
//! ## Performance
//! `distance` is called per-point during RDP simplification, also during widget
//! build. Not on the render hot path.

pub(crate) fn distance(left: (f32, f32), right: (f32, f32)) -> f32 {
    ((right.0 - left.0).powi(2) + (right.1 - left.1).powi(2)).sqrt()
}
