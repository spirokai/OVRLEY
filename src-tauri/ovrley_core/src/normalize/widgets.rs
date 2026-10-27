//! Native widget ingress: collection validation and renderer dispatch.

use super::{config, raster, RenderConfig, ValidatedRenderConfig};
use crate::error::{CoreError, CoreResult};
use ovrley_widgets::model::scene::ValidatedScenePresentation;
use ovrley_widgets::model::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use ovrley_widgets::model::types::{DisplayType, MetricKind};
use ovrley_widgets::widgets::value::config::ValueConfig;
use ovrley_widgets::widgets::{
    backdrop::validation::validate_backdrop,
    diving_profile::{
        config::DivingProfileConfig,
        validation::{validate_diving_profile, ValidatedDivingProfile},
    },
    elevation::{
        config::ElevationPlotConfig,
        validation::{validate_elevation_plot, ValidatedElevationPlot},
    },
    g_force::validation::validate_g_force,
    gauges::validation::{validate_arc_gauge, validate_corner_gauge, validate_gt_gauge, validate_linear_gauge},
    gradient::validation::validate_gradient_widget,
    heading::validation::validate_heading,
    heading_compass::validation::validate_heading_compass,
    heart_zones::validation::validate_heart_zones,
    label::validation::validate_label,
    lap_timer::validation::validate_lap_timer,
    splits_run::validation::validate_splits_run,
    lean_angle::validation::validate_lean_angle,
    rolling_chart::{
        config::RollingChartConfig,
        validation::{validate_rolling_chart, ValidatedRollingChart},
    },
    route::{
        config::CoursePlotConfig,
        validation::{validate_route_plot, ValidatedRoutePlot},
    },
    time::validation::validate_time_value,
    value::validation::{validate_value_widget, ValidatedValue},
};

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
    if display_type_layout_mode(value.display_type) == DisplayTypeLayoutMode::Boxed {
        value = config::promote_value_variant(&value, value.display_type.as_str())?;
    }
    // These arms select native implementations. Capabilities and data dependencies come from the manifests.
    match (value.value, value.display_type) {
        (MetricKind::GForce, DisplayType::GForce) => validate_g_force(value, index)
            .map(ValidatedValue::GForce)
            .map_err(Into::into),
        (MetricKind::Heading, DisplayType::Tape) => {
            if value.label_font.is_none() {
                value.label_font = scene.font.clone();
            }
            validate_heading(value, index)
                .map(ValidatedValue::HeadingTape)
                .map_err(Into::into)
        }
        (MetricKind::Heading, DisplayType::HeadingCompass) => validate_heading_compass(value, index)
            .map(ValidatedValue::HeadingCompass)
            .map_err(Into::into),
        (MetricKind::Heartrate, DisplayType::HeartZones) => validate_heart_zones(value, index)
            .map(ValidatedValue::HeartZones)
            .map_err(Into::into),
        (MetricKind::LeanAngle, DisplayType::LeanAngle) => validate_lean_angle(value, index)
            .map(ValidatedValue::LeanAngle)
            .map_err(Into::into),
        (MetricKind::Gradient, _) => validate_gradient_widget(value, index)
            .map(ValidatedValue::Gradient)
            .map_err(Into::into),
        (MetricKind::LapTimer, _) => validate_lap_timer(value, index)
            .map(ValidatedValue::LapTimer)
            .map_err(Into::into),
        (MetricKind::SplitsRun, _) => validate_splits_run(value, index)
            .map(ValidatedValue::SplitsRun)
            .map_err(Into::into),
        (MetricKind::Time, DisplayType::Text) => validate_time_value(value, index)
            .map(ValidatedValue::TimeText)
            .map_err(Into::into),
        (_, DisplayType::Linear) => validate_linear_gauge(value, index)
            .map(ValidatedValue::LinearGauge)
            .map_err(Into::into),
        (_, DisplayType::Arc) => validate_arc_gauge(value, index)
            .map(ValidatedValue::ArcGauge)
            .map_err(Into::into),
        (_, DisplayType::Corner) => validate_corner_gauge(value, index)
            .map(ValidatedValue::ArcGauge)
            .map_err(Into::into),
        (_, DisplayType::GtGauge) => validate_gt_gauge(value, index)
            .map(ValidatedValue::ArcGauge)
            .map_err(Into::into),
        _ => validate_value_widget(value, index)
            .map(ValidatedValue::StandardText)
            .map_err(Into::into),
    }
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
