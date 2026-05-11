//! Backend command implementations used by the Tauri shell.
//!
//! Functions in this module are framework-agnostic: they accept plain strings,
//! paths, and controller references so the Tauri command layer can delegate here
//! without mixing app-window concerns into render logic. Responsibilities include
//! runtime path resolution, template IO, video render startup, progress/cancel
//! plumbing, and small OS integration helpers.

use crate::activity::{build_dense_activity_report, parse_activity_json};
use crate::config::parse_config_json;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::video::{render_video, RenderController};
use crate::encode::video_pipeline::rendered_frame_count;
use serde::Serialize;
use serde_json::{json, Value};
use skia_safe::FontMgr;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Filesystem locations used by backend operations.
///
/// Paths are derived differently in development and packaged builds, but callers
/// receive one normalized structure so rendering, templates, fonts, temporary
/// files, and downloads are always addressed consistently.
#[derive(Clone, Debug)]
pub struct AppPaths {
    /// Root used for bundled runtime assets such as vendor ffmpeg.
    pub repo_root: PathBuf,
    /// Existing font directories searched before system fonts.
    pub font_dirs: Vec<PathBuf>,
    /// Directory for timing summaries and sample-frame debug artifacts.
    pub debug_render_dir: PathBuf,
    /// Directory for short-lived files such as ffmpeg concat lists.
    pub temp_dir: PathBuf,
    /// Built-in template directories, searched in precedence order.
    pub bundled_templates_dirs: Vec<PathBuf>,
    /// User template directory under Documents/OVRLEY.
    pub user_templates_dir: PathBuf,
    /// Public output directory under Downloads/OVRLEY.
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    /// Builds development paths when repo and resources share the same root.
    pub fn from_repo_root(repo_root: PathBuf) -> Self {
        Self::from_roots(repo_root.clone(), repo_root)
    }

    /// Builds packaged-app paths from separate repository and resource roots.
    pub fn from_resource_root(repo_root: PathBuf, resource_root: PathBuf) -> Self {
        Self::from_roots(repo_root, resource_root)
    }

    /// Builds all backend runtime paths from repository and resource roots.
    fn from_roots(repo_root: PathBuf, resource_root: PathBuf) -> Self {
        let downloads_dir = downloads_ovrley_dir();
        let runtime_dir = downloads_dir.join(".runtime");
        let font_dirs = vec![resource_root.join("fonts"), repo_root.join("fonts")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        // Keep dev debug artifacts under src-tauri/target so `tauri dev`
        // source watchers do not restart the app on every render write.
        let debug_render_dir = if resource_root == repo_root {
            repo_root
                .join("src-tauri")
                .join("target")
                .join("debug_render")
        } else {
            runtime_dir.join("debug_render")
        };
        let temp_dir = runtime_dir.join("tmp");
        let bundled_templates_dirs =
            vec![resource_root.join("templates"), repo_root.join("templates")]
                .into_iter()
                .filter(|path| path.is_dir())
                .collect();
        let user_templates_dir = documents_ovrley_dir();

        Self {
            repo_root: resource_root,
            font_dirs,
            debug_render_dir,
            temp_dir,
            bundled_templates_dirs,
            user_templates_dir,
            downloads_dir,
        }
    }

    /// Ensures all runtime-writable directories exist.
    pub fn ensure_dirs(&self) -> Result<(), String> {
        for dir in [
            &self.debug_render_dir,
            &self.temp_dir,
            &self.user_templates_dir,
            &self.downloads_dir,
        ] {
            fs::create_dir_all(dir)
                .map_err(|error| format!("Failed to create {}: {error}", dir.display()))?;
        }
        Ok(())
    }

    /// Returns the first built-in template path matching `filename`.
    pub fn bundled_template_path(&self, filename: &str) -> Option<PathBuf> {
        self.bundled_templates_dirs
            .iter()
            .map(|dir| dir.join(filename))
            .find(|path| path.is_file())
    }

    /// Returns a user template path if it exists.
    pub fn user_template_path(&self, filename: &str) -> Option<PathBuf> {
        let path = self.user_templates_dir.join(filename);
        path.is_file().then_some(path)
    }
}

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
) -> Result<Value, String> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
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
                let cancelled = error.to_lowercase().contains("cancelled");
                controller_clone.finish_error(error, cancelled);
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
pub fn backend_list_templates(paths: &AppPaths) -> Result<Value, String> {
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
pub fn backend_get_template(paths: &AppPaths, filename: &str) -> Result<String, String> {
    let (source, normalized) = parse_template_id(filename);
    let template_path = match source {
        TemplateSource::User => paths.user_template_path(&normalized),
        TemplateSource::BuiltIn => paths.bundled_template_path(&normalized),
        TemplateSource::Any => paths
            .bundled_template_path(&normalized)
            .or_else(|| paths.user_template_path(&normalized)),
    }
    .ok_or_else(|| format!("Template not found: {normalized}"))?;

    fs::read_to_string(&template_path)
        .map_err(|error| format!("Failed to read {}: {error}", template_path.display()))
}

/// Opens the public downloads directory in the system file browser.
pub fn backend_open_downloads(paths: &AppPaths) -> Result<Value, String> {
    open_path_in_system(&paths.downloads_dir)?;
    Ok(json!({ "message": "Folder opened" }))
}

/// Opens a rendered video from the downloads directory.
pub fn backend_open_video(paths: &AppPaths, filename: &str) -> Result<Value, String> {
    let downloads_path = paths.downloads_dir.join(filename);
    let target = if downloads_path.is_file() {
        downloads_path
    } else {
        return Err(format!("Video file not found: {filename}"));
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

// Returns the user-writable OVRLEY documents directory.
fn documents_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("OVRLEY")
}

// Returns the public OVRLEY downloads/output directory.
fn downloads_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads").join("OVRLEY")
}

// Opens a filesystem path with the operating system's default file launcher.
fn open_path_in_system(path: &Path) -> Result<(), String> {
    // Use native platform launchers rather than shell invocation so paths with
    // spaces are passed as arguments and not interpreted as command text.
    if cfg!(windows) {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
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

    let status = command
        .status()
        .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Failed to open {}", path.display()))
    }
}

/// Probes a video file and returns its metadata.
pub fn backend_probe_video(paths: &AppPaths, file_path: &str) -> Result<Value, String> {
    use crate::encode::video_probe::probe_video;
    let metadata = probe_video(&paths.repo_root, file_path)?;
    serde_json::to_value(&metadata).map_err(|e| format!("Serialization error: {}", e))
}

/// Detects ffmpeg encoders and hardware acceleration methods available locally.
pub fn backend_detect_codecs(paths: &AppPaths) -> Result<Value, String> {
    use crate::encode::codec_detect::detect_codecs;
    let codecs = detect_codecs(&paths.repo_root)?;
    serde_json::to_value(&codecs).map_err(|e| format!("Serialization error: {}", e))
}
