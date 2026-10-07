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

use ovrley_core::activity::{parse_activity_json, validate_render_activity};
use ovrley_core::encode::progress::RenderController;
use ovrley_core::normalize::parse_config_value;
use ovrley_core::output::{RenderOutputKind, RenderOutputTarget};
use ovrley_core::paths::AppPaths;
use ovrley_core::render_jobs::execution::RenderExecutionService;
use ovrley_core::render_jobs::planning::{plan_single_render, VideoRenderModePlan};
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

use ovrley_core::bin_common::{read_arg, read_optional_arg, repo_root};

/// Ensures `config.scene.ffmpeg` is a JSON object, defaulting to an empty
/// one if it was null. Returns a mutable reference so callers can insert
/// override keys without repeated null checks.
fn ensure_ffmpeg_object(config: &mut Value) -> Result<&mut Map<String, Value>, String> {
    let scene = config
        .get_mut("scene")
        .ok_or_else(|| "config missing 'scene'".to_string())?;
    if scene.get("ffmpeg").map_or(true, |v| v.is_null()) {
        scene
            .as_object_mut()
            .ok_or_else(|| "scene must be an object".to_string())?
            .insert("ffmpeg".to_string(), Value::Object(Map::new()));
    }
    scene
        .get_mut("ffmpeg")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| "scene.ffmpeg must be a JSON object".to_string())
}

/// Inserts a string key into the ffmpeg config object when a value is given.
///
/// A no-op when `value` is `None`, so callers can pass optional CLI flags
/// without branching on presence.
fn set_ffmpeg_string(config: &mut Value, key: &str, value: Option<String>) -> Result<(), String> {
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
    let mut config_value: Value =
        serde_json::from_str(&config_json).map_err(|error| format!("config JSON: {error}"))?;
    set_ffmpeg_string(
        &mut config_value,
        "codec",
        read_optional_arg("--codec", &args),
    )?;
    set_ffmpeg_string(
        &mut config_value,
        "container",
        read_optional_arg("--container", &args),
    )?;
    set_ffmpeg_string(
        &mut config_value,
        "pix_fmt",
        read_optional_arg("--pix-fmt", &args),
    )?;
    set_ffmpeg_string(
        &mut config_value,
        "loglevel",
        read_optional_arg("--loglevel", &args),
    )?;
    let plan = plan_single_render(
        parse_config_value(&config_value).map_err(|e| e.to_string())?,
        validate_render_activity(&activity).map_err(|e| e.to_string())?,
        None,
    )
    .map_err(|e| e.to_string())?;
    let output_kind = match plan.mode() {
        VideoRenderModePlan::Composite { .. } => RenderOutputKind::Composite,
        VideoRenderModePlan::Transparent(_) => RenderOutputKind::Transparent,
    };
    let paths = AppPaths::from_repo_root(repo_root()?);
    paths.ensure_dirs().map_err(|e| e.to_string())?;

    let controller = RenderController::default();
    let execution = RenderExecutionService::with_controller(controller.clone());
    let output_path = paths.downloads_dir.join(format!(
        "overlay_{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos(),
        output_kind.extension(),
    ));
    let output_target =
        RenderOutputTarget::validate(output_path.to_str().unwrap(), output_kind, false)
            .map_err(|error| error.to_string())?;
    let outcome = execution.render(&paths, plan, &activity, &output_target);
    let filename = outcome.map_err(|e| e.to_string())?;
    println!("{{\"filename\":\"{filename}\"}}");
    Ok(())
}
