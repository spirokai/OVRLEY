//! Uniform intrinsic text resizing with the opposite visible corner fixed at the current playhead.
use super::{
    format::{lap_timer_content, LapHistory, LapSample},
    layout::prepare_lap_timer_layout,
    validation::validate_lap_timer,
};
use crate::{
    activity::ActivitySampler,
    error::WidgetResult,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    render::text::FontResolver,
    widgets::value::config::ValueConfig,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub label_font_size: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_lap_timer(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_lap_timer(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    config.font_size = (config.font_size * factor).clamp(8.0, 400.0);
    config.label_font_size = (config.label_font_size * factor).clamp(6.0, 400.0);
    let history = activity.map_or_else(LapHistory::default, ActivitySampler::lap_history);
    let sample = activity.map_or_else(LapSample::default, |activity| activity.lap_timer_at(second));
    let content = lap_timer_content(&config, history, sample)?;
    let bounds = prepare_lap_timer_layout(&config, &scene, history, fonts)?
        .prepare_frame(history, &content)?
        .content_bounds;
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        label_font_size: config.label_font_size,
    })
}
