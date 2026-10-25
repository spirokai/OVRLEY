//! Validated text value variants sharing one frame, raster cache, and resize policy.
use super::{
    config::ValueConfig,
    format::{format_metric_parts, MetricDisplayParts},
    validation::{validate_value_widget, ValidatedValueWidget},
};
use crate::widgets::time::{
    format::{format_validated_time_parts, ElapsedTimeValues},
    validation::{validate_time_value, ValidatedTimeValue},
};
use crate::{
    activity::ActivitySampler, error::WidgetResult, host::WidgetHost, model::types::MetricKind,
};

pub enum ValuePresentation {
    Metric(ValidatedValueWidget),
    Time {
        widget: ValidatedTimeValue,
        export_start_seconds: f64,
    },
}

impl ValuePresentation {
    pub fn new(widget: ValueConfig, host: &WidgetHost) -> WidgetResult<Self> {
        if widget.value == MetricKind::Time {
            Self::time(validate_time_value(widget, 0)?, host)
        } else {
            Ok(Self::Metric(validate_value_widget(widget, 0)?))
        }
    }

    /// Elapsed export time counts from the host's clock origin.
    pub fn time(widget: ValidatedTimeValue, host: &WidgetHost) -> WidgetResult<Self> {
        Ok(Self::Time {
            widget,
            export_start_seconds: host.export_start_seconds()?,
        })
    }

    pub fn base(&self) -> &ValidatedValueWidget {
        match self {
            Self::Metric(widget) => widget,
            Self::Time { widget, .. } => &widget.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut ValidatedValueWidget {
        match self {
            Self::Metric(widget) => widget,
            Self::Time { widget, .. } => &mut widget.base,
        }
    }

    pub fn parts(&self, activity: Option<&ActivitySampler>, second: f64) -> MetricDisplayParts {
        match self {
            Self::Metric(widget) => format_metric_parts(
                widget,
                &activity
                    .map(|a| a.sample(widget, second))
                    .unwrap_or_default(),
            ),
            Self::Time {
                widget,
                export_start_seconds,
            } => {
                let raw = activity.and_then(|a| a.time.at(second));
                format_validated_time_parts(
                    widget,
                    raw.as_deref(),
                    ElapsedTimeValues {
                        activity_seconds: second,
                        export_seconds: second - export_start_seconds,
                    },
                    activity.and_then(|a| a.timezone),
                )
            }
        }
    }
}
