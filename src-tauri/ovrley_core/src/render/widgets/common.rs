//! Shared widget geometry, progress, and drawing helpers.
//!
//! Route and elevation widgets use the same marker, polyline, transform,
//! progress interpolation, and shadow logic. Keeping those pieces here avoids
//! subtle visual drift between widget implementations.

use super::types::{
    MarkerLayer, ShadowStyle, StaticLayer, WidgetFrameReport, WidgetGeometry, WidgetGeometryReport,
    WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::MarkerPointConfig;
use crate::config::RenderConfig;
use skia_safe::{BlurStyle, Canvas, MaskFilter, Paint, PaintCap, PaintJoin, Path as SkPath, Point};

pub(crate) const DEFAULT_COLOR: &str = "#ffffff";
pub(crate) const DEFAULT_LINE_WIDTH: f32 = 1.75;
pub(crate) const DEFAULT_POINT_WEIGHT: f32 = 80.0;
pub(crate) const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX: f32 = 1.0;
pub(crate) const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER: f32 = 2.5;
pub(crate) const DEFAULT_ELEVATION_MARKER_SCALE: f32 = 2.5;
pub(crate) const DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER: f32 = 2.5;

// Normalizes opacity from either `0..=1` or legacy `0..=100` input.
pub(crate) fn normalize_opacity(value: Option<f32>, default: f32) -> f32 {
    match value {
        Some(value) if value > 1.0 => (value / 100.0).clamp(0.0, 1.0),
        Some(value) => value.clamp(0.0, 1.0),
        None => default,
    }
}

// Computes Euclidean distance between two widget-space points.
pub(crate) fn distance(left: (f32, f32), right: (f32, f32)) -> f32 {
    ((right.0 - left.0).powi(2) + (right.1 - left.1).powi(2)).sqrt()
}

// Fits source points into widget bounds while preserving aspect ratio.
pub(crate) fn fit_points_to_widget_with_inset(
    points: &[(f32, f32)],
    width: f32,
    height: f32,
    inset_px: f32,
    invert_y: bool,
) -> Vec<(f32, f32)> {
    // Preserve aspect ratio and center the fitted route/profile inside the
    // widget while reserving space for strokes, shadows, and marker radii.
    if points.is_empty() {
        return Vec::new();
    }

    let min_x = points.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    let safe_inset = inset_px.max(0.0).min(width.min(height) * 0.45);
    let inner_width = (width - safe_inset * 2.0).max(1.0);
    let inner_height = (height - safe_inset * 2.0).max(1.0);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    let scale = (inner_width / span_x).min(inner_height / span_y);
    let offset_x = (width - span_x * scale) / 2.0;
    let offset_y = (height - span_y * scale) / 2.0;

    points
        .iter()
        .map(|(x, y)| {
            let fitted_x = (x - min_x) * scale + offset_x;
            let mut fitted_y = (y - min_y) * scale + offset_y;
            if invert_y {
                fitted_y = height - fitted_y;
            }
            (fitted_x, fitted_y)
        })
        .collect()
}

// Interpolates one numeric value from sorted valid points.
fn interpolate_numeric_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    if points.is_empty() {
        return None;
    }
    if target_x <= points[0].0 {
        return Some(points[0].1);
    }
    let last_index = points.len() - 1;
    if target_x >= points[last_index].0 {
        return Some(points[last_index].1);
    }

    let right_index = points.partition_point(|(x, _)| *x < target_x);
    if right_index == 0 {
        return Some(points[0].1);
    }
    if right_index >= points.len() {
        return Some(points[last_index].1);
    }

    let (left_x, left_y) = points[right_index - 1];
    let (right_x, right_y) = points[right_index];
    let delta = (right_x - left_x).max(f64::EPSILON);
    let mix = (target_x - left_x) / delta;
    Some(left_y + (right_y - left_y) * mix)
}

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
        .map(|target_x| interpolate_numeric_points(&valid_points, *target_x).unwrap_or(0.0) as f32)
        .collect()
}

