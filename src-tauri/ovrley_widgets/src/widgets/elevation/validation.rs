use super::config::ElevationPlotConfig;
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::ValidatedScenePresentation,
        validation::{require_f32, require_hex_color, require_opacity, require_positive_f32},
    },
    widgets::plot_marker::MarkerVariant,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedElevationPlot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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
    /// Fades both area fills from their opacity at the plot top to transparent at its bottom.
    pub area_gradient: bool,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_elevation_metric: bool,
    pub show_elevation_imperial: bool,
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
    scene: &ValidatedScenePresentation,
) -> WidgetResult<ValidatedElevationPlot> {
    let p = |field: &str| format!("plots[{index}].{field}");
    let number = |value, field| require_f32(Some(value), &p(field));
    let positive = |value, field| require_positive_f32(Some(value), &p(field));
    let color = |value: &str, field| require_hex_color(Some(value), &p(field));
    let opacity = |value, field| require_opacity(Some(value), &p(field));
    let ranged = |value, field, min, max| {
        let value = number(value, field)?;
        if !(min..=max).contains(&value) {
            return Err(WidgetError::Config(format!(
                "{}: must be between {min} and {max}",
                p(field)
            )));
        }
        Ok(value)
    };
    if plot.value != "elevation" {
        return Err(WidgetError::Config(format!(
            "{}: must be elevation",
            p("value")
        )));
    }
    let altitude_scale = match plot.starting_altitude_unit.as_deref() {
        Some("m") => 1.0,
        Some("ft") => 3.280_84,
        None if plot.starting_altitude.is_none() => 1.0,
        _ => {
            return Err(WidgetError::Config(format!(
                "{}: expected m or ft",
                p("starting_altitude_unit")
            )))
        }
    };
    if plot
        .starting_altitude
        .is_some_and(|value| !value.is_finite())
    {
        return Err(WidgetError::Config(format!(
            "{}: must be finite",
            p("starting_altitude")
        )));
    }
    if plot
        .point_label
        .font
        .as_ref()
        .is_some_and(|font| font.trim().is_empty())
    {
        return Err(WidgetError::Config(format!(
            "{}: must identify a font",
            p("point_label.font")
        )));
    }
    Ok(ValidatedElevationPlot {
        x: number(plot.x, "x")?,
        y: number(plot.y, "y")?,
        width: positive(plot.width, "width")?,
        height: positive(plot.height, "height")?,
        rotation: number(plot.rotation, "rotation")?,
        y_scale: ranged(plot.y_scale, "y_scale", 0.2, 4.0)?,
        simplify_tolerance_px: ranged(
            plot.simplify_tolerance_px,
            "simplify_tolerance_px",
            0.0,
            8.0,
        )?,
        target_density: ranged(plot.target_density, "target_density", 0.1, 2.0)?,
        completed_line_width: positive(plot.completed_line_width, "completed_line_width")?,
        completed_line_color: color(&plot.completed_line_color, "completed_line_color")?,
        completed_line_opacity: opacity(plot.completed_line_opacity, "completed_line_opacity")?,
        remaining_line_width: positive(plot.remaining_line_width, "remaining_line_width")?,
        remaining_line_color: color(&plot.remaining_line_color, "remaining_line_color")?,
        remaining_line_opacity: opacity(plot.remaining_line_opacity, "remaining_line_opacity")?,
        area_remaining_color: color(&plot.area_remaining_color, "area_remaining_color")?,
        area_remaining_opacity: opacity(plot.area_remaining_opacity, "area_remaining_opacity")?,
        area_completed_color: color(&plot.area_completed_color, "area_completed_color")?,
        area_completed_opacity: opacity(plot.area_completed_opacity, "area_completed_opacity")?,
        area_gradient: plot.area_gradient,
        marker_variant: MarkerVariant::parse(&plot.marker_variant, &p("marker_variant"))?,
        marker_variant_diameter: positive(plot.marker_variant_diameter, "marker_variant_diameter")?,
        marker_size: positive(plot.marker_size, "marker_size")?,
        marker_color: color(&plot.marker_color, "marker_color")?,
        marker_opacity: opacity(plot.marker_opacity, "marker_opacity")?,
        show_full_activity: plot.show_full_activity,
        show_elevation_metric: plot.show_elevation_metric,
        show_elevation_imperial: plot.show_elevation_imperial,
        starting_altitude_m: plot.starting_altitude.map(|value| value / altitude_scale),
        metric_label_offset_x: number(plot.metric_label_offset_x, "metric_label_offset_x")?,
        metric_label_offset_y: number(plot.metric_label_offset_y, "metric_label_offset_y")?,
        imperial_label_offset_x: number(plot.imperial_label_offset_x, "imperial_label_offset_x")?,
        imperial_label_offset_y: number(plot.imperial_label_offset_y, "imperial_label_offset_y")?,
        label_font: plot.point_label.font.clone().or_else(|| scene.font.clone()),
        label_font_size: positive(plot.point_label.font_size, "point_label.font_size")?,
        label_color: color(&plot.point_label.color, "point_label.color")?,
    })
}
