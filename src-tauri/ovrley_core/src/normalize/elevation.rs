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
        metric_label_offset_x,
        metric_label_offset_y,
        imperial_label_offset_x,
        imperial_label_offset_y,
        label_font,
        label_font_size,
        label_color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::raw::ElevationPlotConfig;
    use crate::normalize::validate_scene_config;

    fn validated_scene() -> crate::normalize::ValidatedSceneConfig {
        let scene: crate::normalize::SceneConfig = serde_json::from_value(serde_json::json!({
            "fps": 30.0, "start": 0.0, "end": 10.0,
            "width": 1920, "height": 1080, "scale": 1.0,
            "shadow_color": "#000000", "shadow_strength": 0.0, "shadow_distance": 0.0,
            "border_color": "#000000", "border_thickness": 0.0,
            "update_rate": 1, "custom_export_range_active": false,
            "composite_widget_update_rate": 1
        }))
        .unwrap();
        validate_scene_config(scene).unwrap()
    }

    fn full_plot() -> ElevationPlotConfig {
        serde_json::from_value(serde_json::json!({
            "x": 10.0, "y": 20.0, "width": 400, "height": 300,
            "rotation": 0.0, "margin": 0.0, "y_scale": 1.0,
            "simplify_tolerance_px": 1.0, "target_density": 0.75,
            "completed_line_width": 2.0, "completed_line_color": "#ffffff",
            "completed_line_opacity": 1.0,
            "remaining_line_width": 2.0, "remaining_line_color": "#888888",
            "remaining_line_opacity": 0.75,
            "area_remaining_color": "#00ff00", "area_remaining_opacity": 0.12,
            "area_completed_color": "#00ff00", "area_completed_opacity": 0.24,
            "marker_variant": "single", "marker_variant_diameter": 20.0,
            "marker_size": 8.0,             "marker_color": "#ff0000", "marker_opacity": 1.0,
            "show_full_activity": false,
            "show_elevation_metric": true, "show_elevation_imperial": false,
            "metric_label_offset_x": 0.0, "metric_label_offset_y": -28.0,
            "imperial_label_offset_x": 0.0, "imperial_label_offset_y": 6.0,
            "point_label": {
                "font_size": 12.5, "color": "#ffffff"
            }
        }))
        .unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_elevation_plot(&full_plot(), 0, &validated_scene()).is_ok());
    }

    #[test]
    fn missing_completed_line_width_rejected() {
        let mut p = full_plot();
        p.completed_line_width = None;
        let e = validate_elevation_plot(&p, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("completed_line_width"), "{e}");
    }

    #[test]
    fn missing_marker_size_rejected() {
        let mut p = full_plot();
        p.marker_size = None;
        let e = validate_elevation_plot(&p, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("marker_size"), "{e}");
    }

    #[test]
    fn missing_show_elevation_metric_rejected() {
        let mut p = full_plot();
        p.show_elevation_metric = None;
        let e = validate_elevation_plot(&p, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("show_elevation_metric"), "{e}");
    }

    #[test]
    fn missing_label_font_size_rejected() {
        let mut p = full_plot();
        p.point_label.as_mut().unwrap().font_size = None;
        // scene also has no font_size
        let mut scene = validated_scene();
        scene.font_size = None;
        let e = validate_elevation_plot(&p, 0, &scene)
            .unwrap_err()
            .to_string();
        assert!(e.contains("point_label.font_size"), "{e}");
    }

    #[test]
    fn y_scale_out_of_range_rejected() {
        let mut p = full_plot();
        p.y_scale = Some(5.0);
        let e = validate_elevation_plot(&p, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("y_scale"), "{e}");
    }

    #[test]
    fn index_in_error_path() {
        let mut p = full_plot();
        p.marker_size = None;
        let e = validate_elevation_plot(&p, 3, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("plots[3]"), "{e}");
    }
}
