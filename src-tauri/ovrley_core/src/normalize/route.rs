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
use crate::normalize::raw::CoursePlotConfig;

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

pub fn validate_route_plot(
    plot: &CoursePlotConfig,
    index: usize,
) -> CoreResult<ValidatedRoutePlot> {
    let p = |f: &str| format!("plots[{index}].{f}");

    let simplify_tolerance_px = require_f32(plot.simplify_tolerance_px, &p("simplify_tolerance_px"))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::raw::CoursePlotConfig;

    fn full_plot() -> CoursePlotConfig {
        serde_json::from_value(serde_json::json!({
            "x": 10.0,
            "y": 20.0,
            "width": 400,
            "height": 300,
            "rotation": 0.0,
            "simplify_tolerance_px": 1.0,
            "target_density": 1.0,
            "completed_line_width": 2.0,
            "completed_line_color": "#ffffff",
            "completed_line_opacity": 1.0,
            "remaining_line_width": 2.0,
            "remaining_line_color": "#888888",
            "remaining_line_opacity": 0.75,
            "marker_variant": "single",
            "marker_variant_diameter": 20.0,
            "marker_size": 8.0,
            "marker_color": "#ff0000",
            "marker_opacity": 1.0,
            "show_full_activity": false
        }))
        .unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_route_plot(&full_plot(), 0).is_ok());
    }

    #[test]
    fn missing_simplify_tolerance_rejected() {
        let mut p = full_plot();
        p.simplify_tolerance_px = None;
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("simplify_tolerance_px"), "{e}");
    }

    #[test]
    fn zero_simplify_tolerance_accepted() {
        let mut p = full_plot();
        p.simplify_tolerance_px = Some(0.0);
        let validated = validate_route_plot(&p, 0).unwrap();
        assert_eq!(validated.simplify_tolerance_px, 0.0);
    }

    #[test]
    fn negative_simplify_tolerance_rejected() {
        let mut p = full_plot();
        p.simplify_tolerance_px = Some(-1.0);
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("simplify_tolerance_px"), "{e}");
    }

    #[test]
    fn missing_completed_line_width_rejected() {
        let mut p = full_plot();
        p.completed_line_width = None;
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("completed_line_width"), "{e}");
    }

    #[test]
    fn missing_marker_size_rejected() {
        let mut p = full_plot();
        p.marker_size = None;
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("marker_size"), "{e}");
    }

    #[test]
    fn missing_marker_variant_rejected() {
        let mut p = full_plot();
        p.marker_variant = None;
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("marker_variant"), "{e}");
    }

    #[test]
    fn target_density_out_of_range_rejected() {
        let mut p = full_plot();
        p.target_density = Some(3.0);
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("target_density"), "{e}");
    }

    #[test]
    fn zero_marker_size_rejected() {
        let mut p = full_plot();
        p.marker_size = Some(0.0);
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("marker_size"), "{e}");
    }

    #[test]
    fn invalid_hex_color_rejected() {
        let mut p = full_plot();
        p.completed_line_color = Some("red".to_string());
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("completed_line_color"), "{e}");
    }

    #[test]
    fn opacity_out_of_range_rejected() {
        let mut p = full_plot();
        p.marker_opacity = Some(-1.0);
        let e = validate_route_plot(&p, 0).unwrap_err().to_string();
        assert!(e.contains("marker_opacity"), "{e}");
    }

    #[test]
    fn index_in_error_path() {
        let mut p = full_plot();
        p.marker_size = None;
        let e = validate_route_plot(&p, 3).unwrap_err().to_string();
        assert!(e.contains("plots[3]"), "{e}");
    }
}
