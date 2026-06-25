//! Shared widget geometry, progress, and drawing helpers.
//!
//! Route and elevation widgets use the same marker, polyline, transform,
//! progress interpolation, and shadow logic. Keeping those pieces here avoids
//! subtle visual drift between widget implementations.

use super::types::{
    ShadowStyle, StaticLayer, WidgetFrameReport, WidgetGeometry, WidgetGeometryReport,
    WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use skia_safe::Canvas;

pub(crate) const DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER: f32 = 1.0;

// Interpolates one numeric value from sorted valid points.
// Delegates to the shared `interpolation` leaf module.
use crate::interpolation::interpolate_points as interpolate_points_f64; // intentionally aliased

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
    crate::activity::interpolate::interpolate_numeric_series_value(
        x_values,
        &y_values,
        elapsed_second,
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

// Normalizes optional progress values to a `0.0..=1.0` window.
pub(crate) fn normalize_optional_progress_window(
    progress_values: &[Option<f64>],
) -> Option<Vec<f64>> {
    // Optional progress appears after trimming. Normalize from the first and
    // last valid values, filling gaps by index to keep geometry monotonic.
    let start = progress_values.iter().find_map(|value| *value)?;
    let end = progress_values.iter().rev().find_map(|value| *value)?;
    let span = end - start;
    if !start.is_finite() || !end.is_finite() || span <= 0.0 {
        return None;
    }

    Some(
        progress_values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .filter(|progress| progress.is_finite())
                    .map(|progress| ((progress - start) / span).clamp(0.0, 1.0))
                    .unwrap_or_else(|| {
                        if progress_values.len() > 1 {
                            index as f64 / (progress_values.len() - 1) as f64
                        } else {
                            0.0
                        }
                    })
            })
            .collect::<Vec<_>>(),
    )
}

// Finds an interpolated point for target progress using a reusable search cursor.
pub(crate) fn point_at_metric_progress_with_cursor(
    points: &[(f32, f32)],
    progress_values: &[f32],
    target_progress: f32,
    cursor: &mut usize,
) -> Option<(usize, f32, f32)> {
    // The cursor is monotonic during frame iteration, making repeated lookups
    // effectively O(n) across the whole render instead of O(n) per frame.
    if points.is_empty() || progress_values.len() != points.len() {
        return None;
    }

    if points.len() == 1 {
        *cursor = 0;
        return Some((0, points[0].0, points[0].1));
    }

    let safe_target = target_progress.clamp(0.0, 1.0);
    let last_index = points.len() - 1;
    if safe_target <= progress_values[0] {
        *cursor = 0;
        return Some((1, points[0].0, points[0].1));
    }
    if safe_target >= progress_values[last_index] {
        *cursor = last_index.saturating_sub(1);
        return Some((last_index, points[last_index].0, points[last_index].1));
    }

    while *cursor + 1 < progress_values.len() && progress_values[*cursor + 1] < safe_target {
        *cursor += 1;
    }

    while *cursor > 0 && progress_values[*cursor] >= safe_target {
        *cursor -= 1;
    }

    let left_index = (*cursor).min(last_index.saturating_sub(1));
    let right_index = (left_index + 1).min(last_index);
    let left_progress = progress_values[left_index];
    let right_progress = progress_values[right_index];
    let span = (right_progress - left_progress).max(1e-6);
    let mix = (safe_target - left_progress) / span;
    let left_point = points[left_index];
    let right_point = points[right_index];
    if right_progress <= left_progress {
        return Some((right_index, left_point.0, left_point.1));
    }
    Some((
        right_index,
        left_point.0 + (right_point.0 - left_point.0) * mix,
        left_point.1 + (right_point.1 - left_point.1) * mix,
    ))
}

#[cfg(test)]
mod tests {
    use super::point_at_metric_progress_with_cursor;

    #[test]
    fn metric_progress_cursor_rewinds_to_front_of_duplicate_run() {
        let points = vec![(0.0, 0.0), (10.0, 10.0), (10.0, 30.0), (20.0, 40.0)];
        let progress_values = vec![0.0, 0.5, 0.5, 1.0];
        let mut cursor = 0usize;

        let first = point_at_metric_progress_with_cursor(&points, &progress_values, 0.75, &mut cursor)
            .expect("first lookup should resolve");
        assert_eq!(first.0, 3);

        let plateau =
            point_at_metric_progress_with_cursor(&points, &progress_values, 0.5, &mut cursor)
                .expect("plateau lookup should resolve");

        assert_eq!(plateau.0, 1, "should use first point in duplicate-progress run");
        assert!((plateau.1 - 10.0).abs() <= 1e-3);
        assert!((plateau.2 - 10.0).abs() <= 1e-3);
    }
}

