/// Route widget preparation: sample building, geometry fitting, and static
/// layer construction.
///
/// Builds all expensive geometry and frame-position data once before the render
/// loop. Per-frame drawing then only composites cached/static pieces.
use super::super::common::{
    custom_export_range_active, normalize_optional_progress_window, static_layer_padding,
};
use super::super::geometry::fit_points_to_widget_with_inset;
use super::super::marker::marker_layers_from_points;
use super::super::polyline::draw_polyline_with_shadow;
use super::super::types::{
    NormalizedRoutePlot, RouteSample, RouteWidgetCache, StaticLayer, WidgetGeometry,
};
use super::simplify::{downsample_route_samples, simplify_route_samples};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::config::{CoursePlotConfig, RenderConfig, RenderDataRequirements};
use crate::debug::RenderProfiler;
use crate::error::{CoreError, CoreResult};
use crate::render::surface::create_surface;
use std::time::Instant;

/// Prepares cached geometry, static layers, and frame states for a route plot.
pub(crate) fn prepare_route_cache(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    plot: &CoursePlotConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<RouteWidgetCache> {
    let prepare_started = Instant::now();
    let show_full_activity = plot.show_full_activity.unwrap_or(false);
    let plot = super::normalize::normalize_route_plot(config, plot);
    let route_samples = build_route_samples(config, activity, show_full_activity)?;
    let geometry = prepare_profiler.measure("build_route_cache.geometry", || {
        build_route_geometry(&plot, &route_samples)
    })?;
    let marker_layers = marker_layers_from_points(
        &plot.marker_points,
        &plot.marker_variant,
        plot.marker_variant_diameter,
        plot.marker_variant_stroke_width,
        &plot.marker_color,
        plot.marker_opacity,
    );
    let remaining_layer = prepare_profiler.measure("build_route_cache.layers", || {
        build_route_remaining_layer(&plot, &geometry)
    })?;
    let frame_states = prepare_profiler.measure("build_route_cache.frame_states", || {
        super::frame_state::build_route_frame_states(
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

/// Builds simplified widget-space geometry for the route path.
///
/// Downsample before RDP simplification to cap work for long activities while
/// preserving visually important points.
fn build_route_geometry(
    plot: &NormalizedRoutePlot,
    route_samples: &[RouteSample],
) -> CoreResult<WidgetGeometry> {
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

/// Builds the cached static layer for the not-yet-completed route.
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

/// Computes inset needed to keep route strokes and marker inside widget bounds.
fn route_geometry_inset_px(plot: &NormalizedRoutePlot) -> f32 {
    let line_inset = (plot.remaining_line_width.max(plot.completed_line_width) * 0.5).max(0.0);
    let marker_inset = plot.marker_size.max(plot.marker_variant_diameter * 0.5);
    marker_inset.max(line_inset) + 1.0
}

/// Selects full-activity or trimmed course samples for route geometry.
///
/// `show_full_activity` overrides custom export trimming so the full route
/// remains visible while progress can still follow the selected scene.
fn build_route_samples(
    config: &RenderConfig,
    activity: &ParsedActivity,
    show_full_activity: bool,
) -> CoreResult<Vec<RouteSample>> {
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

/// Projects latitude/longitude samples into local 2D route samples.
///
/// Uses an equirectangular approximation centered on mean latitude. This is
/// sufficient for small activity routes and avoids a heavy projection crate.
fn project_course_samples(
    course_points: &[(Option<f64>, Option<f64>)],
    progress_values: &[f64],
) -> Vec<RouteSample> {
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

/// Projects trimmed course samples after normalizing optional progress values.
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
