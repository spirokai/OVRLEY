//! Backend command implementations used by the Tauri shell.
//!
//! Functions in this module are framework-agnostic: they accept plain strings,
//! paths, and controller references so the Tauri command layer can delegate here
//! without mixing app-window concerns into render logic. Responsibilities include
//! runtime path resolution, template IO, video render startup, progress/cancel
//! plumbing, and small OS integration helpers.

use crate::activity::schema::ParsedActivity;
use crate::activity::{build_dense_activity_report, parse_activity_json};
use crate::config::parse_config_json;
use crate::config::RenderConfig;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::video::{
    render_composite_video, render_video, CompositeRenderRequest, RenderController,
};
use crate::encode::video_composite_pipeline::{
    apply_composite_scene_timing, derive_composite_render_plan,
};
use crate::encode::video_pipeline::rendered_frame_count;
use crate::error::{CoreError, CoreResult};
use serde::Serialize;
use serde_json::{json, Value};
use skia_safe::FontMgr;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::paths::AppPaths;

/// Health-check response sent to the frontend.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Machine-readable status string.
    pub status: String,
    /// Human-readable readiness details, including ffmpeg resolution.
    pub message: String,
    /// Whether the Rust backend is usable.
    pub ready: bool,
}

/// Reports backend readiness and ffmpeg discovery state.
pub fn backend_health(paths: &AppPaths) -> HealthResponse {
    let message = match resolve_ffmpeg_binary(&paths.repo_root) {
        Ok(path) => format!("Rust backend ready; ffmpeg={}", path.display()),
        Err(error) => format!("Rust backend ready; {error}"),
    };

    HealthResponse {
        status: "ok".to_string(),
        message,
        ready: true,
    }
}

/// Returns the target operating system name for frontend feature gates.
pub fn backend_current_os() -> Value {
    json!({
        "os": std::env::consts::OS
    })
}

/// Lists system font family names visible to Skia.
pub fn backend_list_system_fonts() -> Value {
    let mut fonts: Vec<String> = FontMgr::default()
        .family_names()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();

    fonts.sort_by_key(|name| name.to_lowercase());
    fonts.dedup_by(|current, next| current.eq_ignore_ascii_case(next));

    Value::Array(fonts.into_iter().map(Value::String).collect())
}

/// Starts a background video render.
///
/// The function returns immediately after validating inputs and registering a
/// render with the controller. Completion, errors, and cancellation are exposed
/// through [`backend_progress`].
pub fn backend_render(
    paths: &AppPaths,
    controller: &RenderController,
    config_json: &str,
    parsed_activity_json: &str,
) -> CoreResult<Value> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    if is_composite_render(&config) {
        return start_composite_render(paths, controller, config, parsed_activity);
    }

    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
    let output_frame_count = rendered_frame_count(
        dense_activity.frame_count,
        config.widget_update_rate() as usize,
    );
    let render_id =
        controller.try_start(output_frame_count as u32, "Preparing render assets...")?;

    let controller_clone = controller.clone();
    let paths = paths.clone();
    std::thread::spawn(move || {
        match render_video(
            &paths,
            &config,
            &parsed_activity,
            &dense_activity,
            &controller_clone,
        ) {
            Ok(filename) => controller_clone.finish_success(filename),
            Err(error) => {
                let cancelled = matches!(error, CoreError::Cancelled);
                controller_clone.finish_error(error.to_string(), cancelled);
            }
        }
    });

    Ok(json!({
        "started": true,
        "render_id": render_id
    }))
}

/// Returns whether the render config requests MP4 compositing mode.
///
/// Composite mode is intentionally gated only by `composite_video_path` so
/// transparent exports continue through their existing path unchanged.
pub fn is_composite_render(config: &RenderConfig) -> bool {
    // test seam
    config.scene.composite_video_path.is_some()
}

