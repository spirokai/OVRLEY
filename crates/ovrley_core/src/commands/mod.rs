//! Core operations the app calls directly: templates, the preview frame, video telemetry, and
//! opening render outputs in the system.
//!
//! Production rendering, progress and cancellation belong to `render_jobs::execution`.

use crate::activity::build_dense_activity_report_validated;
use crate::activity::finalize::FinalizeActivityResponse;
use crate::error::{CoreError, CoreResult};
use crate::normalize::{parse_config_json, parse_template_json};
use crate::paths::AppPaths;
use crate::render::render_preview_frame;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Whether a listed template is bundled or the user's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateKind {
    BuiltIn,
    User,
}

/// One listed template.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateDescriptor {
    pub id: String,
    pub name: String,
    pub kind: TemplateKind,
    /// The scene's size, 0 when the file declares none.
    pub width: u32,
    pub height: u32,
    /// User templates only.
    pub path: Option<String>,
}

/// Renders the frame at `second` of a render config to a transparent PNG in Downloads and returns
/// its path.
pub fn render_preview_frame_config(
    paths: &AppPaths,
    config: crate::normalize::RenderConfig,
    parsed_activity: &crate::activity::schema::ParsedActivity,
    second: f64,
    raster_resources: Option<&dyn crate::raster::RasterResourceResolver>,
    map_source: Option<&ovrley_map::gpu::MapSource>,
) -> CoreResult<PathBuf> {
    let mut validated =
        crate::normalize::validate_render_config_with_resources(config, raster_resources)?;
    validated.scene.narrow_to_frame(second);
    let dense_activity = build_dense_activity_report_validated(parsed_activity, &validated)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Encode(format!("Failed to read system time: {error}")))?
        .as_nanos();
    let second_label = format!("{second:.3}").replace('.', "_");
    let filename = format!("preview_frame_{timestamp}_t{second_label}.png");
    let output_path = paths.downloads_dir.join(&filename);
    render_preview_frame(
        paths,
        &validated,
        parsed_activity,
        &dense_activity,
        second,
        &output_path,
        map_source,
    )?;
    Ok(output_path)
}

/// Lists valid built-in and user templates.
///
/// Built-in templates are de-duplicated by filename, while user templates are
/// exposed with a `user:` id prefix to avoid collisions.
pub fn list_templates(paths: &AppPaths) -> Vec<TemplateDescriptor> {
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
                if let Some(descriptor) = template_descriptor(&path, filename, TemplateKind::BuiltIn, filename)
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
            if let Some(descriptor) = template_descriptor(&path, filename, TemplateKind::User, &id) {
                templates.push(descriptor);
            }
        }
    }

    templates
}

/// Reads a template's document by built-in filename, user-prefixed id, or unqualified id.
pub fn read_template(paths: &AppPaths, filename: &str) -> CoreResult<Value> {
    let (source, normalized) = parse_template_id(filename);
    let template_path = match source {
        TemplateSource::User => paths.user_template_path(&normalized),
        TemplateSource::BuiltIn => paths.bundled_template_path(&normalized),
        TemplateSource::Any => paths
            .bundled_template_path(&normalized)
            .or_else(|| paths.user_template_path(&normalized)),
    }
    .ok_or_else(|| CoreError::Config(format!("Template not found: {normalized}")))?;

    let text = fs::read_to_string(&template_path).map_err(|error| CoreError::Io {
        path: template_path.clone(),
        source: error,
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Opens the render output folder in the system file browser: `directory` (absolute, existing),
/// else Downloads.
pub fn open_output_directory(paths: &AppPaths, directory: Option<&str>) -> CoreResult<()> {
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
    open_path_in_system(&target)
}

/// Opens a rendered file (a video, or the debug PNG frame) in the system player.
pub fn open_rendered_video(output_path: &str) -> CoreResult<()> {
    let target = Path::new(output_path);
    if !target.is_absolute() || !target.is_file() {
        return Err(CoreError::Config(format!(
            "Video file not found: {output_path}"
        )));
    }

    open_path_in_system(target)
}

#[derive(Clone, Copy)]
enum TemplateSource {
    Any,
    BuiltIn,
    User,
}

// Parses a template id into its source namespace and sanitized filename.
fn parse_template_id(template_id: &str) -> (TemplateSource, String) {
    // Prefixes are part of the template list's ids. Unprefixed ids keep
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
fn read_template_resolution(value: &Value) -> Option<(u32, u32)> {
    let scene = value.get("config").and_then(|config| config.get("scene"))?;
    let width = u32::try_from(scene.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(scene.get("height")?.as_u64()?).ok()?;

    Some((width, height))
}

// Builds the list entry of one valid template file.
fn template_descriptor(
    path: &Path,
    filename: &str,
    kind: TemplateKind,
    id: &str,
) -> Option<TemplateDescriptor> {
    // Invalid or non-OVRLEY JSON files are silently skipped so users can keep
    // unrelated notes in the same Documents/OVRLEY directory.
    let value = read_template_file(path)?;
    if !is_app_template(&value) {
        return None;
    }
    let (width, height) = read_template_resolution(&value).unwrap_or((0, 0));

    Some(TemplateDescriptor {
        id: id.to_string(),
        name: filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        kind,
        width,
        height,
        path: (kind == TemplateKind::User).then(|| path.to_string_lossy().into_owned()),
    })
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

/// Extracts embedded MP4 telemetry as a parsed activity, the shape every import shares.
pub fn extract_video_telemetry(
    paths: &AppPaths,
    file_path: &str,
) -> CoreResult<Option<FinalizeActivityResponse>> {
    let mut response = crate::media::mp4_telemetry::extract_activity(&paths.repo_root, file_path)?;
    if let Some(response) = &mut response {
        response.debug_payload = None;
    }
    Ok(response)
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

/// Writes a user template file after validating `contents`, creating its folder as needed.
pub fn write_template(path: &Path, contents: &str) -> CoreResult<()> {
    validate_template_contents(contents)?;
    let io = |source| CoreError::Io { path: path.to_path_buf(), source };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    fs::write(path, contents).map_err(io)
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
