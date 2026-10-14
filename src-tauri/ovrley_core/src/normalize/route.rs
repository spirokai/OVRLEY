//! Native plot-list ingress; the shared family owns the course contract.
use crate::{
    error::{CoreError, CoreResult},
    normalize::raw::find_plot_values,
};
use ovrley_widgets::widgets::route::{
    config::CoursePlotConfig,
    validation::{validate_route_plot, ValidatedRoutePlot},
};

pub fn validate_route_plots(plots: &serde_json::Value) -> CoreResult<Vec<ValidatedRoutePlot>> {
    find_plot_values(plots, "course")
        .into_iter()
        .map(|(index, value)| {
            let plot: CoursePlotConfig = serde_json::from_value(value.clone())
                .map_err(|e| CoreError::Config(format!("course plot config: {e}")))?;
            Ok(validate_route_plot(&plot, index)?)
        })
        .collect()
}
