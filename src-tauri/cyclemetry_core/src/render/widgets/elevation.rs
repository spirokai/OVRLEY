use super::common::{
    draw_area, draw_marker, draw_polyline, fallback_marker_points, format_elevation_label,
    frame_progress_values, interpolate_optional_numeric_series, legacy_line_width,
    marker_layers_from_points, marker_size_from_weights, normalize_opacity, plot_base_color,
    point_at_metric_progress_with_cursor, point_at_progress_x, resolve_style_color,
    rotate_point_to_canvas, widget_render_report, with_widget_transform,
    DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER, DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER,
    DEFAULT_ELEVATION_MARKER_SCALE,
};
use super::types::{
    ElevationFrameState, ElevationWidgetCache, NormalizedElevationPlot, WidgetGeometry,
    WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::{ElevationPlotConfig, RenderConfig};
use crate::debug::RenderProfiler;
use crate::render::text::{draw_text, parse_color, ResolvedTextStyle};
use skia_safe::Canvas;
use std::time::Instant;

pub(crate) fn prepare_elevation_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &ElevationPlotConfig,
    prepare_profiler: &mut RenderProfiler,
) -> Result<ElevationWidgetCache, String> {
    let prepare_started = Instant::now();
    let plot = normalize_elevation_plot(config, plot);
    let geometry = prepare_profiler.measure("build_elevation_cache.geometry", || {
        build_elevation_geometry(&plot, activity)
    })?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let frame_states = prepare_profiler.measure("build_elevation_cache.frame_states", || {
        build_elevation_frame_states(config, activity, dense_activity, &geometry)
    });
    prepare_profiler.record_ms(
        "build_elevation_cache",
        prepare_started.elapsed().as_secs_f64() * 1000.0,
    );

    Ok(ElevationWidgetCache {
        plot,
        geometry,
        frame_states,
        marker_layers,
    })
}

