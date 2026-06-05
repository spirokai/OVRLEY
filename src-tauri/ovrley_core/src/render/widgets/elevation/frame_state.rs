/// Per-frame state precomputation for the elevation widget.
///
/// Computes marker positions, elevation values, and completed-polyline prefix
/// points for every render frame so per-frame drawing performs minimal
/// arithmetic.
use super::super::common::{
    custom_export_range_active, frame_progress_values, point_at_metric_progress_with_cursor,
    point_at_progress_x, relative_distance_frame_progress_values,
};
use super::super::types::{ElevationFrameState, WidgetGeometry};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::normalize::ValidatedSceneConfig;

/// Precomputes marker coordinates and elevation values for each render frame.
///
/// Marker positions follow distance progress; displayed elevation values use
/// dense frame data when available and fall back to progress interpolation.
pub(crate) fn build_elevation_frame_states(
    scene: &ValidatedSceneConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    geometry: &WidgetGeometry,
    show_full_activity: bool,
) -> Vec<ElevationFrameState> {
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
        Some(interpolate_elevation_for_progresses(
            activity,
            &frame_progress,
        ))
    };
    let mut progress_cursor = 0usize;

    frame_progress
        .into_iter()
        .enumerate()
        .map(|(frame_index, progress01)| {
            let (_, marker_x, marker_y) = point_at_metric_progress_with_cursor(
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
            ElevationFrameState {
                progress01,
                marker_x,
                marker_y,
                elevation_m,
            }
        })
        .collect()
}

/// Builds the completed elevation polyline up to the current marker point.
///
/// The completed area/path includes all samples up to the current progress
/// and then the interpolated marker point for a smooth leading edge.
pub(crate) fn build_elevation_completed_points(
    points: &[(f32, f32)],
    progress_values: &[f32],
    progress01: f32,
    marker_point: (f32, f32),
) -> Vec<(f32, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut result = points
        .iter()
        .zip(progress_values.iter())
        .filter_map(|(point, progress)| (*progress <= progress01).then_some(*point))
        .collect::<Vec<_>>();
    if result.is_empty() {
        result.push(points[0]);
    }
    if super::super::geometry::distance(*result.last().unwrap_or(&points[0]), marker_point) > 1e-3 {
        result.push(marker_point);
    }
    result
}

/// Resolves elevation values for marker labels from progress positions.
///
/// Used when dense elevation was not explicitly requested but labels still
/// need a value at each marker progress.
fn interpolate_elevation_for_progresses(
    activity: &ParsedActivity,
    frame_progresses: &[f32],
) -> Vec<f64> {
    let elevations = if activity.sample_elevations.is_empty() {
        &activity.elevation
    } else {
        &activity.sample_elevations
    };
    let progress_values = &activity.sample_distance_progress;
    if elevations.is_empty() || progress_values.is_empty() {
        return vec![0.0; frame_progresses.len()];
    }

    frame_progresses
        .iter()
        .map(|progress01| {
            crate::interpolation::interpolate_optional_numeric_series(
                progress_values,
                elevations,
                *progress01 as f64,
            )
            .unwrap_or(0.0)
        })
        .collect()
}
