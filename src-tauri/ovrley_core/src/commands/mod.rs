use crate::activity::{build_dense_activity_report, parse_activity_json};
use crate::config::parse_config_json;
use crate::debug::RenderProgress;
use crate::encode::ffmpeg::resolve_ffmpeg_binary;
use crate::encode::video_pipeline::rendered_frame_count;
use crate::encode::video::{render_video, RenderController};
use crate::render::{render_preview_to_path, stub_demo_response};
use serde::Serialize;
use serde_json::{json, Value};
use skia_safe::FontMgr;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub repo_root: PathBuf,
    pub font_dirs: Vec<PathBuf>,
    pub debug_render_dir: PathBuf,
    pub preview_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub bundled_templates_dirs: Vec<PathBuf>,
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    pub fn from_repo_root(repo_root: PathBuf) -> Self {
        Self::from_roots(repo_root.clone(), repo_root)
    }

    pub fn from_resource_root(repo_root: PathBuf, resource_root: PathBuf) -> Self {
        Self::from_roots(repo_root, resource_root)
    }

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
        let preview_dir = runtime_dir.join("previews");
        let temp_dir = runtime_dir.join("tmp");
        let bundled_templates_dirs =
            vec![resource_root.join("templates"), repo_root.join("templates")]
                .into_iter()
                .filter(|path| path.is_dir())
                .collect();

        Self {
            repo_root: resource_root,
            font_dirs,
            debug_render_dir,
            preview_dir,
            temp_dir,
            bundled_templates_dirs,
            downloads_dir,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), String> {
        for dir in [
            &self.debug_render_dir,
            &self.preview_dir,
            &self.temp_dir,
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

pub fn backend_current_os() -> Value {
    json!({
        "os": std::env::consts::OS
    })
}

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
    let output_path = paths.preview_dir.join(filename);
    render_preview_to_path(
        paths,
        &config,
        &parsed_activity,
        &dense_activity,
        second,
        &output_path,
    )?;
    Ok(stub_demo_response(filename))
}

pub fn backend_render(
    paths: &AppPaths,
    controller: &RenderController,
    config_json: &str,
    parsed_activity_json: &str,
) -> Result<Value, String> {
    let config = parse_config_json(config_json)?;
    let parsed_activity = parse_activity_json(parsed_activity_json)?;
    let dense_activity = build_dense_activity_report(&parsed_activity, &config)?;
    let output_frame_count =
        rendered_frame_count(dense_activity.frame_count, config.widget_update_rate() as usize);
    let render_id = controller.try_start(
        output_frame_count as u32,
        "Preparing render assets...",
    )?;

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

pub fn backend_progress(controller: &RenderController) -> RenderProgress {
    controller.progress()
}

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
                templates.push(template_descriptor(&path, filename, "built-in"));
            }
        }
    }

    Ok(Value::Array(templates))
}

pub fn backend_get_template(paths: &AppPaths, filename: &str) -> Result<String, String> {
    let normalized = ensure_json_filename(filename);
    let bundled_path = paths
        .bundled_template_path(&normalized)
        .ok_or_else(|| format!("Template not found: {normalized}"))?;

    fs::read_to_string(&bundled_path)
        .map_err(|error| format!("Failed to read {}: {error}", bundled_path.display()))
}

pub fn backend_open_downloads(paths: &AppPaths) -> Result<Value, String> {
    open_path_in_system(&paths.downloads_dir)?;
    Ok(json!({ "message": "Folder opened" }))
}

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

pub fn backend_image_data(paths: &AppPaths, filename: &str) -> Result<String, String> {
    let preview_path = paths.preview_dir.join(filename);
    let downloads_path = paths.downloads_dir.join(filename);
    let path = if preview_path.is_file() {
        preview_path
    } else if downloads_path.is_file() {
        downloads_path
    } else {
        return Err(format!("Image file not found: {filename}"));
    };
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

fn read_template_resolution(path: &Path) -> Option<(u64, u64)> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let scene = value
        .get("config")
        .and_then(|config| config.get("scene"))
        .or_else(|| value.get("scene"))?;
    let width = scene.get("width").and_then(Value::as_u64)?;
    let height = scene.get("height").and_then(Value::as_u64)?;

    Some((width, height))
}

fn template_descriptor(path: &Path, filename: &str, template_type: &str) -> Value {
    let (width, height) = read_template_resolution(path).unwrap_or((0, 0));

    json!({
        "id": filename,
        "name": filename.trim_end_matches(".json").replace('_', " ").to_uppercase(),
        "type": template_type,
        "width": width,
        "height": height
    })
}

fn downloads_ovrley_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Downloads").join("OVRLEY")
}

fn open_path_in_system(path: &Path) -> Result<(), String> {
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
