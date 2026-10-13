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
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    model::{scene::ScenePresentationConfig, types::MetricKind},
};

pub enum ValuePresentation {
    Metric(ValidatedValueWidget),
    Time {
        widget: ValidatedTimeValue,
        export_start_seconds: f64,
    },
}

impl ValuePresentation {
    pub fn new(widget: ValueConfig, scene: &ScenePresentationConfig) -> WidgetResult<Self> {
        if widget.value == MetricKind::Time {
            let export_start_seconds = scene
                .extra
                .get("export_start_seconds")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
                .ok_or_else(|| {
                    WidgetError::Config("scene.export_start_seconds must be a finite number".into())
                })?;
            Ok(Self::Time {
                widget: validate_time_value(widget, 0)?,
                export_start_seconds,
            })
        } else {
            Ok(Self::Metric(validate_value_widget(widget, 0)?))
        }
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
