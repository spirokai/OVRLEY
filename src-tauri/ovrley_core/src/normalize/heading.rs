//! Heading compass tape widget validation.
//!
//! `validate_heading` verifies that every output-affecting heading widget
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{
    require_f32, require_hex_color, require_non_negative_f32, require_percentage,
    require_positive_f32, require_positive_u32,
};
use super::raw::{HeadingWidgetConfig, ValueConfig};
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedScenePresentation;
use crate::render::widgets::common::normalize_shadow_style_validated;
use crate::render::widgets::types::ShadowStyle;
use crate::types::MetricKind;

/// All output-affecting heading widget fields — no `Option`, no defaults at render time.
///
/// The frontend must materialize every value before sending the config.
/// Missing or invalid fields are rejected by `validate_heading`.
#[derive(Clone, Debug)]
pub struct ValidatedHeading {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
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
    pub label_font: Option<String>,
    pub label_font_size: f32,
    pub label_offset: f32,
    pub show_indicator: bool,
    pub indicator_style: String,
    pub indicator_placement: String,
    pub indicator_color: String,
    pub indicator_size: f32,
    pub indicator_shadow: Option<ShadowStyle>,
    pub rotation: f32,
    pub opacity: f32,
}

/// Validates a heading tape value config, resolving all optional fields to
/// explicit values. Returns an error for missing or out-of-range fields.
pub fn validate_heading(
    value: &ValueConfig,
    index: usize,
    scene: &ValidatedScenePresentation,
) -> CoreResult<ValidatedHeading> {
    let p = |f: &str| format!("values[{index}].{f}");

    if value.value != MetricKind::Heading {
        return Err(CoreError::Config(format!(
            "{}: expected Heading, got {:?}",
            p("value"),
            value.value
        )));
    }

    // Deserialize extra heading-specific fields via the existing serde path
    let hw: HeadingWidgetConfig = value.to_heading_widget_config()?;

    let x = hw.x;
    let y = hw.y;
    let width = require_positive_u32(Some(hw.width), &p("width"))?;
    let height = require_positive_u32(Some(hw.height), &p("height"))?;
    let pixels_per_degree = require_f32(hw.pixels_per_degree, &p("pixels_per_degree"))?;
    require_positive_f32(Some(pixels_per_degree), &p("pixels_per_degree"))?;

    let major_tick_interval = hw
        .major_tick_interval
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("major_tick_interval"))))?;
    if major_tick_interval == 0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0",
            p("major_tick_interval")
        )));
    }
    let minor_ticks_per_major = hw
        .minor_ticks_per_major
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("minor_ticks_per_major"))))?;
    if minor_ticks_per_major == 0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0",
            p("minor_ticks_per_major")
        )));
    }

    let major_tick_length_pct = require_f32(hw.major_tick_length_pct, &p("major_tick_length_pct"))?;
    require_percentage(major_tick_length_pct, &p("major_tick_length_pct"))?;
    let minor_tick_length_pct = require_f32(hw.minor_tick_length_pct, &p("minor_tick_length_pct"))?;
    require_percentage(minor_tick_length_pct, &p("minor_tick_length_pct"))?;

    let major_tick_thickness = require_f32(hw.major_tick_thickness, &p("major_tick_thickness"))?;
    require_non_negative_f32(major_tick_thickness, &p("major_tick_thickness"))?;
    let minor_tick_thickness = require_f32(hw.minor_tick_thickness, &p("minor_tick_thickness"))?;
    require_non_negative_f32(minor_tick_thickness, &p("minor_tick_thickness"))?;

    let tick_color = require_hex_color(hw.tick_color.as_deref(), &p("tick_color"))?;
    let cardinal_tick_color =
        require_hex_color(hw.cardinal_tick_color.as_deref(), &p("cardinal_tick_color"))?;

    let tick_alignment = hw
        .tick_alignment
        .clone()
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("tick_alignment"))))?;
    require_tick_alignment(&tick_alignment, &p("tick_alignment"))?;

    let show_minor_labels = hw
        .show_minor_labels
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_minor_labels"))))?;
    let show_major_labels = hw
        .show_major_labels
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_major_labels"))))?;

    let label_color = require_hex_color(hw.label_color.as_deref(), &p("label_color"))?;
    let cardinal_label_color = require_hex_color(
        hw.cardinal_label_color.as_deref(),
        &p("cardinal_label_color"),
    )?;

    let label_font = resolve_label_font(&hw, scene);
    let label_font_size = require_f32(hw.label_font_size, &p("label_font_size"))?;
    require_positive_f32(Some(label_font_size), &p("label_font_size"))?;
    let label_offset = require_f32(hw.label_offset, &p("label_offset"))?;
    require_non_negative_f32(label_offset, &p("label_offset"))?;

    let show_indicator = hw
        .show_indicator
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_indicator"))))?;
    let indicator_style = hw
        .indicator_style
        .clone()
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("indicator_style"))))?;
    require_indicator_style(&indicator_style, &p("indicator_style"))?;
    let indicator_placement = hw
        .indicator_placement
        .clone()
        .ok_or_else(|| CoreError::Config(format!("{}: required", p("indicator_placement"))))?;
    require_indicator_placement(&indicator_placement, &p("indicator_placement"))?;
    let indicator_color = require_hex_color(hw.indicator_color.as_deref(), &p("indicator_color"))?;
    let indicator_size = require_f32(hw.indicator_size, &p("indicator_size"))?;
    require_positive_f32(Some(indicator_size), &p("indicator_size"))?;

    let indicator_shadow = normalize_shadow_style_validated(
        &scene.shadow_color,
        scene.shadow_strength,
        scene.shadow_distance,
        1.0,
    );

    let rotation = hw.rotation;
    let opacity = require_f32(hw.opacity, &p("opacity"))?;

    Ok(ValidatedHeading {
        x,
        y,
        width,
        height,
        pixels_per_degree,
        major_tick_interval,
        minor_ticks_per_major,
        show_major_ticks: hw
            .show_major_ticks
            .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_major_ticks"))))?,
        show_minor_ticks: hw
            .show_minor_ticks
            .ok_or_else(|| CoreError::Config(format!("{}: required", p("show_minor_ticks"))))?,
        major_tick_length_pct,
        minor_tick_length_pct,
        major_tick_thickness,
        minor_tick_thickness,
        tick_color,
        cardinal_tick_color,
        tick_alignment,
        show_minor_labels,
        show_major_labels,
        label_color,
        cardinal_label_color,
        label_font,
        label_font_size,
        label_offset,
        show_indicator,
        indicator_style,
        indicator_placement,
        indicator_color,
        indicator_size,
        indicator_shadow,
        rotation,
        opacity,
    })
}

fn resolve_label_font(
    hw: &HeadingWidgetConfig,
    scene: &ValidatedScenePresentation,
) -> Option<String> {
    hw.label_font.clone().or_else(|| scene.font.clone())
}

fn require_tick_alignment(v: &str, field: &str) -> CoreResult<String> {
    match v {
        "below" | "centered" => Ok(v.to_string()),
        _ => Err(CoreError::Config(format!(
            "{field}: invalid value '{v}' — expected 'below' or 'centered'"
        ))),
    }
}

fn require_indicator_style(v: &str, field: &str) -> CoreResult<String> {
    match v {
        "chevron" | "highlight_bar" => Ok(v.to_string()),
        _ => Err(CoreError::Config(format!(
            "{field}: invalid value '{v}' — expected 'chevron' or 'highlight_bar'"
        ))),
    }
}

fn require_indicator_placement(v: &str, field: &str) -> CoreResult<String> {
    match v {
        "top" | "bottom" | "both" => Ok(v.to_string()),
        _ => Err(CoreError::Config(format!(
            "{field}: invalid value '{v}' — expected 'top', 'bottom', or 'both'"
        ))),
    }
}
