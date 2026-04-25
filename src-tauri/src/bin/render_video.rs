use cyclemetry_core::activity::{build_dense_activity_report, parse_activity_json};
use cyclemetry_core::commands::AppPaths;
use cyclemetry_core::config::parse_config_json;
use cyclemetry_core::encode::video::{render_video, RenderController};
use serde_json::{Map, Number, Value};
use std::fs;
use std::path::PathBuf;

fn read_arg(flag: &str, args: &[String]) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("Missing required argument: {flag}"))
}

fn read_optional_arg(flag: &str, args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn repo_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "Failed to resolve repo root".to_string())
}

fn ensure_ffmpeg_object(
    config: &mut cyclemetry_core::config::RenderConfig,
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
    config: &mut cyclemetry_core::config::RenderConfig,
    key: &str,
    value: Option<String>,
) -> Result<(), String> {
    if let Some(value) = value {
        ensure_ffmpeg_object(config)?.insert(key.to_string(), Value::String(value));
    }
    Ok(())
}

fn set_ffmpeg_u64(
    config: &mut cyclemetry_core::config::RenderConfig,
    key: &str,
    value: Option<String>,
) -> Result<(), String> {
    if let Some(value) = value {
        let parsed = value
            .parse::<u64>()
            .map_err(|error| format!("Invalid {key} value '{value}': {error}"))?;
        ensure_ffmpeg_object(config)?.insert(key.to_string(), Value::Number(Number::from(parsed)));
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

    let activity = parse_activity_json(&payload_json)?;
    let mut config = parse_config_json(&config_json)?;
    set_ffmpeg_string(&mut config, "codec", read_optional_arg("--codec", &args))?;
    set_ffmpeg_string(&mut config, "container", read_optional_arg("--container", &args))?;
    set_ffmpeg_string(&mut config, "pix_fmt", read_optional_arg("--pix-fmt", &args))?;
    set_ffmpeg_string(
        &mut config,
        "prores_profile",
        read_optional_arg("--prores-profile", &args),
    )?;
    set_ffmpeg_string(&mut config, "loglevel", read_optional_arg("--loglevel", &args))?;
    set_ffmpeg_u64(&mut config, "threads", read_optional_arg("--threads", &args))?;
    let dense_activity = build_dense_activity_report(&activity, &config)?;
    let paths = AppPaths::from_repo_root(repo_root()?);
    paths.ensure_dirs()?;

    let controller = RenderController::default();
    controller.try_start(dense_activity.frame_count as u32, "Preparing render assets...")?;
    let filename = render_video(&paths, &config, &activity, &dense_activity, &controller)?;
    controller.finish_success(filename.clone());
    println!("{{\"filename\":\"{filename}\"}}");
    Ok(())
}
