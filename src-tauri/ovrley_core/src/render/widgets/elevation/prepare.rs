/// Elevation widget preparation: sample extraction, geometry fitting, and
/// static layer construction.
///
/// Preparation owns smoothing, downsampling, simplification, static layers,
/// and frame-state generation so the render loop stays predictable.
use super::super::common::{
    custom_export_range_active, normalize_optional_progress_window, static_layer_padding,
};
use super::super::polyline::{draw_area, draw_polyline_with_shadow};
use super::super::types::{
    ElevationWidgetCache, NormalizedElevationPlot, StaticLayer, WidgetGeometry,
};
use super::reduction::{
    downsample_elevation_points, project_elevation_points, simplify_elevation_samples,
};
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::activity::trim::trim_activity;
use crate::debug::RenderProfiler;
use crate::error::{CoreError, CoreResult};
use crate::normalize::RenderDataRequirements;
use crate::render::surface::create_surface;
use std::time::Instant;

/// Prepares cached geometry, static layers, and frame states for an elevation
/// plot.
pub(crate) fn prepare_elevation_cache(
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    validated: &crate::normalize::ValidatedElevationPlot,
    scene: &crate::normalize::ValidatedSceneConfig,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<ElevationWidgetCache> {
    let prepare_started = Instant::now();
    let show_full_activity = validated.show_full_activity;
    let plot = super::normalize::normalize_elevation_plot(validated, scene);
    let raw_points = build_elevation_source_points(activity, show_full_activity, scene)?;
    let geometry = prepare_profiler.measure("build_elevation_cache.geometry", || {
        build_elevation_geometry(&plot, &raw_points)
    })?;
    let marker_layers = super::super::marker::marker_layers_from_plot(
        &plot.marker_variant,
        plot.marker_variant_diameter,
        plot.marker_size,
        &plot.marker_color,
        plot.marker_opacity,
    );
    let remaining_layer = prepare_profiler.measure("build_elevation_cache.layers", || {
        build_elevation_remaining_layer(&plot, &geometry)
    })?;
    let frame_states = prepare_profiler.measure("build_elevation_cache.frame_states", || {
        super::frame_state::build_elevation_frame_states(
            scene,
            activity,
            dense_activity,
            &geometry,
            &plot,
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

/// Builds the cached static layer for the not-yet-completed elevation profile.
///
/// The remaining area/line does not change per frame, so it is cached as a
/// Skia image with enough padding for stroke and shadow overflow.
fn build_elevation_remaining_layer(
    plot: &NormalizedElevationPlot,
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

/// Builds simplified widget-space geometry for the elevation profile.
///
/// Elevation traces are smoothed and reduced before projection, then RDP is
/// applied in screen space to remove visually redundant points.
pub(crate) fn build_elevation_geometry(
    plot: &NormalizedElevationPlot,
    raw_points: &[(f32, f64, f32)],
) -> CoreResult<WidgetGeometry> {
    use super::super::common::DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER;

    if raw_points.is_empty() {
        return Err(CoreError::Render(
            "Elevation plot requires elevation samples".into(),
        ));
    }

    let target_count =
        ((plot.width as f32) * DEFAULT_ELEVATION_DOWNSAMPLE_MULTIPLIER * plot.target_density)
            .round()
            .max(2.0) as usize;
    let downsampled = downsample_elevation_points(raw_points, target_count.min(raw_points.len()));
    let projected = project_elevation_points(
        &downsampled,
        plot.width as f32,
        plot.height as f32,
        0.0,
        plot.y_scale,
    );
    let projected_samples = downsampled
        .iter()
        .zip(projected.iter())
        .map(|(sample, point)| super::reduction::ElevationSample {
            point: *point,
            progress01: sample.progress01,
            elapsed_fraction: sample.elapsed_fraction,
            preserve: sample.preserve,
        })
        .collect::<Vec<_>>();
    let simplified = simplify_elevation_samples(&projected_samples, plot.simplify_tolerance_px);

    let min_elevation = raw_points
        .iter()
        .map(|(_, elev, _)| *elev)
        .fold(f64::INFINITY, f64::min);
    let max_elevation = raw_points
        .iter()
        .map(|(_, elev, _)| *elev)
        .fold(f64::NEG_INFINITY, f64::max);

    Ok(WidgetGeometry {
        bbox: (0.0, 0.0, plot.width as f32, plot.height as f32),
        progress_values: simplified.iter().map(|sample| sample.progress01).collect(),
        elapsed_fractions: simplified.iter().map(|sample| sample.elapsed_fraction).collect(),
        elevation_data_range: Some((min_elevation, max_elevation)),
        points: simplified.iter().map(|sample| sample.point).collect(),
        source_point_count: raw_points.len(),
        simplification: format!(
            "sg11_density_{:.2}_rdp_px_{:.2}",
            plot.target_density, plot.simplify_tolerance_px
        ),
    })
}

/// Extracts finite elevation samples paired with normalized distance progress
/// and elapsed-time fraction (0..1).
fn raw_elevation_points(
    source: &[Option<f64>],
    progress: &[f64],
    elapsed_seconds: &[f64],
) -> Vec<(f32, f64, f32)> {
    let source_duration = elapsed_seconds.last().copied().unwrap_or(1.0).max(1e-9);
    source
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let value = (*value)?;
            let progress_value = progress
                .get(index)
                .copied()
                .unwrap_or_else(|| index as f64 / source.len().saturating_sub(1).max(1) as f64);
            let elapsed = elapsed_seconds
                .get(index)
                .copied()
                .unwrap_or_else(|| index as f64);
            if value.is_finite() && progress_value.is_finite() {
                Some((
                    progress_value.clamp(0.0, 1.0) as f32,
                    value,
                    (elapsed / source_duration).clamp(0.0, 1.0) as f32,
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Extracts elevation samples after normalizing optional trimmed progress
/// values.
fn raw_elevation_points_with_optional_progress(
    source: &[Option<f64>],
    progress_values: &[Option<f64>],
    elapsed_seconds: &[f64],
) -> Vec<(f32, f64, f32)> {
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

    raw_elevation_points(source, &normalized_progress, elapsed_seconds)
}

/// Selects full-activity or trimmed source elevation samples for geometry.
///
/// Custom export ranges trim the source samples so the profile itself can
/// represent only the selected slice unless the template asks for full view.
pub(crate) fn build_elevation_source_points(
    activity: &ParsedActivity,
    show_full_activity: bool,
    scene: &crate::normalize::ValidatedSceneConfig,
) -> CoreResult<Vec<(f32, f64, f32)>> {
    if show_full_activity || !custom_export_range_active(scene) {
        let source = if activity.sample_elevations.is_empty() {
            &activity.elevation
        } else {
            &activity.sample_elevations
        };
        return Ok(raw_elevation_points(
            source,
            &activity.sample_distance_progress,
            &activity.sample_elapsed_seconds,
        ));
    }

    let trimmed = trim_activity(
        activity,
        scene.start,
        scene.end,
        &RenderDataRequirements {
            distance_progress: true,
            elevation: true,
            ..RenderDataRequirements::default()
        },
    )?;

    Ok(raw_elevation_points_with_optional_progress(
        &trimmed.elevation,
        &trimmed.sample_distance_progress,
        &trimmed.sample_elapsed_seconds,
    ))
}
