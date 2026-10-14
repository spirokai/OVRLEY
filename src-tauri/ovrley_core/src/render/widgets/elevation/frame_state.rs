//! Export-only preparation from native dense samples and timing.
use super::super::common::{
    custom_export_range_active, frame_progress_values, relative_distance_frame_progress_values,
    scoped_source_duration,
};
use crate::{
    activity::{
        elevation::{elevation_profile_series, preferred_elevation_series},
        schema::{DenseActivityReport, ParsedActivity},
    },
    normalize::ValidatedSceneConfig,
};
use ovrley_widgets::{
    interpolation::{collect_valid_numeric_points, interpolate_points},
    widgets::elevation::{
        geometry::{ElevationFrameState, ElevationGeometry},
        validation::ValidatedElevationPlot,
    },
};

pub(crate) fn build_elevation_frame_states(
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense: &DenseActivityReport,
    geometry: &ElevationGeometry,
    _plot: &ValidatedElevationPlot,
    show_full_activity: bool,
) -> Vec<ElevationFrameState> {
    let progress = if custom_export_range_active(scene) && !show_full_activity {
        relative_distance_frame_progress_values(activity, dense, scene)
            .unwrap_or_else(|| frame_progress_values(activity, dense, scene))
    } else {
        frame_progress_values(activity, dense, scene)
    };
    let elevations =
        preferred_elevation_series(&dense.series.barometric_altitude, &dense.series.elevation);
    let fallback = (elevations.len() != progress.len()).then(|| {
        collect_valid_numeric_points(
            &activity.sample_elapsed_seconds,
            elevation_profile_series(&activity.sample_elevations, &activity.elevation),
        )
    });
    let duration = scoped_source_duration(scene, activity, show_full_activity);
    let start = if custom_export_range_active(scene) && !show_full_activity {
        scene.start
    } else {
        0.0
    };
    progress
        .into_iter()
        .enumerate()
        .map(|(i, progress)| {
            let second = scene.start + dense.frame_elapsed_seconds.get(i).copied().unwrap_or(0.0);
            let elevation = elevations
                .get(i)
                .and_then(|v| *v)
                .or_else(|| {
                    fallback
                        .as_ref()
                        .and_then(|p| interpolate_points(p, second))
                })
                .unwrap_or(0.0);
            geometry.state(
                progress,
                Some(elevation),
                ((second - start) / duration).clamp(0.0, 1.0) as f32,
            )
        })
        .collect()
}
