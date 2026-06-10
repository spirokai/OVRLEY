//! Elevation geometry IPC command.
//!
//! Provides `build_elevation_geometry_command` — a pure-geometry command that
//! accepts serialized config + activity JSON and returns pre-built elevation
//! widget geometry for the frontend preview. No Skia surfaces, no static
//! layers, no frame states.

use crate::activity::parse_activity_json;
use crate::commands::parse_and_validate_config;
use crate::error::{CoreError, CoreResult};
use serde::Serialize;

/// Serializable geometry model for the elevation widget, returned over IPC.
///
/// The JS frontend consumes this directly for SVG rendering without
/// re-computing geometry locally.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevationGeometryResponse {
    /// Projected (x, y) points in widget-local coordinates.
    pub points: Vec<[f32; 2]>,
    /// Per-point progress values (0.0..=1.0) for marker interpolation.
    pub progress_values: Vec<f32>,
    /// Per-point elapsed fractions (0.0..=1.0) for chronological fill.
    pub elapsed_fractions: Vec<f32>,
    /// Source elevation range [min, max] used for marker-y projection.
    pub data_range: Option<[f64; 2]>,
    /// Bounding box [min_x, min_y, max_x, max_y].
    pub bbox: [f32; 4],
    /// Number of raw source elevation samples before simplification.
    pub source_point_count: usize,
    /// Simplification label for diagnostics.
    pub simplification: String,
    /// Widget width in scaled pixels.
    pub widget_width: u32,
    /// Widget height in scaled pixels.
    pub widget_height: u32,
}

/// Builds elevation widget geometry from serialized config and activity JSON.
///
/// This is a pure-geometry command: no Skia surfaces, no static layers, no
/// frame states. It performs trimming, downsampling, projection, and RDP
/// simplification only.
pub fn build_elevation_geometry_command(
    config_json: &str,
    parsed_activity_json: &str,
) -> CoreResult<ElevationGeometryResponse> {
    let validated = parse_and_validate_config(config_json)?;
    let activity = parse_activity_json(parsed_activity_json)?;

    let elevation_plot = validated.elevation_plot.ok_or_else(|| {
        CoreError::Config("Config has no elevation_plot widget".into())
    })?;

    let show_full_activity = elevation_plot.show_full_activity;

    let raw_points = crate::render::widgets::elevation::prepare::build_elevation_source_points(
        &activity,
        show_full_activity,
        &validated.scene,
    )?;

    let normalized = crate::render::widgets::elevation::normalize::normalize_elevation_plot(
        &elevation_plot,
        &validated.scene,
    );

    let geometry = crate::render::widgets::elevation::prepare::build_elevation_geometry(
        &normalized,
        &raw_points,
    )?;

    Ok(ElevationGeometryResponse {
        points: geometry.points.into_iter().map(|(x, y)| [x, y]).collect(),
        progress_values: geometry.progress_values,
        elapsed_fractions: geometry.elapsed_fractions,
        data_range: geometry.elevation_data_range.map(|(min, max)| [min, max]),
        bbox: [
            geometry.bbox.0,
            geometry.bbox.1,
            geometry.bbox.2,
            geometry.bbox.3,
        ],
        source_point_count: geometry.source_point_count,
        simplification: geometry.simplification,
        widget_width: normalized.width,
        widget_height: normalized.height,
    })
}
