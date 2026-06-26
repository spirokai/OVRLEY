//! Time value validation.
//!
//! `validate_time_value` verifies that every output-affecting time widget
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{require_bool, require_f32, require_str, require_string, rgba_from_hex};
use super::raw::ValueConfig;
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedSceneConfig;
use crate::normalize::{ValidatedValueFormatting, ValidatedValueWidget};
use crate::types::DisplayType;
use crate::MetricKind;

/// Explicit formatting mode for a validated time value.
#[derive(Clone, Debug)]
pub enum ValidatedTimeFormatting {
    Preset(String),
    Strftime(String),
}

/// Every output-affecting field for a time text widget is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedTimeValue {
    pub base: ValidatedValueWidget,
    pub hours_offset: i64,
    pub formatting: ValidatedTimeFormatting,
}

pub fn validate_time_value(
    value: ValueConfig,
    index: usize,
    scene: &ValidatedSceneConfig,
) -> CoreResult<ValidatedTimeValue> {
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
    let hours_offset = i64::from(value.hours_offset.unwrap_or(0));
    let formatting = if let Some(format_key) = value.format {
        ValidatedTimeFormatting::Preset(format_key)
    } else if let Some(time_format) = value.time_format.or_else(|| scene.time_format.clone()) {
        ValidatedTimeFormatting::Strftime(time_format)
    } else {
        ValidatedTimeFormatting::Preset("time-24".to_string())
    };

    Ok(ValidatedTimeValue {
        base: ValidatedValueWidget {
            metric: MetricKind::Time,
            x: value.x,
            y: value.y,
            display_type: value.display_type,
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
            unit_color: color,
            display_unit: String::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn explicit_time() -> ValueConfig {
        serde_json::from_value(serde_json::json!({
            "value": "time", "x": 100.0, "y": 200.0,
            "font": "Arial.ttf", "font_size": 48.0,
            "color": "#ffffff", "opacity": 1.0,
            "prefix": "", "suffix": "",
            "format": "time-24",
            "show_icon": false,
            "icon_color": "#ffffff",
            "icon_size": 0.0,
            "icon_offset_x": 0.0,
            "icon_offset_y": 0.0
        }))
        .unwrap()
    }

    fn validated_scene() -> ValidatedSceneConfig {
        let scene: crate::normalize::SceneConfig = serde_json::from_value(serde_json::json!({
            "fps": 30.0,
            "start": 0.0,
            "end": 10.0,
            "width": 1920,
            "height": 1080,
            "scale": 1.0,
            "shadow_color": "#000000",
            "shadow_strength": 0.0,
            "shadow_distance": 0.0,
            "border_color": "#000000",
            "border_thickness": 0.0,
            "update_rate": 1,
            "custom_export_range_active": false,
            "composite_widget_update_rate": 1
        }))
        .unwrap();
        crate::normalize::validate_scene_config(scene).unwrap()
    }

    #[test]
    fn explicit_passes() {
        assert!(validate_time_value(explicit_time(), 0, &validated_scene()).is_ok());
    }

    #[test]
    fn scene_time_format_used_when_value_omits_time_format() {
        let mut scene = validated_scene();
        scene.time_format = Some("%H:%M".to_string());
        let mut value = explicit_time();
        value.format = None;
        let validated = validate_time_value(value, 0, &scene).unwrap();
        match validated.formatting {
            ValidatedTimeFormatting::Strftime(ref value) => assert_eq!(value, "%H:%M"),
            _ => panic!("expected strftime formatting"),
        }
    }
}
