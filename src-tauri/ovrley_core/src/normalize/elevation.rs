//! Native plot-list ingress; the shared family owns elevation validation.
use crate::{
    error::{CoreError, CoreResult},
    normalize::raw::find_plot_values,
};
use ovrley_widgets::{
    model::scene::ValidatedScenePresentation,
    widgets::elevation::{
        config::ElevationPlotConfig,
        validation::{validate_elevation_plot, ValidatedElevationPlot},
    },
};

pub fn validate_elevation_plots(
    plots: &serde_json::Value,
    scene: &ValidatedScenePresentation,
) -> CoreResult<Vec<ValidatedElevationPlot>> {
    find_plot_values(plots, "elevation")
        .into_iter()
        .map(|(index, value)| {
            let plot: ElevationPlotConfig = serde_json::from_value(value.clone())
                .map_err(|e| CoreError::Config(format!("elevation plot config: {e}")))?;
            Ok(validate_elevation_plot(&plot, index, scene)?)
        })
        .collect()
}
