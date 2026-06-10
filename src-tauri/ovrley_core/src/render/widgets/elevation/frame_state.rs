/// Per-frame state precomputation for the elevation widget.
///
/// Computes marker positions, elevation values, and completed-polyline prefix
/// points for every render frame so per-frame drawing performs minimal
/// arithmetic.
use super::super::common::{
    custom_export_range_active, frame_progress_values, point_at_metric_progress_with_cursor,
    point_at_progress_x, relative_distance_frame_progress_values, scoped_source_duration,
};
use super::super::types::{ElevationFrameState, NormalizedElevationPlot, WidgetGeometry};
use super::reduction::project_single_elevation_y;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::normalize::ValidatedSceneConfig;

/// Precomputes marker coordinates and elevation values for each render frame.
///
/// When `elevation_data_range` is available in geometry, marker_y is projected
/// from the dense elevation data using the same y-projection formula as the
/// geometry. Otherwise falls back to geometry polyline lookup.
pub(crate) fn build_elevation_frame_states(
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    geometry: &WidgetGeometry,
    plot: &NormalizedElevationPlot,
    show_full_activity: bool,
) -> Vec<ElevationFrameState> {
    // Use relative progress for export window (maps onto trimmed geometry),
    // absolute progress for full activity.
    let frame_progress = if custom_export_range_active(scene)
        && !show_full_activity
        && !geometry.progress_values.is_empty()
    {
        relative_distance_frame_progress_values(activity, dense_activity, scene)
            .unwrap_or_else(|| frame_progress_values(activity, dense_activity, scene))
    } else {
        frame_progress_values(activity, dense_activity, scene)
    };
    let fallback_elevations = if dense_activity.series.elevation.len() == frame_progress.len() {
        None
    } else {
        Some(interpolate_elevation_for_elapsed_frames(
            activity,
            &dense_activity.frame_elapsed_seconds,
            scene.start,
        ))
    };

    // Source duration for elapsed fraction normalization — must match the
    // denominator used when building geometry.elapsed_fractions in Part A.
    let source_duration = scoped_source_duration(scene, activity, show_full_activity);

    let mut progress_cursor = 0usize;

    frame_progress
        .into_iter()
        .enumerate()
        .map(|(frame_index, progress01)| {
            let (_, marker_x, marker_y_from_geometry) = point_at_metric_progress_with_cursor(
                &geometry.points,
                &geometry.progress_values,
                progress01,
                &mut progress_cursor,
            )
            .or_else(|| point_at_progress_x(&geometry.points, progress01))
            .unwrap_or((0, 0.0, 0.0));
            let elevation_m = dense_activity
                .series
                .elevation
                .get(frame_index)
                .and_then(|value| *value)
                .or_else(|| {
                    fallback_elevations
                        .as_ref()
                        .and_then(|values| values.get(frame_index).copied())
                })
                .unwrap_or(0.0);
            let marker_y = if let Some((min_elev, max_elev)) = geometry.elevation_data_range {
                project_single_elevation_y(
                    elevation_m,
                    min_elev,
                    max_elev,
                    plot.height as f32,
                    0.0,
                    plot.y_scale,
                )
            } else {
                marker_y_from_geometry
            };
            let frame_elapsed_fraction = dense_activity
                .frame_elapsed_seconds
                .get(frame_index)
                .map(|elapsed| (*elapsed / source_duration).clamp(0.0, 1.0) as f32)
                .unwrap_or(0.0);
            ElevationFrameState {
                progress01,
                marker_x,
                marker_y,
                elevation_m,
                frame_elapsed_fraction,
            }
        })
        .collect()
}

/// Builds the completed elevation polyline up to the current marker point.
///
/// Filters by elapsed fraction (chronological order) so that vertical
/// segments fill progressively as time advances, not all-at-once by progress.
pub(crate) fn build_elevation_completed_points(
    points: &[(f32, f32)],
    elapsed_fractions: &[f32],
    frame_elapsed_fraction: f32,
    marker_point: (f32, f32),
) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut result = points
        .iter()
        .zip(elapsed_fractions.iter())
        .filter_map(|(point, elapsed)| (*elapsed <= frame_elapsed_fraction).then_some(*point))
        .collect::<Vec<_>>();
    if result.is_empty() {
        result.push(points[0]);
    }
    if super::super::geometry::distance(*result.last().unwrap_or(&points[0]), marker_point) > 1e-3 {
        result.push(marker_point);
    }
    result
}

/// Resolves elevation values for marker labels from frame elapsed times.
///
/// Used when dense elevation was not explicitly requested but labels still
/// need a value at each frame.
fn interpolate_elevation_for_elapsed_frames(
    activity: &ParsedActivity,
    frame_elapsed_seconds: &[f64],
    scene_start: f64,
) -> Vec<f64> {
    let elevations = if activity.sample_elevations.is_empty() {
        &activity.elevation
    } else {
        &activity.sample_elevations
    };
    let elapsed_seconds = &activity.sample_elapsed_seconds;
    if elevations.is_empty() || elapsed_seconds.is_empty() {
        return vec![0.0; frame_elapsed_seconds.len()];
    }

    frame_elapsed_seconds
        .iter()
        .map(|frame_elapsed| {
            crate::interpolation::interpolate_optional_numeric_series(
                elapsed_seconds,
                elevations,
                scene_start + *frame_elapsed,
            )
            .unwrap_or(0.0)
        })
        .collect()
}
