//! Render config validation seam.
//!
//! Validates that every output-affecting field arrives explicit. The backend
//! owns zero render-affecting defaults — missing fields are rejected. The
//! frontend must materialise all defaults before sending the config.

mod elevation;
mod gradient;
mod heading;
mod helpers;
mod label;
mod linear_gauge;
pub mod raw;
mod route;
mod scene;
mod time;
mod value;

use crate::error::{CoreError, CoreResult};
use crate::render::widgets::types::PreparedValue;
use crate::types::{DisplayType, MetricKind};
use raw::RenderConfig;

pub use raw::{
    find_plot_value, parse_config_json, parse_config_value, parse_template_json,
    parse_template_value, CoursePlotConfig, ElevationPlotConfig, HeadingWidgetConfig, LabelConfig,
    SceneConfig, ValueConfig, TEMPLATE_FILE_FORMAT, TEMPLATE_FILE_VERSION,
};

pub use elevation::{validate_elevation_plot, ValidatedElevationPlot};
pub use gradient::{validate_gradient_widget, ValidatedGradientWidget};
pub use heading::{validate_heading, ValidatedHeading};
pub use label::{validate_label, ValidatedLabel};
pub use linear_gauge::{
    validate_linear_gauge, ValidatedLinearGaugeLabelPosition, ValidatedLinearGaugeOrientation,
    ValidatedLinearGaugeWidget,
};
pub use route::{validate_route_plot, ValidatedRoutePlot};
pub use scene::{validate_scene_config, ValidatedSceneConfig};
pub use time::{validate_time_value, ValidatedTimeFormatting, ValidatedTimeValue};
pub use value::{validate_value_widget, ValidatedValueFormatting, ValidatedValueWidget};

/// Telemetry series needed by a template.
///
/// These booleans allow trimming/densifying to skip unused high-cardinality
/// series. Plot requirements are derived in addition to explicit `values`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderDataRequirements {
    pub speed: bool,
    pub distance: bool,
    pub elevation: bool,
    pub gradient: bool,
    pub heartrate: bool,
    pub cadence: bool,
    pub power: bool,
    pub temperature: bool,
    pub pace: bool,
    pub g_force: bool,
    pub air_pressure: bool,
    pub ground_contact_time: bool,
    pub left_right_balance: bool,
    pub stride_length: bool,
    pub stroke_rate: bool,
    pub torque: bool,
    pub vertical_speed: bool,
    pub altitude: bool,
    pub iso: bool,
    pub aperture: bool,
    pub shutter_speed: bool,
    pub focal_length: bool,
    pub ev: bool,
    pub color_temperature: bool,
    pub gear_position: bool,
    pub vertical_ratio: bool,
    pub vertical_oscillation: bool,
    pub core_temperature: bool,
    pub heading: bool,
    pub time: bool,
    pub distance_progress: bool,
    pub course: bool,
}

/// Validated render config where all output-affecting fields are explicit.
#[derive(Clone)]
pub struct ValidatedRenderConfig {
    pub scene: ValidatedSceneConfig,
    pub labels: Vec<ValidatedLabel>,
    pub values: Vec<PreparedValue>,
    pub course_plot: Option<ValidatedRoutePlot>,
    pub elevation_plot: Option<ValidatedElevationPlot>,
}

