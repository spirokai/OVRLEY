//! Route/course plot widget.
//!
//! The route widget projects latitude/longitude samples into a local 2D plane,
//! fits and simplifies the path inside the configured widget bounds, caches the
//! remaining route as a static layer, and draws the completed route plus marker
//! for each frame.

use super::common::{
    custom_export_range_active, draw_static_layer, fallback_marker_points, frame_progress_values,
    legacy_line_width, marker_size_from_weights, normalize_optional_progress_window,
    normalize_shadow_style, plot_base_color, point_at_metric_progress_with_cursor,
    relative_distance_frame_progress_values, resolve_style_color, scale_marker_points,
    shadow_with_screen_offset, static_layer_padding, widget_render_report,
    DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER, DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
    DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX,
};
use super::geometry::{distance, fit_points_to_widget_with_inset, normalize_opacity};
use super::marker::{draw_marker, marker_layers_from_points};
use super::polyline::{draw_polyline, draw_polyline_with_shadow};
use super::transform::with_widget_transform;
use super::types::{
    NormalizedRoutePlot, RouteFrameState, RouteSample, RouteWidgetCache, StaticLayer,
    WidgetGeometry, WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::config::{CoursePlotConfig, RenderConfig, RenderDataRequirements};
use crate::debug::RenderProfiler;
use crate::error::{CoreError, CoreResult};
use crate::rdp::simplify_rdp_indices;
use crate::render::surface::create_surface;
use skia_safe::Canvas;
use std::time::Instant;

/// Prepares cached geometry, static layers, and frame states for a route plot.
pub(crate) fn prepare_route_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &CoursePlotConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<RouteWidgetCache> {
    // Build all expensive geometry and frame-position data once before the
    // render loop. Per-frame drawing then only composites cached/static pieces.
    let prepare_started = Instant::now();
    let show_full_activity = plot.show_full_activity.unwrap_or(false);
    let plot = normalize_route_plot(config, plot);
    let route_samples = build_route_samples(config, activity, show_full_activity)?;
    let geometry = prepare_profiler.measure("build_route_cache.geometry", || {
        build_route_geometry(&plot, &route_samples)
    })?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let remaining_layer = prepare_profiler.measure("build_route_cache.layers", || {
        build_route_remaining_layer(&plot, &geometry)
    })?;
    let frame_states = prepare_profiler.measure("build_route_cache.frame_states", || {
        build_route_frame_states(
            config,
            activity,
            &geometry,
            dense_activity,
            show_full_activity,
        )
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
        remaining_layer,
    })
}

/// Draws the route widget for one frame and returns preview diagnostics.
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
                draw_static_layer(canvas, route_cache.remaining_layer.as_ref());
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

/// Normalizes route plot options into concrete scaled drawing settings.
///
/// Resolves legacy flat-style fields and nested styles into a single internal
/// shape, applies scene scale to dimensions and stroke/marker sizes, and
/// resolves color/opacity precedence chains. Called once per widget build.
fn normalize_route_plot(config: &RenderConfig, plot: &CoursePlotConfig) -> NormalizedRoutePlot {
    // Normalize legacy flat style fields and newer nested styles into one
    // internal shape. Scale is applied to dimensions and stroke/marker sizes.
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
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
        simplify_tolerance_px: plot.simplify_tolerance_px.unwrap_or(
            DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX * DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER,
        ),
        target_density: plot.target_density.unwrap_or(1.0).clamp(0.1, 2.0),
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
        remaining_line_shadow: shadow_with_screen_offset(
            normalize_shadow_style(
                config.scene.shadow_color.as_ref(),
                config.scene.shadow_strength,
                config.scene.shadow_distance,
                scale,
            ),
            plot.rotation,
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

// Builds simplified widget-space geometry for the route path.
fn build_route_geometry(
    plot: &NormalizedRoutePlot,
    route_samples: &[RouteSample],
) -> CoreResult<WidgetGeometry> {
    // Downsample before RDP simplification to cap work for long activities while
    // preserving visually important points.
    if route_samples.len() < 2 {
        return Err(CoreError::Render(
            "Route plot requires at least two valid course points".into(),
        ));
    }
    let projected = route_samples
        .iter()
        .map(|sample| sample.point)
        .collect::<Vec<_>>();
    let stroke_inset = route_geometry_inset_px(plot);
    let fitted = fit_points_to_widget_with_inset(
        &projected,
        plot.width as f32,
        plot.height as f32,
        stroke_inset,
        true,
    );
    let fitted_samples = route_samples
        .iter()
        .zip(fitted.iter())
        .map(|(sample, point)| RouteSample {
            point: *point,
            progress01: sample.progress01,
        })
        .collect::<Vec<_>>();
    let target_count = ((plot.width as f32) * plot.target_density).round().max(2.0) as usize;
    let downsampled =
        downsample_route_samples(&fitted_samples, target_count.min(fitted_samples.len()));
    let simplified = simplify_route_samples(&downsampled, plot.simplify_tolerance_px.max(0.05));

    Ok(WidgetGeometry {
        bbox: (0.0, 0.0, plot.width as f32, plot.height as f32),
        progress_values: simplified.iter().map(|sample| sample.progress01).collect(),
        points: simplified.iter().map(|sample| sample.point).collect(),
        source_point_count: route_samples.len(),
        simplification: format!(
            "lttb_density_{:.2}_rdp_px_{:.2}",
            plot.target_density, plot.simplify_tolerance_px
        ),
    })
}

// Builds the cached static layer for the not-yet-completed route.
fn build_route_remaining_layer(
    plot: &NormalizedRoutePlot,
    geometry: &WidgetGeometry,
) -> CoreResult<Option<StaticLayer>> {
    if geometry.points.len() < 2 {
        return Ok(None);
    }

    let padding = static_layer_padding(
        plot.remaining_line_width,
        plot.remaining_line_shadow.as_ref(),
    );
    let layer_width = plot.width.saturating_add(padding.saturating_mul(2)).max(1);
    let layer_height = plot.height.saturating_add(padding.saturating_mul(2)).max(1);
    let mut surface = create_surface(layer_width, layer_height)?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    surface.canvas().save();
    surface.canvas().translate((padding as f32, padding as f32));
    draw_polyline_with_shadow(
        surface.canvas(),
        &geometry.points,
        &plot.remaining_line_color,
        plot.remaining_line_width,
        plot.remaining_line_opacity,
        plot.remaining_line_shadow.as_ref(),
    );
    surface.canvas().restore();

    Ok(Some(StaticLayer {
        image: surface.image_snapshot(),
        x: -(padding as f32),
        y: -(padding as f32),
    }))
}

// Computes inset needed to keep route strokes and marker inside widget bounds.
fn route_geometry_inset_px(plot: &NormalizedRoutePlot) -> f32 {
    let line_inset = (plot.remaining_line_width.max(plot.completed_line_width) * 0.5).max(0.0);
    plot.marker_size.max(line_inset) + 1.0
}

// Precomputes route marker coordinates for each render frame.
fn build_route_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    geometry: &WidgetGeometry,
    dense_activity: &DenseActivityReport,
    show_full_activity: bool,
) -> Vec<RouteFrameState> {
    // Precompute marker positions for all frames. Custom export windows can use
    // trim-relative distance progress so the marker starts at the beginning of
    // the exported slice rather than the full activity.
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

// Selects full-activity or trimmed course samples for route geometry.
fn build_route_samples(
    config: &RenderConfig,
    activity: &ParsedActivity,
    show_full_activity: bool,
) -> CoreResult<Vec<RouteSample>> {
    // `show_full_activity` overrides custom export trimming so the full route
    // remains visible while progress can still follow the selected scene.
    if show_full_activity || !custom_export_range_active(config) {
        return Ok(project_course_samples(
            &activity.sample_course_points,
            &activity.sample_distance_progress,
        ));
    }

    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &RenderDataRequirements {
            distance_progress: true,
            course: true,
            ..RenderDataRequirements::default()
        },
    )?;

    Ok(project_course_samples_with_optional_progress(
        &trimmed.course,
        &trimmed.sample_distance_progress,
    ))
}

// Projects latitude/longitude samples into local 2D route samples.
fn project_course_samples(
    course_points: &[(Option<f64>, Option<f64>)],
    progress_values: &[f64],
) -> Vec<RouteSample> {
    // Use an equirectangular approximation centered on mean latitude. This is
    // sufficient for small activity routes and avoids a heavy projection crate.
    let valid_points = course_points
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
            progress01: progress_values
                .get(index)
                .copied()
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| {
                    index as f64 / course_points.len().saturating_sub(1).max(1) as f64
                })
                .clamp(0.0, 1.0) as f32,
        })
        .collect()
}

