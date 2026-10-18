//! Independent frame resizing with the original prepared profile and a fixed corner.
use super::{layout::DivingProfileLayout, validation::ValidatedDivingProfile};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::ValidatedScenePresentation,
};

#[derive(serde::Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub marker_size: f32,
}

pub fn resize_diving_profile(
    config: &mut ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    size: [f32; 2],
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
    // Preserve diving profile's existing marker scaling; strokes and labels keep their own sizes.
    config.marker_size =
        (config.marker_size * (width / config.width + height / config.height) * 0.5)
            .clamp(0.01, 400.0);
    config.width = width;
    config.height = height;
    Ok(())
}

pub fn anchored_update(
    config: &ValidatedDivingProfile,
    scene: &ValidatedScenePresentation,
    fixed: [f32; 2],
    point: [f32; 2],
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
    }
}
