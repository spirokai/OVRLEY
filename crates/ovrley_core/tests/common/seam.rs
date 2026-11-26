#![allow(dead_code)]

use ovrley_core::config::{parse_config_json, parse_config_value, validate_render_config};
use ovrley_core::error::CoreResult;
use ovrley_core::config::{
    validate_ffmpeg_config, ValidatedFfmpegConfig, ValidatedRenderConfig,
};
use ovrley_widgets::widgets::value::ValidatedValue;
use ovrley_widgets::widgets::value::ValidatedValueWidget;
use serde_json::{json, Value};

pub fn explicit_scene_json() -> Value {
    json!({
        "fps": 30.0,
        "start": 0.0,
        "end": 10.0,
        "width": 1920,
        "height": 1080,
        "scale": 1.0,
        "opacity": 1.0,
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

/// Parses and validates a render config JSON string.
pub fn parse_and_validate_config(config_json: &str) -> CoreResult<ValidatedRenderConfig> {
    validate_render_config(parse_config_json(config_json)?, None)
}

/// Validates a render config value.
pub fn validate_config_value(config_value: &Value) -> CoreResult<ValidatedRenderConfig> {
    validate_render_config(parse_config_value(config_value)?, None)
}

pub fn validated_config_from_json(config_json: &str) -> ValidatedRenderConfig {
    parse_and_validate_config(config_json).unwrap()
}

pub fn validated_config_from_value(config_value: Value) -> ValidatedRenderConfig {
    validate_config_value(&config_value).unwrap()
}

pub fn validate_scene_ffmpeg(ffmpeg: Value) -> CoreResult<ValidatedFfmpegConfig> {
    validate_ffmpeg_config(
        ffmpeg,
        ovrley_core::encode::ffmpeg::catalog::CodecSelection::Transparent(
            ovrley_core::encode::ffmpeg::catalog::TransparentCodecId::ProresKs,
        ),
    )
}

pub fn expect_standard_value(value: ValidatedValue, index: usize) -> ValidatedValueWidget {
    match value {
        ValidatedValue::StandardText(value) => value,
        other => panic!("expected values[{index}] to be a standard text metric, got {other:?}"),
    }
}
