use super::common::{
    distance, draw_marker, draw_polyline, fallback_marker_points, fit_points_to_widget,
    frame_progress_values, legacy_line_width, marker_layers_from_points, marker_size_from_weights,
    normalize_opacity, plot_base_color, point_at_metric_progress_with_cursor, resolve_style_color,
    scale_marker_points, widget_render_report, with_widget_transform, DEFAULT_MARGIN,
    DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER, DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
    DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX,
};
use super::types::{
    NormalizedRoutePlot, RouteFrameState, RouteSample, RouteWidgetCache, WidgetGeometry,
    WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::{CoursePlotConfig, RenderConfig};
use crate::debug::RenderProfiler;
use skia_safe::Canvas;
use std::time::Instant;

pub(crate) fn prepare_route_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &CoursePlotConfig,
    prepare_profiler: &mut RenderProfiler,
) -> Result<RouteWidgetCache, String> {
    let prepare_started = Instant::now();
    let plot = normalize_route_plot(config, plot);
    let geometry = prepare_profiler.measure("build_route_cache.geometry", || {
        build_route_geometry(&plot, activity)
    })?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let frame_states = prepare_profiler.measure("build_route_cache.frame_states", || {
        build_route_frame_states(config, activity, &geometry, dense_activity)
    });
    prepare_profiler.record_ms(
        "build_route_cache",
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    );

    Ok(RouteWidgetCache {
        plot,
        geometry,
        frame_states,
        marker_layers,
    })
}

pub(crate) fn draw_route_widget(
    canvas: &Canvas,
    route_cache: &RouteWidgetCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let state = route_cache
        .frame_states
        .get(frame_index.min(route_cache.frame_states.len().saturating_sub(1)))?;
    let prefix_points = build_route_prefix_points(&route_cache.geometry, state);

    frame_profiler.measure("composite.route", || {
        with_widget_transform(
            canvas,
            route_cache.plot.x,
            route_cache.plot.y,
            route_cache.plot.width as f32,
            route_cache.plot.height as f32,
            route_cache.plot.rotation,
            |canvas| {
                draw_polyline(
                    canvas,
                    &route_cache.geometry.points,
                    &route_cache.plot.remaining_line_color,
                    route_cache.plot.remaining_line_width,
                    route_cache.plot.remaining_line_opacity,
                );
                draw_polyline(
                    canvas,
                    &prefix_points,
                    &route_cache.plot.completed_line_color,
                    route_cache.plot.completed_line_width,
                    route_cache.plot.completed_line_opacity,
                );
                draw_marker(
                    canvas,
                    &route_cache.marker_layers,
                    state.marker_x,
                    state.marker_y,
                    &route_cache.plot.marker_color,
                    route_cache.plot.marker_size,
                    route_cache.plot.marker_opacity,
                );
            },
        );
    });

    Some(widget_render_report(
        route_cache.plot.x,
        route_cache.plot.y,
        route_cache.plot.width,
        route_cache.plot.height,
        route_cache.plot.rotation,
        &route_cache.geometry,
        state.progress01,
        state.marker_x,
        state.marker_y,
    ))
}