/// Starts the composite render branch after deriving composite timing.
///
/// This branch validates inputs, builds the adjusted dense report, starts
/// progress, and dispatches to the composite pipeline shell.
fn start_composite_render(
    paths: &AppPaths,
    controller: &RenderController,
    mut config: RenderConfig,
    parsed_activity: ParsedActivity,
) -> CoreResult<Value> {
    let plan = derive_composite_render_plan(&config)?;
    apply_composite_scene_timing(&mut config, &plan);
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;

    let output_frame_count = (plan.render_duration * plan.source_fps.as_f64())
        .ceil()
        .max(1.0) as u32;
    let render_id = controller.try_start(output_frame_count, "Compositing video...")?;

    let controller_clone = controller.clone();
    let paths = paths.clone();
    std::thread::spawn(move || {
        match render_composite_video(&CompositeRenderRequest {
            paths: &paths,
            config: &config,
            activity: &parsed_activity,
            dense_activity: &dense_activity,
            controller: &controller_clone,
            composite_video_path: &plan.video_path,
            composite_bitrate: &plan.bitrate,
            composite_sync_offset: plan.sync_offset,
            composite_video_fps_num: plan.source_fps.num,
            composite_video_fps_den: plan.source_fps.den,
            composite_video_duration: plan.video_duration,
            composite_render_duration: Some(plan.render_duration),
            composite_video_trim_start: Some(plan.trim_start),
            composite_widget_update_rate: Some(plan.update_rate),
        }) {
            Ok(filename) => controller_clone.finish_success(filename),
            Err(error) => {
                let cancelled = matches!(error, CoreError::Cancelled);
                controller_clone.finish_error(error.to_string(), cancelled);
            }
        }
    });

    Ok(json!({
        "started": true,
        "render_id": render_id
    }))
}

/// Returns the current render progress snapshot.
pub fn backend_progress(controller: &RenderController) -> RenderProgress {
    controller.progress()
}

/// Requests cancellation of the active render, if one is running.
pub fn backend_cancel(controller: &RenderController) -> Value {
    let had_active_render = controller.cancel();
    json!({
        "success": true,
        "message": if had_active_render {
            "Cancellation requested"
        } else {
            "No active render"
        }
    })
}

/// Lists valid built-in and user templates.
///
/// Built-in templates are de-duplicated by filename, while user templates are
/// exposed with a `user:` id prefix to avoid collisions.
pub fn backend_list_templates(paths: &AppPaths) -> CoreResult<Value> {
    let mut templates = Vec::new();
    let mut seen = BTreeSet::new();

    for dir in &paths.bundled_templates_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !matches!(
                    path.extension().and_then(|value| value.to_str()),
                    Some("json")
                ) {
                    continue;
                }
                let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !seen.insert(filename.to_string()) {
                    continue;
                }
                if let Some(descriptor) = template_descriptor(&path, filename, "built-in", filename)
                {
                    templates.push(descriptor);
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir(&paths.user_templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("json")
            ) {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let id = format!("user:{filename}");
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(descriptor) = template_descriptor(&path, filename, "user", &id) {
                templates.push(descriptor);
            }
        }
    }

    Ok(Value::Array(templates))
}

/// Reads a template by built-in filename, user-prefixed id, or unqualified id.
pub fn backend_get_template(paths: &AppPaths, filename: &str) -> CoreResult<String> {
    let (source, normalized) = parse_template_id(filename);
    let template_path = match source {
        TemplateSource::User => paths.user_template_path(&normalized),
        TemplateSource::BuiltIn => paths.bundled_template_path(&normalized),
        TemplateSource::Any => paths
            .bundled_template_path(&normalized)
            .or_else(|| paths.user_template_path(&normalized)),
    }
    .ok_or_else(|| CoreError::Config(format!("Template not found: {normalized}")))?;

    fs::read_to_string(&template_path).map_err(|error| CoreError::Io {
        path: template_path.clone(),
        source: error,
    })
}

/// Opens the public downloads directory in the system file browser.
pub fn backend_open_downloads(paths: &AppPaths) -> CoreResult<Value> {
    open_path_in_system(&paths.downloads_dir)?;
    Ok(json!({ "message": "Folder opened" }))
}