// Returns frame-by-frame progress values used to animate plot markers.
pub(crate) fn frame_progress_values(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
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
            .map(|elapsed| config.scene.start + *elapsed)
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

// Returns whether the template is rendering a custom scene subset.
pub(crate) fn custom_export_range_active(config: &RenderConfig) -> bool {
    config
        .scene
        .extra
        .get("custom_export_range_active")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
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
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> Option<Vec<f32>> {
    // Convert absolute activity progress into trim-local progress for custom
    // export windows, preserving the visual start/end of the selected range.
    let start_progress = interpolate_distance_progress_at_elapsed(activity, config.scene.start)?;
    let end_progress = interpolate_distance_progress_at_elapsed(activity, config.scene.end)?;
    let span = end_progress - start_progress;
    if span <= 0.0 {
        return None;
    }

    let frame_progress = frame_progress_values(config, activity, dense_activity);
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

// Interpolates one optional numeric series at a target x-value.
pub(crate) fn interpolate_optional_numeric_series(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    let valid = x_values
        .iter()
        .copied()
        .zip(y_values.iter().copied())
        .filter_map(|(x, y)| y.map(|value| (x, value)))
        .collect::<Vec<_>>();
    if valid.is_empty() {
        return None;
    }
    if target_x <= valid[0].0 {
        return Some(valid[0].1);
    }
    let last_index = valid.len() - 1;
    if target_x >= valid[last_index].0 {
        return Some(valid[last_index].1);
    }
    for index in 1..valid.len() {
        let (left_x, left_y) = valid[index - 1];
        let (right_x, right_y) = valid[index];
        if right_x < target_x {
            continue;
        }
        let delta = (right_x - left_x).max(f64::EPSILON);
        let mix = (target_x - left_x) / delta;
        return Some(left_y + (right_y - left_y) * mix);
    }
    Some(valid[last_index].1)
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

    while *cursor > 0 && progress_values[*cursor] > safe_target {
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
    Some((
        right_index,
        left_point.0 + (right_point.0 - left_point.0) * mix,
        left_point.1 + (right_point.1 - left_point.1) * mix,
    ))
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

// Draws a stroked polyline without shadow.
pub(crate) fn draw_polyline(
    canvas: &Canvas,
    points: &[(f32, f32)],
    color: &str,
    width: f32,
    opacity: f32,
) {
    draw_polyline_with_shadow(canvas, points, color, width, opacity, None);
}

// Draws a stroked polyline with an optional drop shadow.
pub(crate) fn draw_polyline_with_shadow(
    canvas: &Canvas,
    points: &[(f32, f32)],
    color: &str,
    width: f32,
    opacity: f32,
    shadow: Option<&ShadowStyle>,
) {
    if points.len() < 2 {
        return;
    }
    let path = path_from_points(points, false, None);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width.max(1.0));
    paint.set_stroke_cap(PaintCap::Round);
    paint.set_stroke_join(PaintJoin::Round);
    paint.set_color(crate::render::text::parse_color(color, opacity));

    if let Some(shadow) = shadow {
        if shadow.strength > 0.0 || shadow.distance != 0.0 {
            let mut shadow_paint = Paint::default();
            shadow_paint.set_anti_alias(true);
            shadow_paint.set_style(skia_safe::paint::Style::Stroke);
            shadow_paint.set_stroke_width(width.max(1.0));
            shadow_paint.set_stroke_cap(PaintCap::Round);
            shadow_paint.set_stroke_join(PaintJoin::Round);
            shadow_paint.set_color(crate::render::text::parse_color(&shadow.color, opacity));
            if shadow.strength > 0.0 {
                shadow_paint.set_mask_filter(MaskFilter::blur(
                    BlurStyle::Normal,
                    shadow.strength,
                    true,
                ));
            }
            canvas.save();
            canvas.translate((shadow.offset_x, shadow.offset_y));
            canvas.draw_path(&path, &shadow_paint);
            canvas.restore();
        }
    }

    canvas.draw_path(&path, &paint);
}

// Draws a filled area under a polyline down to `baseline_y`.
pub(crate) fn draw_area(
    canvas: &Canvas,
    points: &[(f32, f32)],
    baseline_y: f32,
    color: &str,
    opacity: f32,
) {
    if points.len() < 2 {
        return;
    }
    let path = path_from_points(points, true, Some(baseline_y));
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::paint::Style::Fill);
    paint.set_color(crate::render::text::parse_color(color, opacity));
    canvas.draw_path(&path, &paint);
}

// Draws the configured marker layers or a fallback circular marker.
pub(crate) fn draw_marker(
    canvas: &Canvas,
    layers: &[MarkerLayer],
    x: f32,
    y: f32,
    fallback_color: &str,
    fallback_radius: f32,
    fallback_opacity: f32,
) {
    // Marker points are rendered largest-to-smallest. The smallest layer is
    // filled to create a visible center; larger layers become rings.
    if layers.is_empty() {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Fill);
        paint.set_color(crate::render::text::parse_color(
            fallback_color,
            fallback_opacity,
        ));
        canvas.draw_circle(Point::new(x, y), fallback_radius.max(2.0), &paint);
        return;
    }

    for layer in layers {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(crate::render::text::parse_color(
            &layer.color,
            layer.opacity,
        ));
        if layer.solid_fill {
            paint.set_style(skia_safe::paint::Style::Fill);
            canvas.draw_circle(Point::new(x, y), layer.radius, &paint);
        } else {
            paint.set_style(skia_safe::paint::Style::Stroke);
            paint.set_stroke_width((layer.radius * 0.18).round().clamp(1.0, 3.0));
            canvas.draw_circle(Point::new(x, y), layer.radius, &paint);
        }
    }
}

// Converts marker point config into sorted drawable marker layers.
pub(crate) fn marker_layers_from_points(points: &[MarkerPointConfig]) -> Vec<MarkerLayer> {
    let mut layers = points
        .iter()
        .map(|point| MarkerLayer {
            radius: point
                .weight
                .unwrap_or(DEFAULT_POINT_WEIGHT)
                .max(1.0)
                .sqrt()
                .max(2.0),
            color: point
                .color
                .clone()
                .unwrap_or_else(|| DEFAULT_COLOR.to_string()),
            opacity: normalize_opacity(point.opacity, 1.0),
            solid_fill: false,
        })
        .collect::<Vec<_>>();
    layers.sort_by(|left, right| right.radius.total_cmp(&left.radius));
    if let Some(last) = layers.last_mut() {
        last.solid_fill = true;
    }
    layers
}

// Resolves the base plot color with a white default.
pub(crate) fn plot_base_color(color: Option<&str>) -> String {
    color.unwrap_or(DEFAULT_COLOR).to_string()
}

// Applies the historical line-width multiplier used by plot templates.
pub(crate) fn legacy_line_width(line_width: Option<f32>, multiplier: f32) -> f32 {
    line_width.unwrap_or(DEFAULT_LINE_WIDTH) * multiplier
}

// Derives a marker size from configured marker weights.
pub(crate) fn marker_size_from_weights(
    points: &[MarkerPointConfig],
    default_size: f32,
    weight_to_radius: impl Fn(f32) -> f32,
) -> f32 {
    points
        .iter()
        .filter_map(|point| point.weight)
        .map(weight_to_radius)
        .fold(default_size, f32::max)
}

// Supplies marker-point config when a legacy template only has flat marker fields.
pub(crate) fn fallback_marker_points(
    points: &[MarkerPointConfig],
    marker_size: f32,
    marker_color: &str,
    marker_opacity: f32,
) -> Vec<MarkerPointConfig> {
    // Legacy templates often specify only marker_size/color fields. Convert
    // those into the newer layered marker representation.
    if points.is_empty() {
        vec![MarkerPointConfig {
            weight: Some(marker_size.powi(2)),
            color: Some(marker_color.to_string()),
            opacity: Some(marker_opacity),
            extra: super::types::empty_extra(),
        }]
    } else {
        points.to_vec()
    }
}

// Scales marker weights so marker radii track scene scale.
pub(crate) fn scale_marker_points(
    points: &[MarkerPointConfig],
    scale: f32,
) -> Vec<MarkerPointConfig> {
    if (scale - 1.0).abs() <= f32::EPSILON {
        return points.to_vec();
    }

    let weight_scale = scale * scale;
    points
        .iter()
        .cloned()
        .map(|mut point| {
            point.weight = point.weight.map(|weight| (weight * weight_scale).max(1.0));
            point
        })
        .collect()
}

// Resolves explicit, inherited, and base style colors in precedence order.
pub(crate) fn resolve_style_color(
    explicit_color: Option<&String>,
    inherited_color: Option<&String>,
    base_color: &str,
) -> String {
    explicit_color
        .cloned()
        .or_else(|| inherited_color.cloned())
        .unwrap_or_else(|| base_color.to_string())
}

// Converts scene shadow fields into a drawable shadow style.
pub(crate) fn normalize_shadow_style(
    color: Option<&String>,
    strength: Option<f32>,
    distance: Option<f32>,
    scale: f32,
) -> Option<ShadowStyle> {
    let color = color?.clone();
    let strength = strength.unwrap_or(0.0) * scale;
    let distance = distance.unwrap_or(0.0) * scale;
    if strength <= 0.0 && distance == 0.0 {
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

// Builds a Skia path from widget points, optionally closing to a baseline.
pub(crate) fn path_from_points(
    points: &[(f32, f32)],
    close_path: bool,
    baseline_y: Option<f32>,
) -> SkPath {
    let mut path = SkPath::new();
    if points.is_empty() {
        return path;
    }
    if let Some(baseline) = baseline_y {
        path.move_to((points[0].0, baseline));
    } else {
        path.move_to(points[0]);
    }
    for point in points {
        path.line_to(*point);
    }
    if let Some(baseline) = baseline_y {
        path.line_to((points.last().unwrap().0, baseline));
    }
    if close_path {
        path.close();
    }
    path
}

// Applies widget translation and rotation while drawing local widget contents.
pub(crate) fn with_widget_transform(
    canvas: &Canvas,
    x: f32,
    y: f32,
    _width: f32,
    _height: f32,
    rotation_deg: f32,
    draw: impl FnOnce(&Canvas),
) {
    // Match the editor preview: plot widgets rotate around their top-left
    // origin after being translated to the configured canvas position.
    canvas.save();
    canvas.translate((x, y));
    if rotation_deg != 0.0 {
        canvas.rotate(rotation_deg, None);
    }
    draw(canvas);
    canvas.restore();
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
