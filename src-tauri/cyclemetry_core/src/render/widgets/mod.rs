mod common;
mod elevation;
mod route;
mod types;

use crate::activity::schema::{DenseActivityReport, ParsedActivity};
use crate::config::RenderConfig;
use crate::debug::RenderProfiler;

pub(crate) use elevation::draw_elevation_widget;
pub(crate) use route::draw_route_widget;
pub use types::{PreparedRenderAssets, WidgetRenderReport};

pub fn prepare_render_assets(
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
    prepare_profiler: &mut RenderProfiler,
) -> Result<PreparedRenderAssets, String> {
    let mut assets = PreparedRenderAssets::default();

    if let Some(route_plot) = config.course_plot()? {
        assets.route_cache = Some(prepare_profiler.measure("build_route_cache", || {
            route::prepare_route_cache(config, activity, dense_activity, &route_plot)
        })?);
    }

    if let Some(elevation_plot) = config.elevation_plot()? {
        assets.elevation_cache = Some(prepare_profiler.measure(
            "build_elevation_cache",
            || elevation::prepare_elevation_cache(config, activity, dense_activity, &elevation_plot),
        )?);
    }

    Ok(assets)
}