pub(crate) fn draw_elevation_widget(
    canvas: &Canvas,
    paths: &AppPaths,
    scene_font: Option<&str>,
    elevation_cache: &ElevationWidgetCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let state = elevation_cache
        .frame_states
        .get(frame_index.min(elevation_cache.frame_states.len().saturating_sub(1)))?;
    let completed_points = build_elevation_completed_points(
        &elevation_cache.geometry.points,
        &elevation_cache.geometry.progress_values,
        state.progress01,
        (state.marker_x, state.marker_y),
    );
    let baseline_y = elevation_cache.plot.height as f32;

    frame_profiler.measure("composite.elevation", || {
        with_widget_transform(
            canvas,
            elevation_cache.plot.x,
            elevation_cache.plot.y,
            elevation_cache.plot.width as f32,
            elevation_cache.plot.height as f32,
            elevation_cache.plot.rotation,
            |canvas| {
                draw_area(
                    canvas,
                    &elevation_cache.geometry.points,
                    baseline_y,
                    &elevation_cache.plot.area_remaining_color,
                    elevation_cache.plot.area_remaining_opacity,
                );
                draw_polyline(
                    canvas,
                    &elevation_cache.geometry.points,
                    &elevation_cache.plot.remaining_line_color,
                    elevation_cache.plot.remaining_line_width,
                    elevation_cache.plot.remaining_line_opacity,
                );
                draw_area(
                    canvas,
                    &completed_points,
                    baseline_y,
                    &elevation_cache.plot.area_completed_color,
                    elevation_cache.plot.area_completed_opacity,
                );
                draw_polyline(
                    canvas,
                    &completed_points,
                    &elevation_cache.plot.completed_line_color,
                    elevation_cache.plot.completed_line_width,
                    elevation_cache.plot.completed_line_opacity,
                );
                draw_marker(
                    canvas,
                    &elevation_cache.marker_layers,
                    state.marker_x,
                    state.marker_y,
                    &elevation_cache.plot.marker_color,
                    elevation_cache.plot.marker_size,
                    elevation_cache.plot.marker_opacity,
                );
            },
        );
    });

    let (marker_abs_x, marker_abs_y) = rotate_point_to_canvas(
        state.marker_x,
        state.marker_y,
        elevation_cache.plot.x,
        elevation_cache.plot.y,
        elevation_cache.plot.width as f32,
        elevation_cache.plot.height as f32,
        elevation_cache.plot.rotation,
    );

    if elevation_cache.plot.show_elevation_metric {
        frame_profiler.measure("text.elevation_label", || {
            let text = format_elevation_label(
                state.elevation_m,
                "metric",
                elevation_cache.plot.label_decimal_rounding,
            );
            let style = ResolvedTextStyle {
                x: marker_abs_x + elevation_cache.plot.metric_label_offset_x,
                y: marker_abs_y + elevation_cache.plot.metric_label_offset_y,
                font_name: elevation_cache
                    .plot
                    .label_font
                    .clone()
                    .or_else(|| scene_font.map(ToOwned::to_owned)),
                font_size: elevation_cache.plot.label_font_size,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: None,
                shadow_strength: 0.0,
                shadow_distance: 0.0,
                border_color: None,
                border_thickness: 0.0,
                border_distance: 0.0,
            };
            draw_text(canvas, &text, &style, &paths.font_dirs);
        });
    }

    if elevation_cache.plot.show_elevation_imperial {
        frame_profiler.measure("text.elevation_label", || {
            let text = format_elevation_label(
                state.elevation_m,
                "imperial",
                elevation_cache.plot.label_decimal_rounding,
            );
            let style = ResolvedTextStyle {
                x: marker_abs_x + elevation_cache.plot.imperial_label_offset_x,
                y: marker_abs_y + elevation_cache.plot.imperial_label_offset_y,
                font_name: elevation_cache
                    .plot
                    .label_font
                    .clone()
                    .or_else(|| scene_font.map(ToOwned::to_owned)),
                font_size: elevation_cache.plot.label_font_size,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: None,
                shadow_strength: 0.0,
                shadow_distance: 0.0,
                border_color: None,
                border_thickness: 0.0,
                border_distance: 0.0,
            };
            draw_text(canvas, &text, &style, &paths.font_dirs);
        });
    }

    if !elevation_cache.plot.legacy_label_units.is_empty()
        && !elevation_cache.plot.show_elevation_metric
        && !elevation_cache.plot.show_elevation_imperial
    {
        frame_profiler.measure("text.elevation_label", || {
            let text = elevation_cache
                .plot
                .legacy_label_units
                .iter()
                .map(|unit| {
                    format_elevation_label(
                        state.elevation_m,
                        unit,
                        elevation_cache.plot.label_decimal_rounding,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let style = ResolvedTextStyle {
                x: marker_abs_x + elevation_cache.plot.metric_label_offset_x,
                y: marker_abs_y + elevation_cache.plot.metric_label_offset_y,
                font_name: elevation_cache
                    .plot
                    .label_font
                    .clone()
                    .or_else(|| scene_font.map(ToOwned::to_owned)),
                font_size: elevation_cache.plot.label_font_size,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: None,
                shadow_strength: 0.0,
                shadow_distance: 0.0,
                border_color: None,
                border_thickness: 0.0,
                border_distance: 0.0,
            };
            draw_text(canvas, &text, &style, &paths.font_dirs);
        });
    }

    Some(widget_render_report(
        elevation_cache.plot.x,
        elevation_cache.plot.y,
        elevation_cache.plot.width,
        elevation_cache.plot.height,
        elevation_cache.plot.rotation,
        &elevation_cache.geometry,
        state.progress01,
        state.marker_x,
        state.marker_y,
    ))
}

fn normalize_elevation_plot(
    config: &RenderConfig,
    plot: &ElevationPlotConfig,
) -> NormalizedElevationPlot {
    let base_color = plot_base_color(plot.color.as_deref());
    let legacy_width = legacy_line_width(
        plot.line.as_ref().and_then(|line| line.width),
        DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER,
    );
    let marker_size = plot.marker_size.unwrap_or_else(|| {
        marker_size_from_weights(&plot.points, 16.0, |weight| {
            weight.sqrt() * DEFAULT_ELEVATION_MARKER_SCALE.sqrt()
        })
    });
    let point_label = plot.point_label.clone().unwrap_or_default();
    let marker_color = plot
        .marker_color
        .clone()
        .unwrap_or_else(|| base_color.clone());
    let marker_opacity = normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0);

    NormalizedElevationPlot {
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(0.0),
        y_scale: plot.y_scale.unwrap_or(1.0).clamp(0.2, 4.0),
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
            1.0,
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
        area_remaining_color: resolve_style_color(
            plot.area_remaining_color.as_ref(),
            plot.fill.as_ref().and_then(|fill| fill.color.as_ref()),
            &base_color,
        ),
        area_remaining_opacity: normalize_opacity(
            plot.area_remaining_opacity.or_else(|| {
                plot.fill
                    .as_ref()
                    .and_then(|fill| fill.opacity)
                    .map(|opacity| opacity * 0.35)
            }),
            0.12,
        ),
        area_completed_color: resolve_style_color(
            plot.area_completed_color.as_ref(),
            plot.fill.as_ref().and_then(|fill| fill.color.as_ref()),
            &base_color,
        ),
        area_completed_opacity: normalize_opacity(
            plot.area_completed_opacity
                .or_else(|| plot.fill.as_ref().and_then(|fill| fill.opacity)),
            0.24,
        ),
        marker_size,
        marker_color: marker_color.clone(),
        marker_opacity,
        marker_points: fallback_marker_points(
            &plot.points,
            marker_size,
            &marker_color,
            marker_opacity,
        ),
        show_elevation_metric: plot.show_elevation_metric.unwrap_or(false),
        show_elevation_imperial: plot.show_elevation_imperial.unwrap_or(false),
        metric_label_offset_x: plot
            .metric_label_offset_x
            .or(point_label.x_offset)
            .unwrap_or(0.0),
        metric_label_offset_y: plot
            .metric_label_offset_y
            .or(point_label.y_offset)
            .unwrap_or(-28.0),
        imperial_label_offset_x: plot.imperial_label_offset_x.unwrap_or(0.0),
        imperial_label_offset_y: plot.imperial_label_offset_y.unwrap_or(6.0),
        label_font: point_label.font.or_else(|| config.scene.font.clone()),
        label_font_size: point_label
            .font_size
            .or(config.scene.font_size)
            .unwrap_or(12.5),
        label_color: point_label.color.unwrap_or_else(|| base_color.clone()),
        label_decimal_rounding: point_label
            .decimal_rounding
            .or(config.scene.decimal_rounding),
        legacy_label_units: point_label.units,
    }
}

fn build_elevation_geometry(
    plot: &NormalizedElevationPlot,
    activity: &ParsedActivity,
) -> Result<WidgetGeometry, String> {
    let raw_points = raw_elevation_points(activity);
    if raw_points.is_empty() {
        return Err("Elevation plot requires elevation samples".to_string());
    }

    let target_count = ((plot.width as f32) * DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER)
        .round()
        .max(2.0) as usize;
    let downsampled = downsample_elevation_points(&raw_points, target_count.min(raw_points.len()));
    let fitted = project_elevation_points(
        &downsampled,
        plot.width as f32,
        plot.height as f32,
        plot.margin,
        plot.y_scale,
    );

    Ok(WidgetGeometry {
        bbox: (0.0, 0.0, plot.width as f32, plot.height as f32),
        progress_values: downsampled.iter().map(|(progress, _)| *progress).collect(),
        points: fitted,
    })
}

fn build_elevation_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    geometry: &WidgetGeometry,
) -> Vec<ElevationFrameState> {
    let frame_progress = frame_progress_values(config, activity, dense_activity);
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

fn raw_elevation_points(activity: &ParsedActivity) -> Vec<(f32, f64)> {
    let source = if activity.sample_elevations.is_empty() {
        &activity.elevation
    } else {
        &activity.sample_elevations
    };

    let progress = &activity.sample_distance_progress;
    source
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let value = (*value)?;
            let progress_value = progress
                .get(index)
                .copied()
                .unwrap_or_else(|| index as f64 / source.len().saturating_sub(1).max(1) as f64);
            if value.is_finite() && progress_value.is_finite() {
                Some((progress_value.clamp(0.0, 1.0) as f32, value))
            } else {
                None
            }
        })
        .collect()
}