// Selects a point by evenly mapping progress to the point index domain.
pub(crate) fn point_at_progress_x(
    points: &[(f32, f32)],
    progress01: f32,
) -> Option<(usize, f32, f32)> {
    let target_x = progress01.clamp(0.0, 1.0);
    let scaled_index = target_x * (points.len().saturating_sub(1) as f32);
    let index = scaled_index.floor() as usize;
    let point = points.get(index.min(points.len().saturating_sub(1)))?;
    Some((index, point.0, point.1))
}

// Converts validated scene shadow fields into a drawable shadow style.
pub(crate) fn normalize_shadow_style_validated(
    color: &String,
    strength: f32,
    distance: f32,
    scale: f32,
) -> Option<ShadowStyle> {
    let color = color.clone();
    let strength = strength * scale;
    let distance = distance * scale;
    if strength <= 0.0 {
        return None;
    }
    Some(ShadowStyle {
        color,
        strength,
        distance,
        offset_x: distance,
        offset_y: distance,
    })
}

// Converts a desired screen-space shadow into widget-local coordinates so it
// remains visually down/right after the widget itself is rotated.
pub(crate) fn shadow_with_screen_offset(
    shadow: Option<ShadowStyle>,
    rotation_deg: f32,
) -> Option<ShadowStyle> {
    shadow.map(|mut shadow| {
        if rotation_deg != 0.0 && shadow.distance != 0.0 {
            let radians = rotation_deg.to_radians();
            let distance = shadow.distance;
            shadow.offset_x = (radians.cos() + radians.sin()) * distance;
            shadow.offset_y = (radians.cos() - radians.sin()) * distance;
        }
        shadow
    })
}

// Computes cached-layer padding needed for strokes and shadows.
pub(crate) fn static_layer_padding(line_width: f32, shadow: Option<&ShadowStyle>) -> u32 {
    // Static layers can include shadows that extend beyond the widget bounds.
    // Padding prevents the cached image from clipping those pixels.
    let shadow_extent = shadow
        .map(|shadow| shadow.offset_x.abs().max(shadow.offset_y.abs()) + shadow.strength * 3.0)
        .unwrap_or(0.0);
    (line_width.max(1.0) * 0.5 + shadow_extent).ceil().max(0.0) as u32
}

// Draws a pre-rendered static widget layer if it exists.
pub(crate) fn draw_static_layer(canvas: &Canvas, layer: Option<&StaticLayer>) {
    if let Some(layer) = layer {
        canvas.draw_image(&layer.image, (layer.x, layer.y), None);
    }
}

// Converts a widget-local point into absolute canvas coordinates.
pub(crate) fn rotate_point_to_canvas(
    x: f32,
    y: f32,
    widget_x: f32,
    widget_y: f32,
    _width: f32,
    _height: f32,
    rotation_deg: f32,
) -> (f32, f32) {
    // Used for preview reports and labels that are drawn outside the widget's
    // transformed canvas state.
    if rotation_deg == 0.0 {
        return (widget_x + x, widget_y + y);
    }
    let radians = rotation_deg.to_radians();
    let rotated_x = x * radians.cos() - y * radians.sin();
    let rotated_y = x * radians.sin() + y * radians.cos();
    (widget_x + rotated_x, widget_y + rotated_y)
}

// Builds preview diagnostics for a rendered widget frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn widget_render_report(
    widget_x: f32,
    widget_y: f32,
    widget_width: u32,
    widget_height: u32,
    rotation_deg: f32,
    geometry: &WidgetGeometry,
    progress01: f32,
    marker_x: f32,
    marker_y: f32,
) -> WidgetRenderReport {
    let (marker_abs_x, marker_abs_y) = rotate_point_to_canvas(
        marker_x,
        marker_y,
        widget_x,
        widget_y,
        widget_width as f32,
        widget_height as f32,
        rotation_deg,
    );
    WidgetRenderReport {
        geometry: WidgetGeometryReport {
            point_count: geometry.points.len(),
            source_point_count: geometry.source_point_count,
            simplification: geometry.simplification.clone(),
            bbox: [
                geometry.bbox.0,
                geometry.bbox.1,
                geometry.bbox.2,
                geometry.bbox.3,
            ],
            widget_width,
            widget_height,
            rotation_deg,
        },
        frame: WidgetFrameReport {
            progress01,
            marker_x,
            marker_y,
            marker_abs_x,
            marker_abs_y,
        },
    }
}

// Formats a marker elevation label in metric or imperial units.
pub(crate) fn format_elevation_label(
    value_m: f64,
    unit: &str,
    decimal_rounding: Option<i32>,
) -> String {
    let (converted, suffix) = match unit {
        "imperial" => (value_m * 3.28084, " FT"),
        _ => (value_m, " M"),
    };
    let value_text = match decimal_rounding {
        Some(0) => format!("{}", converted.round() as i64),
        Some(decimals) if decimals > 0 => format!("{:.*}", decimals as usize, converted),
        _ => format!("{}", converted.round() as i64),
    };
    format!("{value_text}{suffix}")
}
