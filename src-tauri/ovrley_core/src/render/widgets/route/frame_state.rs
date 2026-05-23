/// Frame-state precomputation for the route widget.
///
/// Computes marker coordinates and completed-polyline prefix points for every
/// render frame so per-frame drawing performs minimal arithmetic.

use super::super::common::{
    custom_export_range_active, frame_progress_values, point_at_metric_progress_with_cursor,
    relative_distance_frame_progress_values,
};
use super::super::geometry::distance;
use super::super::types::{RouteFrameState, WidgetGeometry};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;

/// Precomputes route marker coordinates for each render frame.
///
/// Custom export windows can use trim-relative distance progress so the
/// marker starts at the beginning of the exported slice rather than the
/// full activity.
pub(crate) fn build_route_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    geometry: &WidgetGeometry,
    dense_activity: &DenseActivityReport,
    show_full_activity: bool,
) -> Vec<RouteFrameState> {
    let frame_progress = if custom_export_range_active(config)
        && !show_full_activity
        && !geometry.progress_values.is_empty()
    {
        relative_distance_frame_progress_values(config, activity, dense_activity)
            .unwrap_or_else(|| frame_progress_values(config, activity, dense_activity))
    } else {
        frame_progress_values(config, activity, dense_activity)
    };
    let mut progress_cursor = 0usize;
    frame_progress
        .into_iter()
        .map(|progress01| {
            let (segment_index, marker_x, marker_y) = point_at_metric_progress_with_cursor(
                &geometry.points,
                &geometry.progress_values,
                progress01,
                &mut progress_cursor,
            )
            .unwrap_or_else(|| route_position_at_progress(&geometry.points, progress01));
            RouteFrameState {
                progress01,
                marker_x,
                marker_y,
                segment_index,
            }
        })
        .collect()
}

/// Falls back to index-based route marker placement when metric progress
/// is unavailable.
fn route_position_at_progress(points: &[(f32, f32)], progress_limit: f32) -> (usize, f32, f32) {
    if points.is_empty() {
        return (0, 0.0, 0.0);
    }
    if points.len() == 1 {
        return (0, points[0].0, points[0].1);
    }
    if progress_limit <= 0.0 {
        return (1, points[0].0, points[0].1);
    }
    if progress_limit >= 1.0 {
        let last_index = points.len() - 1;
        return (last_index, points[last_index].0, points[last_index].1);
    }
    for index in 1..points.len() {
        let start_progress = (index - 1) as f32 / (points.len() - 1) as f32;
        let end_progress = index as f32 / (points.len() - 1) as f32;
        if progress_limit >= end_progress {
            continue;
        }
        let span = (end_progress - start_progress).max(1e-6);
        let mix = (progress_limit - start_progress) / span;
        let (start_x, start_y) = points[index - 1];
        let (end_x, end_y) = points[index];
        return (
            index,
            start_x + (end_x - start_x) * mix,
            start_y + (end_y - start_y) * mix,
        );
    }
    let last_index = points.len() - 1;
    (last_index, points[last_index].0, points[last_index].1)
}

/// Builds the completed route polyline up to the current marker point.
///
/// The completed route includes all simplified points before the marker plus
/// the interpolated marker point itself.
pub(crate) fn build_route_prefix_points(
    geometry: &WidgetGeometry,
    state: &RouteFrameState,
) -> Vec<(f32, f32)> {
    if geometry.points.is_empty() {
        return Vec::new();
    }

    let last_point = *geometry
        .points
        .last()
        .unwrap_or(&(state.marker_x, state.marker_y));
    let mut points = if distance(last_point, (state.marker_x, state.marker_y)) <= 1e-3 {
        geometry.points.clone()
    } else {
        let prefix_len = state.segment_index.min(geometry.points.len());
        geometry.points[..prefix_len].to_vec()
    };
    if points.is_empty() {
        points.push(geometry.points[0]);
    }
    if distance(
        *points.last().unwrap_or(&(f32::MIN, f32::MIN)),
        (state.marker_x, state.marker_y),
    ) > 1e-3
    {
        points.push((state.marker_x, state.marker_y));
    }
    points
}
