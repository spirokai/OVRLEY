//! Uniform intrinsic resizing of a course, using its existing prepared vector bounds.
use super::{geometry::RouteGeometry, layout::RouteLayout, validation::ValidatedRoutePlot};
use crate::model::scene::ValidatedScenePresentation;

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub completed_line_width: f32,
    pub remaining_line_width: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
}

pub fn scale_route(config: &mut ValidatedRoutePlot, factor: f32) {
    // Preserve the existing minimum widget size and a uniform aspect ratio.
    let factor = factor.max(8.0 / config.width.min(config.height));
    config.width *= factor;
    config.height *= factor;
    config.completed_line_width *= factor;
    config.remaining_line_width *= factor;
    config.marker_size *= factor;
    config.marker_variant_diameter *= factor;
}

pub fn resize_route(
    config: &ValidatedRoutePlot,
    scene: &ValidatedScenePresentation,
    geometry: &RouteGeometry,
    fixed: [f32; 2],
    point: [f32; 2],
) -> ResizeUpdate {
    let bounds = RouteLayout::measure_content(config, scene, geometry);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let local_x = bounds.left + bounds.width() * fixed[0];
    let local_y = bounds.top + bounds.height() * fixed[1];
    ResizeUpdate {
        x: point[0] - (local_x * cos - local_y * sin),
        y: point[1] - (local_x * sin + local_y * cos),
        width: config.width,
        height: config.height,
        completed_line_width: config.completed_line_width,
        remaining_line_width: config.remaining_line_width,
        marker_size: config.marker_size,
        marker_variant_diameter: config.marker_variant_diameter,
    }
}
