//! Native widget ingress: collection validation and renderer dispatch.

use super::{config, raster, RenderConfig, ValidatedRenderConfig};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::catalog::LayoutMode;
use ovrley_widgets::model::scene::ValidatedScenePresentation;
use ovrley_widgets::widgets::backdrop::validate_backdrop;
use ovrley_widgets::widgets::diving_profile::{
    validate_diving_profile, DivingProfileConfig, ValidatedDivingProfile,
};
use ovrley_widgets::widgets::elevation::{
    validate_elevation_plot, ElevationPlotConfig, ValidatedElevationPlot,
};
use ovrley_widgets::widgets::label::validate_label;
use ovrley_widgets::widgets::map::{validate_map_widget, MapConfig, ValidatedMapWidget};
use ovrley_widgets::widgets::rolling_chart::{
    validate_rolling_chart, RollingChartConfig, ValidatedRollingChart,
};
use ovrley_widgets::widgets::route::{validate_route_plot, CoursePlotConfig, ValidatedRoutePlot};
use ovrley_widgets::widgets::value::{ValidatedValue, ValueConfig, ValueFamily};

pub(super) fn validate_render_contents(
    raw: RenderConfig<ValidatedScenePresentation>,
    resources: Option<&dyn crate::raster::RasterResourceResolver>,
) -> CoreResult<ValidatedRenderConfig<ValidatedScenePresentation>> {
    let scene = raw.scene;
    raster::validate_unique_raster_ids(&raw.rasters)?;
    let mut rasters = Vec::with_capacity(raw.rasters.len());
    for (index, raster) in raw.rasters.iter().enumerate() {
        rasters.push(raster::validate_raster(raster, index, resources)?);
    }
    let mut backdrops = Vec::with_capacity(raw.backdrops.len());
    for (index, backdrop) in raw.backdrops.iter().enumerate() {
        backdrops.push(validate_backdrop(backdrop, index)?);
    }
    let mut values = Vec::with_capacity(raw.values.len());
    for (index, value) in raw.values.into_iter().enumerate() {
        values.push(validate_render_value(value, index, &scene)?);
    }
    let mut labels = Vec::with_capacity(raw.labels.len());
    for (index, label) in raw.labels.iter().enumerate() {
        labels.push(validate_label(label, index)?);
    }
    Ok(ValidatedRenderConfig {
        course_plots: validate_route_plots(&raw.plots)?,
        maps: validate_map_widgets(&raw.plots)?,
        elevation_plots: validate_elevation_plots(&raw.plots, &scene)?,
        diving_profiles: validate_diving_profiles(&raw.plots, &scene)?,
        rolling_charts: validate_rolling_charts(&raw.plots)?,
        scene,
        backdrops,
        rasters,
        labels,
        values,
    })
}

fn validate_render_value(
    mut value: ValueConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> CoreResult<ValidatedValue> {
    if value.display_type.layout_mode() == LayoutMode::Boxed {
        value = config::promote_value_variant(&value, value.display_type.as_str())?;
    }
    // Templates saved before heading tapes had their own label font use the scene's.
    if ValueFamily::of(value.value, value.display_type) == ValueFamily::HeadingTape
        && value.label_font.is_none()
    {
        value.label_font = scene.font.clone();
    }
    Ok(ValidatedValue::validate(value, index)?)
}

pub fn validate_route_plots(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRoutePlot>> {
    let mut validated = Vec::new();
    for (index, value) in config::find_plot_values(plots, "course") {
        let plot: CoursePlotConfig =
            serde_json::from_value(value.clone()).map_err(course_config_error)?;
        validated.push(validate_route_plot(&plot, index)?);
    }
    Ok(validated)
}

fn course_config_error(error: serde_json::Error) -> CoreError {
    CoreError::Config(format!("course plot config: {error}"))
}

fn validate_map_widgets(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedMapWidget>> {
    config::find_plot_values(plots, "map")
        .into_iter()
        .map(|(index, value)| {
            let map: MapConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("map config: {error}")))?;
            Ok(validate_map_widget(&map, index)?)
        })
        .collect()
}

pub fn validate_elevation_plots(
    plots: &serde_json::Value,
    scene: &ValidatedScenePresentation,
) -> CoreResult<Vec<ValidatedElevationPlot>> {
    let mut validated = Vec::new();
    for (index, value) in config::find_plot_values(plots, "elevation") {
        let plot: ElevationPlotConfig =
            serde_json::from_value(value.clone()).map_err(elevation_config_error)?;
        validated.push(validate_elevation_plot(&plot, index, scene)?);
    }
    Ok(validated)
}

fn elevation_config_error(error: serde_json::Error) -> CoreError {
    CoreError::Config(format!("elevation plot config: {error}"))
}

fn validate_rolling_charts(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRollingChart>> {
    config::find_plot_values(plots, "rolling_chart")
        .into_iter()
        .map(|(index, value)| {
            let chart: RollingChartConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("rolling chart config: {error}")))?;
            Ok(validate_rolling_chart(&chart, index)?)
        })
        .collect()
}

fn validate_diving_profiles(
    plots: &serde_json::Value,
    scene: &ValidatedScenePresentation,
) -> CoreResult<Vec<ValidatedDivingProfile>> {
    config::find_plot_values(plots, "diving_profile")
        .into_iter()
        .map(|(index, value)| {
            let plot: DivingProfileConfig = serde_json::from_value(value.clone())
                .map_err(|error| CoreError::Config(format!("diving profile config: {error}")))?;
            Ok(validate_diving_profile(&plot, index, scene)?)
        })
        .collect()
}