/// Validates every value widget and label in the config. Returns the first
/// missing or invalid field as an error. Plots are pre-parsed and validated.
pub fn validate_render_config(raw: RenderConfig) -> CoreResult<ValidatedRenderConfig> {
    let scene = validate_scene_config(raw.scene)?;

    let values = raw
        .values
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            if value.value == MetricKind::Heading && value.display_type == DisplayType::Tape {
                return validate_heading(&value, idx, &scene).map(PreparedValue::HeadingTape);
            }
            if value.value == MetricKind::Gradient {
                return validate_gradient_widget(value, idx).map(PreparedValue::Gradient);
            }
            if value.value == MetricKind::Time && value.display_type == DisplayType::Text {
                return validate_time_value(value, idx, &scene).map(PreparedValue::TimeText);
            }
            if value.display_type == DisplayType::Linear {
                let value = value.with_promoted_display_variant("linear")?;
                return validate_linear_gauge(value, idx).map(PreparedValue::LinearGauge);
            }
            validate_value_widget(value, idx).map(PreparedValue::StandardText)
        })
        .collect::<CoreResult<Vec<_>>>()?;

    let labels = raw
        .labels
        .iter()
        .enumerate()
        .map(|(i, l)| validate_label(l, i))
        .collect::<CoreResult<Vec<_>>>()?;

    let course_plot = raw::find_plot_value(&raw.plots, "course")
        .map(|v| {
            serde_json::from_value::<raw::CoursePlotConfig>(v.clone())
                .map_err(|e| CoreError::Config(format!("course plot config: {e}")))
        })
        .transpose()?
        .map(|p| validate_route_plot(&p, 0))
        .transpose()?;

    let elevation_plot = raw::find_plot_value(&raw.plots, "elevation")
        .map(|v| {
            serde_json::from_value::<raw::ElevationPlotConfig>(v.clone())
                .map_err(|e| CoreError::Config(format!("elevation plot config: {e}")))
        })
        .transpose()?
        .map(|p| validate_elevation_plot(&p, 0, &scene))
        .transpose()?;

    Ok(ValidatedRenderConfig {
        scene,
        labels,
        values,
        course_plot,
        elevation_plot,
    })
}

impl ValidatedRenderConfig {
    /// Returns the frame decimation factor used for the encoded video stream.
    pub fn widget_update_rate(&self) -> u32 {
        self.scene.update_rate.max(1)
    }

    /// Returns the ffmpeg container FPS after applying update_rate.
    pub fn container_fps(&self) -> f64 {
        self.scene.fps / f64::from(self.widget_update_rate())
    }

    /// Returns whether any value entry is a heading metric using the tape display.
    pub fn has_heading_tape_value(&self) -> bool {
        self.values
            .iter()
            .any(|v| matches!(v, PreparedValue::HeadingTape(_)))
    }

    /// Computes which telemetry series are required by this configuration.
    ///
    /// Metric values enable their direct series. Plot widgets also request
    /// distance progress and any source series required to build their geometry.
    pub fn render_data_requirements(&self) -> CoreResult<RenderDataRequirements> {
        let mut requirements = RenderDataRequirements::default();

        for value in &self.values {
            match value.metric_kind() {
                MetricKind::Speed => requirements.speed = true,
                MetricKind::Distance => requirements.distance = true,
                MetricKind::Elevation => requirements.elevation = true,
                MetricKind::Gradient => requirements.gradient = true,
                MetricKind::Heartrate => requirements.heartrate = true,
                MetricKind::Cadence => requirements.cadence = true,
                MetricKind::Power => requirements.power = true,
                MetricKind::Temperature => requirements.temperature = true,
                MetricKind::Pace => requirements.pace = true,
                MetricKind::GForce => requirements.g_force = true,
                MetricKind::AirPressure => requirements.air_pressure = true,
                MetricKind::GroundContactTime => requirements.ground_contact_time = true,
                MetricKind::LeftRightBalance => requirements.left_right_balance = true,
                MetricKind::StrideLength => requirements.stride_length = true,
                MetricKind::StrokeRate => requirements.stroke_rate = true,
                MetricKind::Torque => requirements.torque = true,
                MetricKind::VerticalSpeed => requirements.vertical_speed = true,
                MetricKind::Altitude => requirements.altitude = true,
                MetricKind::Iso => requirements.iso = true,
                MetricKind::Aperture => requirements.aperture = true,
                MetricKind::ShutterSpeed => requirements.shutter_speed = true,
                MetricKind::FocalLength => requirements.focal_length = true,
                MetricKind::Ev => requirements.ev = true,
                MetricKind::ColorTemperature => requirements.color_temperature = true,
                MetricKind::GearPosition => requirements.gear_position = true,
                MetricKind::VerticalRatio => requirements.vertical_ratio = true,
                MetricKind::VerticalOscillation => requirements.vertical_oscillation = true,
                MetricKind::CoreTemperature => requirements.core_temperature = true,
                MetricKind::Heading => requirements.heading = true,
                MetricKind::Time => requirements.time = true,
            }
        }

        if self.course_plot.is_some() {
            requirements.distance_progress = true;
        }

        if self.elevation_plot.is_some() {
            requirements.elevation = true;
            requirements.distance_progress = true;
        }

        if self.has_heading_tape_value() {
            requirements.heading = true;
        }

        Ok(requirements)
    }
}
