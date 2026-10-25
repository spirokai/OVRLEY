//! Framed resizing reflows the plot area; strokes, markers and the legend keep their sizes.
use super::{
    config::RollingChartConfig, layout::layout_legend, validation::validate_rolling_chart,
};
use crate::{
    error::{WidgetError, WidgetResult},
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    render::text::FontResolver,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResizeUpdate {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// `size` is the requested scene-pixel content size: the plot area and the legend rows below it.
/// The legend rewraps at the new width, so the plot takes the height the legend leaves.
pub fn resize_rolling_chart(
    widget: RollingChartConfig,
    scene: ScenePresentationConfig,
    size: [f32; 2],
    fixed: [f32; 2],
    point: [f32; 2],
    fonts: &dyn FontResolver,
) -> WidgetResult<ResizeUpdate> {
    if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(WidgetError::Config(
            "invalid rolling chart resize size".into(),
        ));
    }
    let chart = validate_rolling_chart(&widget, 0)?;
    let scale = validate_scene_presentation(scene)?.scale;
    let width = (size[0] / scale).clamp(8.0, 10_000.0);
    let legend = match &chart.legend {
        Some(legend) => {
            layout_legend(&chart, legend, scale, fonts, (0.0, 0.0, width * scale))?.height / scale
        }
        None => 0.0,
    };
    let height = (size[1] / scale - legend).clamp(8.0, 10_000.0);
    Ok(ResizeUpdate {
        x: point[0] - width * scale * fixed[0],
        y: point[1] - (height + legend) * scale * fixed[1],
        width,
        height,
    })
}
