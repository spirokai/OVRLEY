//! DisplayType contract tests.
//!
//! These keep coverage on the canonical enum strings and on the validation seam
//! behavior that now owns config health. Older parser-fallback tests were
//! superseded once nonconforming configs stopped flowing past the seam.

mod common;

use ovrley_core::render::widgets::types::PreparedValue;
use ovrley_core::standard_metrics::{display_type_layout_mode, DisplayTypeLayoutMode};
use ovrley_core::types::DisplayType;
use serde_json::json;

#[test]
fn recognized_display_type_strings_parse_to_expected_variants() {
    let cases = [
        (r#""text""#, DisplayType::Text),
        (r#""linear""#, DisplayType::Linear),
        (r#""bars""#, DisplayType::Bars),
        (r#""arc""#, DisplayType::Arc),
        (r#""corner""#, DisplayType::Corner),
        (r#""heading_tape""#, DisplayType::Tape),
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
fn display_type_round_trips_each_variant() {
    let cases = [
        (DisplayType::Text, r#""text""#),
        (DisplayType::Linear, r#""linear""#),
        (DisplayType::Bars, r#""bars""#),
        (DisplayType::Arc, r#""arc""#),
        (DisplayType::Corner, r#""corner""#),
        (DisplayType::Tape, r#""heading_tape""#),
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
            "display_type": "text"
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
            "min_max_label_font_size": 12,
            "min_max_label_color": "#ffffff"
        }],
        "plots": []
    }));

    let PreparedValue::LinearGauge(value) = config.values.into_iter().next().unwrap() else {
        panic!("linear display type should validate as a linear gauge prepared value");
    };
    assert_eq!(value.display_type, DisplayType::Linear);
    assert_eq!(value.width, 200);
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
        display_type_layout_mode(DisplayType::Bars),
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
}
