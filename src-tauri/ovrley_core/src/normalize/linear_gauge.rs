//! Linear gauge metric widget validation.
//!
//! Validates a raw `ValueConfig` into a `ValidatedLinearGaugeWidget` when the
//! display type is `linear`. All gauge-specific fields (orientation,
//! track styling, labels) are required and validated here.

use super::helpers::{require_bool, require_f32, require_string};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::standard_metrics::is_standard_metric;
use crate::types::{DisplayType, MetricKind};

/// Validated linear gauge orientation, restricted to horizontal or vertical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedLinearGaugeOrientation {
    Horizontal,
    Vertical,
}

/// Validated min/max label position, constrained by gauge orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedLinearGaugeLabelPosition {
    Left,
    Right,
    Bottom,
    Top,
}

/// Fully validated linear gauge widget configuration.
#[derive(Clone, Debug)]
pub struct ValidatedLinearGaugeWidget {
    pub metric: MetricKind,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub rotation: f32,
    pub display_type: DisplayType,
    pub orientation: ValidatedLinearGaugeOrientation,
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_border_color: String,
    pub track_empty_color: String,
    pub track_empty_opacity: f32,
    pub track_filled_color: String,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    pub show_min_max_labels: bool,
    pub min_max_label_font: String,
    pub min_max_label_font_size: f32,
    pub min_max_label_position: ValidatedLinearGaugeLabelPosition,
    pub min_max_label_color: String,
}

/// Validates a raw value config as a linear gauge widget.
///
/// All gauge-specific fields are required (not optional) and are checked
/// for valid ranges (opacity 0-1, orientation horizontal/vertical, etc.).
pub fn validate_linear_gauge(
    value: ValueConfig,
    index: usize,
) -> CoreResult<ValidatedLinearGaugeWidget> {
    let p = |f: &str| format!("values[{index}].{f}");

    if !is_standard_metric(value.value) {
        return Err(CoreError::Config(format!(
            "{}: metric {:?} is outside the gauge validation slice",
            p("value"),
            value.value
        )));
    }

    if value.display_type != DisplayType::Linear {
        return Err(CoreError::Config(format!(
            "{}: expected linear display_type, got '{}'",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let width = value
        .width
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("width"))))?;
    let height = value
        .height
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("height"))))?;
    if width == 0 {
        return Err(CoreError::Config(format!("{}: must be > 0", p("width"))));
    }
    if height == 0 {
        return Err(CoreError::Config(format!("{}: must be > 0", p("height"))));
    }

    let orientation = match require_string(value.orientation, &p("orientation"))?.as_str() {
        "horizontal" => ValidatedLinearGaugeOrientation::Horizontal,
        "vertical" => ValidatedLinearGaugeOrientation::Vertical,
        other => {
            return Err(CoreError::Config(format!(
                "{}: expected horizontal or vertical, got {other}",
                p("orientation")
            )));
        }
    };

    let min_max_label_position = match (
        orientation,
        require_string(value.min_max_label_position, &p("min_max_label_position"))?.as_str(),
    ) {
        (ValidatedLinearGaugeOrientation::Horizontal, "bottom") => {
            ValidatedLinearGaugeLabelPosition::Bottom
        }
        (ValidatedLinearGaugeOrientation::Horizontal, "top") => {
            ValidatedLinearGaugeLabelPosition::Top
        }
        (ValidatedLinearGaugeOrientation::Vertical, "left") => {
            ValidatedLinearGaugeLabelPosition::Left
        }
        (ValidatedLinearGaugeOrientation::Vertical, "right") => {
            ValidatedLinearGaugeLabelPosition::Right
        }
        (ValidatedLinearGaugeOrientation::Horizontal, other) => {
            return Err(CoreError::Config(format!(
                "{}: expected bottom or top for horizontal gauge, got {other}",
                p("min_max_label_position")
            )));
        }
        (ValidatedLinearGaugeOrientation::Vertical, other) => {
            return Err(CoreError::Config(format!(
                "{}: expected left or right for vertical gauge, got {other}",
                p("min_max_label_position")
            )));
        }
    };

    let track_empty_opacity = require_f32(value.track_empty_opacity, &p("track_empty_opacity"))?;
    let track_filled_opacity = require_f32(value.track_filled_opacity, &p("track_filled_opacity"))?;
    for (field, opacity) in [
        ("track_empty_opacity", track_empty_opacity),
        ("track_filled_opacity", track_filled_opacity),
    ] {
        if !(0.0..=1.0).contains(&opacity) {
            return Err(CoreError::Config(format!(
                "{}: must be 0.0..1.0, got {opacity}",
                p(field)
            )));
        }
    }

    Ok(ValidatedLinearGaugeWidget {
        metric: value.value,
        x: value.x,
        y: value.y,
        width,
        height,
        rotation: value.rotation.unwrap_or(0.0),
        display_type: value.display_type,
        orientation,
        track_corner_radius: require_f32(value.track_corner_radius, &p("track_corner_radius"))?,
        track_border_thickness: require_f32(
            value.track_border_thickness,
            &p("track_border_thickness"),
        )?,
        track_border_color: require_string(value.track_border_color, &p("track_border_color"))?,
        track_empty_color: require_string(value.track_empty_color, &p("track_empty_color"))?,
        track_empty_opacity,
        track_filled_color: require_string(value.track_filled_color, &p("track_filled_color"))?,
        track_filled_opacity,
        track_fill_flat: require_bool(value.track_fill_flat, &p("track_fill_flat"))?,
        show_min_max_labels: require_bool(value.show_min_max_labels, &p("show_min_max_labels"))?,
        min_max_label_font: require_string(value.min_max_label_font, &p("min_max_label_font"))?,
        min_max_label_font_size: require_f32(
            value.min_max_label_font_size,
            &p("min_max_label_font_size"),
        )?,
        min_max_label_position,
        min_max_label_color: require_string(value.min_max_label_color, &p("min_max_label_color"))?,
    })
}
