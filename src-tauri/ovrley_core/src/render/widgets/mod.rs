//! Overlay widget preparation and drawing.
//!
//! Widgets are split into a preparation phase and a per-frame composition phase.
//! Preparation normalizes template options, projects source telemetry into
//! widget-local geometry, builds static layers, and precomputes marker states so
//! video rendering can draw each frame with predictable cost.

/// Static backdrop widget implementation.
pub(crate) mod backdrop;
/// Shared geometry, style, and drawing helpers for all widgets.
pub(crate) mod common;
/// Elevation profile widget implementation.
pub(crate) mod elevation;
/// G-force friction-circle widget implementation.
pub mod g_force;
/// Gauge renderers and gauge-specific shared infrastructure.
pub mod gauges;
/// Point/rect/math and layout-fitting helpers.
mod geometry;
/// Heading compass tape widget implementation.
pub mod heading;
/// Current and best lap timer text widget implementation.
pub(crate) mod lap_timer;
/// Lean-angle sector widget implementation.
pub mod lean_angle;
/// Marker and dot drawing helpers.
mod marker;
/// DisplayType-driven metric presentation dispatch.
pub mod metric_presentation;
/// Polyline and area drawing helpers.
mod polyline;
/// Route/course widget implementation.
pub(crate) mod route;
/// Skia path and coordinate transform helpers.
mod transform;
/// Shared widget cache and report types.
pub mod types;
/// Metric value widgets, including icons and gradient triangles.
pub mod value;

use crate::activity::elevation::preferred_elevation_series;
use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::ValidatedRenderConfig;
use crate::paths::AppPaths;
use crate::render::format::altitude_offset_m;
use crate::render::widgets::types::PreparedValue;

fn apply_altitude_value_offset(value: &mut PreparedValue, altitude_series: &[Option<f64>]) {
    match value {
        PreparedValue::StandardText(widget) => {
            widget.altitude_offset_m =
                altitude_offset_m(widget.validated.starting_altitude_m, altitude_series);
        }
        PreparedValue::LinearGauge(widget) => {
            widget.altitude_offset_m =
                altitude_offset_m(widget.validated.starting_altitude_m, altitude_series);
        }
        PreparedValue::ArcGauge(widget) => {
            widget.altitude_offset_m = altitude_offset_m(
                widget.validated.inner_value.starting_altitude_m,
                altitude_series,
            );
        }
        _ => {}
    }
}

pub(crate) use backdrop::draw_backdrops_static_layer;
pub(crate) use elevation::draw_elevation_widget;
pub use g_force::{draw_g_force_widget, prepare_g_force_cache};
pub use gauges::arc::{draw_arc_gauge_widget, prepare_arc_gauge_cache};
pub use gauges::linear::{draw_linear_gauge_widget, prepare_linear_gauge_cache};
pub use lap_timer::{lap_log_text_state, lap_timer_value_text, LapLogTextState};
pub use lean_angle::{draw_lean_angle_widget, prepare_lean_angle_cache};
pub use metric_presentation::draw_metric_presentation;
pub(crate) use route::draw_route_widget;
pub use types::{MetricPresentationReport, PreparedRenderAssets, WidgetRenderReport};
pub(crate) use value::{
    draw_metric_value_widget_with_config, draw_static_metric_parts_for_value,
    static_metric_parts_for_value, StaticMetricParts,
};

// White-box widget tests exercise internal geometry and reduction contracts
// that are not available from crate-level integration tests. They live in the
// dedicated `tests/` subdirectory and are excluded from production builds.
#[cfg(test)]
mod tests;

/// Prepares all widget-specific caches needed by the active template.
///
/// All config validation has already happened at the seam. This function
/// clones validated data into widget caches without re-validating.
pub fn prepare_render_assets(
    paths: &AppPaths,
    config: &ValidatedRenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<PreparedRenderAssets> {
    let scene = config.scene.clone();
    let backdrops = config.backdrops.clone();
    let labels = config.labels.clone();
    let values = config.values.clone();

    let mut assets = PreparedRenderAssets {
        scene,
        timezone: activity.timezone.clone(),
        backdrops,
        labels,
        values,
        route_cache: None,
        elevation_cache: None,
        base_rgba: None,
    };

    if let Some(validated) = &config.course_plot {
        assets.route_cache = Some(route::prepare_route_cache(
            activity,
            dense_activity,
            validated,
            &assets.scene,
            prepare_profiler,
        )?);
    }

    if let Some(validated) = &config.elevation_plot {
        assets.elevation_cache = Some(elevation::prepare_elevation_cache(
            activity,
            dense_activity,
            validated,
            &assets.scene,
            prepare_profiler,
        )?);
    }

    let altitude_series =
        preferred_elevation_series(&activity.barometric_altitude, &activity.elevation);

    for value in &mut assets.values {
        apply_altitude_value_offset(value, altitude_series);
        match value {
            PreparedValue::HeadingTape(widget) => {
                let cache = heading::prepare_heading_cache(
                    &assets.scene,
                    &widget.validated,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            PreparedValue::LinearGauge(widget) => {
                let cache = gauges::linear::prepare_linear_gauge_cache(
                    &widget.validated,
                    widget.altitude_offset_m,
                    dense_activity,
                    &assets.scene,
                    assets.scene.scale,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            PreparedValue::ArcGauge(widget) => {
                let cache = gauges::arc::prepare_arc_gauge_cache(
                    &widget.validated,
                    widget.altitude_offset_m,
                    dense_activity,
                    &assets.scene,
                    assets.scene.scale,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            PreparedValue::LeanAngle(widget) => {
                let cache = lean_angle::prepare_lean_angle_cache(
                    &widget.validated,
                    &assets.scene,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            PreparedValue::GForce(widget) => {
                let cache = g_force::prepare_g_force_cache(
                    &widget.validated,
                    &assets.scene,
                    activity,
                    dense_activity,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            PreparedValue::LapTimer(widget) => {
                let cache = lap_timer::prepare_lap_timer_cache(
                    &widget.validated,
                    dense_activity,
                    &assets.scene,
                    assets.scene.scale,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                widget.cache = Some(cache);
            }
            _ => {}
        }
    }

    Ok(assets)
}
