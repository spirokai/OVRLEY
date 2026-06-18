//! Heading compass tape widget validation.
//!
//! `validate_heading` verifies that every output-affecting heading widget
//! field is explicit. Missing fields are rejected — the backend owns zero
//! render-affecting defaults. The frontend must materialise all defaults
//! before sending the config.

use super::helpers::{
    require_f32, require_hex_color, require_non_negative_f32, require_percentage,
    require_positive_f32,
};
use super::raw::{HeadingWidgetConfig, ValueConfig};
use crate::error::{CoreError, CoreResult};
use crate::normalize::ValidatedSceneConfig;
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
    scene: &ValidatedSceneConfig,
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
    let width = require_positive_u32(hw.width, &p("width"))?;
    let height = require_positive_u32(hw.height, &p("height"))?;
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

fn resolve_label_font(hw: &HeadingWidgetConfig, scene: &ValidatedSceneConfig) -> Option<String> {
    hw.label_font.clone().or_else(|| scene.font.clone())
}

fn require_positive_u32(v: u32, field: &str) -> CoreResult<u32> {
    if v == 0 {
        Err(CoreError::Config(format!("{field}: must be > 0")))
    } else {
        Ok(v)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn explicit_heading_value() -> ValueConfig {
        ValueConfig {
            value: MetricKind::Heading,
            x: 100.0,
            y: 200.0,
            font: None,
            font_family: None,
            font_size: None,
            color: None,
            opacity: None,
            suffix: None,
            prefix: None,
            unit: None,
            hours_offset: None,
            time_format: None,
            format: None,
            decimal_rounding: None,
            decimals: None,
            show_icon: None,
            icon_color: None,
            icon_size: None,
            icon_offset_x: None,
            icon_offset_y: None,
            show_units: None,
            unit_color: None,
            display_unit: None,
            balance_format: None,
            value_offset: None,
            triangle_positive_color: None,
            triangle_negative_color: None,
            show_sign: None,
            show_triangle: None,
            triangle_width: None,
            shadow_color: None,
            shadow_strength: None,
            shadow_distance: None,
            border_color: None,
            border_thickness: None,
            border_strength: None,
            border_distance: None,
            display_type: crate::types::DisplayType::Tape,
            width: None,
            height: None,
            rotation: None,
            orientation: None,
            track_corner_radius: None,
            track_border_thickness: None,
            track_border_color: None,
            track_empty_color: None,
            track_empty_opacity: None,
            track_filled_color: None,
            track_filled_opacity: None,
            track_fill_flat: None,
            show_min_max_labels: None,
            min_max_label_font: None,
            min_max_label_font_size: None,
            min_max_label_position: None,
            min_max_label_color: None,
            extra: {
                let mut m = BTreeMap::new();
                m.insert("width".into(), serde_json::json!(400));
                m.insert("height".into(), serde_json::json!(80));
                m.insert("pixels_per_degree".into(), serde_json::json!(5.0));
                m.insert("major_tick_interval".into(), serde_json::json!(15));
                m.insert("minor_ticks_per_major".into(), serde_json::json!(3));
                m.insert("show_major_ticks".into(), serde_json::json!(true));
                m.insert("show_minor_ticks".into(), serde_json::json!(true));
                m.insert("major_tick_length_pct".into(), serde_json::json!(40.0));
                m.insert("minor_tick_length_pct".into(), serde_json::json!(20.0));
                m.insert("major_tick_thickness".into(), serde_json::json!(2.0));
                m.insert("minor_tick_thickness".into(), serde_json::json!(2.0));
                m.insert("tick_color".into(), serde_json::json!("#ffffff"));
                m.insert("cardinal_tick_color".into(), serde_json::json!("#ff0000"));
                m.insert("tick_alignment".into(), serde_json::json!("below"));
                m.insert("show_minor_labels".into(), serde_json::json!(true));
                m.insert("show_major_labels".into(), serde_json::json!(true));
                m.insert("label_color".into(), serde_json::json!("#cccccc"));
                m.insert("cardinal_label_color".into(), serde_json::json!("#ff0000"));
                m.insert("label_font".into(), serde_json::json!("Arial.ttf"));
                m.insert("label_font_size".into(), serde_json::json!(12.0));
                m.insert("label_offset".into(), serde_json::json!(4.0));
                m.insert("indicator_style".into(), serde_json::json!("chevron"));
                m.insert("indicator_placement".into(), serde_json::json!("top"));
                m.insert("show_indicator".into(), serde_json::json!(true));
                m.insert("indicator_color".into(), serde_json::json!("#ff0000"));
                m.insert("indicator_size".into(), serde_json::json!(10.0));
                m.insert("rotation".into(), serde_json::json!(0.0));
                m.insert("opacity".into(), serde_json::json!(1.0));
                m
            },
        }
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
        let v = validate_heading(&explicit_heading_value(), 0, &validated_scene());
        assert!(v.is_ok(), "valid heading should pass: {:?}", v.err());
    }

    #[test]
    fn non_heading_metric_rejected() {
        let mut v = explicit_heading_value();
        v.value = MetricKind::Speed;
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("expected Heading"), "{e}");
    }

    #[test]
    fn missing_width_rejected() {
        let mut v = explicit_heading_value();
        v.extra.insert("width".into(), serde_json::json!(0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("width"), "{e}");
    }

    #[test]
    fn missing_height_rejected() {
        let mut v = explicit_heading_value();
        v.extra.insert("height".into(), serde_json::json!(0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("height"), "{e}");
    }

    #[test]
    fn missing_pixels_per_degree_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("pixels_per_degree".into(), serde_json::json!(0.0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("pixels_per_degree"), "{e}");
    }

    #[test]
    fn zero_major_tick_interval_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("major_tick_interval".into(), serde_json::json!(0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("major_tick_interval"), "{e}");
    }

    #[test]
    fn zero_minor_ticks_per_major_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("minor_ticks_per_major".into(), serde_json::json!(0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("minor_ticks_per_major"), "{e}");
    }

    #[test]
    fn missing_tick_color_rejected() {
        let mut v = explicit_heading_value();
        v.extra.remove("tick_color");
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("tick_color"), "{e}");
    }

    #[test]
    fn invalid_tick_color_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("tick_color".into(), serde_json::json!("red"));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("tick_color") && e.contains("hex"), "{e}");
    }

    #[test]
    fn invalid_tick_alignment_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("tick_alignment".into(), serde_json::json!("invalid"));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("tick_alignment"), "{e}");
    }

    #[test]
    fn invalid_indicator_style_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("indicator_style".into(), serde_json::json!("invalid"));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("indicator_style"), "{e}");
    }

    #[test]
    fn invalid_indicator_placement_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("indicator_placement".into(), serde_json::json!("invalid"));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("indicator_placement"), "{e}");
    }

    #[test]
    fn missing_indicator_color_rejected() {
        let mut v = explicit_heading_value();
        v.extra.remove("indicator_color");
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("indicator_color"), "{e}");
    }

    #[test]
    fn missing_label_color_rejected() {
        let mut v = explicit_heading_value();
        v.extra.remove("label_color");
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("label_color"), "{e}");
    }

    #[test]
    fn invalid_label_font_size_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("label_font_size".into(), serde_json::json!(-1.0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("label_font_size"), "{e}");
    }

    #[test]
    fn invalid_indicator_size_rejected() {
        let mut v = explicit_heading_value();
        v.extra
            .insert("indicator_size".into(), serde_json::json!(0.0));
        let e = validate_heading(&v, 0, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("indicator_size"), "{e}");
    }

    #[test]
    fn index_in_error_path() {
        let mut v = explicit_heading_value();
        v.extra.insert("width".into(), serde_json::json!(0));
        let e = validate_heading(&v, 3, &validated_scene())
            .unwrap_err()
            .to_string();
        assert!(e.contains("values[3]"), "{e}");
    }
}
