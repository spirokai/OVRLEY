//! Time value validation.
//!
//! `validate_time_value` verifies that every output-affecting time widget
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{require_bool, require_f32, require_str, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use super::value::validate_content_alignment;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{ValidatedValueFormatting, ValidatedValueWidget};
use crate::types::DisplayType;
use crate::MetricKind;

/// Explicit formatting mode for a validated time value.
#[derive(Clone, Debug)]
pub enum ValidatedTimeFormatting {
    Preset(String),
}

/// Every output-affecting field for a time text widget is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedTimeValue {
    pub base: ValidatedValueWidget,
    pub hours_offset: i64,
    pub formatting: ValidatedTimeFormatting,
}

pub fn validate_time_value(value: ValueConfig, index: usize) -> CoreResult<ValidatedTimeValue> {
    let p = |field: &str| format!("values[{index}].{field}");

    if value.value != MetricKind::Time {
        return Err(CoreError::Config(format!(
            "{}: expected Time, got {:?}",
            p("value"),
            value.value
        )));
    }

    if value.display_type != DisplayType::Text {
        return Err(CoreError::Config(format!(
            "{}: display_type '{}' is outside the time text validation slice",
            p("display_type"),
            value.display_type.as_str()
        )));
    }

    let font_name = require_string(value.font, &p("font"))?;
    let content_alignment =
        validate_content_alignment(value.content_alignment, &p("content_alignment"))?;
    let opacity = require_f32(value.opacity, &p("opacity"))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..1.0, got {opacity}",
            p("opacity")
        )));
    }

    let font_size = require_f32(value.font_size, &p("font_size"))?;
    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be > 0, got {font_size}",
            p("font_size")
        )));
    }

    let color = rgba_from_hex(
        require_str(value.color.as_deref(), &p("color"))?,
        &p("color"),
        opacity,
    )?;

    let show_icon = require_bool(value.show_icon, &p("show_icon"))?;
    let icon_color = rgba_from_hex(
        require_str(value.icon_color.as_deref(), &p("icon_color"))?,
        &p("icon_color"),
        opacity,
    )?;
    let icon_size = require_f32(value.icon_size, &p("icon_size"))?;
    if icon_size < 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be >= 0, got {icon_size}",
            p("icon_size")
        )));
    }
    let icon_offset_x = require_f32(value.icon_offset_x, &p("icon_offset_x"))?;
    let icon_offset_y = require_f32(value.icon_offset_y, &p("icon_offset_y"))?;

    let prefix = require_string(value.prefix, &p("prefix"))?;
    let suffix = require_string(value.suffix, &p("suffix"))?;
    if value.time_format.is_some() {
        return Err(CoreError::Config(format!(
            "{}: legacy field is not supported; use format",
            p("time_format")
        )));
    }
    let hours_offset = i64::from(
        value
            .hours_offset
            .ok_or_else(|| CoreError::Config(format!("{}: required", p("hours_offset"))))?,
    );
    let format_key = require_string(value.format, &p("format"))?;
    validate_time_format_key(&format_key, &p("format"))?;
    let formatting = ValidatedTimeFormatting::Preset(format_key);

    Ok(ValidatedTimeValue {
        base: ValidatedValueWidget {
            metric: MetricKind::Time,
            x: value.x,
            y: value.y,
            display_type: value.display_type,
            content_alignment,
            font_name,
            font_size,
            color,
            opacity,
            show_icon,
            icon_color,
            icon_size,
            icon_offset_x,
            icon_offset_y,
            show_units: false,
            show_full_distance: None,
            show_full_ascent: None,
            coordinate_format: None,
            unit_color: color,
            display_unit: String::new(),
            starting_altitude_m: None,
            prefix,
            suffix,
            formatting: ValidatedValueFormatting::DecimalPlaces { decimals: 0 },
            hours_offset: Some(hours_offset),
            format: None,
        },
        hours_offset,
        formatting,
    })
}

fn validate_time_format_key(format_key: &str, field: &str) -> CoreResult<()> {
    match format_key {
        "time-24" | "time-24s" | "time-12" | "time-12s" | "date-dd-mm-yyyy" | "date-mm-dd-yyyy"
        | "date-yyyy-mm-dd" | "date-dd-mmm-yyyy" | "date-mmm-dd-yyyy" | "date-dd-mmmm-yyyy"
        | "date-mmmm-dd-yyyy" | "date-time-24" | "date-time-24s" | "date-time-12"
        | "date-time-12s" | "date-mmm-time-24" | "date-mmm-time-12" | "date-mmmm-time-24"
        | "date-mmmm-time-12" => Ok(()),
        _ => Err(CoreError::Config(format!(
            "{field}: unsupported time format '{format_key}'"
        ))),
    }
}
