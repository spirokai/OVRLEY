//! Independent frame resizing with the original prepared profile and a fixed corner.
use super::{layout::DivingProfileLayout, validation::ValidatedDivingProfile};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
    widgets::{elevation::config::PointLabelConfig, plot_marker::frame_resize_marker_factor},
};

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub marker_size: f32,
    pub marker_variant_diameter: f32,
    pub metric_label_offset_x: f32,
    pub metric_label_offset_y: f32,
    pub imperial_label_offset_x: f32,
    pub imperial_label_offset_y: f32,
    /// Whole object: widget patches merge shallowly, so a partial label would drop its font.
    pub point_label: PointLabelConfig,
}

pub fn resize_diving_profile(
    config: &mut ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    size: [f32; 2],
    fixed: [f32; 2],
) -> WidgetResult<()> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid diving profile resize size".into(),
        ));
    }
    let stroke =
        (config.completed_line_width.max(config.remaining_line_width) * scene.scale).max(1.0);
    let width = ((size[0] - stroke) / scene.scale).clamp(8.0, 10_000.0);
    let height = ((size[1] - stroke) / scene.scale).clamp(8.0, 10_000.0);
    // Strokes keep their own widths; the marker and its labels follow the handle's resize policy.
    let factor = frame_resize_marker_factor([config.width, config.height], [width, height], fixed);
    config.marker_size = (config.marker_size * factor).clamp(0.01, 400.0);
    config.marker_variant_diameter = (config.marker_variant_diameter * factor).clamp(0.01, 400.0);
    config.metric_label_offset_x *= factor;
    config.metric_label_offset_y *= factor;
    config.imperial_label_offset_x *= factor;
    config.imperial_label_offset_y *= factor;
    config.label_font_size = (config.label_font_size * factor).clamp(1.0, 400.0);
    config.width = width;
    config.height = height;
    Ok(())
}

pub fn anchored_update(
    config: &ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    fixed: [f32; 2],
    point: [f32; 2],
    point_label: &PointLabelConfig,
) -> ResizeUpdate {
    let bounds = DivingProfileLayout::measure_content(config, scene);
    let (sin, cos) = config.rotation.to_radians().sin_cos();
    let (x, y) = (
        bounds.left + bounds.width() * fixed[0],
        bounds.top + bounds.height() * fixed[1],
    );
    ResizeUpdate {
        x: point[0] - (x * cos - y * sin),
        y: point[1] - (x * sin + y * cos),
        width: config.width,
        height: config.height,
        marker_size: config.marker_size,
        marker_variant_diameter: config.marker_variant_diameter,
        metric_label_offset_x: config.metric_label_offset_x,
        metric_label_offset_y: config.metric_label_offset_y,
        imperial_label_offset_x: config.imperial_label_offset_x,
        imperial_label_offset_y: config.imperial_label_offset_y,
        point_label: PointLabelConfig {
            font_size: config.label_font_size,
            ..point_label.clone()
        },
    }
}
