//! Route plot validation.
//!
//! `validate_route_plot` verifies that every output-affecting route plot
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{
    normalize_marker_variant, require_f32, require_hex_color, require_non_negative_f32,
    require_opacity, require_positive_f32,
};
use crate::error::{CoreError, CoreResult};
use crate::normalize::raw::{find_plot_values, CoursePlotConfig};

#[derive(Clone, Debug)]
pub struct ValidatedRoutePlot {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub marker_variant: String,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
}

pub fn validate_route_plots(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRoutePlot>> {
    find_plot_values(plots, "course")
        .into_iter()
        .map(|(index, value)| {
            serde_json::from_value::<CoursePlotConfig>(value.clone())
                .map_err(|e| CoreError::Config(format!("course plot config: {e}")))
                .and_then(|plot| validate_route_plot(&plot, index))
        })
        .collect()
}

pub fn validate_route_plot(
    plot: &CoursePlotConfig,
    index: usize,
) -> CoreResult<ValidatedRoutePlot> {
    let p = |f: &str| format!("plots[{index}].{f}");

    let simplify_tolerance_px =
        require_f32(plot.simplify_tolerance_px, &p("simplify_tolerance_px"))?;
    require_non_negative_f32(simplify_tolerance_px, &p("simplify_tolerance_px"))?;
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

    Ok(ValidatedRoutePlot {
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        rotation: plot.rotation,
        simplify_tolerance_px,
        target_density,
        completed_line_width,
        completed_line_color,
        completed_line_opacity,
        remaining_line_width,
        remaining_line_color,
        remaining_line_opacity,
        marker_variant,
        marker_variant_diameter,
        marker_size,
        marker_color,
        marker_opacity,
        show_full_activity: plot
            .show_full_activity
            .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_full_activity"))))?,
    })
}
