use super::common::{
    custom_export_range_active, draw_area, draw_marker, draw_polyline, draw_polyline_with_shadow,
    draw_static_layer, fallback_marker_points, format_elevation_label, frame_progress_values,
    interpolate_optional_numeric_series, legacy_line_width, marker_layers_from_points,
    marker_size_from_weights, normalize_opacity, normalize_optional_progress_window,
    normalize_shadow_style, plot_base_color, point_at_metric_progress_with_cursor,
    point_at_progress_x, relative_distance_frame_progress_values, resolve_style_color,
    rotate_point_to_canvas, scale_marker_points, static_layer_padding, widget_render_report,
    with_widget_transform, DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER,
    DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER, DEFAULT_ELEVATION_MARKER_SCALE,
};
use super::types::{
    ElevationFrameState, ElevationWidgetCache, NormalizedElevationPlot, StaticLayer,
    WidgetGeometry, WidgetRenderReport,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::commands::AppPaths;
use crate::config::{ElevationPlotConfig, RenderConfig, RenderDataRequirements};
use crate::debug::RenderProfiler;
use crate::render::surface::create_surface;
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
    let show_full_activity = plot.show_full_activity.unwrap_or(false);
    let plot = normalize_elevation_plot(config, plot);
    let raw_points = build_elevation_source_points(config, activity, show_full_activity)?;
    let geometry = prepare_profiler.measure("build_elevation_cache.geometry", || {
        build_elevation_geometry(&plot, &raw_points)
    })?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let remaining_layer = prepare_profiler.measure("build_elevation_cache.layers", || {
        build_elevation_remaining_layer(&plot, &geometry)
    })?;
    let frame_states = prepare_profiler.measure("build_elevation_cache.frame_states", || {
        build_elevation_frame_states(
            config,
            activity,
            dense_activity,
            &geometry,
            show_full_activity,
        )
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
        remaining_layer,
    })
}

