//! Native font resource boundary; layout and static-part rules live in the shared renderer.
use crate::CoreResult;
use ovrley_widgets::widgets::value::layout::StaticMetricParts;
use ovrley_widgets::{
    render::text::ResolvedTextStyle,
    widgets::value::{format::MetricDisplayParts, validation::ValidatedValueWidget},
};
use skia_safe::Canvas;
use std::path::PathBuf;
pub(crate) fn draw_metric_parts(
    canvas: &Canvas,
    style: &ResolvedTextStyle,
    parts: &MetricDisplayParts,
    scale: f32,
    font_dirs: &[PathBuf],
    static_parts: StaticMetricParts,
    validated: &ValidatedValueWidget,
) -> CoreResult<()> {
    Ok(ovrley_widgets::widgets::value::layout::draw_metric_parts(
        canvas,
        style,
        parts,
        scale,
        &crate::fonts::NativeFonts(font_dirs),
        static_parts,
        validated,
    )?)
}
pub(crate) fn draw_static_metric_parts_for_value(
    canvas: &Canvas,
    validated: &ValidatedValueWidget,
    style: &ResolvedTextStyle,
    scale: f32,
    font_dirs: &[PathBuf],
) -> CoreResult<StaticMetricParts> {
    Ok(
        ovrley_widgets::widgets::value::layout::draw_static_metric_parts_for_value(
            canvas,
            validated,
            style,
            scale,
            &crate::fonts::NativeFonts(font_dirs),
        )?,
    )
}
