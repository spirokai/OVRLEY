//! Effective heading-tape configuration is validated once at ingress.
use crate::{
    error::{WidgetError, WidgetResult},
    model::{
        types::{DisplayType, MetricKind},
        validation::{
            require_bool, require_f32, require_hex_color, require_non_negative_f32,
            require_percentage, require_positive_f32, require_positive_u32, require_string,
            require_unit_opacity,
        },
    },
    widgets::value::config::ValueConfig,
};

#[derive(Clone, Debug)]
pub struct ValidatedHeading {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub pixels_per_degree: f32,
    pub major_tick_interval: u32,
    pub minor_ticks_per_major: u32,
    pub show_major_ticks: bool,
    pub show_minor_ticks: bool,
    pub major_tick_length_pct: f32,
    pub minor_tick_length_pct: f32,
    pub major_tick_thickness: f32,
    pub minor_tick_thickness: f32,
    pub tick_color: String,
    pub cardinal_tick_color: String,
    pub tick_alignment: String,
    pub show_minor_labels: bool,
    pub show_major_labels: bool,
    pub label_color: String,
    pub cardinal_label_color: String,
    pub label_font: String,
    pub label_font_size: f32,
    pub label_offset: f32,
    pub show_indicator: bool,
    pub indicator_style: String,
    pub indicator_placement: String,
    pub indicator_color: String,
    pub indicator_size: f32,
    pub opacity: f32,
}

pub fn validate_heading(value: ValueConfig, index: usize) -> WidgetResult<ValidatedHeading> {
    let p = |field: &str| format!("values[{index}].{field}");
    if value.value != MetricKind::Heading || value.display_type != DisplayType::Tape {
        return Err(WidgetError::Config(format!(
            "values[{index}]: expected heading/heading_tape"
        )));
    }
    let alignment = require_string(value.tick_alignment, &p("tick_alignment"))?;
    let indicator_style = require_string(value.indicator_style, &p("indicator_style"))?;
    let indicator_placement = require_string(value.indicator_placement, &p("indicator_placement"))?;
    for (field, value, choices) in [
        (
            "tick_alignment",
            alignment.as_str(),
            &["below", "centered"][..],
        ),
        (
            "indicator_style",
            indicator_style.as_str(),
            &["chevron", "highlight_bar"][..],
        ),
        (
            "indicator_placement",
            indicator_placement.as_str(),
            &["top", "bottom", "both"][..],
        ),
    ] {
        if !choices.contains(&value) {
            return Err(WidgetError::Config(format!(
                "{}: invalid value '{value}'",
                p(field)
            )));
        }
    }
    let non_negative =
        |raw, field| require_non_negative_f32(require_f32(raw, &p(field))?, &p(field));
    let percentage = |raw, field| require_percentage(require_f32(raw, &p(field))?, &p(field));
    let label_font = require_string(value.label_font, &p("label_font"))?;
    if label_font.trim().is_empty() {
        return Err(WidgetError::Config(format!(
            "{}: required",
            p("label_font")
        )));
    }
    Ok(ValidatedHeading {
        x: require_f32(Some(value.x), &p("x"))?,
        y: require_f32(Some(value.y), &p("y"))?,
        width: require_positive_f32(value.width, &p("width"))?,
        height: require_positive_f32(value.height, &p("height"))?,
        pixels_per_degree: require_positive_f32(value.pixels_per_degree, &p("pixels_per_degree"))?,
        major_tick_interval: require_positive_u32(
            value.major_tick_interval,
            &p("major_tick_interval"),
        )?,
        minor_ticks_per_major: require_positive_u32(
            value.minor_ticks_per_major,
            &p("minor_ticks_per_major"),
        )?,
        show_major_ticks: require_bool(value.show_major_ticks, &p("show_major_ticks"))?,
        show_minor_ticks: require_bool(value.show_minor_ticks, &p("show_minor_ticks"))?,
        major_tick_length_pct: percentage(value.major_tick_length_pct, "major_tick_length_pct")?,
        minor_tick_length_pct: percentage(value.minor_tick_length_pct, "minor_tick_length_pct")?,
        major_tick_thickness: non_negative(value.major_tick_thickness, "major_tick_thickness")?,
        minor_tick_thickness: non_negative(value.minor_tick_thickness, "minor_tick_thickness")?,
        tick_color: require_hex_color(value.tick_color.as_deref(), &p("tick_color"))?,
        cardinal_tick_color: require_hex_color(
            value.cardinal_tick_color.as_deref(),
            &p("cardinal_tick_color"),
        )?,
        tick_alignment: alignment,
        show_minor_labels: require_bool(value.show_minor_labels, &p("show_minor_labels"))?,
        show_major_labels: require_bool(value.show_major_labels, &p("show_major_labels"))?,
        label_color: require_hex_color(value.label_color.as_deref(), &p("label_color"))?,
        cardinal_label_color: require_hex_color(
            value.cardinal_label_color.as_deref(),
            &p("cardinal_label_color"),
        )?,
        label_font,
        label_font_size: require_positive_f32(value.label_font_size, &p("label_font_size"))?,
        label_offset: non_negative(value.label_offset, "label_offset")?,
        show_indicator: require_bool(value.show_indicator, &p("show_indicator"))?,
        indicator_style,
        indicator_placement,
        indicator_color: require_hex_color(
            value.indicator_color.as_deref(),
            &p("indicator_color"),
        )?,
        indicator_size: require_positive_f32(value.indicator_size, &p("indicator_size"))?,
        opacity: require_unit_opacity(value.opacity, &p("opacity"))?,
    })
}
