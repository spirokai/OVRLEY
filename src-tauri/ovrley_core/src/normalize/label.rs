//! Label widget validation.
//!
//! `validate_label` verifies that every output-affecting field for a static
//! text label is already explicit. Missing fields are rejected — the backend
//! owns zero render-affecting defaults. The frontend must materialise all
//! defaults before sending the config.
//!
//! Shadow, border, and other scene-level properties are NOT part of the
//! label contract — they belong to the scene validation contract.

use super::helpers::rgba_from_hex;
use super::raw::LabelConfig;
use crate::error::{CoreError, CoreResult};

// ---------------------------------------------------------------------------
// ValidatedLabel — zero backend-side defaults
// ---------------------------------------------------------------------------

/// Every output-affecting field for a static text label is explicit.
#[derive(Clone, Debug)]
pub struct ValidatedLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_name: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub opacity: f32,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validates a label config and returns a `ValidatedLabel` with all
/// output-affecting fields explicit. Missing or invalid fields cause an
/// immediate error.
pub fn validate_label(label: &LabelConfig, index: usize) -> CoreResult<ValidatedLabel> {
    let p = |f: &str| format!("labels[{index}].{f}");

    // text: allow empty (labels can be empty by design)
    let text = label.text.clone();

    // x, y: required
    // (already non-optional in LabelConfig, so always present)

    // font_name: required — frontend must resolve from label.font →
    // label.font_family → scene.font before sending
    let font_name = label
        .font
        .clone()
        .or_else(|| label.font_family.clone())
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("font"))))?;

    if font_name.is_empty() {
        return Err(CoreError::Config(format!(
            "{}: font name must not be empty",
            p("font")
        )));
    }

    // font_size: required
    let font_size = label
        .font_size
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("font_size"))))?;

    if font_size <= 0.0 {
        return Err(CoreError::Config(format!(
            "{}: must be positive, got {font_size}",
            p("font_size")
        )));
    }

    // color: required
    let color_hex = label
        .color
        .as_deref()
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("color"))))?;

    let opacity = label
        .opacity
        .ok_or_else(|| CoreError::Config(format!("{}: required field missing", p("opacity"))))?;

    if !(0.0..=1.0).contains(&opacity) {
        return Err(CoreError::Config(format!(
            "{}: must be 0.0..=1.0, got {opacity}",
            p("opacity")
        )));
    }

    let color = rgba_from_hex(color_hex, &p("color"), opacity)?;

    Ok(ValidatedLabel {
        text,
        x: label.x,
        y: label.y,
        font_name,
        font_size,
        color,
        opacity,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn full_label_json() -> serde_json::Value {
        serde_json::json!({
            "text": "HELLO",
            "x": 100.0,
            "y": 200.0,
            "font": "Oxanium.ttf",
            "font_size": 30.0,
            "color": "#ffffff",
            "opacity": 1.0
        })
    }

    fn parse_label(json: serde_json::Value) -> LabelConfig {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn explicit_label_passes() {
        let label = parse_label(full_label_json());
        let validated = validate_label(&label, 0).unwrap();
        assert_eq!(validated.text, "HELLO");
        assert_eq!(validated.x, 100.0);
        assert_eq!(validated.y, 200.0);
        assert_eq!(validated.font_name, "Oxanium.ttf");
        assert_eq!(validated.font_size, 30.0);
        assert_eq!(validated.color, [0xff, 0xff, 0xff, 0xff]);
        assert_eq!(validated.opacity, 1.0);
    }

    #[test]
    fn empty_text_passes() {
        let mut json = full_label_json();
        json["text"] = serde_json::json!("");
        let label = parse_label(json);
        let validated = validate_label(&label, 0).unwrap();
        assert_eq!(validated.text, "");
    }

    #[test]
    fn missing_font_rejected() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("font");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("labels[0].font"));
    }

    #[test]
    fn missing_font_size_rejected() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("font_size");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("labels[0].font_size"));
    }

    #[test]
    fn missing_color_rejected() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("color");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("labels[0].color"));
    }

    #[test]
    fn missing_opacity_rejected() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("opacity");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("labels[0].opacity"));
    }

    #[test]
    fn negative_font_size_rejected() {
        let mut json = full_label_json();
        json["font_size"] = serde_json::json!(-10.0);
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("must be positive"));
    }

    #[test]
    fn opacity_out_of_range_rejected() {
        let mut json = full_label_json();
        json["opacity"] = serde_json::json!(1.5);
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("must be 0.0..=1.0"));
    }

    #[test]
    fn invalid_color_hex_rejected() {
        let mut json = full_label_json();
        json["color"] = serde_json::json!("not-a-color");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("expected 6-digit or 8-digit hex"));
    }

    #[test]
    fn rgba_hex_parsed_correctly() {
        let mut json = full_label_json();
        json["color"] = serde_json::json!("#ff8000aa");
        let label = parse_label(json);
        let validated = validate_label(&label, 0).unwrap();
        assert_eq!(validated.color, [0xff, 0x80, 0x00, 0xaa]);
    }

    #[test]
    fn font_family_used_as_fallback() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("font");
        json["font_family"] = serde_json::json!("Roboto.ttf");
        let label = parse_label(json);
        let validated = validate_label(&label, 0).unwrap();
        assert_eq!(validated.font_name, "Roboto.ttf");
    }

    #[test]
    fn empty_font_name_rejected() {
        let mut json = full_label_json();
        json["font"] = serde_json::json!("");
        let label = parse_label(json);
        let err = validate_label(&label, 0).unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[test]
    fn index_in_error_path() {
        let mut json = full_label_json();
        json.as_object_mut().unwrap().remove("color");
        let label = parse_label(json);
        let err = validate_label(&label, 3).unwrap_err();
        assert!(err.to_string().contains("labels[3].color"));
    }
}