// Projects trimmed course samples after normalizing optional progress values.
fn project_course_samples_with_optional_progress(
    course_points: &[(Option<f64>, Option<f64>)],
    progress_values: &[Option<f64>],
) -> Vec<RouteSample> {
    let normalized_progress =
        normalize_optional_progress_window(progress_values).unwrap_or_else(|| {
            (0..course_points.len())
                .map(|index| {
                    if course_points.len() > 1 {
                        index as f64 / (course_points.len() - 1) as f64
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>()
        });

    project_course_samples(course_points, &normalized_progress)
}

// Simplifies route samples using Ramer-Douglas-Peucker.
// test seam
pub(crate) fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    let tuples: Vec<(f32, f32)> = points.iter().map(|p| p.point).collect();
    let indices = simplify_rdp_indices(&tuples, tolerance);
    indices.iter().map(|&i| points[i]).collect()
}

// Reduces dense route samples with Largest-Triangle-Three-Buckets.
fn downsample_route_samples(points: &[RouteSample], target_count: usize) -> Vec<RouteSample> {
    // Largest-Triangle-Three-Buckets preserves the visible shape better than
    // uniform sampling when reducing dense route traces.
    if points.len() <= target_count || target_count < 3 {
        return points.to_vec();
    }

    let bucket_size = (points.len() - 2) as f64 / (target_count - 2) as f64;
    let mut sampled = Vec::with_capacity(target_count);
    let mut a = 0usize;
    sampled.push(points[a]);

    for bucket_index in 0..(target_count - 2) {
        let avg_start = ((bucket_index + 1) as f64 * bucket_size).floor() as usize + 1;
        let avg_end = ((bucket_index + 2) as f64 * bucket_size).floor() as usize + 1;
        let avg_range_end = avg_end.min(points.len());
        let avg_range_start = avg_start.min(avg_range_end.saturating_sub(1));

        let average = if avg_range_start < avg_range_end {
            let range = &points[avg_range_start..avg_range_end];
            let (sum_x, sum_y) = range.iter().fold((0.0f64, 0.0f64), |(sx, sy), sample| {
                (sx + sample.point.0 as f64, sy + sample.point.1 as f64)
            });
            let count = range.len() as f64;
            (sum_x / count, sum_y / count)
        } else {
            let fallback = points[points.len() - 1];
            (fallback.point.0 as f64, fallback.point.1 as f64)
        };

        let range_start = (bucket_index as f64 * bucket_size).floor() as usize + 1;
        let range_end = ((bucket_index + 1) as f64 * bucket_size).floor() as usize + 1;
        let candidate_start = range_start.min(points.len().saturating_sub(2));
        let candidate_end = range_end.min(points.len().saturating_sub(1));

        let point_a = points[a];
        let mut next_a = candidate_start;
        let mut max_area = -1.0f64;

        for (offset, point_b) in points[candidate_start..candidate_end.max(candidate_start + 1)]
            .iter()
            .enumerate()
        {
            let area = triangle_area(point_a.point, point_b.point, average);
            if area > max_area {
                max_area = area;
                next_a = candidate_start + offset;
            }
        }

        a = next_a;
        sampled.push(points[a]);
    }

    sampled.push(*points.last().unwrap_or(&points[0]));
    sampled
}

// Computes triangle area used by Largest-Triangle-Three-Buckets downsampling.
fn triangle_area(point_a: (f32, f32), point_b: (f32, f32), point_c: (f64, f64)) -> f64 {
    (((point_a.0 as f64 - point_c.0) * (point_b.1 as f64 - point_a.1 as f64))
        - ((point_a.0 as f64 - point_b.0 as f64) * (point_c.1 - point_a.1 as f64)))
        .abs()
        * 0.5
}

// Falls back to index-based route marker placement when metric progress is unavailable.
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

// Builds the completed route polyline up to the current marker point.
fn build_route_prefix_points(
    geometry: &WidgetGeometry,
    state: &RouteFrameState,
) -> Vec<(f32, f32)> {
    // The completed route includes all simplified points before the marker plus
    // the interpolated marker point itself.
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
