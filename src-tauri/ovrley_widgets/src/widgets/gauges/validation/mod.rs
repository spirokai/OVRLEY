//! Canonical gauge ingress; both hosts consume these validated configurations.
mod arc;
mod linear;

pub use arc::{
    validate_arc_gauge, validate_corner_gauge, validate_gt_gauge, ValidatedArcGaugeWidget,
    ValidatedCornerGaugeOrientation, ValidatedGtGaugeStyle, CORNER_GAUGE_ANGLE_DEGREES,
    MAX_ARC_ANGLE_DEGREES, MIN_ARC_ANGLE_DEGREES, MIN_GT_GAUGE_TICK_COUNT,
};
pub use linear::{
    validate_linear_gauge, ValidatedLinearGaugeLabelPosition, ValidatedLinearGaugeOrientation,
    ValidatedLinearGaugeWidget,
};

use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        standard_metrics::is_display_type_supported,
        types::{DisplayType, MetricKind},
    },
    widgets::value::{config::ValueConfig, format::format_metric_parts},
};

#[derive(Clone, Debug)]
pub enum ValidatedGauge {
    Linear(ValidatedLinearGaugeWidget),
    Arc(ValidatedArcGaugeWidget),
}

impl ValidatedGauge {
    pub fn new(widget: ValueConfig) -> WidgetResult<Self> {
        match widget.display_type {
            DisplayType::Linear => Ok(Self::Linear(validate_linear_gauge(widget, 0)?)),
            DisplayType::Arc => Ok(Self::Arc(validate_arc_gauge(widget, 0)?)),
            DisplayType::Corner => Ok(Self::Arc(validate_corner_gauge(widget, 0)?)),
            DisplayType::GtGauge => Ok(Self::Arc(validate_gt_gauge(widget, 0)?)),
            _ => Err(WidgetError::Config("unsupported gauge display type".into())),
        }
    }

    pub fn metric(&self) -> MetricKind {
        match self {
            Self::Linear(widget) => widget.metric,
            Self::Arc(widget) => widget.metric,
        }
    }

    pub fn starting_altitude_m(&self) -> Option<f64> {
        match self {
            Self::Linear(widget) => widget.starting_altitude_m,
            Self::Arc(widget) => widget.inner_value.starting_altitude_m,
        }
    }

    pub fn unit_text(&self) -> Option<String> {
        match self {
            Self::Linear(_) => None,
            Self::Arc(widget) => format_metric_parts(&widget.inner_value, &Default::default())
                .standard_text()
                .1
                .map(str::to_owned),
        }
    }
}

fn validate_supported_metric(metric: MetricKind, display: DisplayType) -> WidgetResult<()> {
    if !is_display_type_supported(metric, display.as_str()) {
        return Err(WidgetError::Config(
            "unsupported gauge metric/display type".into(),
        ));
    }
    Ok(())
}

fn validate_label_font(font: Option<String>, field: &str) -> WidgetResult<String> {
    let font = crate::model::validation::require_string(font, field)?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{field}: must not be empty")));
    }
    Ok(font)
}
