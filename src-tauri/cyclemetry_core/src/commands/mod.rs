use crate::activity::{build_dense_activity_report, parse_activity_json};
use crate::config::parse_config_json;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::render::{render_preview_to_path, stub_demo_response, stub_render_response};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub repo_root: PathBuf,
    pub backend_dir: PathBuf,
    pub font_dirs: Vec<PathBuf>,
    pub public_dir: PathBuf,
    pub uploads_dir: PathBuf,
    pub user_templates_dir: PathBuf,
    pub bundled_templates_dirs: Vec<PathBuf>,
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    pub fn from_repo_root(repo_root: PathBuf) -> Self {
        let backend_dir = repo_root.join("backend");
        let font_dirs = vec![repo_root.join("fonts"), backend_dir.join("fonts")]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        let public_dir = backend_dir.join("public");
        let uploads_dir = backend_dir.join("uploads");
        let user_templates_dir = backend_dir.join("templates");
        let bundled_templates_dirs =
            vec![repo_root.join("templates"), backend_dir.join("templates")]
                .into_iter()
                .filter(|path| path.is_dir())
                .collect();
        let downloads_dir = downloads_cyclemetry_dir();

        Self {
            repo_root,
            backend_dir,
            font_dirs,
            public_dir,
            uploads_dir,
            user_templates_dir,
            bundled_templates_dirs,
            downloads_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), String> {
        for dir in [
            &self.backend_dir,
            &self.public_dir,
            &self.uploads_dir,
            &self.user_templates_dir,
            &self.downloads_dir,
        ] {
            fs::create_dir_all(dir)
                .map_err(|error| format!("Failed to create {}: {error}", dir.display()))?;
        }
        Ok(())
    }

    pub fn bundled_template_path(&self, filename: &str) -> Option<PathBuf> {
        self.bundled_templates_dirs
            .iter()
            .map(|dir| dir.join(filename))
            .find(|path| path.is_file())
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub message: String,
    pub ready: bool,
}

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

pub fn backend_demo(
    paths: &AppPaths,
    config_json: &str,
    parsed_activity_json: &str,
    second: u32,
) -> Result<Value, String> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
    let filename = "demo_preview.png";
    let output_path = paths.public_dir.join(filename);
    render_preview_to_path(paths, &config, &dense_activity, second, &output_path)?;
    Ok(stub_demo_response(filename))
}

pub fn backend_render(config_json: &str, parsed_activity_json: &str) -> Result<Value, String> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
    Ok(stub_render_response(&config, &dense_activity))
}

pub fn backend_progress(progress: &RenderProgress) -> RenderProgress {
    progress.clone()
}

pub fn backend_cancel(progress: &mut RenderProgress) -> Value {
    progress.status = "cancelled".to_string();
    progress.message = "No active render. Phase 1 keeps rendering stubbed.".to_string();
    json!({
        "success": true,
        "message": progress.message
    })
}

pub fn backend_load_gpx(paths: &AppPaths, source_path: &str) -> Result<Value, String> {
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err(format!("File not found: {}", source.display()));
    }

    let filename = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid source filename".to_string())?;

    let destination = paths.uploads_dir.join(filename);
    fs::copy(&source, &destination)
        .map_err(|error| format!("Failed to copy {}: {error}", source.display()))?;

    Ok(json!({
        "data": "file loaded",
        "filename": filename
    }))
}

pub fn backend_upload(paths: &AppPaths, file_data: &[u8], filename: &str) -> Result<Value, String> {
    if filename.trim().is_empty() {
        return Err("filename is required".to_string());
    }

    let destination = paths.uploads_dir.join(filename);
    fs::write(&destination, file_data)
        .map_err(|error| format!("Failed to write {}: {error}", destination.display()))?;

    Ok(json!({
        "data": "file uploaded",
        "filename": filename
    }))
}

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
                templates.push(template_descriptor(filename, "built-in"));
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
            if seen.contains(filename) {
                continue;
            }
            templates.push(template_descriptor(filename, "user"));
        }
    }

    Ok(Value::Array(templates))
}

pub fn backend_get_template(paths: &AppPaths, filename: &str) -> Result<String, String> {
    let normalized = ensure_json_filename(filename);
    let user_path = paths.user_templates_dir.join(&normalized);
    if user_path.is_file() {
        return fs::read_to_string(&user_path)
            .map_err(|error| format!("Failed to read {}: {error}", user_path.display()));
    }

    let bundled_path = paths
        .bundled_template_path(&normalized)
        .ok_or_else(|| format!("Template not found: {normalized}"))?;

    fs::read_to_string(&bundled_path)
        .map_err(|error| format!("Failed to read {}: {error}", bundled_path.display()))
}

pub fn backend_save_template(
    paths: &AppPaths,
    filename: &str,
    config_json: &str,
) -> Result<Value, String> {
    let normalized = ensure_json_filename(filename);
    let config = parse_config_json(config_json)?;
    let destination = paths.user_templates_dir.join(&normalized);
    let pretty = serde_json::to_string_pretty(&config)
        .map_err(|error| format!("Failed to serialize template: {error}"))?;
    fs::write(&destination, pretty)
        .map_err(|error| format!("Failed to write {}: {error}", destination.display()))?;

    Ok(json!({
        "message": format!("Template saved to {normalized}"),
        "filename": normalized
    }))
}

pub fn backend_open_templates(paths: &AppPaths) -> Result<Value, String> {
    open_path_in_system(&paths.user_templates_dir)?;
    Ok(json!({ "message": "Templates folder opened" }))
}

pub fn backend_open_downloads(paths: &AppPaths) -> Result<Value, String> {
    open_path_in_system(&paths.downloads_dir)?;
    Ok(json!({ "message": "Folder opened" }))
}

pub fn backend_open_video(paths: &AppPaths, filename: &str) -> Result<Value, String> {
    let public_path = paths.public_dir.join(filename);
    let downloads_path = paths.downloads_dir.join(filename);
    let target = if public_path.is_file() {
        public_path
    } else if downloads_path.is_file() {
        downloads_path
    } else {
        return Err(format!("Video file not found: {filename}"));
    };

    open_path_in_system(&target)?;
    Ok(json!({ "message": "Video opened" }))
}

pub fn backend_image_data(paths: &AppPaths, filename: &str) -> Result<String, String> {
    let path = paths.public_dir.join(filename);
    let bytes =
        fs::read(&path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    let content_type = match path.extension().and_then(|value| value.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    };
    Ok(format!(
        "data:{};base64,{}",
        content_type,
        base64_encode(&bytes)
    ))
}

fn ensure_json_filename(filename: &str) -> String {
    if filename.ends_with(".json") {
        filename.to_string()
    } else {
        format!("{filename}.json")
    }
}

fn template_descriptor(filename: &str, template_type: &str) -> Value {
    json!({
        "id": filename,
        "name": filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        "type": template_type
    })
}

fn downloads_cyclemetry_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads").join("Cyclemetry")
}

fn open_path_in_system(path: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd
    } else if cfg!(windows) {
        let mut cmd = Command::new("explorer");
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

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut chunks = bytes.chunks_exact(3);

    for chunk in &mut chunks {
        let block = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        output.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        output.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
        output.push(TABLE[(block & 0x3f) as usize] as char);
    }

    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let first = remainder[0] as u32;
        let second = remainder.get(1).copied().unwrap_or_default() as u32;
        let block = (first << 16) | (second << 8);
        output.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        if remainder.len() == 2 {
            output.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
            output.push('=');
        } else {
            output.push('=');
            output.push('=');
        }
    }

    output
}
