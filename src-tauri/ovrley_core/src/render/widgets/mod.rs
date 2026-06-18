//! Overlay widget preparation and drawing.
//!
//! Widgets are split into a preparation phase and a per-frame composition phase.
//! Preparation normalizes template options, projects source telemetry into
//! widget-local geometry, builds static layers, and precomputes marker states so
//! video rendering can draw each frame with predictable cost.

/// Shared geometry, style, and drawing helpers for all widgets.
pub(crate) mod common;
/// Elevation profile widget implementation.
pub(crate) mod elevation;
/// Point/rect/math and layout-fitting helpers.
mod geometry;
/// Heading compass tape widget implementation.
pub mod heading;
/// Linear gauge metric widget implementation.
pub mod linear_gauge;
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

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::debug::RenderProfiler;
use crate::error::CoreResult;
use crate::normalize::ValidatedRenderConfig;
use crate::paths::AppPaths;
use crate::render::widgets::types::PreparedValue;
use std::collections::BTreeMap;

pub(crate) use elevation::draw_elevation_widget;
pub use linear_gauge::{draw_linear_gauge_widget, prepare_linear_gauge_cache};
pub use metric_presentation::draw_metric_presentation;
pub(crate) use route::draw_route_widget;
pub use types::{
    MetricPresentationReport, PreparedRenderAssets, PresentationCache, WidgetRenderReport,
};
pub(crate) use value::{
    draw_metric_value_widget_with_config, draw_static_metric_icon_for_value_validated,
    has_static_metric_icon_validated,
};

// Module-local tests for widget-specific RDP behavior that exercise internal
// types (`ElevationSample`, `RouteSample`) not available from crate-level
// integration tests. These tests are in a `tests/` subdirectory, not inline,
// and are gated by `#[cfg(test)]` so they are excluded from production builds.
// The tested functions (`simplify_elevation_samples_segment`,
// `simplify_route_samples`) are `pub(crate)` — exposing them as full `pub`
// would leak widget internals into the public API.
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
    let labels = config.labels.clone();
    let values = config.values.clone();

    let mut assets = PreparedRenderAssets {
        scene,
        labels,
        values,
        route_cache: None,
        elevation_cache: None,
        presentation_caches: BTreeMap::new(),
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

    for (idx, value) in assets.values.iter().enumerate() {
        match value {
            PreparedValue::HeadingTape(validated) => {
                let cache = heading::prepare_heading_cache(
                    &assets.scene,
                    validated,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                assets
                    .presentation_caches
                    .insert(idx, types::PresentationCache::HeadingTape(cache));
            }
            PreparedValue::LinearGauge(validated) => {
                let cache = linear_gauge::prepare_linear_gauge_cache(
                    validated,
                    dense_activity,
                    &assets.scene,
                    assets.scene.scale,
                    &paths.font_dirs,
                    prepare_profiler,
                )?;
                assets
                    .presentation_caches
                    .insert(idx, types::PresentationCache::LinearGauge(cache));
            }
            _ => {}
        }
    }

    Ok(assets)
}
