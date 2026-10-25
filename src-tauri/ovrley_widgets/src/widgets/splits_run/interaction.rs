//! Uniform resizing: text, bars and gaps scale together with the opposite visible corner fixed.
use super::{
    format::splits_content, layout::prepare_splits_layout, validation::validate_splits_run,
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
    pub row_gap: f32,
    pub bar_width: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn resize_splits_run(
    widget: ValueConfig,
    scene: ScenePresentationConfig,
    factor: f32,
    fixed: [f32; 2],
    point: [f32; 2],
    activity: Option<&ActivitySampler>,
    second: f64,
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    let mut config = validate_splits_run(widget, 0)?;
    let scene = validate_scene_presentation(scene)?;
    // The font clamp bounds the factor every other dimension follows, so proportions hold.
    let font_size = (config.font_size * factor).clamp(8.0, 400.0);
    let factor = font_size / config.font_size;
    config.font_size = font_size;
    config.label_font_size *= factor;
    config.row_gap *= factor;
    config.bar_width *= factor;
    config.track_thickness *= factor;
    config.track_corner_radius *= factor;
    config.track_border_thickness *= factor;
    let table = activity.and_then(|activity| activity.split_table(config.length));
    let content = splits_content(table, second);
    let (bounds, _) =
        prepare_splits_layout(&config, &scene, table, fonts)?.frame_bounds(table, &content);
    Ok(ResizeUpdate {
        x: config.x + point[0] - (bounds.left + bounds.width() * fixed[0]),
        y: config.y + point[1] - (bounds.top + bounds.height() * fixed[1]),
        font_size: config.font_size,
        label_font_size: config.label_font_size,
        row_gap: config.row_gap,
        bar_width: config.bar_width,
        track_thickness: config.track_thickness,
        track_corner_radius: config.track_corner_radius,
        track_border_thickness: config.track_border_thickness,
    })
}
