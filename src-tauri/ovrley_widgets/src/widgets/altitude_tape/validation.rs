//! Effective altitude-tape configuration is validated once at ingress; its source metric is
//! resolved here.
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        standard_metrics::{altitude_tape_major_tick_intervals, standard_metric_definition},
        types::{DisplayType, MetricKind},
        validation::{
            require_bool, require_f32, require_hex_color, require_non_negative_f32,
            require_percentage, require_positive_f32, require_positive_u32, require_string,
            require_unit_opacity,
        },
    },
    widgets::value::{config::ValueConfig, validation::normalize_starting_altitude_m},
};

/// The metric the tape scrolls. Altitude grows upwards; depth grows downwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeSource {
    Altitude,
    Depth,
}

impl TapeSource {
    pub fn metric(self) -> MetricKind {
        match self {
            Self::Altitude => MetricKind::Altitude,
            Self::Depth => MetricKind::Depth,
        }
    }

    /// +1 when larger values sit higher on the tape, -1 when they sit lower.
    pub fn direction(self) -> f64 {
        match self {
            Self::Altitude => 1.0,
            Self::Depth => -1.0,
        }
    }
}

/// Direction the readout points; ticks and labels sit on the opposite edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapeOrientation {
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct ValidatedAltitudeTape {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub opacity: f32,
    pub source: TapeSource,
    pub orientation: TapeOrientation,
    /// Tape and readout unit, one of the source metric's display units.
    pub display_unit: String,
    pub starting_altitude_m: Option<f64>,
    pub major_tick_interval: u32,
    pub minor_ticks_per_major: u32,
    pub major_tick_spacing: f32,
    pub major_tick_length_pct: f32,
    pub minor_tick_length_pct: f32,
    pub major_tick_thickness: f32,
    pub minor_tick_thickness: f32,
    pub major_tick_color: String,
    pub minor_tick_color: String,
    pub show_tick_labels: bool,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_color: String,
    pub label_offset: f32,
    pub font: String,
    pub font_size: f32,
    pub color: String,
    pub show_value_decimal: bool,
    pub value_background_color: String,
    pub value_border_color: String,
    pub value_border_thickness: f32,
    pub value_shadow_color: String,
    pub value_shadow_sigma: f32,
    pub value_shadow_offset: f32,
    pub show_vertical_speed: bool,
    pub vertical_speed_unit: String,
    pub show_vertical_speed_decimal: bool,
    pub vertical_speed_font_size: f32,
    pub vertical_speed_bar_thickness: f32,
    pub vertical_speed_positive_color: String,
    pub vertical_speed_negative_color: String,
    pub vertical_speed_trend_seconds: f32,
}

