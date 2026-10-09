//! Backend command implementations used by the Tauri shell.
//!
//! Framework-agnostic template, activity, preview and OS helpers. Production
//! rendering, progress and cancellation belong to `render_jobs::execution`.

pub mod elevation_geometry;
pub mod route_geometry;

use crate::activity::finalize::FinalizeActivityResponse;
use crate::activity::{build_dense_activity_report_validated, parse_activity_json};
use crate::encode::ffmpeg::binary::resolve_ffmpeg_binary;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{parse_config_json, parse_template_json};
use crate::output::RenderOutputKind;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths::AppPaths;
use crate::render::render_preview_to_path;

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

/// Finalizes frontend-extracted raw activity samples into the canonical payload.
///
/// The core command owns JSON serialization at the framework-agnostic boundary
/// so the Tauri layer can stay a thin string-in/string-out adapter while tests
/// exercise the same backend finalization path.
pub fn backend_finalize_activity(paths: &AppPaths, raw_activity_json: &str) -> CoreResult<Value> {
    serde_json::to_value(crate::activity::finalize::finalize_raw_activity_json(
        raw_activity_json,
        Some(&paths.repo_root),
    )?)
    .map_err(CoreError::Serialization)
}

/// Parses and finalizes a native CSV activity without a frontend RawActivity hop.
pub fn backend_parse_csv_activity(
    paths: &AppPaths,
    path: &str,
) -> CoreResult<FinalizeActivityResponse> {
    crate::activity::csv::parse_csv_activity_path(Path::new(path), Some(&paths.repo_root))
}

/// Parses and finalizes a native VBO activity without a frontend RawActivity hop.
pub fn backend_parse_vbo_activity(
    paths: &AppPaths,
    path: &str,
) -> CoreResult<FinalizeActivityResponse> {
    let mut response =
        crate::activity::vbo::parse_vbo_activity_path(Path::new(path), Some(&paths.repo_root))?;
    response.debug_payload = None;
    Ok(response)
}

/// Parses and finalizes a native TCX activity through the shared raw-sample pipeline.
pub fn backend_parse_tcx_activity(
    paths: &AppPaths,
    path: &str,
) -> CoreResult<FinalizeActivityResponse> {
    let mut response =
        crate::activity::tcx::parse_tcx_activity_path(Path::new(path), Some(&paths.repo_root))?;
    response.debug_payload = None;
    Ok(response)
}

/// Lists canonical bundled capabilities and lazy system font identities.
pub fn backend_list_system_fonts(paths: &AppPaths) -> CoreResult<Value> {
    Ok(serde_json::to_value(crate::fonts::font_catalog(
        &paths.font_dirs,
    )?)?)
}

/// Resolves the selected font's actual faces, including italic availability.
pub fn backend_font_capabilities(paths: &AppPaths, font_id: &str) -> CoreResult<Value> {
    Ok(serde_json::to_value(crate::fonts::font_capabilities(
        &paths.font_dirs,
        font_id,
    )?)?)
}

/// Supplies the session's bundled face bytes for browser font registration.
pub fn backend_font_data(
    paths: &AppPaths,
    font_id: &str,
    face_index: usize,
) -> CoreResult<Vec<u8>> {
    crate::fonts::bundled_face_data(&paths.font_dirs, font_id, face_index)
}

/// Renders one transparent preview PNG for the requested second.
///
/// The file is written into the public downloads directory so it is easy to
/// inspect from the desktop app during development workflows.
pub fn backend_render_preview_frame(
    paths: &AppPaths,
    config_json: &str,
    parsed_activity_json: &str,
    second: f64,
    raster_resources: Option<&dyn crate::raster::RasterResourceResolver>,
) -> CoreResult<Value> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    let validated =
        crate::normalize::validate_render_config_with_resources(config, raster_resources)?;
    let dense_activity = build_dense_activity_report_validated(&parsed_activity, &validated)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Encode(format!("Failed to read system time: {error}")))?
        .as_nanos();
    let second_label = format!("{second:.3}").replace('.', "_");
    let filename = format!("preview_frame_{timestamp}_t{second_label}.png");
    let output_path = paths.downloads_dir.join(&filename);
    render_preview_to_path(
        paths,
        &validated,
        &parsed_activity,
        &dense_activity,
        second,
        &output_path,
    )?;

    Ok(json!({
        "filename": filename,
        "path": output_path,
        "second": second
    }))
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

/// Opens the remembered render output directory, or the platform default.
pub fn backend_open_output_directory(
    paths: &AppPaths,
    directory: Option<&str>,
) -> CoreResult<Value> {
    let target = match directory {
        None => paths.downloads_dir.clone(),
        Some(directory) => {
            let path = Path::new(directory);
            if !path.is_absolute() {
                return Err(CoreError::Config(format!(
                    "Render output directory must be absolute: {directory}"
                )));
            }
            if !path.is_dir() {
                return Err(CoreError::Config(format!(
                    "Render output directory is not available: {directory}"
                )));
            }
            path.to_path_buf()
        }
    };
    open_path_in_system(&target)?;
    Ok(json!({ "message": "Folder opened" }))
}

