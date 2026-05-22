//! Skia path construction and coordinate transform helpers for widgets.
//!
//! Owns: `path_from_points` (converts point arrays into Skia paths with optional
//!       closure and baseline-y), `with_widget_transform` (applies Skia canvas
//!       save/restore with translation, rotation, and clipping for a widget rect).
//! Does not own: polyline drawing (see [`super::polyline`]), route/elevation
//!       point projection (see [`super::common`]).
//!
//! Allowed dependencies: `skia_safe`.
//! Forbidden dependencies: `crate::config`, `crate::activity`,
//!       `crate::encode`, `crate::commands`.
//!
//! ## Performance
//! `with_widget_transform` calls `canvas.save()` + `canvas.restore()` per widget
//! per frame — these are lightweight Skia state stack operations. `path_from_points`
//! is O(n) in point count and called during widget preparation (not per-frame).

use skia_safe::Path as SkPath;

pub(crate) fn path_from_points(
    points: &[(f32, f32)],
    close_path: bool,
    baseline_y: Option<f32>,
) -> SkPath {
    let mut path = SkPath::new();
    if points.is_empty() {
        return path;
    }
    if let Some(baseline) = baseline_y {
        path.move_to((points[0].0, baseline));
    } else {
        path.move_to(points[0]);
    }
    for point in points {
        path.line_to(*point);
    }
    if let Some(baseline) = baseline_y {
        path.line_to((points.last().unwrap().0, baseline));
    }
    if close_path {
        path.close();
    }
    path
}

pub(crate) fn with_widget_transform(
    canvas: &skia_safe::Canvas,
    x: f32,
    y: f32,
    _width: f32,
    _height: f32,
    rotation_deg: f32,
    draw: impl FnOnce(&skia_safe::Canvas),
) {
    canvas.save();
    canvas.translate((x, y));
    if rotation_deg != 0.0 {
        canvas.rotate(rotation_deg, None);
    }
    draw(canvas);
    canvas.restore();
}
