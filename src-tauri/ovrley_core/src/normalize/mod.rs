//! Render config validation seam.
//!
//! Validates that every output-affecting field arrives explicit. The backend
//! owns zero render-affecting defaults — missing fields are rejected. The
//! frontend must materialise all defaults before sending the config.

mod arc_gauge;
mod backdrop;
mod bar_geometry;
mod elevation;
mod g_force;
mod gradient;
mod heading;
mod helpers;
mod label;
mod lap_timer;
mod lean_angle;
mod linear_gauge;
pub mod raw;
mod route;
mod scene;
mod time;
mod value;

use crate::error::{CoreError, CoreResult};
use crate::render::widgets::types::{
    PreparedArcGauge, PreparedGForce, PreparedHeadingTape, PreparedLapTimer, PreparedLeanAngle,
    PreparedLinearGauge, PreparedStandardText, PreparedValue,
};
use crate::types::{DisplayType, MetricKind};
use raw::RenderConfig;

pub use raw::{
    find_plot_value, parse_config_json, parse_config_value, parse_template_json,
    parse_template_value, BackdropConfig, CoursePlotConfig, ElevationPlotConfig,
    HeadingWidgetConfig, LabelConfig, SceneConfig, ValueConfig, TEMPLATE_FILE_FORMAT,
    TEMPLATE_FILE_VERSION,
};

pub use arc_gauge::{
    validate_arc_gauge, validate_corner_gauge, ValidatedArcGaugeWidget,
    ValidatedCornerGaugeOrientation, CORNER_GAUGE_ANGLE_DEGREES, MAX_ARC_ANGLE_DEGREES,
    MIN_ARC_ANGLE_DEGREES,
};
pub use backdrop::{validate_backdrop, ValidatedBackdrop};
pub use bar_geometry::ResolvedBarGeometry;
pub(crate) use bar_geometry::{
    arc_track_radius, corner_track_cap_padding, corner_track_radius, resolve_bar_style_geometry,
    scale_bar_geometry, track_corner_radius_max,
};
pub use elevation::{validate_elevation_plot, ValidatedElevationPlot};
pub use g_force::{validate_g_force, GForceAxis, ValidatedGForceWidget};
pub use gradient::{validate_gradient_widget, ValidatedGradientWidget};
pub use heading::{validate_heading, ValidatedHeading};
pub use label::{validate_label, ValidatedLabel};
pub use lap_timer::{validate_lap_timer, LapTimerMode, ValidatedLapTimer};
pub(crate) use lean_angle::LEAN_ANGLE_MAX_FILL_SWEEP;
pub use lean_angle::{
    lean_angle_layout, validate_lean_angle, LeanAngleLayout, ValidatedLeanAngleWidget,
};
pub use linear_gauge::{
    validate_linear_gauge, ValidatedLinearGaugeLabelPosition, ValidatedLinearGaugeOrientation,
    ValidatedLinearGaugeWidget,
};
pub use route::{validate_route_plot, ValidatedRoutePlot};
pub use scene::{validate_scene_config, ValidatedFfmpegConfig, ValidatedSceneConfig};
pub use time::{validate_time_value, ValidatedTimeFormatting, ValidatedTimeValue};
pub use value::{
    validate_value_widget, ContentAlignment, ValidatedValueFormatting, ValidatedValueWidget,
};

/// Telemetry series needed by a template.
///
/// These booleans allow trimming/densifying to skip unused high-cardinality
/// series. Plot requirements are derived in addition to explicit `values`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderDataRequirements {
    pub speed: bool,
    pub distance: bool,
    pub elevation: bool,
    pub barometric_altitude: bool,
    pub calories: bool,
    pub distance_to_home: bool,
    pub total_ascent: bool,
    pub gradient: bool,
    pub heartrate: bool,
    pub cadence: bool,
    pub power: bool,
    pub engine_power: bool,
    pub temperature: bool,
    pub pace: bool,
    pub g_force: bool,
    pub g_force_x: bool,
    pub g_force_y: bool,
    pub g_force_z: bool,
    pub rpm: bool,
    pub throttle_position: bool,
    pub brake_position: bool,
    pub lean_angle: bool,
    pub air_pressure: bool,
    pub ground_contact_time: bool,
    pub left_right_balance: bool,
    pub stride_length: bool,
    pub stroke_rate: bool,
    pub torque: bool,
    pub vertical_speed: bool,
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
    pub lap_number: bool,
    pub lap_time_seconds: bool,
    pub delta_to_best_lap_seconds: bool,
}

