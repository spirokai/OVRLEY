use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::commands::AppPaths;
use crate::config::{CoursePlotConfig, ElevationPlotConfig, MarkerPointConfig, RenderConfig};
use crate::debug::RenderProfiler;
use crate::render::text::{draw_text, parse_color, ResolvedTextStyle};
use skia_safe::{Canvas, Paint, PaintCap, PaintJoin, Path as SkPath, Point};
use std::collections::BTreeMap;

const DEFAULT_COLOR: &str = "#ffffff";
const DEFAULT_LINE_WIDTH: f32 = 1.75;
const DEFAULT_MARGIN: f32 = 0.1;
const DEFAULT_POINT_WEIGHT: f32 = 80.0;
const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX: f32 = 1.0;
const DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER: f32 = 1.0;
const DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER: f32 = 2.0;
const DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER: f32 = 2.5;
const DEFAULT_ELEVATION_MARKER_SCALE: f32 = 2.5;
const DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER: f32 = 2.5;

#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetGeometryReport {
    pub point_count: usize,
    pub bbox: [f32; 4],
    pub widget_width: u32,
    pub widget_height: u32,
    pub rotation_deg: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetFrameReport {
    pub progress01: f32,
    pub marker_x: f32,
    pub marker_y: f32,
    pub marker_abs_x: f32,
    pub marker_abs_y: f32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct WidgetRenderReport {
    pub geometry: WidgetGeometryReport,
    pub frame: WidgetFrameReport,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedRenderAssets {
    pub route_cache: Option<RouteWidgetCache>,
    pub elevation_cache: Option<ElevationWidgetCache>,
}

#[derive(Clone, Debug)]
pub struct WidgetGeometry {
    points: Vec<(f32, f32)>,
    bbox: (f32, f32, f32, f32),
    progress_values: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct RouteFrameState {
    progress01: f32,
    marker_x: f32,
    marker_y: f32,
}

#[derive(Clone, Debug)]
pub struct ElevationFrameState {
    progress01: f32,
    marker_x: f32,
    marker_y: f32,
    elevation_m: f64,
}

#[derive(Clone, Debug)]
struct MarkerLayer {
    radius: f32,
    color: String,
    opacity: f32,
    solid_fill: bool,
}

#[derive(Clone, Debug)]
pub struct RouteWidgetCache {
    plot: NormalizedRoutePlot,
    geometry: WidgetGeometry,
    frame_states: Vec<RouteFrameState>,
    marker_layers: Vec<MarkerLayer>,
}

#[derive(Clone, Debug)]
pub struct ElevationWidgetCache {
    plot: NormalizedElevationPlot,
    geometry: WidgetGeometry,
    frame_states: Vec<ElevationFrameState>,
    marker_layers: Vec<MarkerLayer>,
}

#[derive(Clone, Debug)]
struct NormalizedRoutePlot {
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    rotation: f32,
    margin: f32,
    remaining_line_width: f32,
    remaining_line_color: String,
    remaining_line_opacity: f32,
    completed_line_width: f32,
    completed_line_color: String,
    completed_line_opacity: f32,
    marker_size: f32,
    marker_color: String,
    marker_opacity: f32,
    marker_points: Vec<MarkerPointConfig>,
}

#[derive(Clone, Debug)]
struct NormalizedElevationPlot {
    x: f32,
    y: f32,
    width: u32,
    height: u32,
    rotation: f32,
    margin: f32,
    y_scale: f32,
    remaining_line_width: f32,
    remaining_line_color: String,
    remaining_line_opacity: f32,
    completed_line_width: f32,
    completed_line_color: String,
    completed_line_opacity: f32,
    area_remaining_color: String,
    area_remaining_opacity: f32,
    area_completed_color: String,
    area_completed_opacity: f32,
    marker_size: f32,
    marker_color: String,
    marker_opacity: f32,
    marker_points: Vec<MarkerPointConfig>,
    show_elevation_metric: bool,
    show_elevation_imperial: bool,
    metric_label_offset_x: f32,
    metric_label_offset_y: f32,
    imperial_label_offset_x: f32,
    imperial_label_offset_y: f32,
    label_font: Option<String>,
    label_font_size: f32,
    label_color: String,
    label_decimal_rounding: Option<i32>,
    legacy_label_units: Vec<String>,
}

pub fn prepare_render_assets(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> Result<PreparedRenderAssets, String> {
    let mut assets = PreparedRenderAssets::default();

    if let Some(route_plot) = config.course_plot()? {
        assets.route_cache = Some(prepare_profiler.measure("build_route_cache", || {
            build_route_cache(config, activity, dense_activity, &route_plot)
        })?);
    }

    if let Some(elevation_plot) = config.elevation_plot()? {
        assets.elevation_cache = Some(prepare_profiler.measure(
            "build_elevation_cache",
            || build_elevation_cache(config, activity, dense_activity, &elevation_plot),
        )?);
    }

    Ok(assets)
}

pub fn draw_route_widget(
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

pub fn draw_elevation_widget(
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

fn build_route_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &CoursePlotConfig,
) -> Result<RouteWidgetCache, String> {
    let plot = normalize_route_plot(config, plot);
    let geometry = build_route_geometry(&plot, activity)?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let frame_states = build_route_frame_states(config, activity, &geometry, dense_activity);

    Ok(RouteWidgetCache {
        plot,
        geometry,
        frame_states,
        marker_layers,
    })
}

fn build_elevation_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &ElevationPlotConfig,
) -> Result<ElevationWidgetCache, String> {
    let plot = normalize_elevation_plot(config, plot);
    let geometry = build_elevation_geometry(&plot, activity)?;
    let marker_layers = marker_layers_from_points(&plot.marker_points);
    let frame_states = build_elevation_frame_states(config, activity, dense_activity, &geometry);

    Ok(ElevationWidgetCache {
        plot,
        geometry,
        frame_states,
        marker_layers,
    })
}

fn normalize_route_plot(_config: &RenderConfig, plot: &CoursePlotConfig) -> NormalizedRoutePlot {
    let base_color = plot.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string());
    let legacy_width = plot
        .line
        .as_ref()
        .and_then(|line| line.width)
        .unwrap_or(DEFAULT_LINE_WIDTH)
        * DEFAULT_ROUTE_LINE_WIDTH_MULTIPLIER;
    let marker_size = plot.marker_size.unwrap_or_else(|| {
        plot.points
            .iter()
            .filter_map(|point| point.weight)
            .map(|weight| weight.sqrt())
            .fold(18.0, f32::max)
    });

    NormalizedRoutePlot {
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(DEFAULT_MARGIN),
        remaining_line_width: plot.remaining_line_width.unwrap_or(legacy_width),
        remaining_line_color: plot
            .remaining_line_color
            .clone()
            .or_else(|| plot.line.as_ref().and_then(|line| line.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        remaining_line_opacity: normalize_opacity(
            plot.remaining_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            0.75,
        ),
        completed_line_width: plot.completed_line_width.unwrap_or(legacy_width),
        completed_line_color: plot
            .completed_line_color
            .clone()
            .or_else(|| plot.line.as_ref().and_then(|line| line.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        completed_line_opacity: normalize_opacity(
            plot.completed_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            1.0,
        ),
        marker_size,
        marker_color: plot.marker_color.clone().unwrap_or_else(|| base_color.clone()),
        marker_opacity: normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0),
        marker_points: if plot.points.is_empty() {
            vec![MarkerPointConfig {
                weight: Some(marker_size.powi(2)),
                color: Some(plot.marker_color.clone().unwrap_or_else(|| base_color.clone())),
                opacity: Some(normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0)),
                extra: BTreeMap::new(),
            }]
        } else {
            plot.points.clone()
        },
    }
}

fn normalize_elevation_plot(
    config: &RenderConfig,
    plot: &ElevationPlotConfig,
) -> NormalizedElevationPlot {
    let base_color = plot.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string());
    let legacy_width = plot
        .line
        .as_ref()
        .and_then(|line| line.width)
        .unwrap_or(DEFAULT_LINE_WIDTH)
        * DEFAULT_ELEVATION_LINE_WIDTH_MULTIPLIER;
    let marker_size = plot.marker_size.unwrap_or_else(|| {
        plot.points
            .iter()
            .filter_map(|point| point.weight)
            .map(|weight| weight.sqrt() * DEFAULT_ELEVATION_MARKER_SCALE.sqrt())
            .fold(16.0, f32::max)
    });
    let point_label = plot.point_label.clone().unwrap_or_default();

    NormalizedElevationPlot {
        x: plot.x,
        y: plot.y,
        width: plot.width,
        height: plot.height,
        rotation: plot.rotation,
        margin: plot.margin.unwrap_or(0.0),
        y_scale: plot.y_scale.unwrap_or(1.0).clamp(0.2, 4.0),
        remaining_line_width: plot.remaining_line_width.unwrap_or(legacy_width),
        remaining_line_color: plot
            .remaining_line_color
            .clone()
            .or_else(|| plot.line.as_ref().and_then(|line| line.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        remaining_line_opacity: normalize_opacity(
            plot.remaining_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            1.0,
        ),
        completed_line_width: plot.completed_line_width.unwrap_or(legacy_width),
        completed_line_color: plot
            .completed_line_color
            .clone()
            .or_else(|| plot.line.as_ref().and_then(|line| line.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        completed_line_opacity: normalize_opacity(
            plot.completed_line_opacity
                .or_else(|| plot.line.as_ref().and_then(|line| line.opacity))
                .or(plot.opacity),
            1.0,
        ),
        area_remaining_color: plot
            .area_remaining_color
            .clone()
            .or_else(|| plot.fill.as_ref().and_then(|fill| fill.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        area_remaining_opacity: normalize_opacity(
            plot.area_remaining_opacity.or_else(|| {
                plot.fill
                    .as_ref()
                    .and_then(|fill| fill.opacity)
                    .map(|opacity| opacity * 0.35)
            }),
            0.12,
        ),
        area_completed_color: plot
            .area_completed_color
            .clone()
            .or_else(|| plot.fill.as_ref().and_then(|fill| fill.color.clone()))
            .unwrap_or_else(|| base_color.clone()),
        area_completed_opacity: normalize_opacity(
            plot.area_completed_opacity
                .or_else(|| plot.fill.as_ref().and_then(|fill| fill.opacity)),
            0.24,
        ),
        marker_size,
        marker_color: plot.marker_color.clone().unwrap_or_else(|| base_color.clone()),
        marker_opacity: normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0),
        marker_points: if plot.points.is_empty() {
            vec![MarkerPointConfig {
                weight: Some(marker_size.powi(2)),
                color: Some(plot.marker_color.clone().unwrap_or_else(|| base_color.clone())),
                opacity: Some(normalize_opacity(plot.marker_opacity.or(plot.opacity), 1.0)),
                extra: BTreeMap::new(),
            }]
        } else {
            plot.points.clone()
        },
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
        label_decimal_rounding: point_label.decimal_rounding.or(config.scene.decimal_rounding),
        legacy_label_units: point_label.units,
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
    let tolerance = DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_PX * DEFAULT_ROUTE_SIMPLIFY_TOLERANCE_MULTIPLIER;
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
    })
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

fn build_route_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    geometry: &WidgetGeometry,
    dense_activity: &DenseActivityReport,
) -> Vec<RouteFrameState> {
    let total_frames = dense_activity.frame_count.max(1);
    (0..total_frames)
        .map(|frame_index| {
            let progress01 =
                absolute_distance_progress_for_frame(config, activity, dense_activity, frame_index);
            let (_, marker_x, marker_y) = point_at_metric_progress(
                &geometry.points,
                &geometry.progress_values,
                progress01,
            )
            .unwrap_or_else(|| route_position_at_progress(&geometry.points, progress01));
            RouteFrameState {
                progress01,
                marker_x,
                marker_y,
            }
        })
        .collect()
}

fn build_elevation_frame_states(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    geometry: &WidgetGeometry,
) -> Vec<ElevationFrameState> {
    let total_frames = dense_activity.frame_count.max(1);

    (0..total_frames)
        .map(|frame_index| {
            let progress01 =
                absolute_distance_progress_for_frame(config, activity, dense_activity, frame_index);
            let (_, marker_x, marker_y) = point_at_metric_progress(
                &geometry.points,
                &geometry.progress_values,
                progress01,
            )
            .or_else(|| point_at_progress_x(&geometry.points, progress01))
            .unwrap_or((0, 0.0, 0.0));
            let elevation_m = interpolated_elevation_at_progress(activity, progress01)
                .or_else(|| dense_activity.series.elevation.get(frame_index).and_then(|value| *value))
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

#[derive(Clone, Copy, Debug)]
struct RouteSample {
    point: (f32, f32),
    progress01: f32,
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
                .unwrap_or_else(|| index as f64 / activity.sample_course_points.len().saturating_sub(1).max(1) as f64)
                .clamp(0.0, 1.0) as f32,
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
        if let Some(min_point) = bucket.iter().copied().min_by(|left, right| left.1.total_cmp(&right.1)) {
            sampled.push(min_point);
        }
        if let Some(max_point) = bucket.iter().copied().max_by(|left, right| left.1.total_cmp(&right.1)) {
            sampled.push(max_point);
        }
        bucket_index += bucket_size.max(1.0);
    }
    sampled.push(*points.last().unwrap_or(&points[0]));
    sampled.sort_by(|left, right| left.0.total_cmp(&right.0));
    sampled.dedup_by(|left, right| (left.0 - right.0).abs() <= f32::EPSILON && (left.1 - right.1).abs() <= f64::EPSILON);
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

fn fit_points_to_widget(
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
    let max_x = points.iter().map(|(x, _)| *x).fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|(_, y)| *y).fold(f32::NEG_INFINITY, f32::max);
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
        let distance = perpendicular_distance(points[index].point, points[0].point, points.last().unwrap().point);
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

fn absolute_distance_progress_for_frame(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
) -> f32 {
    let absolute_second = config.scene.start
        + dense_activity
            .frame_elapsed_seconds
            .get(frame_index)
            .copied()
            .unwrap_or_default();
    interpolate_progress_at_elapsed(activity, absolute_second)
}

fn interpolate_progress_at_elapsed(activity: &ParsedActivity, elapsed_second: f64) -> f32 {
    let elapsed = &activity.sample_elapsed_seconds;
    let progress = &activity.sample_distance_progress;
    if elapsed.len() >= 2 && progress.len() >= 2 {
        if let Some(value) = interpolate_numeric_series(elapsed, progress, elapsed_second) {
            return value.clamp(0.0, 1.0) as f32;
        }
    }

    let first = elapsed.first().copied().unwrap_or(0.0);
    let last = elapsed.last().copied().unwrap_or(first);
    let duration = (last - first).max(1e-9);
    ((elapsed_second - first) / duration).clamp(0.0, 1.0) as f32
}

fn interpolate_numeric_series(x_values: &[f64], y_values: &[f64], target_x: f64) -> Option<f64> {
    if x_values.len() != y_values.len() || x_values.is_empty() {
        return None;
    }
    if target_x <= x_values[0] {
        return Some(y_values[0]);
    }
    let last_index = x_values.len() - 1;
    if target_x >= x_values[last_index] {
        return Some(y_values[last_index]);
    }

    for index in 1..x_values.len() {
        let left_x = x_values[index - 1];
        let right_x = x_values[index];
        let left_y = y_values[index - 1];
        let right_y = y_values[index];
        if !left_x.is_finite() || !right_x.is_finite() || !left_y.is_finite() || !right_y.is_finite() {
            continue;
        }
        if right_x < target_x {
            continue;
        }
        let delta = (right_x - left_x).max(f64::EPSILON);
        let mix = (target_x - left_x) / delta;
        return Some(left_y + (right_y - left_y) * mix);
    }

    Some(y_values[last_index])
}

fn route_position_at_progress(
    points: &[(f32, f32)],
    progress_limit: f32,
) -> (usize, f32, f32) {
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

fn build_route_prefix_points(geometry: &WidgetGeometry, state: &RouteFrameState) -> Vec<(f32, f32)> {
    let mut points = geometry
        .points
        .iter()
        .zip(geometry.progress_values.iter())
        .filter_map(|(point, progress)| (*progress <= state.progress01).then_some(*point))
        .collect::<Vec<_>>();
    if points.is_empty()
        || distance(*points.last().unwrap_or(&(f32::MIN, f32::MIN)), (state.marker_x, state.marker_y)) > 1e-3
    {
        points.push((state.marker_x, state.marker_y));
    }
    points
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
    if distance(*result.last().unwrap_or(&points[0]), marker_point) > 1e-3 {
        result.push(marker_point);
    }
    result
}

fn point_at_metric_progress(
    points: &[(f32, f32)],
    progress_values: &[f32],
    target_progress: f32,
) -> Option<(usize, f32, f32)> {
    if points.is_empty() || progress_values.len() != points.len() {
        return None;
    }
    let safe_target = target_progress.clamp(0.0, 1.0);
    if safe_target <= progress_values[0] {
        return Some((0, points[0].0, points[0].1));
    }
    let last_index = points.len() - 1;
    if safe_target >= progress_values[last_index] {
        return Some((last_index, points[last_index].0, points[last_index].1));
    }

    for index in 1..points.len() {
        let left_progress = progress_values[index - 1];
        let right_progress = progress_values[index];
        if right_progress < safe_target {
            continue;
        }
        let span = (right_progress - left_progress).max(1e-6);
        let mix = (safe_target - left_progress) / span;
        let left_point = points[index - 1];
        let right_point = points[index];
        return Some((
            index,
            left_point.0 + (right_point.0 - left_point.0) * mix,
            left_point.1 + (right_point.1 - left_point.1) * mix,
        ));
    }

    Some((last_index, points[last_index].0, points[last_index].1))
}

fn point_at_progress_x(points: &[(f32, f32)], progress01: f32) -> Option<(usize, f32, f32)> {
    let target_x = progress01.clamp(0.0, 1.0);
    let scaled_index = target_x * (points.len().saturating_sub(1) as f32);
    let index = scaled_index.floor() as usize;
    let point = points.get(index.min(points.len().saturating_sub(1)))?;
    Some((index, point.0, point.1))
}

fn interpolated_elevation_at_progress(activity: &ParsedActivity, progress01: f32) -> Option<f64> {
    let elevations = if activity.sample_elevations.is_empty() {
        &activity.elevation
    } else {
        &activity.sample_elevations
    };
    let progress_values = &activity.sample_distance_progress;
    if elevations.is_empty() || progress_values.is_empty() {
        return None;
    }

    let values = elevations
        .iter()
        .map(|value| *value)
        .collect::<Vec<_>>();
    interpolate_optional_numeric_series(progress_values, &values, progress01 as f64)
}

fn interpolate_optional_numeric_series(
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

fn draw_polyline(
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
    paint.set_color(parse_color(color, opacity));
    canvas.draw_path(&path, &paint);
}

fn draw_area(
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
    paint.set_color(parse_color(color, opacity));
    canvas.draw_path(&path, &paint);
}

fn draw_marker(
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
        paint.set_color(parse_color(fallback_color, fallback_opacity));
        canvas.draw_circle(Point::new(x, y), fallback_radius.max(2.0), &paint);
        return;
    }

    for layer in layers {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(parse_color(&layer.color, layer.opacity));
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

fn marker_layers_from_points(points: &[MarkerPointConfig]) -> Vec<MarkerLayer> {
    let mut layers = points
        .iter()
        .map(|point| MarkerLayer {
            radius: point.weight.unwrap_or(DEFAULT_POINT_WEIGHT).max(1.0).sqrt().max(2.0),
            color: point.color.clone().unwrap_or_else(|| DEFAULT_COLOR.to_string()),
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

fn path_from_points(points: &[(f32, f32)], close_path: bool, baseline_y: Option<f32>) -> SkPath {
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

fn with_widget_transform(
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

fn rotate_point_to_canvas(
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
    (widget_x + rotated_x + center_x, widget_y + rotated_y + center_y)
}

fn widget_render_report(
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

fn format_elevation_label(value_m: f64, unit: &str, decimal_rounding: Option<i32>) -> String {
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

fn normalize_opacity(value: Option<f32>, default: f32) -> f32 {
    match value {
        Some(value) if value > 1.0 => (value / 100.0).clamp(0.0, 1.0),
        Some(value) => value.clamp(0.0, 1.0),
        None => default,
    }
}

fn distance(left: (f32, f32), right: (f32, f32)) -> f32 {
    ((right.0 - left.0).powi(2) + (right.1 - left.1).powi(2)).sqrt()
}
