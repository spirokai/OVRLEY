use crate::activity::schema::{DenseActivityReport, NumericSeries, ParsedActivity};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::ValidatedSceneConfig;
use crate::render::widgets::types::GForceWidgetCache;
use ovrley_widgets::widgets::g_force::{
    format::derive_max_g,
    validation::{GForceAxis, ValidatedGForceWidget},
};

fn activity_axis(activity: &ParsedActivity, axis: GForceAxis) -> &NumericSeries {
    match axis {
        GForceAxis::X => &activity.g_force_x,
        GForceAxis::Y => &activity.g_force_y,
        GForceAxis::Z => &activity.g_force_z,
    }
}

fn dense_axis(dense: &DenseActivityReport, axis: GForceAxis) -> &[Option<f64>] {
    match axis {
        GForceAxis::X => &dense.series.g_force_x,
        GForceAxis::Y => &dense.series.g_force_y,
        GForceAxis::Z => &dense.series.g_force_z,
    }
}

fn oriented_values(values: &[Option<f64>], invert: bool) -> Vec<Option<f64>> {
    let multiplier = if invert { -1.0 } else { 1.0 };
    values
        .iter()
        .map(|value| value.map(|value| value * multiplier))
        .collect()
}

pub fn prepare_g_force_cache(
    widget: &ValidatedGForceWidget,
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<GForceWidgetCache> {
    prepare_profiler.measure("g_force.prepare", || {
        Ok(GForceWidgetCache {
            drawing: ovrley_widgets::widgets::g_force::layout::prepare_g_force_cache(
                widget,
                &scene.presentation,
            )?,
            max_g: derive_max_g(
                activity_axis(activity, widget.axis_horizontal),
                activity_axis(activity, widget.axis_vertical),
                widget.clip_percentile,
            ),
            horizontal_values: oriented_values(
                dense_axis(dense_activity, widget.axis_horizontal),
                widget.invert_horizontal,
            ),
            vertical_values: oriented_values(
                dense_axis(dense_activity, widget.axis_vertical),
                widget.invert_vertical,
            ),
        })
    })
}