/// Validated render config where all output-affecting fields are explicit.
#[derive(Clone)]
pub struct ValidatedRenderConfig {
    pub scene: ValidatedSceneConfig,
    pub backdrops: Vec<ValidatedBackdrop>,
    pub labels: Vec<ValidatedLabel>,
    pub values: Vec<PreparedValue>,
    pub course_plot: Option<ValidatedRoutePlot>,
    pub elevation_plot: Option<ValidatedElevationPlot>,
}

/// Validates every value widget and label in the config. Returns the first
/// missing or invalid field as an error. Plots are pre-parsed and validated.
pub fn validate_render_config(raw: RenderConfig) -> CoreResult<ValidatedRenderConfig> {
    let scene = validate_scene_config(raw.scene)?;

    let backdrops = raw
        .backdrops
        .iter()
        .enumerate()
        .map(|(i, backdrop)| validate_backdrop(backdrop, i))
        .collect::<CoreResult<Vec<_>>>()?;

    let values = raw
        .values
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            if value.value == MetricKind::GForce && value.display_type == DisplayType::GForce {
                let value = value.with_promoted_display_variant("g_force")?;
                return validate_g_force(value, idx).map(|validated| {
                    PreparedValue::GForce(PreparedGForce {
                        validated,
                        cache: None,
                    })
                });
            }
            if value.value == MetricKind::Heading && value.display_type == DisplayType::Tape {
                return validate_heading(&value, idx, &scene).map(|validated| {
                    PreparedValue::HeadingTape(PreparedHeadingTape {
                        validated,
                        cache: None,
                    })
                });
            }
            if value.value == MetricKind::LeanAngle && value.display_type == DisplayType::LeanAngle
            {
                let value = value.with_promoted_display_variant("lean_angle")?;
                return validate_lean_angle(value, idx).map(|validated| {
                    PreparedValue::LeanAngle(PreparedLeanAngle {
                        validated,
                        cache: None,
                    })
                });
            }
            if value.value == MetricKind::Gradient {
                return validate_gradient_widget(value, idx).map(PreparedValue::Gradient);
            }
            if value.value == MetricKind::LapTimer {
                return validate_lap_timer(value, idx).map(|validated| {
                    PreparedValue::LapTimer(PreparedLapTimer {
                        validated,
                        cache: None,
                    })
                });
            }
            if value.value == MetricKind::Time && value.display_type == DisplayType::Text {
                return validate_time_value(value, idx).map(PreparedValue::TimeText);
            }
            if value.display_type == DisplayType::Linear {
                let value = value.with_promoted_display_variant("linear")?;
                return validate_linear_gauge(value, idx).map(|validated| {
                    PreparedValue::LinearGauge(PreparedLinearGauge {
                        validated,
                        altitude_offset_m: 0.0,
                        cache: None,
                    })
                });
            }
            if value.display_type == DisplayType::Arc {
                let value = value.with_promoted_display_variant("arc")?;
                return validate_arc_gauge(value, idx).map(|validated| {
                    PreparedValue::ArcGauge(PreparedArcGauge {
                        validated,
                        altitude_offset_m: 0.0,
                        cache: None,
                    })
                });
            }
            if value.display_type == DisplayType::Corner {
                let value = value.with_promoted_display_variant("corner")?;
                return validate_corner_gauge(value, idx).map(|validated| {
                    PreparedValue::ArcGauge(PreparedArcGauge {
                        validated,
                        altitude_offset_m: 0.0,
                        cache: None,
                    })
                });
            }
            validate_value_widget(value, idx).map(|validated| {
                PreparedValue::StandardText(PreparedStandardText {
                    validated,
                    altitude_offset_m: 0.0,
                })
            })
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
        backdrops,
        labels,
        values,
        course_plot,
        elevation_plot,
    })
}

