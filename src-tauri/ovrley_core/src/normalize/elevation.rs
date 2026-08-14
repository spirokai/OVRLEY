//! Elevation plot validation.
//!
//! `validate_elevation_plot` verifies that every output-affecting elevation
//! plot field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{
    normalize_marker_variant, require_f32, require_hex_color, require_opacity, require_positive_f32,
};
use crate::error::{CoreError, CoreResult};
use crate::normalize::raw::ElevationPlotConfig;

#[derive(Clone, Debug)]
pub struct ValidatedElevationPlot {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub area_remaining_color: String,
    pub area_remaining_opacity: f32,
    pub area_completed_color: String,
    pub area_completed_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_elevation_metric: bool,
    pub show_elevation_imperial: bool,
    /// Optional presentation target for the first elevation sample, normalized to meters.
    pub starting_altitude_m: Option<f64>,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub label_font: Option<String>,
    pub label_font_size: f32,
    pub label_color: String,
}

pub fn validate_elevation_plot(
    plot: &ElevationPlotConfig,
    index: usize,
    scene: &crate::normalize::ValidatedSceneConfig,
) -> CoreResult<ValidatedElevationPlot> {
    let p = |f: &str| format!("plots[{index}].{f}");

    let y_scale = require_f32(plot.y_scale, &p("y_scale"))?;
    if !(0.2..=4.0).contains(&y_scale) {
        return Err(CoreError::Config(format!(
            "{}: must be between 0.2 and 4.0",
            p("y_scale")
        )));
    }

    let simplify_tolerance_px =
        require_f32(plot.simplify_tolerance_px, &p("simplify_tolerance_px"))?;
    if !(0.0..=8.0).contains(&simplify_tolerance_px) {
        return Err(CoreError::Config(format!(
            "{}: must be between 0.0 and 8.0",
            p("simplify_tolerance_px")
        )));
    }

    let target_density = require_f32(plot.target_density, &p("target_density"))?;
    if !(0.1..=2.0).contains(&target_density) {
        return Err(CoreError::Config(format!(
            "{}: must be between 0.1 and 2.0",
            p("target_density")
        )));
    }

    let completed_line_width =
        require_positive_f32(plot.completed_line_width, &p("completed_line_width"))?;
    let completed_line_color = require_hex_color(
        plot.completed_line_color.as_deref(),
        &p("completed_line_color"),
    )?;
    let completed_line_opacity =
        require_opacity(plot.completed_line_opacity, &p("completed_line_opacity"))?;

    let remaining_line_width =
        require_positive_f32(plot.remaining_line_width, &p("remaining_line_width"))?;
    let remaining_line_color = require_hex_color(
        plot.remaining_line_color.as_deref(),
        &p("remaining_line_color"),
    )?;
    let remaining_line_opacity =
        require_opacity(plot.remaining_line_opacity, &p("remaining_line_opacity"))?;

    let area_remaining_color = require_hex_color(
        plot.area_remaining_color.as_deref(),
        &p("area_remaining_color"),
    )?;
    let area_remaining_opacity =
        require_opacity(plot.area_remaining_opacity, &p("area_remaining_opacity"))?;
    let area_completed_color = require_hex_color(
        plot.area_completed_color.as_deref(),
        &p("area_completed_color"),
    )?;
    let area_completed_opacity =
        require_opacity(plot.area_completed_opacity, &p("area_completed_opacity"))?;

    let marker_size = require_positive_f32(plot.marker_size, &p("marker_size"))?;
    let marker_color = require_hex_color(plot.marker_color.as_deref(), &p("marker_color"))?;
    let marker_opacity = require_opacity(plot.marker_opacity, &p("marker_opacity"))?;
    let marker_variant = normalize_marker_variant(
        plot.marker_variant
            .as_deref()
            .ok_or_else(|| CoreError::Config(format!("{}: required", p("marker_variant"))))?,
    );
    let marker_variant_diameter =
        require_positive_f32(plot.marker_variant_diameter, &p("marker_variant_diameter"))?;

    let show_elevation_metric = plot
        .show_elevation_metric
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_elevation_metric"))))?;
    let show_elevation_imperial = plot
        .show_elevation_imperial
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_elevation_imperial"))))?;

    let show_full_activity = plot
        .show_full_activity
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_full_activity"))))?;

    let starting_altitude_meter_scale = match plot.starting_altitude_unit.as_deref() {
        Some("m") => Some(1.0),
        Some("ft") => Some(3.280_84),
        Some(unit) => {
            return Err(CoreError::Config(format!(
                "{}: expected 'm' or 'ft', got '{unit}'",
                p("starting_altitude_unit")
            )))
        }
        None => None,
    };
    let starting_altitude_m = match (plot.starting_altitude, starting_altitude_meter_scale) {
        (Some(altitude), Some(scale)) => Some(altitude / scale),
        (Some(_), None) => {
            return Err(CoreError::Config(format!(
                "{}: required",
                p("starting_altitude_unit")
            )))
        }
        (None, _) => None,
    };

    let metric_label_offset_x =
        require_f32(plot.metric_label_offset_x, &p("metric_label_offset_x"))?;
    let metric_label_offset_y =
        require_f32(plot.metric_label_offset_y, &p("metric_label_offset_y"))?;
    let imperial_label_offset_x =
        require_f32(plot.imperial_label_offset_x, &p("imperial_label_offset_x"))?;
    let imperial_label_offset_y =
        require_f32(plot.imperial_label_offset_y, &p("imperial_label_offset_y"))?;

    let label_font_size = require_positive_f32(
        plot.point_label
            .as_ref()
            .and_then(|pl| pl.font_size)
            .or(scene.font_size),
        &p("point_label.font_size"),
    )?;
    let label_color = plot
        .point_label
        .as_ref()
        .and_then(|pl| pl.color.clone())
        .map(|c| require_hex_color(Some(c.as_str()), &p("point_label.color")))
        .transpose()?
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("point_label.color"))))?;

    let label_font = plot
        .point_label
        .as_ref()
        .and_then(|pl| pl.font.clone().or_else(|| pl.font_family.clone()))
        .or_else(|| scene.font.clone());

    Ok(ValidatedElevationPlot {
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        rotation: plot.rotation,
        y_scale,
        simplify_tolerance_px,
        target_density,
        completed_line_width,
        completed_line_color,
        completed_line_opacity,
        remaining_line_width,
        remaining_line_color,
        remaining_line_opacity,
        area_remaining_color,
        area_remaining_opacity,
        area_completed_color,
        area_completed_opacity,
        marker_variant,
        marker_variant_diameter,
        marker_size,
        marker_color,
        marker_opacity,
        show_full_activity,
        show_elevation_metric,
        show_elevation_imperial,
        starting_altitude_m,
        metric_label_offset_x,
        metric_label_offset_y,
        imperial_label_offset_x,
        imperial_label_offset_y,
        label_font,
        label_font_size,
        label_color,
    })
}
