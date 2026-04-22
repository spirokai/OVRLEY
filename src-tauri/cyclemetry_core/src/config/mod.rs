use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneConfig {
    pub fps: f64,
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub update_rate: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenderConfig {
    pub scene: SceneConfig,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

pub fn parse_config_json(input: &str) -> Result<RenderConfig, String> {
    let config: RenderConfig =
        serde_json::from_str(input).map_err(|error| format!("Invalid config JSON: {error}"))?;
    if config.scene.fps <= 0.0 {
        return Err(format!("Invalid scene.fps: {}", config.scene.fps));
    }
    if config.scene.end <= config.scene.start {
        return Err(format!(
            "Invalid scene range. scene.end ({}) must be greater than scene.start ({})",
            config.scene.end, config.scene.start
        ));
    }
    Ok(config)
}
