//! Framed resizing: width scales the tick lengths, height reveals more tape. Text and tick
//! spacing keep their size.
use super::{
    layout::{TEXT_ROW_RATIO, VALUE_BOX_HEIGHT_RATIO},
    validation::validate_altitude_tape,
};
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
    pub altitude_tape: ResizeVariant,
}

#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel frame size; the content rectangle is the frame itself.
pub fn resize_altitude_tape(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid altitude tape resize size".into(),
        ));
    }
    let config = validate_altitude_tape(widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    // The body always keeps room for the readout box above the footer.
    let footer = if config.show_vertical_speed {
        config.vertical_speed_font_size
    } else {
        0.0
    };
    let min_height = footer * TEXT_ROW_RATIO
        + config.font_size * VALUE_BOX_HEIGHT_RATIO;
    let width = (size[0] / scale).clamp(8.0, 10_000.0);
    let height = (size[1] / scale).clamp(min_height, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - height * scale * fixed[1],
        display_variants: ResizeVariants {
            altitude_tape: ResizeVariant { width, height },
        },
    })
}
