use super::types::{
    MarkerLayer, WidgetFrameReport, WidgetGeometry, WidgetGeometryReport, WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::MarkerPointConfig;
use crate::config::RenderConfig;
use skia_safe::{Canvas, Paint, PaintCap, PaintJoin, Path as SkPath, Point};

pub(crate) const DEFAULT_COLOR: &str = "#ffffff";
pub(crate) const DEFAULT_LINE_WIDTH: f32 = 1.75;
pub(crate) const DEFAULT_MARGIN: f32 = 0.1;
pub(crate) const DEFAULT_POINT_WEIGHT: f32 = 80.0;
pub(crate) const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX: f32 = 1.0;
pub(crate) const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER: f32 = 1.0;
pub(crate) const DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER: f32 = 2.5;
pub(crate) const DEFAULT_ELEVATION_MARKER_SCALE: f32 = 2.5;
pub(crate) const DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER: f32 = 2.5;

pub(crate) fn normalize_opacity(value: Option<f32>, default: f32) -> f32 {
    match value {
        Some(value) if value > 1.0 => (value / 100.0).clamp(0.0, 1.0),
        Some(value) => value.clamp(0.0, 1.0),
        None => default,
    }
}

pub(crate) fn distance(left: (f32, f32), right: (f32, f32)) -> f32 {
    ((right.0 - left.0).powi(2) + (right.1 - left.1).powi(2)).sqrt()
}

pub(crate) fn fit_points_to_widget(
    points: &[(f32, f32)],
    width: f32,
    height: f32,
    margin: f32,
    invert_y: bool,
) -> Vec<(f32, f32)> {
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
    let inner_width = (width * (1.0 - 2.0 * margin)).max(1.0);
    let inner_height = (height * (1.0 - 2.0 * margin)).max(1.0);
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

pub(crate) fn frame_progress_values(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> Vec<f32> {
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

pub(crate) fn point_at_metric_progress_with_cursor(
    points: &[(f32, f32)],
    progress_values: &[f32],
    target_progress: f32,
    cursor: &mut usize,
) -> Option<(usize, f32, f32)> {
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

pub(crate) fn draw_polyline(
    canvas: &Canvas,
    points: &[(f32, f32)],
    color: &str,
    width: f32,
    opacity: f32,
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
    canvas.draw_path(&path, &paint);
}

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

pub(crate) fn draw_marker(
    canvas: &Canvas,
    layers: &[MarkerLayer],
    x: f32,
    y: f32,
    fallback_color: &str,
    fallback_radius: f32,
    fallback_opacity: f32,
) {
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

pub(crate) fn plot_base_color(color: Option<&str>) -> String {
    color.unwrap_or(DEFAULT_COLOR).to_string()
}

pub(crate) fn legacy_line_width(line_width: Option<f32>, multiplier: f32) -> f32 {
    line_width.unwrap_or(DEFAULT_LINE_WIDTH) * multiplier
}

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

pub(crate) fn fallback_marker_points(
    points: &[MarkerPointConfig],
    marker_size: f32,
    marker_color: &str,
    marker_opacity: f32,
) -> Vec<MarkerPointConfig> {
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

pub(crate) fn with_widget_transform(
    canvas: &Canvas,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation_deg: f32,
    draw: impl FnOnce(&Canvas),
) {
    canvas.save();
    canvas.translate((x, y));
    if rotation_deg != 0.0 {
        canvas.translate((width / 2.0, height / 2.0));
        canvas.rotate(rotation_deg, None);
        canvas.translate((-width / 2.0, -height / 2.0));
    }
    draw(canvas);
    canvas.restore();
}

pub(crate) fn rotate_point_to_canvas(
    x: f32,
    y: f32,
    widget_x: f32,
    widget_y: f32,
    width: f32,
    height: f32,
    rotation_deg: f32,
) -> (f32, f32) {
    if rotation_deg == 0.0 {
        return (widget_x + x, widget_y + y);
    }
    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let radians = (-rotation_deg).to_radians();
    let translated_x = x - center_x;
    let translated_y = y - center_y;
    let rotated_x = translated_x * radians.cos() - translated_y * radians.sin();
    let rotated_y = translated_x * radians.sin() + translated_y * radians.cos();
    (
        widget_x + rotated_x + center_x,
        widget_y + rotated_y + center_y,
    )
}

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

pub(crate) fn format_elevation_label(
    value_m: f64,
    unit: &str,
    decimal_rounding: Option<i32>,
) -> String {
    let (converted, suffix) = match unit {
        "imperial" => (value_m * 3.28084, " ft"),
        _ => (value_m, " m"),
    };
    let value_text = match decimal_rounding {
        Some(0) => format!("{}", converted.round() as i64),
        Some(decimals) if decimals > 0 => format!("{:.*}", decimals as usize, converted),
        _ => format!("{}", converted.round() as i64),
    };
    format!("{value_text}{suffix}")
}
