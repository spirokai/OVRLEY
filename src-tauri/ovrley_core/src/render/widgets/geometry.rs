//! Point/rect/math and layout-fitting helpers for overlay widgets.
//!
//! Owns: `distance` (Euclidean distance between 2D points),
//!       `fit_points_to_widget_with_inset` (bounding-box scaling with padding).
//! Does not own: RDP simplification (see [`crate::rdp`]), polyline/area drawing
//!       (see [`super::polyline`]), marker drawing (see [`super::marker`]).
//!
//! Allowed dependencies: `std`.
//! Forbidden dependencies: `skia_safe`, `crate::normalize`, `crate::activity`.
//!
//! ## Performance
//! `fit_points_to_widget_with_inset` is O(n) in point count and called once per
//! widget build (not per-frame). `distance` is called per-point during RDP
//! simplification, also during widget build. Not on the render hot path.

pub(crate) fn distance(left: (f32, f32), right: (f32, f32)) -> f32 {
    ((right.0 - left.0).powi(2) + (right.1 - left.1).powi(2)).sqrt()
}

pub(crate) fn fit_points_to_widget_with_inset(
    points: &[(f32, f32)],
    width: f32,
    height: f32,
    inset_px: f32,
    invert_y: bool,
) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }

    let min_x = points.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    let safe_inset = inset_px.max(0.0).min(width.min(height) * 0.45);
    let inner_width = (width - safe_inset * 2.0).max(1.0);
    let inner_height = (height - safe_inset * 2.0).max(1.0);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    let scale = (inner_width / span_x).min(inner_height / span_y);
    let offset_x = (width - span_x * scale) / 2.0;
    let offset_y = (height - span_y * scale) / 2.0;

    points
        .iter()
        .map(|(x, y)| {
            let fitted_x = (x - min_x) * scale + offset_x;
            let mut fitted_y = (y - min_y) * scale + offset_y;
            if invert_y {
                fitted_y = height - fitted_y;
            }
            (fitted_x, fitted_y)
        })
        .collect()
}
