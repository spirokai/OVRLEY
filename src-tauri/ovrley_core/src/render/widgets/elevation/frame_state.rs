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
    progress_values: &[f32],
    elapsed_fractions: &[f32],
    progress01: f32,
    frame_elapsed_fraction: f32,
) -> Vec<(f32, f32)> {
    if points.is_empty() || progress_values.len() != points.len() {
        return Vec::new();
    }

    let mut progress_cursor = 0usize;
    let metric_hit = point_at_metric_progress_with_cursor(
        points,
        progress_values,
        progress01,
        &mut progress_cursor,
    )
    .map(|(index, x, y)| (index, (x, y)))
    .unwrap_or((points.len().saturating_sub(1), points[points.len() - 1]));
    let metric_index = metric_hit.0;
    let duplicate_run = find_duplicate_progress_run(progress_values, progress01, metric_index);
    let mut completed_points = Vec::new();
    let completed_endpoint = if let Some((run_start, run_end)) = duplicate_run {
        completed_points.extend_from_slice(&points[..run_start]);

        for index in run_start..=run_end {
            if elapsed_fractions.get(index).copied().unwrap_or(0.0) < frame_elapsed_fraction {
                completed_points.push(points[index]);
            }
        }

        let run_points = &points[run_start..=run_end];
        let run_elapsed_fractions = &elapsed_fractions[run_start..=run_end];
        let mut run_cursor = 0usize;
        point_at_metric_progress_with_cursor(
            run_points,
            run_elapsed_fractions,
            frame_elapsed_fraction,
            &mut run_cursor,
        )
        .map(|(_, x, y)| (x, y))
        .unwrap_or(*run_points.last().unwrap_or(&metric_hit.1))
    } else {
        completed_points.extend_from_slice(&points[..metric_index.min(points.len())]);
        metric_hit.1
    };

    if completed_points.is_empty() {
        completed_points.push(points[0]);
    }

    if super::super::geometry::distance(
        *completed_points.last().unwrap_or(&points[0]),
        completed_endpoint,
    ) > 1e-3
    {
        completed_points.push(completed_endpoint);
    }

    completed_points
}

const METRIC_PROGRESS_EPSILON: f32 = 1e-6;

fn metric_progress_equal(left: f32, right: f32) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= METRIC_PROGRESS_EPSILON
}

fn find_duplicate_progress_run(
    progress_values: &[f32],
    target_progress: f32,
    anchor_index: usize,
) -> Option<(usize, usize)> {
    if progress_values.is_empty() {
        return None;
    }

    let safe_anchor_index = anchor_index.min(progress_values.len() - 1);
    let anchor_progress = progress_values[safe_anchor_index];
    if !metric_progress_equal(anchor_progress, target_progress) {
        return None;
    }

    let mut start = safe_anchor_index;
    let mut end = safe_anchor_index;

    while start > 0 && metric_progress_equal(progress_values[start - 1], anchor_progress) {
        start -= 1;
    }

    while end + 1 < progress_values.len()
        && metric_progress_equal(progress_values[end + 1], anchor_progress)
    {
        end += 1;
    }

    (end > start).then_some((start, end))
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
