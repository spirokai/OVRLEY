//! Route plot validation.
//!
//! `validate_route_plot` verifies that every output-affecting route plot
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::config::CoursePlotConfig;
use crate::error::{WidgetError, WidgetResult};
use crate::model::validation::{
    require_f32, require_hex_color, require_non_negative_f32, require_opacity, require_positive_f32,
};
use crate::widgets::plot_marker::MarkerVariant;

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedRoutePlot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
}

pub fn validate_route_plot(
    plot: &CoursePlotConfig,
    index: usize,
) -> WidgetResult<ValidatedRoutePlot> {
    let p = |f: &str| format!("plots[{index}].{f}");
    if plot.value != "course" {
        return Err(WidgetError::Config(format!(
            "{}: must be course",
            p("value")
        )));
    }

    let simplify_tolerance_px = require_f32(
        Some(plot.simplify_tolerance_px),
        &p("simplify_tolerance_px"),
    )?;
    require_non_negative_f32(simplify_tolerance_px, &p("simplify_tolerance_px"))?;
    let target_density = require_f32(Some(plot.target_density), &p("target_density"))?;
    if !(0.1..=2.0).contains(&target_density) {
        return Err(WidgetError::Config(format!(
            "{}: must be between 0.1 and 2.0",
            p("target_density")
        )));
    }

    let completed_line_width =
        require_positive_f32(Some(plot.completed_line_width), &p("completed_line_width"))?;
    let completed_line_color =
        require_hex_color(Some(&plot.completed_line_color), &p("completed_line_color"))?;
    let completed_line_opacity = require_opacity(
        Some(plot.completed_line_opacity),
        &p("completed_line_opacity"),
    )?;

    let remaining_line_width =
        require_positive_f32(Some(plot.remaining_line_width), &p("remaining_line_width"))?;
    let remaining_line_color =
        require_hex_color(Some(&plot.remaining_line_color), &p("remaining_line_color"))?;
    let remaining_line_opacity = require_opacity(
        Some(plot.remaining_line_opacity),
        &p("remaining_line_opacity"),
    )?;

    let marker_size = require_positive_f32(Some(plot.marker_size), &p("marker_size"))?;
    let marker_color = require_hex_color(Some(&plot.marker_color), &p("marker_color"))?;
    let marker_opacity = require_opacity(Some(plot.marker_opacity), &p("marker_opacity"))?;
    let marker_variant = MarkerVariant::parse(&plot.marker_variant, &p("marker_variant"))?;
    let marker_variant_diameter = require_positive_f32(
        Some(plot.marker_variant_diameter),
        &p("marker_variant_diameter"),
    )?;

    Ok(ValidatedRoutePlot {
        x: require_f32(Some(plot.x), &p("x"))?,
        y: require_f32(Some(plot.y), &p("y"))?,
        width: require_positive_f32(Some(plot.width), &p("width"))?,
        height: require_positive_f32(Some(plot.height), &p("height"))?,
        rotation: require_f32(Some(plot.rotation), &p("rotation"))?,
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
        show_full_activity: plot.show_full_activity,
    })
}