/// Returns a fresh suggested render output path.
pub fn backend_suggest_output_path(
    paths: &AppPaths,
    output_kind: RenderOutputKind,
    remembered_directory: Option<&str>,
) -> CoreResult<PathBuf> {
    let remembered_directory = remembered_directory.map(Path::new);
    crate::output::suggest_output_path(paths, output_kind, remembered_directory)
}

/// Opens the user templates directory in the system file browser.
pub fn backend_open_templates(paths: &AppPaths) -> CoreResult<Value> {
    open_path_in_system(&paths.user_templates_dir)?;
    Ok(json!({ "message": "Folder opened" }))
}

/// Opens a rendered video from its accepted absolute path.
pub fn backend_open_video(output_path: &str) -> CoreResult<Value> {
    let target = Path::new(output_path);
    if !target.is_absolute() || !target.is_file() {
        return Err(CoreError::Config(format!(
            "Video file not found: {output_path}"
        )));
    }

    open_path_in_system(target)?;
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

    let mut descriptor = json!({
        "id": id,
        "name": filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        "type": template_type,
        "width": width,
        "height": height
    });
    if template_type == "user" {
        descriptor["path"] = Value::String(path.to_string_lossy().into_owned());
    }
    Some(descriptor)
}

// Opens a path with the operating system's default handler.
//
// Uses the `open` crate which delegates to ShellExecuteW on Windows,
// NSWorkspace on macOS, and xdg-open/gnome-open/etc on Linux — all
// without blocking the caller.
fn open_path_in_system(path: &Path) -> CoreResult<()> {
    open::that(path)
        .map_err(|error| CoreError::Encode(format!("Failed to open {}: {error}", path.display())))
}

/// Probes a video file and returns its metadata.
pub fn backend_probe_video(paths: &AppPaths, file_path: &str) -> CoreResult<Value> {
    let metadata = crate::media::prepared_video::probe_video_metadata(paths, file_path)?;
    serde_json::to_value(&metadata).map_err(CoreError::Serialization)
}

/// Extracts embedded MP4 telemetry as a parsed activity payload.
///
/// This command is kept as frontend wiring for video imports. It deliberately
/// returns the same [`ParsedActivity`] shape used by the rest of the import
/// pipeline, not the old debug-only columnar telemetry JSON.
pub fn backend_extract_video_telemetry(
    paths: &AppPaths,
    file_path: &str,
) -> CoreResult<Option<FinalizeActivityResponse>> {
    let mut response = crate::media::mp4_telemetry::extract_activity(&paths.repo_root, file_path)?;
    if let Some(response) = &mut response {
        response.debug_payload = None;
    }
    Ok(response)
}

/// Detects ffmpeg encoders and hardware acceleration methods available locally.
pub fn backend_detect_codecs(paths: &AppPaths) -> CoreResult<Value> {
    use crate::encode::ffmpeg::detect::detect_codecs;
    let codecs = detect_codecs(&paths.repo_root)?;
    serde_json::to_value(&codecs).map_err(CoreError::Serialization)
}

/// Validates template contents without exposing raw config types.
///
/// Parses the template JSON, validates format/version, and runs the full
/// normalization seam. Returns `Ok(())` if valid, or a descriptive error.
/// This is the single entry point for write-time validation — callers never
/// see `RenderConfig` or `ValidatedRenderConfig`.
pub fn validate_template_contents(input: &str) -> CoreResult<()> {
    let mut config = parse_template_json(input)?;
    crate::normalize::validate_template_rasters(&config.rasters)?;
    config.rasters.clear();
    crate::normalize::validate_render_config(config)?;
    Ok(())
}

/// Parses a config JSON string and runs the full normalization seam.
///
/// Returns the validated config ready for rendering. This is the primary
/// entry point for CLI binaries that need to validate and render.
pub fn parse_and_validate_config(
    config_json: &str,
) -> CoreResult<crate::normalize::ValidatedRenderConfig> {
    let config = parse_config_json(config_json)?;
    crate::normalize::validate_render_config(config)
}

/// Validates a pre-built config `Value` through the normalization seam.
///
/// Use this when the caller needs to mutate the config as a `Value` before
/// validation (e.g., injecting ffmpeg CLI overrides).
pub fn validate_config_value(
    config_value: &serde_json::Value,
) -> CoreResult<crate::normalize::ValidatedRenderConfig> {
    let config = crate::normalize::parse_config_value(config_value)?;
    crate::normalize::validate_render_config(config)
}
