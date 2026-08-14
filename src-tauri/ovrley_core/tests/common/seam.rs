#![allow(dead_code)]

use ovrley_core::commands::{parse_and_validate_config, validate_config_value};
use ovrley_core::error::CoreResult;
use ovrley_core::normalize::{
    validate_scene_config, SceneConfig, ValidatedFfmpegConfig, ValidatedRenderConfig,
    ValidatedValueWidget,
};
use ovrley_core::render::widgets::types::PreparedValue;
use serde_json::{json, Value};

pub fn explicit_scene_json() -> Value {
    json!({
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
        "ffmpeg": {}
    })
}

pub fn validated_config_from_json(config_json: &str) -> ValidatedRenderConfig {
    parse_and_validate_config(config_json).unwrap()
}

pub fn validated_config_from_value(config_value: Value) -> ValidatedRenderConfig {
    validate_config_value(&config_value).unwrap()
}

pub fn validate_scene_ffmpeg(ffmpeg: Value) -> CoreResult<ValidatedFfmpegConfig> {
    let mut scene: SceneConfig = serde_json::from_value(explicit_scene_json()).unwrap();
    scene.ffmpeg = ffmpeg;
    validate_scene_config(scene).map(|scene| scene.ffmpeg)
}

pub fn expect_standard_value(value: PreparedValue, index: usize) -> ValidatedValueWidget {
    match value {
        PreparedValue::StandardText(value) => value.validated,
        other => panic!("expected values[{index}] to be a standard text metric, got {other:?}"),
    }
}