pub fn validate_altitude_tape(
    value: ValueConfig,
    index: usize,
) -> WidgetResult<ValidatedAltitudeTape> {
    let p = |field: &str| format!("values[{index}].{field}");
    let source = match (value.value, value.display_type) {
        (MetricKind::Altitude, DisplayType::AltitudeTape) => TapeSource::Altitude,
        (MetricKind::Depth, DisplayType::AltitudeTape) => TapeSource::Depth,
        _ => {
            return Err(WidgetError::Config(format!(
                "values[{index}]: expected altitude or depth with altitude_tape"
            )))
        }
    };
    let starting_altitude_m = normalize_starting_altitude_m(
        value.value,
        value.starting_altitude,
        value.display_unit.as_deref(),
        &p("starting_altitude"),
        &p("display_unit"),
    )?;
    let display_unit = require_string(value.display_unit, &p("display_unit"))?;
    let supports = |metric: MetricKind, unit: &str| {
        standard_metric_definition(metric)
            .expect("tape metrics are standard metrics")
            .supported_display_units
            .iter()
            .any(|option| option.value == unit)
    };
    if !supports(source.metric(), &display_unit) {
        return Err(WidgetError::Config(format!(
            "{}: invalid value '{display_unit}'",
            p("display_unit")
        )));
    }
    let vertical_speed_unit = require_string(value.vertical_speed_unit, &p("vertical_speed_unit"))?;
    // The readout offers exactly the vertical_speed metric's display units.
    if !supports(MetricKind::VerticalSpeed, &vertical_speed_unit) {
        return Err(WidgetError::Config(format!(
            "{}: invalid value '{vertical_speed_unit}'",
            p("vertical_speed_unit")
        )));
    }
    let major_tick_interval =
        require_positive_u32(value.major_tick_interval, &p("major_tick_interval"))?;
    if !altitude_tape_major_tick_intervals().contains(&major_tick_interval) {
        return Err(WidgetError::Config(format!(
            "{}: {major_tick_interval} is not an offered interval",
            p("major_tick_interval")
        )));
    }
    let orientation = match require_string(value.tape_orientation, &p("tape_orientation"))?.as_str() {
        "left" => TapeOrientation::Left,
        "right" => TapeOrientation::Right,
        other => {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{other}'",
                p("tape_orientation")
            )))
        }
    };
    let font = |raw: Option<String>, field: &str| {
        let font = require_string(raw, &p(field))?;
        if font.trim().is_empty() {
            return Err(WidgetError::Config(format!("{}: required", p(field))));
        }
        Ok(font)
    };
    let non_negative =
        |raw, field| require_non_negative_f32(require_f32(raw, &p(field))?, &p(field));
    let percentage = |raw, field| require_percentage(require_f32(raw, &p(field))?, &p(field));
    let color = |raw: Option<String>, field: &str| require_hex_color(raw.as_deref(), &p(field));
    let positive = |raw, field: &str| require_positive_f32(raw, &p(field));
    Ok(ValidatedAltitudeTape {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width: positive(value.width, "width")?,
        height: positive(value.height, "height")?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
        source,
        orientation,
        display_unit,
        starting_altitude_m,
        major_tick_interval,
        minor_ticks_per_major: require_positive_u32(
            value.minor_ticks_per_major,
            &p("minor_ticks_per_major"),
        )?,
        major_tick_spacing: positive(value.major_tick_spacing, "major_tick_spacing")?,
        major_tick_length_pct: percentage(value.major_tick_length_pct, "major_tick_length_pct")?,
        minor_tick_length_pct: percentage(value.minor_tick_length_pct, "minor_tick_length_pct")?,
        major_tick_thickness: positive(value.major_tick_thickness, "major_tick_thickness")?,
        minor_tick_thickness: positive(value.minor_tick_thickness, "minor_tick_thickness")?,
        major_tick_color: color(value.major_tick_color, "major_tick_color")?,
        minor_tick_color: color(value.minor_tick_color, "minor_tick_color")?,
        show_tick_labels: require_bool(value.show_tick_labels, &p("show_tick_labels"))?,
        label_font: font(value.label_font, "label_font")?,
        label_font_size: positive(value.label_font_size, "label_font_size")?,
        label_color: color(value.label_color, "label_color")?,
        label_offset: non_negative(value.label_offset, "label_offset")?,
        font: font(value.font, "font")?,
        font_size: positive(value.font_size, "font_size")?,
        color: color(value.color, "color")?,
        show_value_decimal: require_bool(value.show_value_decimal, &p("show_value_decimal"))?,
        value_background_color: color(value.value_background_color, "value_background_color")?,
        value_border_color: color(value.value_border_color, "value_border_color")?,
        value_border_thickness: non_negative(
            value.value_border_thickness,
            "value_border_thickness",
        )?,
        value_shadow_color: color(value.value_shadow_color, "value_shadow_color")?,
        value_shadow_sigma: non_negative(value.value_shadow_sigma, "value_shadow_sigma")?,
        value_shadow_offset: non_negative(value.value_shadow_offset, "value_shadow_offset")?,
        show_vertical_speed: require_bool(value.show_vertical_speed, &p("show_vertical_speed"))?,
        vertical_speed_unit,
        show_vertical_speed_decimal: require_bool(
            value.show_vertical_speed_decimal,
            &p("show_vertical_speed_decimal"),
        )?,
        vertical_speed_font_size: positive(
            value.vertical_speed_font_size,
            "vertical_speed_font_size",
        )?,
        vertical_speed_bar_thickness: positive(
            value.vertical_speed_bar_thickness,
            "vertical_speed_bar_thickness",
        )?,
        vertical_speed_positive_color: color(
            value.vertical_speed_positive_color,
            "vertical_speed_positive_color",
        )?,
        vertical_speed_negative_color: color(
            value.vertical_speed_negative_color,
            "vertical_speed_negative_color",
        )?,
        vertical_speed_trend_seconds: positive(
            value.vertical_speed_trend_seconds,
            "vertical_speed_trend_seconds",
        )?,
    })
}
