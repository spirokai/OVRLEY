//! Route geometry IPC command.
//!
//! Provides `build_route_geometry_command` — a pure-geometry command that
//! accepts serialized config + activity JSON and returns pre-built route
//! widget geometry for the frontend preview. No Skia surfaces, no static
//! layers, no frame states.

use crate::activity::parse_activity_json;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{parse_config_json, validate_route_plots, validate_scene_config};
use serde::Serialize;

/// Serializable geometry model for the route widget, returned over IPC.
///
/// The JS frontend consumes this directly for SVG rendering without
/// re-computing geometry locally.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGeometryResponse {
    /// Projected (x, y) points in widget-local coordinates.
    pub points: Vec<[f32; 2]>,
    /// Per-point progress values (0.0..=1.0) for marker interpolation.
    pub progress_values: Vec<f32>,
    /// Bounding box [min_x, min_y, max_x, max_y].
    pub bbox: [f32; 4],
    /// Number of raw source route samples before simplification.
    pub source_point_count: usize,
    /// Simplification label for diagnostics.
    pub simplification: String,
    /// Widget width in scaled pixels.
    pub widget_width: u32,
    /// Widget height in scaled pixels.
    pub widget_height: u32,
}

/// Builds route widget geometry from serialized config and activity JSON.
///
/// This is a pure-geometry command: no Skia surfaces, no static layers, no
/// frame states. It performs trimming, Mercator projection, LTTB downsampling,
/// and RDP simplification only.
pub fn build_route_geometry_command(
    config_json: &str,
    parsed_activity_json: &str,
) -> CoreResult<RouteGeometryResponse> {
    let config = parse_config_json(config_json)?;
    let scene = validate_scene_config(config.scene)?;
    let course_plots = validate_route_plots(&config.plots)?;
    let activity = parse_activity_json(parsed_activity_json)?;

    let course_plot = course_plots
        .first()
        .ok_or_else(|| CoreError::Config("Config has no course_plot widget".into()))?;

    let show_full_activity = course_plot.show_full_activity;

    let route_samples = crate::render::widgets::route::prepare::build_route_samples(
        &activity,
        show_full_activity,
        &scene,
    );
    let geometry = ovrley_widgets::widgets::route::geometry::RouteGeometry::new(course_plot, &route_samples)?;
    let scale = scene.presentation.scale;
    let width = course_plot.width * scale;
    let height = course_plot.height * scale;

    Ok(RouteGeometryResponse {
        points: geometry.points.into_iter().map(|(x, y)| [x * scale, y * scale]).collect(),
        progress_values: geometry.progress_values,
        bbox: [0.0, 0.0, width, height],
        source_point_count: geometry.source_point_count,
        simplification: geometry.simplification,
        widget_width: width.round() as u32,
        widget_height: height.round() as u32,
    })
}
