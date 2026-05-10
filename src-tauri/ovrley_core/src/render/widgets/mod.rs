//! Overlay widget preparation and drawing.
//!
//! Widgets are split into a preparation phase and a per-frame composition phase.
//! Preparation normalizes template options, projects source telemetry into
//! widget-local geometry, builds static layers, and precomputes marker states so
//! video rendering can draw each frame with predictable cost.

/// Shared geometry, style, and drawing helpers for all widgets.
mod common;
/// Elevation profile widget implementation.
mod elevation;
/// Route/course widget implementation.
mod route;
/// Shared widget cache and report types.
mod types;
/// Metric value widgets, including icons and gradient triangles.
mod value;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::debug::RenderProfiler;

pub(crate) use elevation::draw_elevation_widget;
pub(crate) use route::draw_route_widget;
pub use types::{PreparedRenderAssets, WidgetRenderReport};
pub(crate) use value::{
    draw_metric_value_widget_with_config, draw_static_metric_icon_for_value, has_static_metric_icon,
};

/// Prepares all widget-specific caches needed by the active template.
///
/// Plot configuration is parsed lazily; absent widgets produce no cache, while
/// invalid present widgets return an error before rendering starts.
pub fn prepare_render_assets(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> Result<PreparedRenderAssets, String> {
    let mut assets = PreparedRenderAssets::default();

    if let Some(route_plot) = config.course_plot()? {
        assets.route_cache = Some(route::prepare_route_cache(
            config,
            activity,
            dense_activity,
            &route_plot,
            prepare_profiler,
        )?);
    }

    if let Some(elevation_plot) = config.elevation_plot()? {
        assets.elevation_cache = Some(elevation::prepare_elevation_cache(
            config,
            activity,
            dense_activity,
            &elevation_plot,
            prepare_profiler,
        )?);
    }

    Ok(assets)
}
