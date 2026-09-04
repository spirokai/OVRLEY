//! DisplayType contract tests.
//!
//! These keep coverage on the canonical enum strings and on the validation seam
//! behavior that now owns config health. Older parser-fallback tests were
//! superseded once nonconforming configs stopped flowing past the seam.

mod common;

use ovrley_core::render::widgets::types::PreparedValue;
use ovrley_core::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use ovrley_core::types::{DisplayType, TrackFillStyle};
use serde_json::json;

#[test]
fn recognized_display_type_strings_parse_to_expected_variants() {
    let cases = [
        (r#""text""#, DisplayType::Text),
        (r#""linear""#, DisplayType::Linear),
        (r#""arc""#, DisplayType::Arc),
        (r#""corner""#, DisplayType::Corner),
        (r#""heading_tape""#, DisplayType::Tape),
        (r#""lean_angle""#, DisplayType::LeanAngle),
        (r#""g_force""#, DisplayType::GForce),
    ];

    for (json_value, expected) in cases {
        let parsed: DisplayType = serde_json::from_str(json_value).unwrap();
        assert_eq!(
            parsed, expected,
            "{json_value} should parse to {expected:?}"
        );
    }
}

#[test]
fn legacy_bars_display_type_falls_back_to_text() {
    let parsed: DisplayType = serde_json::from_str(r#""bars""#).unwrap();
    assert_eq!(parsed, DisplayType::Text);
}

#[test]
fn track_fill_style_is_forgiving() {
    let bars: TrackFillStyle = serde_json::from_str(r#""bars""#).unwrap();
    let unknown: TrackFillStyle = serde_json::from_str(r#""unknown""#).unwrap();
    assert_eq!(bars, TrackFillStyle::Bars);
    assert_eq!(bars.as_str(), "bars");
    assert_eq!(unknown, TrackFillStyle::Fill);
}

#[test]
fn display_type_round_trips_each_variant() {
    let cases = [
        (DisplayType::Text, r#""text""#),
        (DisplayType::Linear, r#""linear""#),
        (DisplayType::Arc, r#""arc""#),
        (DisplayType::Corner, r#""corner""#),
        (DisplayType::Tape, r#""heading_tape""#),
        (DisplayType::LeanAngle, r#""lean_angle""#),
        (DisplayType::GForce, r#""g_force""#),
    ];

    for (variant, expected_json) in cases {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, expected_json);

        let deserialized: DisplayType = serde_json::from_str(expected_json).unwrap();
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn text_display_type_passes_standard_metric_validation() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "speed",
            "x": 10,
            "y": 20,
            "font": "Arial.ttf",
            "font_size": 32.0,
            "color": "#ffffff",
            "opacity": 1.0,
            "show_icon": true,
            "icon_color": "#ffffff",
            "icon_size": 45.0,
            "icon_offset_x": 0.0,
            "icon_offset_y": 0.0,
            "show_units": true,
            "unit_color": "#ffffff",
            "display_unit": "kmh",
            "prefix": "",
            "suffix": "",
            "decimals": 0,
            "triangle_width": 0.0,
            "display_type": "text",
            "content_alignment": "left"
        }],
        "plots": []
    }));

    let value = common::seam::expect_standard_value(config.values.into_iter().next().unwrap(), 0);
    assert_eq!(value.display_type, DisplayType::Text);
}

#[test]
fn incomplete_linear_display_type_requires_gauge_fields() {
    let error = ovrley_core::commands::validate_config_value(&json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "speed",
            "x": 10,
            "y": 20,
            "font": "Arial.ttf",
            "font_size": 32.0,
            "color": "#ffffff",
            "opacity": 1.0,
            "show_icon": true,
            "icon_color": "#ffffff",
            "icon_size": 45.0,
            "icon_offset_x": 0.0,
            "icon_offset_y": 0.0,
            "show_units": true,
            "unit_color": "#ffffff",
            "display_unit": "kmh",
            "prefix": "",
            "suffix": "",
            "decimals": 0,
            "triangle_width": 0.0,
            "display_type": "linear"
        }],
        "plots": []
    }))
    .err()
    .unwrap();

    assert!(
        error.to_string().contains("values[0].width: required"),
        "got: '{error}'"
    );
}

#[test]
fn complete_linear_display_type_validates_as_gauge() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "speed",
            "x": 10,
            "y": 20,
            "display_type": "linear",
            "width": 200,
            "height": 60,
            "rotation": 0,
            "orientation": "horizontal",
            "track_corner_radius": 6,
            "track_border_thickness": 2,
            "track_border_color": "#ffffff",
            "track_empty_color": "#222222",
            "track_empty_opacity": 0.5,
            "track_filled_color": "#40e0d0",
            "track_filled_opacity": 1,
            "show_min_max_labels": false,
            "min_max_label_font": "Arial.ttf",
            "min_max_label_font_size": 12,
            "min_max_label_position": "bottom",
            "min_max_label_color": "#ffffff",
            "track_fill_flat": false
        }],
        "plots": []
    }));

    let PreparedValue::LinearGauge(value) = config.values.into_iter().next().unwrap() else {
        panic!("linear display type should validate as a linear gauge prepared value");
    };
    assert_eq!(value.validated.display_type, DisplayType::Linear);
    assert_eq!(value.validated.width, 200);
}

#[test]
fn complete_g_force_display_type_validates_with_canonical_marker_and_label_fields() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "g_force",
            "x": 10,
            "y": 20,
            "display_type": "g_force",
            "width": 220,
            "height": 220,
            "rotation": 0,
            "opacity": 1,
            "diameter": 200,
            "fill_color": "#212121",
            "fill_opacity": 0.5,
            "border_thickness": 2,
            "border_color": "#ffffff",
            "border_opacity": 1,
            "marker_size": 12,
            "marker_color": "#ffffff",
            "marker_opacity": 1,
            "axis_horizontal": "x",
            "axis_vertical": "y",
            "invert_horizontal": false,
            "invert_vertical": false,
            "clip_percentile": 99,
            "label_font": "Arial.ttf",
            "label_font_size": 14,
            "label_color": "#ffffff",
            "label_decimals": 1,
            "label_unit": "G",
            "label_unit_color": "#ffffff",
            "label_offset_x": 0,
            "label_offset_y": 0
        }],
        "plots": []
    }));

    let PreparedValue::GForce(value) = config.values.into_iter().next().unwrap() else {
        panic!("g_force display type should validate as a G-force prepared value");
    };
    assert_eq!(value.validated.marker_size, 12.0);
    assert_eq!(value.validated.label_decimals, 1);
}

#[test]
fn display_type_is_intrinsic_only_for_text() {
    assert_eq!(
        display_type_layout_mode(DisplayType::Text),
        DisplayTypeLayoutMode::Intrinsic
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::Linear),
        DisplayTypeLayoutMode::Boxed
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::Arc),
        DisplayTypeLayoutMode::Boxed
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::Corner),
        DisplayTypeLayoutMode::Boxed
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::Tape),
        DisplayTypeLayoutMode::Boxed
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::LeanAngle),
        DisplayTypeLayoutMode::Boxed
    );
    assert_eq!(
        display_type_layout_mode(DisplayType::GForce),
        DisplayTypeLayoutMode::Boxed
    );
}