fn normalize_route_plot(_config: &RenderConfig, plot: &CoursePlotConfig) -> NormalizedRoutePlot {
    let scale = _config.scene.scale.unwrap_or(1.0).max(0.1);
    let base_color = plot_base_color(plot.color.as_deref());
    let legacy_width = legacy_line_width(
        plot.line.as_ref().and_then(|line| line.width),
        DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER,
    ) * scale;
    let marker_size = plot
        .marker_size
        .unwrap_or_else(|| marker_size_from_weights(&plot.points, 18.0, f32::sqrt))
        * scale;
    let marker_color = plot
        .marker_color
        .clone()
        .unwrap_or_else(|| base_color.clone());
    let marker_opacity = normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0);
    let scaled_width = ((plot.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((plot.height as f32) * scale).round().max(1.0) as u32;
    let scaled_points = scale_marker_points(&plot.points, scale);

    NormalizedRoutePlot {
        x: plot.x,
        y: plot.y,
        width: scaled_width,
        height: scaled_height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(DEFAULT_MARGIN),
        remaining_line_width: plot.remaining_line_width.unwrap_or(legacy_width),
        remaining_line_color: resolve_style_color(
            plot.remaining_line_color.as_ref(),
            plot.line.as_ref().and_then(|line| line.color.as_ref()),
            &base_color,
        ),
        remaining_line_opacity: normalize_opacity(
            plot.remaining_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            0.75,
        ),
        completed_line_width: plot.completed_line_width.unwrap_or(legacy_width),
        completed_line_color: resolve_style_color(
            plot.completed_line_color.as_ref(),
            plot.line.as_ref().and_then(|line| line.color.as_ref()),
            &base_color,
        ),
        completed_line_opacity: normalize_opacity(
            plot.completed_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            1.0,
        ),
        marker_size,
        marker_color: marker_color.clone(),
        marker_opacity,
        marker_points: fallback_marker_points(
            &scaled_points,
            marker_size,
            &marker_color,
            marker_opacity,
        ),
    }
}

fn build_route_geometry(
    plot: &NormalizedRoutePlot,
    activity: &ParsedActivity,
) -> Result<WidgetGeometry, String> {
    let route_samples = project_course_samples(activity);
    if route_samples.len() < 2 {
        return Err("Route plot requires at least two valid course points".to_string());
    }
    let projected = route_samples
        .iter()
        .map(|sample| sample.point)
        .collect::<Vec<_>>();
    let fitted = fit_points_to_widget(
        &projected,
        plot.width as f32,
        plot.height as f32,
        plot.margin,
        true,
    );
    let tolerance =
        DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX * DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER;
    let fitted_samples = route_samples
        .iter()
        .zip(fitted.iter())
        .map(|(sample, point)| RouteSample {
            point: *point,
            progress01: sample.progress01,
        })
        .collect::<Vec<_>>();
    let simplified = simplify_route_samples(&fitted_samples, tolerance.max(0.05));

    Ok(WidgetGeometry {
        bbox: (0.0, 0.0, plot.width as f32, plot.height as f32),
        progress_values: simplified.iter().map(|sample| sample.progress01).collect(),
        points: simplified.iter().map(|sample| sample.point).collect(),
        source_point_count: route_samples.len(),
        simplification: "rdp_px_1.0",
    })
}

fn build_route_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    geometry: &WidgetGeometry,
    dense_activity: &DenseActivityReport,
) -> Vec<RouteFrameState> {
    let frame_progress = frame_progress_values(config, activity, dense_activity);
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

fn project_course_samples(activity: &ParsedActivity) -> Vec<RouteSample> {
    let valid_points = activity
        .sample_course_points
        .iter()
        .enumerate()
        .filter_map(|(index, (lat, lon))| match (*lat, *lon) {
            (Some(lat), Some(lon)) if lat.is_finite() && lon.is_finite() => Some((index, lat, lon)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if valid_points.is_empty() {
        return Vec::new();
    }
    let mean_latitude =
        valid_points.iter().map(|(_, lat, _)| lat).sum::<f64>() / valid_points.len() as f64;
    let mean_latitude_radians = mean_latitude.to_radians();
    valid_points
        .into_iter()
        .map(|(index, lat, lon)| RouteSample {
            point: ((lon * mean_latitude_radians.cos()) as f32, lat as f32),
            progress01: activity
                .sample_distance_progress
                .get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| {
                    index as f64
                        / activity.sample_course_points.len().saturating_sub(1).max(1) as f64
                })
                .clamp(0.0, 1.0) as f32,
        })
        .collect()
}

fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    fn perpendicular_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
        let (x0, y0) = point;
        let (x1, y1) = start;
        let (x2, y2) = end;
        let dx = x2 - x1;
        let dy = y2 - y1;
        if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
            return ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();
        }
        (dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()
    }

    let mut max_distance = 0.0f32;
    let mut split_index = 0usize;
    for index in 1..points.len() - 1 {
        let distance = perpendicular_distance(
            points[index].point,
            points[0].point,
            points.last().unwrap().point,
        );
        if distance > max_distance {
            max_distance = distance;
            split_index = index;
        }
    }

    if max_distance <= tolerance {
        return vec![points[0], *points.last().unwrap()];
    }

    let left = simplify_route_samples(&points[..=split_index], tolerance);
    let right = simplify_route_samples(&points[split_index..], tolerance);
    [left[..left.len() - 1].to_vec(), right].concat()
}

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

fn build_route_prefix_points(
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