fn downsample_elevation_points(points: &[(f32, f64)], target_count: usize) -> Vec<(f32, f64)> {
    if points.len() <= target_count {
        return points.to_vec();
    }

    let bucket_size = points.len() as f32 / ((target_count / 2).max(1) as f32);
    let mut sampled = vec![points[0]];
    let mut bucket_index = 0.0f32;
    while (bucket_index as usize) < points.len().saturating_sub(1) {
        let start_index = bucket_index as usize;
        let end_index = points
            .len()
            .min((bucket_index + bucket_size).floor() as usize);
        let bucket = &points[start_index..end_index.max(start_index + 1)];
        if let Some(min_point) = bucket
            .iter()
            .copied()
            .min_by(|left, right| left.1.total_cmp(&right.1))
        {
            sampled.push(min_point);
        }
        if let Some(max_point) = bucket
            .iter()
            .copied()
            .max_by(|left, right| left.1.total_cmp(&right.1))
        {
            sampled.push(max_point);
        }
        bucket_index += bucket_size.max(1.0);
    }
    sampled.push(*points.last().unwrap_or(&points[0]));
    sampled.sort_by(|left, right| left.0.total_cmp(&right.0));
    sampled.dedup_by(|left, right| {
        (left.0 - right.0).abs() <= f32::EPSILON && (left.1 - right.1).abs() <= f64::EPSILON
    });
    sampled
}

fn project_elevation_points(
    points: &[(f32, f64)],
    width: f32,
    height: f32,
    margin: f32,
    y_scale: f32,
) -> Vec<(f32, f32)> {
    let min_elevation = points
        .iter()
        .map(|(_, elevation)| *elevation)
        .fold(f64::INFINITY, f64::min);
    let max_elevation = points
        .iter()
        .map(|(_, elevation)| *elevation)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max_elevation - min_elevation).max(1e-9);
    let inner_width = (width * (1.0 - 2.0 * margin)).max(1.0);
    let inner_height = (height * (1.0 - 2.0 * margin)).max(1.0);

    points
        .iter()
        .map(|(progress, elevation)| {
            let progress01 = (*progress).clamp(0.0, 1.0);
            let normalized = ((*elevation - min_elevation) / span) as f32;
            let centered = ((normalized - 0.5) * y_scale + 0.5).clamp(0.0, 1.0);
            let point_x = width * margin + inner_width * progress01;
            let point_y = height - (height * margin + inner_height * centered);
            (point_x, point_y)
        })
        .collect()
}

fn build_elevation_completed_points(
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
    if super::common::distance(*result.last().unwrap_or(&points[0]), marker_point) > 1e-3 {
        result.push(marker_point);
    }
    result
}

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
            interpolate_optional_numeric_series(progress_values, elevations, *progress01 as f64)
                .unwrap_or(0.0)
        })
        .collect()
}
