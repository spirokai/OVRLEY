//! Framed resizing reflows the tape without scaling its tick/label/indicator styling.
use super::{
    geometry::{heading_tape_has_highlight_bar, HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT},
    layout::prepare_heading_cache,
    validation::validate_heading,
};
use crate::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::{
        scene::{validate_scene_presentation, ScenePresentationConfig},
        validation::require_non_negative_f32,
    },
    render::text::FontResolver,
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
    pub heading_tape: ResizeVariant,
}
#[derive(Serialize)]
pub struct ResizeVariant {
    pub width: f32,
    pub height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_offset: Option<f32>,
}

/// Changing the label gap grows/shrinks the frame while preserving tick lengths.
pub fn resize_heading_label_offset(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    label_offset: f32,
) -> WidgetResult<ResizeUpdate> {
    let config = validate_heading(widget, 0)?;
    validate_scene_presentation(scene)?;
    let label_offset = require_non_negative_f32(label_offset, "heading label_offset")?;
    let body_ratio =
        if heading_tape_has_highlight_bar(config.show_indicator, &config.indicator_style) {
            1.0 - 2.0 * HIGHLIGHT_BAR_BODY_MARGIN_FRAME_PCT
        } else {
            1.0
        };
    let height =
        (config.height + (label_offset - config.label_offset) / body_ratio).clamp(8.0, 10_000.0);
    Ok(ResizeUpdate {
        x: config.x,
        y: config.y,
        display_variants: ResizeVariants {
            heading_tape: ResizeVariant {
                width: config.width,
                height,
                label_offset: Some(label_offset),
            },
        },
    })
}

#[allow(clippy::too_many_arguments)]
pub fn resize_heading(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config("invalid heading resize size".into()));
    }
    let mut config = validate_heading(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    let raw = activity.and_then(|activity| activity.heading_at(second));
    let original = prepare_heading_cache(&config, &scene, fonts)?
        .prepare_frame(raw)
        .content_bounds;
    if original.is_empty() {
        return Err(WidgetError::Render(
            "heading tape has no visible content to resize".into(),
        ));
    }
    config.width = (config.width * size[0] / original.width()).clamp(8.0, 10_000.0);
    config.height = (config.height * size[1] / original.height()).clamp(8.0, 10_000.0);
    let bounds = prepare_heading_cache(&config, &scene, fonts)?
        .prepare_frame(raw)
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        display_variants: ResizeVariants {
            heading_tape: ResizeVariant {
                width: config.width,
                height: config.height,
                label_offset: None,
            },
        },
    })
}
