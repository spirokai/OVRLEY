//! Effective heading-compass configuration is validated once at ingress.
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        types::{DisplayType, MetricKind},
        validation::{
            require_bool, require_f32, require_hex_color, require_percentage, require_positive_f32,
            require_string, require_unit_opacity,
        },
    },
    widgets::{
        gauges::validation::{MAX_ARC_ANGLE_DEGREES, MIN_ARC_ANGLE_DEGREES},
        value::config::ValueConfig,
    },
};

/// Which part of the dial follows the heading; the other part stays fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompassRotationMode {
    /// The needle turns to the heading over a fixed north-up card.
    Indicator,
    /// The card turns so the heading sits under a fixed up-pointing needle.
    Compass,
}

#[derive(Clone, Debug)]
pub struct ValidatedHeadingCompass {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    pub opacity: f32,
    pub rotation_mode: CompassRotationMode,
    pub arc_angle: f32,
    pub track_thickness: f32,
    pub track_corner_radius: f32,
    pub track_fill: String,
    pub indicator_size: f32,
    pub indicator_corner_radius: f32,
    pub indicator_color: String,
    pub show_cardinal_labels: bool,
    pub cardinal_font_size: f32,
    pub cardinal_label_color: String,
    pub font: String,
    pub show_value: bool,
    pub font_size: f32,
    pub color: String,
    pub unit_color: String,
    pub show_units: bool,
    pub value_offset_x: f32,
    pub value_offset_y: f32,
}

pub fn validate_heading_compass(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedHeadingCompass> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heading || value.display_type != DisplayType::HeadingCompass {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heading/heading_compass"
        )));
    }
    if value.width.is_some() || value.height.is_some() {
        return Err(WidgetError::Config(format!(
            "{} and {}: must be absent for diameter-based geometry",
            p("width"),
            p("height")
        )));
    }
    let rotation_mode = match require_string(value.rotation_mode, &p("rotation_mode"))?.as_str() {
        "indicator" => CompassRotationMode::Indicator,
        "compass" => CompassRotationMode::Compass,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("rotation_mode")
            )))
        }
    };
    let arc_angle = require_f32(value.arc_angle, &p("arc_angle"))?;
    if !(MIN_ARC_ANGLE_DEGREES..=MAX_ARC_ANGLE_DEGREES).contains(&arc_angle) {
        return Err(WidgetError::Config(format!(
            "{}: must be {MIN_ARC_ANGLE_DEGREES}..={MAX_ARC_ANGLE_DEGREES}, got {arc_angle}",
            p("arc_angle")
        )));
    }
    let track_corner_radius = require_f32(value.track_corner_radius, &p("track_corner_radius"))?;
    if track_corner_radius < 0.0 {
        return Err(WidgetError::Config(format!(
            "{}: must be >= 0",
            p("track_corner_radius")
        )));
    }
    let font = require_string(value.font, &p("font"))?;
    if font.trim().is_empty() {
        return Err(WidgetError::Config(format!("{}: required", p("font"))));
    }
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    Ok(ValidatedHeadingCompass {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        diameter: require_positive_f32(value.diameter, &p("diameter"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        rotation_mode,
        arc_angle,
        track_thickness: require_positive_f32(value.track_thickness, &p("track_thickness"))?,
        track_corner_radius,
        track_fill: color(value.track_fill, "track_fill")?,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        indicator_corner_radius: require_percentage(
            require_f32(value.indicator_corner_radius, &p("indicator_corner_radius"))?,
            &p("indicator_corner_radius"),
        )?,
        indicator_color: color(value.indicator_color, "indicator_color")?,
        show_cardinal_labels: require_bool(value.show_cardinal_labels, &p("show_cardinal_labels"))?,
        cardinal_font_size: require_positive_f32(
            value.cardinal_font_size,
            &p("cardinal_font_size"),
        )?,
        cardinal_label_color: color(value.cardinal_label_color, "cardinal_label_color")?,
        font,
        show_value: require_bool(value.show_value, &p("show_value"))?,
        font_size: require_positive_f32(value.font_size, &p("font_size"))?,
        color: color(value.color, "color")?,
        unit_color: color(value.unit_color, "unit_color")?,
        show_units: require_bool(value.show_units, &p("show_units"))?,
        value_offset_x: require_f32(value.value_offset_x, &p("value_offset_x"))?,
        value_offset_y: require_f32(value.value_offset_y, &p("value_offset_y"))?,
    })
}