impl ValidatedRenderConfig {
    /// Returns the frame decimation factor used for the encoded video stream.
    pub fn widget_update_rate(&self) -> std::num::NonZeroU32 {
        self.scene.update_rate
    }

    /// Returns the ffmpeg container FPS after applying update_rate.
    pub fn container_fps(&self) -> f64 {
        self.scene.fps / f64::from(self.widget_update_rate().get())
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
            if let PreparedValue::GForce(widget) = value {
                for axis in [
                    widget.validated.axis_horizontal,
                    widget.validated.axis_vertical,
                ] {
                    match axis {
                        GForceAxis::X => requirements.g_force_x = true,
                        GForceAxis::Y => requirements.g_force_y = true,
                        GForceAxis::Z => requirements.g_force_z = true,
                    }
                }
                continue;
            }
            if let PreparedValue::LapTimer(widget) = value {
                match widget.validated.mode {
                    LapTimerMode::CurrentLap | LapTimerMode::BestLap => {
                        requirements.lap_number = true;
                        requirements.lap_time_seconds = true;
                    }
                    LapTimerMode::Delta => requirements.delta_to_best_lap_seconds = true,
                    LapTimerMode::LapLog => {
                        requirements.lap_number = true;
                        requirements.lap_time_seconds = true;
                        requirements.delta_to_best_lap_seconds = true;
                    }
                }
                continue;
            }
            match value.metric_kind() {
                MetricKind::Speed => requirements.speed = true,
                MetricKind::Distance => requirements.distance = true,
                MetricKind::DistanceToHome => {
                    requirements.distance_to_home = true;
                    requirements.course = true;
                }
                MetricKind::GpsCoordinates => requirements.course = true,
                MetricKind::TotalAscent => {
                    requirements.elevation = true;
                    requirements.barometric_altitude = true;
                    requirements.total_ascent = true;
                }
                MetricKind::Elevation => requirements.elevation = true,
                MetricKind::Gradient => requirements.gradient = true,
                MetricKind::Heartrate => requirements.heartrate = true,
                MetricKind::Cadence => requirements.cadence = true,
                MetricKind::Power => requirements.power = true,
                MetricKind::EnginePower => requirements.engine_power = true,
                MetricKind::Temperature => requirements.temperature = true,
                MetricKind::Pace => requirements.pace = true,
                MetricKind::GForce => requirements.g_force = true,
                MetricKind::Rpm => requirements.rpm = true,
                MetricKind::ThrottlePosition => requirements.throttle_position = true,
                MetricKind::BrakePosition => requirements.brake_position = true,
                MetricKind::LeanAngle => requirements.lean_angle = true,
                MetricKind::AirPressure => requirements.air_pressure = true,
                MetricKind::GroundContactTime => requirements.ground_contact_time = true,
                MetricKind::LeftRightBalance => requirements.left_right_balance = true,
                MetricKind::StrideLength => requirements.stride_length = true,
                MetricKind::StrokeRate => requirements.stroke_rate = true,
                MetricKind::Torque => requirements.torque = true,
                MetricKind::VerticalSpeed => requirements.vertical_speed = true,
                MetricKind::Altitude => {
                    requirements.elevation = true;
                    requirements.barometric_altitude = true;
                }
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
                MetricKind::Calories => requirements.calories = true,
                MetricKind::LapTimer => unreachable!("lap_timer must use dedicated validation"),
            }
        }

        if self.course_plot.is_some() {
            requirements.distance_progress = true;
        }

        if self.elevation_plot.is_some() {
            requirements.elevation = true;
            requirements.barometric_altitude = true;
            requirements.distance_progress = true;
        }

        if self.has_heading_tape_value() {
            requirements.heading = true;
        }

        Ok(requirements)
    }
}