pub(crate) fn draw_elevation_widget(
    canvas: &Canvas,
    paths: &AppPaths,
    config: &RenderConfig,
    elevation_cache: &ElevationWidgetCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let scene_scale = config.scene.scale.unwrap_or(1.0).max(0.1);
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
                draw_static_layer(canvas, elevation_cache.remaining_layer.as_ref());
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
                    .or_else(|| config.scene.font.clone()),
                font_size: elevation_cache.plot.label_font_size,
                line_height: elevation_cache.plot.label_font_size * 0.92,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: config
                    .scene
                    .shadow_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                shadow_strength: config.scene.shadow_strength.unwrap_or(0.0) * scene_scale,
                shadow_distance: config.scene.shadow_distance.unwrap_or(0.0) * scene_scale,
                border_color: config
                    .scene
                    .border_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                border_thickness: config.scene.border_thickness.unwrap_or(0.0) * scene_scale,
                border_distance: config.scene.border_distance.unwrap_or(0.0) * scene_scale,
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
                    .or_else(|| config.scene.font.clone()),
                font_size: elevation_cache.plot.label_font_size,
                line_height: elevation_cache.plot.label_font_size * 0.92,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: config
                    .scene
                    .shadow_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                shadow_strength: config.scene.shadow_strength.unwrap_or(0.0) * scene_scale,
                shadow_distance: config.scene.shadow_distance.unwrap_or(0.0) * scene_scale,
                border_color: config
                    .scene
                    .border_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                border_thickness: config.scene.border_thickness.unwrap_or(0.0) * scene_scale,
                border_distance: config.scene.border_distance.unwrap_or(0.0) * scene_scale,
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
                    .or_else(|| config.scene.font.clone()),
                font_size: elevation_cache.plot.label_font_size,
                line_height: elevation_cache.plot.label_font_size * 0.92,
                color: parse_color(&elevation_cache.plot.label_color, 1.0),
                opacity: 1.0,
                shadow_color: config
                    .scene
                    .shadow_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                shadow_strength: config.scene.shadow_strength.unwrap_or(0.0) * scene_scale,
                shadow_distance: config.scene.shadow_distance.unwrap_or(0.0) * scene_scale,
                border_color: config
                    .scene
                    .border_color
                    .as_deref()
                    .map(|color| parse_color(color, 1.0)),
                border_thickness: config.scene.border_thickness.unwrap_or(0.0) * scene_scale,
                border_distance: config.scene.border_distance.unwrap_or(0.0) * scene_scale,
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
    let scale = config.scene.scale.unwrap_or(1.0).max(0.1);
    let base_color = plot_base_color(plot.color.as_deref());
    let legacy_width = legacy_line_width(
        plot.line.as_ref().and_then(|line| line.width),
        DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER,
    ) * scale;
    let marker_size = plot.marker_size.unwrap_or_else(|| {
        marker_size_from_weights(&plot.points, 16.0, |weight| {
            weight.sqrt() * DEFAULT_ELEVATION_MARKER_SCALE.sqrt()
        })
    }) * scale;
    let point_label = plot.point_label.clone().unwrap_or_default();
    let marker_color = plot
        .marker_color
        .clone()
        .unwrap_or_else(|| base_color.clone());
    let marker_opacity = normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0);
    let scaled_width = ((plot.width as f32) * scale).round().max(1.0) as u32;
    let scaled_height = ((plot.height as f32) * scale).round().max(1.0) as u32;
    let scaled_points = scale_marker_points(&plot.points, scale);

    NormalizedElevationPlot {
        x: plot.x,
        y: plot.y,
        width: scaled_width,
        height: scaled_height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(0.0),
        y_scale: plot.y_scale.unwrap_or(1.0).clamp(0.2, 4.0),
        simplify_tolerance_px: plot.simplify_tolerance_px.unwrap_or(1.0).clamp(0.0, 8.0),
        target_density: plot.target_density.unwrap_or(0.75).clamp(0.1, 2.0),
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
        remaining_line_shadow: normalize_shadow_style(
            config.scene.shadow_color.as_ref(),
            config.scene.shadow_strength,
            config.scene.shadow_distance,
            scale,
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
            &scaled_points,
            marker_size,
            &marker_color,
            marker_opacity,
        ),
        show_elevation_metric: plot.show_elevation_metric.unwrap_or(false),
        show_elevation_imperial: plot.show_elevation_imperial.unwrap_or(false),
        metric_label_offset_x: plot
            .metric_label_offset_x
            .or(point_label.x_offset)
            .unwrap_or(0.0)
            * scale,
        metric_label_offset_y: plot
            .metric_label_offset_y
            .or(point_label.y_offset)
            .unwrap_or(-28.0)
            * scale,
        imperial_label_offset_x: plot.imperial_label_offset_x.unwrap_or(0.0) * scale,
        imperial_label_offset_y: plot.imperial_label_offset_y.unwrap_or(6.0) * scale,
        label_font: point_label
            .font
            .or_else(|| first_value_font(config))
            .or_else(|| config.scene.font.clone()),
        label_font_size: point_label
            .font_size
            .or(config.scene.font_size)
            .unwrap_or(12.5)
            * scale,
        label_color: point_label.color.unwrap_or_else(|| base_color.clone()),
        label_decimal_rounding: point_label
            .decimal_rounding
            .or(config.scene.decimal_rounding),
        legacy_label_units: point_label.units,
    }
}

