use super::config::DivingProfileConfig;
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        scene::ValidatedScenePresentation,
        validation::{require_f32, require_hex_color, require_opacity, require_positive_f32},
    },
    widgets::plot_marker::MarkerVariant,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedDivingProfile {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub y_scale: f32,
    pub simplify_tolerance_px: f32,
    pub target_density: f32,
    pub completed_line_width: f32,
    pub completed_line_color: String,
    pub completed_line_opacity: f32,
    pub remaining_line_width: f32,
    pub remaining_line_color: String,
    pub remaining_line_opacity: f32,
    pub ceiling_line_width: f32,
    pub ceiling_line_color: String,
    pub ceiling_line_opacity: f32,
    pub ceiling_area_color: String,
    pub ceiling_area_opacity: f32,
    pub marker_variant: MarkerVariant,
    pub marker_variant_diameter: f32,
    pub marker_size: f32,
    pub marker_color: String,
    pub marker_opacity: f32,
    pub show_full_activity: bool,
    pub show_depth_metric: bool,
    pub show_depth_imperial: bool,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    pub label_font: Option<String>,
    pub label_font_size: f32,
    pub label_color: String,
}

pub fn validate_diving_profile(
    plot: &DivingProfileConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> WidgetResult<ValidatedDivingProfile> {
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
    if plot.value != "diving_profile" {
        return Err(WidgetError::Config(format!(
            "{}: must be diving_profile",
            p("value")
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
    Ok(ValidatedDivingProfile {
        x: number(plot.x, "x")?,
        y: number(plot.y, "y")?,
        width: positive(plot.width, "width")?,
        height: positive(plot.height, "height")?,
        rotation: number(plot.rotation, "rotation")?,
        opacity: ranged(plot.opacity, "opacity", 0.0, 1.0)?,
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
        ceiling_line_width: ranged(plot.ceiling_line_width, "ceiling_line_width", 0.0, 100.0)?,
        ceiling_line_color: color(&plot.ceiling_line_color, "ceiling_line_color")?,
        ceiling_line_opacity: opacity(plot.ceiling_line_opacity, "ceiling_line_opacity")?,
        ceiling_area_color: color(&plot.ceiling_area_color, "ceiling_area_color")?,
        ceiling_area_opacity: opacity(plot.ceiling_area_opacity, "ceiling_area_opacity")?,
        marker_variant: MarkerVariant::parse(&plot.marker_variant, &p("marker_variant"))?,
        marker_variant_diameter: positive(plot.marker_variant_diameter, "marker_variant_diameter")?,
        marker_size: positive(plot.marker_size, "marker_size")?,
        marker_color: color(&plot.marker_color, "marker_color")?,
        marker_opacity: opacity(plot.marker_opacity, "marker_opacity")?,
        show_full_activity: plot.show_full_activity,
        show_depth_metric: plot.show_depth_metric,
        show_depth_imperial: plot.show_depth_imperial,
        metric_label_offset_x: number(plot.metric_label_offset_x, "metric_label_offset_x")?,
        metric_label_offset_y: number(plot.metric_label_offset_y, "metric_label_offset_y")?,
        imperial_label_offset_x: number(plot.imperial_label_offset_x, "imperial_label_offset_x")?,
        imperial_label_offset_y: number(plot.imperial_label_offset_y, "imperial_label_offset_y")?,
        label_font: plot.point_label.font.clone().or_else(|| scene.font.clone()),
        label_font_size: positive(plot.point_label.font_size, "point_label.font_size")?,
        label_color: color(&plot.point_label.color, "point_label.color")?,
    })
}
