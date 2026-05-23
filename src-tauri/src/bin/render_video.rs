use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::paths::AppPaths;
use ovrley_core::config::parse_config_json;
use ovrley_core::encode::video::{render_video, RenderController};
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

#[path = "../bin_common.rs"]
mod common;
use common::{read_arg, read_optional_arg, repo_root};

fn ensure_ffmpeg_object(
    config: &mut ovrley_core::config::RenderConfig,
) -> Result<&mut Map<String, Value>, String> {
    if config.scene.ffmpeg.is_null() {
        config.scene.ffmpeg = Value::Object(Map::new());
    }
    config
        .scene
        .ffmpeg
        .as_object_mut()
        .ok_or_else(|| "scene.ffmpeg must be a JSON object".to_string())
}

fn set_ffmpeg_string(
    config: &mut ovrley_core::config::RenderConfig,
    key: &str,
    value: Option<String>,
) -> Result<(), String> {
    if let Some(value) = value {
        ensure_ffmpeg_object(config)?.insert(key.to_string(), Value::String(value));
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let payload_path = PathBuf::from(read_arg("--payload", &args)?);
    let config_path = PathBuf::from(read_arg("--config", &args)?);

    let payload_json = fs::read_to_string(&payload_path)
        .map_err(|error| format!("Failed to read {}: {error}", payload_path.display()))?;
    let config_json = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read {}: {error}", config_path.display()))?;

    let activity = parse_activity_json(&payload_json).map_err(|e| e.to_string())?;
    let mut config = parse_config_json(&config_json).map_err(|e| e.to_string())?;
    set_ffmpeg_string(&mut config, "codec", read_optional_arg("--codec", &args))?;
    set_ffmpeg_string(
        &mut config,
        "container",
        read_optional_arg("--container", &args),
    )?;
    set_ffmpeg_string(
        &mut config,
        "pix_fmt",
        read_optional_arg("--pix-fmt", &args),
    )?;
    set_ffmpeg_string(
        &mut config,
        "loglevel",
        read_optional_arg("--loglevel", &args),
    )?;
    let dense_activity =
        build_dense_activity_report(&activity, &config).map_err(|e| e.to_string())?;
    let paths = AppPaths::from_repo_root(repo_root()?);
    paths.ensure_dirs().map_err(|e| e.to_string())?;

    let controller = RenderController::default();
    controller
        .try_start(
            dense_activity.frame_count as u32,
            "Preparing render assets...",
        )
        .map_err(|e| e.to_string())?;
    let filename = render_video(&paths, &config, &activity, &dense_activity, &controller)
        .map_err(|e| e.to_string())?;
    controller.finish_success(filename.clone());
    println!("{{\"filename\":\"{filename}\"}}");
    Ok(())
}
