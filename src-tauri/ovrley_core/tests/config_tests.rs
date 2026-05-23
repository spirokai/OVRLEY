//! Configuration parsing and serialization tests.
//!
//! Verifies `parse_config_json` for transparent and composite configs,
//! default handling, composite-field serialization suppression, input
//! validation, and serde round-trip idempotency.
//!
//! ## Fixtures
//!
//! - `test_config::simple_config_path()` — minimal valid transparent config.
//! - `test_config::composite_config_path()` — valid composite config with
//!   all render-time fields.
//! - `fixtures/config/invalid.json` — deliberately malformed config for
//!   error-reporting tests.
//!
//! ## Type
//! Integration test. Reads fixture files from disk; does not require
//! ffmpeg or video assets.
//!
//! ## Regressions guarded
//! - Old transparent templates silently failing due to new composite fields
//! - Composite render-time fields leaking into serialized output
//! - Zero widget update rate accepted (division by zero)
//! - Serde round-trip losing fields

use ovrley_core::config::parse_config_json;

mod common;

#[test]
// Verifies old transparent-render configs parse without composite fields.
fn parses_transparent_config_without_composite_fields() {
    let config = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {}
            },
            "labels": [],
            "values": [],
            "plots": []
        }"#,
    )
    .unwrap();

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
// Verifies all composite render-time fields deserialize into scene config.
fn parses_config_with_all_composite_fields() {
    let config = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {},
                "composite_video_path": "test.mp4",
                "composite_bitrate": "60M",
                "composite_sync_offset": 300.0,
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 10.0,
                "composite_video_trim_start": 0.0,
                "composite_widget_update_rate": 2
            },
            "labels": [],
            "values": [],
            "plots": []
        }"#,
    )
    .unwrap();

    assert_eq!(
        config.scene.composite_video_path.as_deref(),
        Some("test.mp4")
    );
    assert_eq!(config.scene.composite_bitrate.as_deref(), Some("60M"));
    assert_eq!(config.scene.composite_sync_offset, Some(300.0));
    assert_eq!(config.scene.composite_video_fps_num, Some(30000));
    assert_eq!(config.scene.composite_video_fps_den, Some(1001));
    assert_eq!(config.scene.composite_video_duration, Some(20.0));
    assert_eq!(config.scene.composite_render_duration, Some(10.0));
    assert_eq!(config.scene.composite_video_trim_start, Some(0.0));
    assert_eq!(config.scene.composite_widget_update_rate, Some(2));
}

#[test]
// Verifies composite render-time fields are not emitted by Rust serialization.
fn skips_composite_fields_when_serializing_scene() {
    let config = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {},
                "composite_video_path": "test.mp4",
                "composite_bitrate": "60M",
                "composite_sync_offset": 300.0,
                "composite_video_fps_num": 30000,
                "composite_video_fps_den": 1001,
                "composite_video_duration": 20.0,
                "composite_render_duration": 10.0,
                "composite_video_trim_start": 0.0,
                "composite_widget_update_rate": 2
            },
            "labels": [],
            "values": [],
            "plots": []
        }"#,
    )
    .unwrap();

    let serialized = serde_json::to_value(&config).unwrap();
    let scene = serialized.get("scene").unwrap();
    for key in [
        "composite_video_path",
        "composite_bitrate",
        "composite_sync_offset",
        "composite_video_fps_num",
        "composite_video_fps_den",
        "composite_video_duration",
        "composite_render_duration",
        "composite_video_trim_start",
        "composite_widget_update_rate",
    ] {
        assert_eq!(scene.get(key), None, "{key} should not serialize");
    }
}

#[test]
// Verifies composite overlay update-rate validation prevents division by zero.
fn rejects_zero_composite_widget_update_rate() {
    let error = parse_config_json(
        r#"{
            "scene": {
                "fps": 30,
                "start": 0,
                "end": 10,
                "ffmpeg": {},
                "composite_widget_update_rate": 0
            },
            "labels": [],
            "values": [],
            "plots": []
        }"#,
    )
    .unwrap_err();

    let error_msg = error.to_string();
    assert!(
        error_msg.contains("scene.composite_widget_update_rate must be at least 1"),
        "got: '{error_msg}'"
    );
}

// --- Snapshot / golden tests (Step 11a) ---

#[test]
fn valid_minimal_config_fills_defaults() {
    let json = std::fs::read_to_string(common::test_config::simple_config_path()).unwrap();
    let config = parse_config_json(&json).unwrap();

    assert_eq!(config.scene.fps, 30.0);
    assert_eq!(config.scene.width, Some(1920));
    assert_eq!(config.scene.height, Some(1080));
    assert_eq!(config.scene.composite_video_path, None);
    assert_eq!(config.values.len(), 1);
}

#[test]
fn valid_composite_config_preserves_fields() {
    let json = std::fs::read_to_string(common::test_config::composite_config_path()).unwrap();
    let config = parse_config_json(&json).unwrap();

    assert_eq!(
        config.scene.composite_video_path.as_deref(),
        Some("test.mp4")
    );
    assert_eq!(config.scene.composite_bitrate.as_deref(), Some("60M"));
    assert_eq!(config.scene.composite_sync_offset, Some(300.0));
    assert_eq!(config.scene.composite_widget_update_rate, Some(2));
}

#[test]
fn invalid_json_reports_error() {
    let json = std::fs::read_to_string(
        common::test_config::fixtures()
            .join("config")
            .join("invalid.json"),
    )
    .unwrap();
    let error = parse_config_json(&json).unwrap_err();
    assert!(!error.to_string().is_empty());
}

#[test]
fn config_roundtrip_is_idempotent() {
    let json = std::fs::read_to_string(common::test_config::simple_config_path()).unwrap();
    let config1 = parse_config_json(&json).unwrap();
    let serialized = serde_json::to_string(&config1).unwrap();
    let config2 = parse_config_json(&serialized).unwrap();
    assert_eq!(config1.scene.fps, config2.scene.fps);
    assert_eq!(config1.scene.width, config2.scene.width);
    assert_eq!(config1.scene.height, config2.scene.height);
}
