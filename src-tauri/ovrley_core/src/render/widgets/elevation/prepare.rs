//! Native activity/timing preparation; profile geometry and drawing are shared.
use super::super::{common::custom_export_range_active, types::ElevationWidgetCache};
use crate::{
    activity::{
        elevation::{elevation_profile_series, preferred_elevation_series},
        schema::{DenseActivityReport, ParsedActivity},
    },
    debug::RenderProfiler,
    error::CoreResult,
    fonts::NativeFonts,
    normalize::ValidatedSceneConfig,
    paths::AppPaths,
};
use ovrley_widgets::widgets::{
    elevation::{
        geometry::ElevationGeometry, layout::ElevationLayout, sampling::ElevationActivity,
        validation::ValidatedElevationPlot,
    },
    route::sampling::RouteWindow,
};
use std::sync::Arc;

fn elevation_activity(activity: &ParsedActivity) -> ElevationActivity {
    ElevationActivity::new(
        &activity.sample_elapsed_seconds,
        elevation_profile_series(&activity.sample_elevations, &activity.elevation),
        preferred_elevation_series(&activity.barometric_altitude, &activity.elevation),
        &activity.sample_distance_progress,
        activity.trim_end_seconds,
    )
}

fn window(scene: &ValidatedSceneConfig, show_full_activity: bool) -> Option<RouteWindow> {
    (custom_export_range_active(scene) && !show_full_activity).then_some(RouteWindow {
        start: scene.start,
        end: scene.end,
    })
}

pub(crate) fn build_elevation_samples(
    activity: &ParsedActivity,
    show_full_activity: bool,
    scene: &ValidatedSceneConfig,
) -> Arc<Vec<(f32, f64, f32)>> {
    elevation_activity(activity).samples(window(scene, show_full_activity))
}

pub(crate) fn prepare_elevation_cache(
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    validated: &ValidatedElevationPlot,
    scene: &ValidatedSceneConfig,
    paths: &AppPaths,
    profiler: &mut RenderProfiler,
) -> CoreResult<ElevationWidgetCache> {
    profiler.measure("build_elevation_cache", || {
        let sampler = elevation_activity(activity);
        let samples = sampler.samples(window(scene, validated.show_full_activity));
        let geometry = Arc::new(ElevationGeometry::new(
            validated,
            scene.presentation.scale,
            &samples,
        )?);
        let drawing = ElevationLayout::new(
            validated.clone(),
            scene.presentation.clone(),
            geometry,
            sampler.altitude_offset(validated.starting_altitude_m),
            &NativeFonts(&paths.font_dirs),
        )?;
        let frame_states = super::frame_state::build_elevation_frame_states(
            scene,
            activity,
            dense,
            &drawing.geometry,
            validated,
            validated.show_full_activity,
        );
        Ok(ElevationWidgetCache {
            drawing,
            frame_states,
        })
    })
}
