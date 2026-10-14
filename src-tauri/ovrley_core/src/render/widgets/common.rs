//! Native export progress and time-window preparation.

use crate::activity::schema::{DenseActivityReport, ParsedActivity};

// Interpolates one numeric value from sorted valid points.
// Delegates to the shared `interpolation` leaf module.
use ovrley_widgets::interpolation::interpolate_points as interpolate_points_f64; // intentionally aliased

// Interpolates a numeric series at many target positions.
fn interpolate_numeric_series_many(
    x_values: &[f64],
    y_values: &[f64],
    target_x_values: &[f64],
) -> Vec<f32> {
    let valid_points = x_values
        .iter()
        .copied()
        .zip(y_values.iter().copied())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect::<Vec<_>>();
    if valid_points.is_empty() {
        return vec![0.0; target_x_values.len()];
    }

    target_x_values
        .iter()
        .map(|target_x| interpolate_points_f64(&valid_points, *target_x).unwrap_or(0.0) as f32)
        .collect()
}

// Returns frame-by-frame progress values used to animate plot markers.
pub(crate) fn frame_progress_values(
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    scene: &crate::normalize::ValidatedSceneConfig,
) -> Vec<f32> {
    // Prefer dense distance progress when available because it maps widget
    // marker movement to real activity distance rather than elapsed time.
    let total_frames = dense_activity.frame_count.max(1);
    if dense_activity.frame_distance_progress.len() == total_frames {
        return dense_activity
            .frame_distance_progress
            .iter()
            .map(|value| value.unwrap_or_default().clamp(0.0, 1.0) as f32)
            .collect();
    }

    if activity.sample_elapsed_seconds.len() >= 2
        && activity.sample_distance_progress.len() >= 2
        && !dense_activity.frame_elapsed_seconds.is_empty()
    {
        let target_elapsed_seconds = dense_activity
            .frame_elapsed_seconds
            .iter()
            .map(|elapsed| scene.start + *elapsed)
            .collect::<Vec<_>>();
        return interpolate_numeric_series_many(
            &activity.sample_elapsed_seconds,
            &activity.sample_distance_progress,
            &target_elapsed_seconds,
        )
        .into_iter()
        .map(|value| value.clamp(0.0, 1.0))
        .collect();
    }

    if total_frames == 1 {
        return vec![0.0];
    }
    (0..total_frames)
        .map(|frame_index| frame_index as f32 / (total_frames - 1) as f32)
        .collect()
}

/// Returns whether the template is rendering a custom scene subset.
///
/// Reads the typed `SceneConfig::custom_export_range_active` field.
/// Templates that omit the field (including all older JSON) default to `false`,
/// preserving backward-compatible behavior.
pub(crate) fn custom_export_range_active(scene: &crate::normalize::ValidatedSceneConfig) -> bool {
    scene.custom_export_range_active.unwrap_or(false)
}

// Returns the elapsed-time denominator used by trimmed/full-activity widgets.
pub(crate) fn scoped_source_duration(
    scene: &crate::normalize::ValidatedSceneConfig,
    activity: &ParsedActivity,
    show_full_activity: bool,
) -> f64 {
    if custom_export_range_active(scene) && !show_full_activity {
        (scene.end - scene.start).max(1e-9)
    } else {
        activity
            .sample_elapsed_seconds
            .last()
            .copied()
            .unwrap_or(1.0)
            .max(1e-9)
    }
}

// Interpolates absolute activity distance progress at an elapsed second.
pub(crate) fn interpolate_distance_progress_at_elapsed(
    activity: &ParsedActivity,
    elapsed_second: f64,
) -> Option<f64> {
    let x_values = &activity.sample_elapsed_seconds;
    let y_values = activity
        .sample_distance_progress
        .iter()
        .copied()
        .map(Some)
        .collect::<Vec<_>>();
    ovrley_widgets::interpolation::interpolate_numeric_series_value(
        x_values,
        &y_values,
        elapsed_second,
        ovrley_widgets::interpolation::MissingSamplePolicy::Bridge,
    )
}

// Converts absolute frame progress values into the current trim window.
pub(crate) fn relative_distance_frame_progress_values(
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    scene: &crate::normalize::ValidatedSceneConfig,
) -> Option<Vec<f32>> {
    // Convert absolute activity progress into trim-local progress for custom
    // export windows, preserving the visual start/end of the selected range.
    let start_progress = interpolate_distance_progress_at_elapsed(activity, scene.start)?;
    let end_progress = interpolate_distance_progress_at_elapsed(activity, scene.end)?;
    let span = end_progress - start_progress;
    if span <= 0.0 {
        return None;
    }

    let frame_progress = frame_progress_values(activity, dense_activity, scene);
    Some(
        frame_progress
            .into_iter()
            .map(|progress| ((progress as f64 - start_progress) / span).clamp(0.0, 1.0) as f32)
            .collect(),
    )
}
