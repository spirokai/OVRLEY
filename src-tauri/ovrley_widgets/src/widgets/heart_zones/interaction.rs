//! Framed resizing reflows the track without scaling its gap, radius or indicator.
use super::{validation::validate_heart_zones, zones::ZONE_COUNT};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    widgets::value::config::ValueConfig,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub display_variants: ResizeVariants,
}

#[derive(Serialize)]
pub struct ResizeVariants {
    pub heart_zones: ResizeVariant,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel track size; the content rectangle is the track itself.
pub fn resize_heart_zones(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid heart zones resize size".into()));
    }
    let config = validate_heart_zones(widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    // Every zone keeps at least a pixel between the fixed gaps.
    let min_width = config.zone_gap * (ZONE_COUNT - 1) as f32 + ZONE_COUNT as f32;
    let width = (size[0] / scale).clamp(min_width.max(8.0), 10_000.0);
    let height = (size[1] / scale).clamp(4.0, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - height * scale * fixed[1],
        display_variants: ResizeVariants {
            heart_zones: ResizeVariant { width, height },
        },
    })
}
