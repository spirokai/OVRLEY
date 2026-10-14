//! Native export preparation: timing and source activity belong to this host.
use super::super::common::custom_export_range_active;
use super::super::types::RouteWidgetCache;
use crate::{
    activity::schema::{DenseActivityReport, ParsedActivity},
    debug::RenderProfiler,
    error::CoreResult,
    normalize::ValidatedSceneConfig,
};
use ovrley_widgets::widgets::route::{
    geometry::{RouteGeometry, RouteSample},
    layout::RouteLayout,
    sampling::{RouteActivity, RouteWindow},
    validation::ValidatedRoutePlot,
};
use std::sync::Arc;

pub(crate) fn prepare_route_cache(
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    validated: &ValidatedRoutePlot,
    scene: &ValidatedSceneConfig,
    profiler: &mut RenderProfiler,
) -> CoreResult<RouteWidgetCache> {
    profiler.measure("build_route_cache", || {
        let route = RouteActivity::new(
            &activity.sample_elapsed_seconds,
            &activity.sample_course_points,
            &activity.sample_distance_progress,
            activity.trim_end_seconds,
        );
        let window = (custom_export_range_active(scene) && !validated.show_full_activity)
            .then_some(RouteWindow {
                start: scene.start,
                end: scene.end,
            });
        let samples = route.samples(window);
        let geometry = Arc::new(RouteGeometry::new(validated, &samples)?);
        let drawing = RouteLayout::new(validated.clone(), scene.presentation.clone(), geometry)?;
        let frame_progress = dense
            .frame_elapsed_seconds
            .iter()
            .map(|second| route.progress_at(scene.start + second, window))
            .collect();
        Ok(RouteWidgetCache {
            drawing,
            frame_progress,
        })
    })
}

/// Both hosts use the same boundary interpolation, projection and window normalization.
pub(crate) fn build_route_samples(
    activity: &ParsedActivity,
    show_full_activity: bool,
    scene: &ValidatedSceneConfig,
) -> Arc<Vec<RouteSample>> {
    let route = RouteActivity::new(
        &activity.sample_elapsed_seconds,
        &activity.sample_course_points,
        &activity.sample_distance_progress,
        activity.trim_end_seconds,
    );
    let window =
        (custom_export_range_active(scene) && !show_full_activity).then_some(RouteWindow {
            start: scene.start,
            end: scene.end,
        });
    route.samples(window)
}