fn build_elevation_remaining_layer(
    plot: &NormalizedElevationPlot,
    geometry: &WidgetGeometry,
) -> Result<Option<StaticLayer>, String> {
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
    draw_area(
        surface.canvas(),
        &geometry.points,
        plot.height as f32,
        &plot.area_remaining_color,
        plot.area_remaining_opacity,
    );
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

fn first_value_font(config: &RenderConfig) -> Option<String> {
    config
        .values
        .iter()
        .find_map(|value| value.font.clone().or_else(|| value.font_family.clone()))
}

fn build_elevation_geometry(
    plot: &NormalizedElevationPlot,
    raw_points: &[(f32, f64)],
) -> Result<WidgetGeometry, String> {
    if raw_points.is_empty() {
        return Err("Elevation plot requires elevation samples".to_string());
    }

    let target_count =
        ((plot.width as f32) * DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER * plot.target_density)
            .round()
            .max(2.0) as usize;
    let downsampled = downsample_elevation_points(&raw_points, target_count.min(raw_points.len()));
    let projected = project_elevation_points(
        &downsampled,
        plot.width as f32,
        plot.height as f32,
        plot.margin,
        plot.y_scale,
    );
    let projected_samples = downsampled
        .iter()
        .zip(projected.iter())
        .map(|(sample, point)| ElevationSample {
            point: *point,
            progress01: sample.progress01,
            preserve: sample.preserve,
        })
        .collect::<Vec<_>>();
    let simplified = simplify_elevation_samples(&projected_samples, plot.simplify_tolerance_px);

    Ok(WidgetGeometry {
        bbox: (0.0, 0.0, plot.width as f32, plot.height as f32),
        progress_values: simplified.iter().map(|sample| sample.progress01).collect(),
        points: simplified.iter().map(|sample| sample.point).collect(),
        source_point_count: raw_points.len(),
        simplification: format!(
            "sg11_density_{:.2}_rdp_px_{:.2}",
            plot.target_density, plot.simplify_tolerance_px
        ),
    })
}

fn build_elevation_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    geometry: &WidgetGeometry,
    show_full_activity: bool,
) -> Vec<ElevationFrameState> {
    let frame_progress = if custom_export_range_active(config)
        && !show_full_activity
        && !geometry.progress_values.is_empty()
    {
        relative_distance_frame_progress_values(config, activity, dense_activity)
            .unwrap_or_else(|| frame_progress_values(config, activity, dense_activity))
    } else {
        frame_progress_values(config, activity, dense_activity)
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

fn raw_elevation_points(source: &[Option<f64>], progress: &[f64]) -> Vec<(f32, f64)> {
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

fn raw_elevation_points_with_optional_progress(
    source: &[Option<f64>],
    progress_values: &[Option<f64>],
) -> Vec<(f32, f64)> {
    let normalized_progress =
        normalize_optional_progress_window(progress_values).unwrap_or_else(|| {
            (0..source.len())
                .map(|index| {
                    if source.len() > 1 {
                        index as f64 / (source.len() - 1) as f64
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>()
        });

    raw_elevation_points(source, &normalized_progress)
}

fn build_elevation_source_points(
    config: &RenderConfig,
    activity: &ParsedActivity,
    show_full_activity: bool,
) -> Result<Vec<(f32, f64)>, String> {
    if show_full_activity || !custom_export_range_active(config) {
        let source = if activity.sample_elevations.is_empty() {
            &activity.elevation
        } else {
            &activity.sample_elevations
        };
        return Ok(raw_elevation_points(
            source,
            &activity.sample_distance_progress,
        ));
    }

    let trimmed = trim_activity(
        activity,
        config.scene.start,
        config.scene.end,
        &RenderDataRequirements {
            distance_progress: true,
            elevation: true,
            ..RenderDataRequirements::default()
        },
    )?;

    Ok(raw_elevation_points_with_optional_progress(
        &trimmed.elevation,
        &trimmed.sample_distance_progress,
    ))
}

#[derive(Clone, Copy)]
struct ReducedElevationPoint {
    progress01: f32,
    elevation: f64,
    preserve: bool,
}

fn downsample_elevation_points(
    points: &[(f32, f64)],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count || target_count < 3 {
        return points
            .iter()
            .enumerate()
            .map(|(index, (progress01, elevation))| ReducedElevationPoint {
                progress01: *progress01,
                elevation: *elevation,
                preserve: index == 0 || index + 1 == points.len(),
            })
            .collect();
    }

    let smoothed = smooth_elevation_points(points);
    select_evenly_spaced_elevation_points(&smoothed, target_count)
}

fn smooth_elevation_points(points: &[(f32, f64)]) -> Vec<ReducedElevationPoint> {
    const COEFFICIENTS: [f64; 11] = [
        -36.0, 9.0, 44.0, 69.0, 84.0, 89.0, 84.0, 69.0, 44.0, 9.0, -36.0,
    ];
    let radius = COEFFICIENTS.len() / 2;

    points
        .iter()
        .enumerate()
        .map(|(index, (progress01, elevation))| {
            let mut total = 0.0f64;
            let mut coefficient_total = 0.0f64;

            for (offset, coefficient) in COEFFICIENTS.iter().enumerate() {
                let neighbor_index = index as isize + offset as isize - radius as isize;
                if neighbor_index < 0 || neighbor_index >= points.len() as isize {
                    continue;
                }
                let neighbor_value = points[neighbor_index as usize].1;
                if !neighbor_value.is_finite() {
                    continue;
                }
                total += neighbor_value * coefficient;
                coefficient_total += coefficient;
            }

            let smoothed_elevation = if coefficient_total.abs() <= f64::EPSILON {
                *elevation
            } else {
                total / coefficient_total
            };

            ReducedElevationPoint {
                progress01: *progress01,
                elevation: smoothed_elevation,
                preserve: index == 0 || index + 1 == points.len(),
            }
        })
        .collect()
}

fn select_evenly_spaced_elevation_points(
    points: &[ReducedElevationPoint],
    target_count: usize,
) -> Vec<ReducedElevationPoint> {
    if points.len() <= target_count {
        return points.to_vec();
    }

    let mut selected = Vec::with_capacity(target_count);
    let last_index = points.len() - 1;
    for sample_index in 0..target_count {
        let source_index = ((sample_index as f64 * last_index as f64)
            / (target_count.saturating_sub(1).max(1) as f64))
            .round() as usize;
        if let Some(point) = points.get(source_index.min(last_index)).copied() {
            if selected
                .last()
                .map(|last: &ReducedElevationPoint| {
                    (last.progress01 - point.progress01).abs() <= f32::EPSILON
                })
                .unwrap_or(false)
            {
                continue;
            }
            selected.push(point);
        }
    }

    if selected.len() == 1 && points.len() > 1 {
        selected.push(*points.last().unwrap());
    }

    selected
}

#[derive(Clone, Copy)]
struct ElevationSample {
    point: (f32, f32),
    progress01: f32,
    preserve: bool,
}

fn simplify_elevation_samples(points: &[ElevationSample], tolerance: f32) -> Vec<ElevationSample> {
    if points.len() <= 2 || tolerance <= 0.0 {
        return points.to_vec();
    }

    let preserved_indexes = points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| point.preserve.then_some(index))
        .collect::<Vec<_>>();
    if preserved_indexes.len() >= 2 {
        let mut result = Vec::new();
        for window in preserved_indexes.windows(2) {
            let start = window[0];
            let end = window[1];
            let simplified_segment =
                simplify_elevation_samples_segment(&points[start..=end], tolerance);
            if result.is_empty() {
                result.extend(simplified_segment);
            } else {
                result.extend(simplified_segment.into_iter().skip(1));
            }
        }
        return result;
    }

    simplify_elevation_samples_segment(points, tolerance)
}

fn simplify_elevation_samples_segment(
    points: &[ElevationSample],
    tolerance: f32,
) -> Vec<ElevationSample> {
    if points.len() <= 2 || tolerance <= 0.0 {
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

    let left = simplify_elevation_samples_segment(&points[..=split_index], tolerance);
    let right = simplify_elevation_samples_segment(&points[split_index..], tolerance);
    [left[..left.len() - 1].to_vec(), right].concat()
}

fn project_elevation_points(
    points: &[ReducedElevationPoint],
    width: f32,
    height: f32,
    margin: f32,
    y_scale: f32,
) -> Vec<(f32, f32)> {
    let min_elevation = points
        .iter()
        .map(|point| point.elevation)
        .fold(f64::INFINITY, f64::min);
    let max_elevation = points
        .iter()
        .map(|point| point.elevation)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max_elevation - min_elevation).max(1e-9);
    let inner_width = (width * (1.0 - 2.0 * margin)).max(1.0);
    let inner_height = (height * (1.0 - 2.0 * margin)).max(1.0);

    points
        .iter()
        .map(|point| {
            let progress01 = point.progress01.clamp(0.0, 1.0);
            let normalized = ((point.elevation - min_elevation) / span) as f32;
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
