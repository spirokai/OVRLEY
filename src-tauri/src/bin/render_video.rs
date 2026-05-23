//! Offline overlay rendering binary.
//!
//! This binary accepts pre-built activity and config JSON files (typically
//! produced by the frontend) and runs a single-pass Skia + ffmpeg render
//! without a Tauri window. It is used as a subprocess by the frontend so that
//! long renders don't block the UI.
//!
//! Responsibilities:
//! - Deserialize activity data and render config.
//! - Inject optional ffmpeg overrides (codec, container, pix_fmt) from CLI.
//! - Run the full render pipeline and print the output filename as JSON.
//!
//! Does not own: parsing or encoding — those live in `ovrley_core`.

use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
use ovrley_core::config::parse_config_json;
use ovrley_core::encode::video::{render_video, RenderController};
use ovrley_core::paths::AppPaths;
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

#[path = "../bin_common.rs"]
mod common;
use common::{read_arg, read_optional_arg, repo_root};

/// Ensures `config.scene.ffmpeg` is a JSON object, defaulting to an empty
/// one if it was null. Returns a mutable reference so callers can insert
/// override keys without repeated null checks.
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

/// Inserts a string key into the ffmpeg config object when a value is given.
///
/// A no-op when `value` is `None`, so callers can pass optional CLI flags
/// without branching on presence.
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

/// Runs a single-pass overlay video render from pre-built JSON files.
///
/// # Arguments (via `--flag value` CLI)
///
/// * `--payload <path>` — parsed activity JSON (required).
/// * `--config <path>` — render configuration JSON (required).
/// * `--codec`, `--container`, `--pix-fmt`, `--loglevel` — optional ffmpeg overrides.
///
/// # Output
///
/// Prints `{"filename":"<output>"}` to stdout on success so the frontend can
/// pick up the generated file path.
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
