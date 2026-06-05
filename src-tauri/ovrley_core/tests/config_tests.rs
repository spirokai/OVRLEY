//! Configuration seam tests.
//!
//! These tests exercise the public validation seam rather than the raw config
//! DTOs. Old parser-only tests that asserted backend defaults or fallback
//! behavior were superseded once validation took full responsibility for config
//! health.

mod common;

use ovrley_core::commands::{parse_and_validate_config, validate_template_contents};
use ovrley_core::normalize::TEMPLATE_FILE_VERSION;
use serde_json::json;

#[test]
fn validated_transparent_config_preserves_absent_composite_fields() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [explicit_speed_value()],
        "plots": []
    }));

    assert_eq!(config.scene.composite_video_path, None);
    assert_eq!(config.scene.composite_bitrate, None);
    assert_eq!(config.scene.composite_sync_offset, None);
    assert_eq!(config.scene.composite_video_fps_num, None);
    assert_eq!(config.scene.composite_video_fps_den, None);
    assert_eq!(config.scene.composite_video_duration, None);
    assert_eq!(config.scene.composite_render_duration, None);
    assert_eq!(config.scene.composite_video_trim_start, None);
    assert_eq!(config.scene.composite_widget_update_rate, None);
}

#[test]
fn validated_composite_config_preserves_fields() {
    let mut config = json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [explicit_speed_value()],
        "plots": []
    });
    config["scene"]["composite_video_path"] = json!("test.mp4");
    config["scene"]["composite_bitrate"] = json!("60M");
    config["scene"]["composite_sync_offset"] = json!(300.0);
    config["scene"]["composite_video_fps_num"] = json!(30000);
    config["scene"]["composite_video_fps_den"] = json!(1001);
    config["scene"]["composite_video_duration"] = json!(20.0);
    config["scene"]["composite_render_duration"] = json!(10.0);
    config["scene"]["composite_video_trim_start"] = json!(0.0);
    config["scene"]["composite_widget_update_rate"] = json!(2);

    let validated = common::seam::validated_config_from_value(config);
    assert_eq!(
        validated.scene.composite_video_path.as_deref(),
        Some("test.mp4")
    );
    assert_eq!(validated.scene.composite_bitrate.as_deref(), Some("60M"));
    assert_eq!(validated.scene.composite_sync_offset, Some(300.0));
    assert_eq!(validated.scene.composite_video_fps_num, Some(30000));
    assert_eq!(validated.scene.composite_video_fps_den, Some(1001));
    assert_eq!(validated.scene.composite_video_duration, Some(20.0));
    assert_eq!(validated.scene.composite_render_duration, Some(10.0));
    assert_eq!(validated.scene.composite_video_trim_start, Some(0.0));
    assert_eq!(validated.scene.composite_widget_update_rate, Some(2));
}

#[test]
fn rejects_zero_composite_widget_update_rate() {
    let error = parse_and_validate_config(
        r##"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
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
                "ffmpeg": {},
                "composite_widget_update_rate": 0
            },
            "labels": [],
            "values": [],
            "plots": []
        }"##,
    )
    .err()
    .unwrap();

    let error_msg = error.to_string();
    assert!(
        error_msg.contains("scene.composite_widget_update_rate must be at least 1"),
        "got: '{error_msg}'"
    );
}

#[test]
fn legacy_simple_fixture_is_rejected_by_validation_seam() {
    let json = std::fs::read_to_string(common::test_config::simple_config_path()).unwrap();
    let error = parse_and_validate_config(&json).err().unwrap();
    assert!(
        error.to_string().contains("scene.shadow_color")
            || error.to_string().contains("scene.scale")
            || error.to_string().contains("scene.update_rate"),
        "got: '{error}'"
    );
}

#[test]
fn legacy_composite_fixture_is_rejected_by_validation_seam() {
    let json = std::fs::read_to_string(common::test_config::composite_config_path()).unwrap();
    let error = parse_and_validate_config(&json).err().unwrap();
    assert!(
        error.to_string().contains("scene.shadow_color")
            || error.to_string().contains("scene.scale")
            || error.to_string().contains("scene.update_rate"),
        "got: '{error}'"
    );
}

#[test]
fn invalid_json_reports_error() {
    let json = std::fs::read_to_string(
        common::test_config::fixtures()
            .join("config")
            .join("invalid.json"),
    )
    .unwrap();
    let error = parse_and_validate_config(&json).err().unwrap();
    assert!(!error.to_string().is_empty());
}

#[test]
fn validated_standard_metric_display_unit_survives_seam() {
    let config = common::seam::validated_config_from_value(json!({
        "scene": common::seam::explicit_scene_json(),
        "labels": [],
        "values": [{
            "value": "speed",
            "x": 0,
            "y": 0,
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
            "display_unit": "mph",
            "prefix": "",
            "suffix": "",
            "decimals": 0,
            "triangle_width": 0.0,
            "display_type": "text"
        }],
        "plots": []
    }));

    let value = common::seam::expect_standard_value(config.values.into_iter().next().unwrap(), 0);
    assert_eq!(value.display_unit, "mph");
}

#[test]
fn rejects_older_template_versions_explicitly() {
    let error = validate_template_contents(&format!(
        r#"{{
            "format": "ovrley-template",
            "version": {},
            "config": {{
                "scene": {{
                    "fps": 30,
                    "start": 0,
                    "end": 10
                }},
                "labels": [],
                "values": [],
                "plots": []
            }}
        }}"#,
        TEMPLATE_FILE_VERSION - 1
    ))
    .unwrap_err();

    assert!(
        error.to_string().contains(&format!(
            "unsupported template version: {}. expected {}",
            TEMPLATE_FILE_VERSION - 1,
            TEMPLATE_FILE_VERSION
        )),
        "got: '{error}'"
    );
}

fn explicit_speed_value() -> serde_json::Value {
    json!({
        "value": "speed",
        "x": 0,
        "y": 0,
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
    })
}