/// Opens a rendered video from the downloads directory.
pub fn backend_open_video(paths: &AppPaths, filename: &str) -> CoreResult<Value> {
    let downloads_path = paths.downloads_dir.join(filename);
    let target = if downloads_path.is_file() {
        downloads_path
    } else {
        return Err(CoreError::Config(format!(
            "Video file not found: {filename}"
        )));
    };

    open_path_in_system(&target)?;
    Ok(json!({ "message": "Video opened" }))
}

#[derive(Clone, Copy)]
enum TemplateSource {
    Any,
    BuiltIn,
    User,
}

// Parses a template id into its source namespace and sanitized filename.
fn parse_template_id(template_id: &str) -> (TemplateSource, String) {
    // Prefixes are part of the frontend template list API. Unprefixed ids keep
    // backward compatibility by searching built-ins first, then user files.
    let (source, filename) = if let Some(filename) = template_id.strip_prefix("user:") {
        (TemplateSource::User, filename)
    } else if let Some(filename) = template_id.strip_prefix("built-in:") {
        (TemplateSource::BuiltIn, filename)
    } else {
        (TemplateSource::Any, template_id)
    };

    (source, ensure_json_filename(filename))
}

// Normalizes a template filename and ensures it has a `.json` extension.
fn ensure_json_filename(filename: &str) -> String {
    // Strip any directory components before resolving inside template roots.
    // This prevents traversal while still accepting user-provided bare names.
    let safe_filename = Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("template.json");

    if safe_filename.ends_with(".json") {
        safe_filename.to_string()
    } else {
        format!("{safe_filename}.json")
    }
}

// Reads and parses a template JSON file, returning `None` for invalid files.
fn read_template_file(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

// Returns whether a parsed JSON document is an OVRLEY template.
fn is_app_template(value: &Value) -> bool {
    value
        .get("format")
        .and_then(Value::as_str)
        .map(|format| format == "ovrley-template")
        .unwrap_or(false)
}

// Extracts the declared scene resolution from a template JSON document.
fn read_template_resolution(value: &Value) -> Option<(u64, u64)> {
    let scene = value.get("config").and_then(|config| config.get("scene"))?;
    let width = scene.get("width").and_then(Value::as_u64)?;
    let height = scene.get("height").and_then(Value::as_u64)?;

    Some((width, height))
}

// Builds the frontend list descriptor for one valid template file.
fn template_descriptor(
    path: &Path,
    filename: &str,
    template_type: &str,
    id: &str,
) -> Option<Value> {
    // Invalid or non-OVRLEY JSON files are silently skipped so users can keep
    // unrelated notes in the same Documents/OVRLEY directory.
    let value = read_template_file(path)?;
    if !is_app_template(&value) {
        return None;
    }
    let (width, height) = read_template_resolution(&value).unwrap_or((0, 0));

    Some(json!({
        "id": id,
        "name": filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        "type": template_type,
        "width": width,
        "height": height
    }))
}

// Opens a filesystem path with the operating system's default file launcher.
fn open_path_in_system(path: &Path) -> CoreResult<()> {
    // Use native platform launchers rather than shell invocation so paths with
    // spaces are passed as arguments and not interpreted as command text.
    if cfg!(windows) {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| {
                CoreError::Encode(format!("Failed to open {}: {error}", path.display()))
            })?;
        return Ok(());
    }

    let mut command = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd
    } else {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path);
        cmd
    };

    let status = command.status().map_err(|error| {
        CoreError::Encode(format!("Failed to open {}: {error}", path.display()))
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(CoreError::Encode(format!(
            "Failed to open {}",
            path.display()
        )))
    }
}

/// Probes a video file and returns its metadata.
pub fn backend_probe_video(paths: &AppPaths, file_path: &str) -> CoreResult<Value> {
    use crate::encode::video_probe::probe_video;
    let metadata = probe_video(&paths.repo_root, file_path)?;
    serde_json::to_value(&metadata).map_err(CoreError::Serialization)
}

/// Detects ffmpeg encoders and hardware acceleration methods available locally.
pub fn backend_detect_codecs(paths: &AppPaths) -> CoreResult<Value> {
    use crate::encode::codec_detect::detect_codecs;
    let codecs = detect_codecs(&paths.repo_root)?;
    serde_json::to_value(&codecs).map_err(CoreError::Serialization)
}
