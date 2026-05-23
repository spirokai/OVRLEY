//! Tauri application shell — command registration, preview server lifecycle,
//! and platform resource resolution.
//!
//! Owns: Tauri command wiring, `BackendState` management, `AppPaths` resolution
//!       for bundled resources, the `video_server` lifecycle, and the `run()`
//!       entry point that glues the Rust core to the Electron/Tauri window.
//! Does not own: rendering, encoding, activity parsing, or config validation —
//!       those live in `ovrley_core`. This file is the boundary layer.
//!
//! Allowed dependencies: `ovrley_core`, `tauri`, `video_server`.
//! Forbidden dependencies: none (this is the outermost layer — it may import
//!       anything from `ovrley_core` but should not implement domain logic).
//!
//! ## Thread Safety
//! `BackendState` is managed by Tauri as app-level state (Send + Sync via Tauri's
//! `manage`). The `RenderController` inside it is the shared coordination point
//! for all render progress and cancellation. The video server runs on a dedicated
//! thread spawned at startup and joined on app teardown.
//!
//! ## Performance
//! Not a hot path — called once at application startup. Resource path resolution
//! and plugin registration happen before the frontend loads.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub mod video_server; // test seam

use ovrley_core::commands::{self, AppPaths};
use ovrley_core::encode::video::RenderController;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use video_server::VideoServerHandle;

struct BackendState {
    render_controller: RenderController,
}

/// Returns the repository root when running from the Tauri crate.
///
/// `CARGO_MANIFEST_DIR` points at `src-tauri`, so this walks one directory up
/// to the project root used by bundled tooling and development assets.
fn source_repo_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root
}

/// Builds the backend path configuration for the current runtime mode.
///
/// Development builds read resources from the source checkout. Packaged builds
/// read resources from Tauri's resource directory. User templates always live
/// under the user's documents directory in `OVRLEY`.
fn app_paths(app: &AppHandle) -> Result<AppPaths, String> {
    let repo_root = source_repo_root();
    // cfg!() is intentional here, not #[cfg(debug_assertions)]. Both branches
    // must compile because this function serves both debug and release builds.
    // #[cfg] would exclude the release branch during debug compilation, causing
    // compile errors when a debug build of the app runs in dev mode.
    let resource_root = if cfg!(debug_assertions) {
        repo_root.clone()
    } else {
        app.path().resource_dir().map_err(|e| e.to_string())?
    };
    let mut paths = AppPaths::from_resource_root(repo_root, resource_root);
    paths.user_templates_dir = app
        .path()
        .document_dir()
        .map_err(|e| e.to_string())?
        .join("OVRLEY");
    paths.ensure_dirs().map_err(|e| e.to_string())?;
    Ok(paths)
}

/// Returns a basic backend health payload for the frontend runtime check.
#[tauri::command]
async fn backend_health(app: AppHandle) -> Result<String, String> {
    let response = commands::backend_health(&app_paths(&app)?);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Returns the current operating system identifier used for platform-specific UI.
#[tauri::command]
async fn backend_current_os() -> Result<String, String> {
    let response = commands::backend_current_os();
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Lists system fonts available to the backend renderer.
#[tauri::command]
async fn backend_list_system_fonts() -> Result<String, String> {
    let response = commands::backend_list_system_fonts();
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Starts an overlay video render from serialized scene config and activity data.
///
/// The render controller in managed state tracks progress and cancellation for
/// the long-running encoder task.
#[tauri::command]
async fn backend_render(
    app: AppHandle,
    state: tauri::State<'_, BackendState>,
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    let response = commands::backend_render(
        &app_paths(&app)?,
        &state.render_controller,
        &config_json,
        &parsed_activity_json,
    )
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Returns progress for the currently active or most recent render job.
#[tauri::command]
async fn backend_progress(state: tauri::State<'_, BackendState>) -> Result<String, String> {
    let response = commands::backend_progress(&state.render_controller);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Opens the application's downloads/output directory in the platform file manager.
#[tauri::command]
async fn backend_open_downloads(app: AppHandle) -> Result<String, String> {
    let response =
        commands::backend_open_downloads(&app_paths(&app)?).map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Opens a rendered video file from the output directory.
#[tauri::command]
async fn backend_open_video(app: AppHandle, filename: String) -> Result<String, String> {
    let response =
        commands::backend_open_video(&app_paths(&app)?, &filename).map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Lists bundled and user-created overlay templates.
#[tauri::command]
async fn backend_list_templates(app: AppHandle) -> Result<String, String> {
    let response =
        commands::backend_list_templates(&app_paths(&app)?).map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Reads one overlay template by filename.
#[tauri::command]
async fn backend_get_template(app: AppHandle, filename: String) -> Result<String, String> {
    commands::backend_get_template(&app_paths(&app)?, &filename).map_err(|e| e.to_string())
}

/// Requests cancellation for the active render job.
#[tauri::command]
async fn backend_cancel(state: tauri::State<'_, BackendState>) -> Result<String, String> {
    let response = commands::backend_cancel(&state.render_controller);
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Probes a video file with ffprobe and returns serialized metadata.
///
/// This command is retained for diagnostics; the normal import path uses the
/// same core probe through `backend_import_preview_video`.
#[tauri::command]
async fn backend_probe_video(app: AppHandle, file_path: String) -> Result<String, String> {
    let response =
        commands::backend_probe_video(&app_paths(&app)?, &file_path).map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportPreviewVideoResponse {
    import_id: String,
    preview_url: String,
    metadata: serde_json::Value,
    warnings: Vec<String>,
}

/// Imports a local video into the HTTP preview server and returns preview state.
///
/// The original filesystem path remains the source of truth for export. The
/// returned `preview_url` is only for native `<video>` preview playback.
#[tauri::command]
async fn backend_import_preview_video(
    app: AppHandle,
    state: tauri::State<'_, VideoServerHandle>,
    path: String,
) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    let metadata = std::fs::metadata(&path_buf)
        .map_err(|error| format!("Failed to read video file metadata: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("Video path is not a file: {}", path_buf.display()));
    }

    let video_metadata =
        commands::backend_probe_video(&app_paths(&app)?, &path).map_err(|e| e.to_string())?;
    let preview_url = state.set_video(path_buf, content_type_for_path(&path))?;
    let import_id = preview_url
        .rsplit('/')
        .next()
        .ok_or_else(|| "Failed to read import ID from preview URL".to_string())?
        .to_string();
    let response = ImportPreviewVideoResponse {
        import_id,
        preview_url,
        warnings: preview_warnings_for_metadata(&video_metadata),
        metadata: video_metadata,
    };

    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Clears the currently registered local HTTP preview video.
///
/// Any previously issued `/video/<import_id>` URL becomes invalid after this
/// command because the server only serves the current import.
#[tauri::command]
async fn backend_clear_preview_video(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<String, String> {
    state.clear_video()?;
    Ok("null".to_string())
}

/// Returns diagnostic state for the currently registered preview video.
///
/// This is not needed for normal playback, but is useful for DevTools/manual
/// verification of server state and source-file availability.
#[tauri::command]
async fn backend_get_video_state(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<String, String> {
    serde_json::to_string(&state.current_state()).map_err(|error| error.to_string())
}

/// Builds conservative user-facing preview warnings from ffprobe metadata.
///
/// These warnings never block import. They only flag formats that native WebView
/// media decoders commonly struggle with, such as HEVC, high bit depth, or
/// 4:2:2/4:4:4 chroma formats.
fn preview_warnings_for_metadata(metadata: &serde_json::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let codec_name = metadata
        .get("codecName")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let pix_fmt = metadata
        .get("pixFmt")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bits_per_raw_sample = metadata
        .get("bitsPerRawSample")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if codec_name == "hevc" || codec_name == "h265" {
        warnings.push(
            "HEVC/H.265 playback depends on the OS video decoder and may not work on every system."
                .to_string(),
        );
    }

    if bits_per_raw_sample > 8 || pix_fmt.contains("10") || pix_fmt.contains("12") {
        warnings.push(
            "10-bit or higher-bit-depth footage may not play reliably in the native preview."
                .to_string(),
        );
    }

    if pix_fmt.contains("422") || pix_fmt.contains("444") {
        warnings.push(
            "4:2:2 or 4:4:4 footage may decode slowly or fail in the native preview.".to_string(),
        );
    }

    warnings
}

/// Maps a source path extension to the MIME type sent by the preview server.
///
/// Unknown extensions fall back to `application/octet-stream` so the preview
/// server can still attempt playback without claiming an incorrect video type.
fn content_type_for_path(path: &str) -> String {
    match PathBuf::from(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Detects available ffmpeg encoders and hardware acceleration paths.
#[tauri::command]
async fn backend_detect_codecs(app: AppHandle) -> Result<String, String> {
    let response = commands::backend_detect_codecs(&app_paths(&app)?).map_err(|e| e.to_string())?;
    serde_json::to_string(&response).map_err(|error| error.to_string())
}

/// Returns the default save path for a user template under the documents folder.
#[tauri::command]
fn default_template_save_path(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let mut path = app.path().document_dir().map_err(|e| e.to_string())?;
    path.push("OVRLEY");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);
    Ok(path.to_string_lossy().to_string())
}

/// Writes a user template file, creating parent directories as needed.
#[tauri::command]
fn write_template_file(path: String, contents: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);

    if let Some(parent) = path_buf.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    std::fs::write(&path_buf, contents).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Writes parser/debug output under `debug/activities` in the source checkout.
///
/// This command is intended for development diagnostics rather than packaged
/// user data.
#[tauri::command]
fn write_parse_debug_file(filename: String, contents: String) -> Result<String, String> {
    let mut path = source_repo_root();
    path.push("debug");
    path.push("activities");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);

    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Builds and runs the Tauri application.
///
/// The setup hook installs development logging when appropriate and starts the
/// loopback preview video server before the frontend can invoke commands.
pub fn run() {
    let video_server = VideoServerHandle::new();

    tauri::Builder::default()
        .manage(BackendState {
            render_controller: RenderController::default(),
        })
        .manage(video_server)
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            backend_health,
            backend_current_os,
            backend_list_system_fonts,
            backend_render,
            backend_progress,
            backend_cancel,
            backend_list_templates,
            backend_get_template,
            backend_open_downloads,
            backend_open_video,
            backend_probe_video,
            backend_import_preview_video,
            backend_clear_preview_video,
            backend_get_video_state,
            backend_detect_codecs,
            default_template_save_path,
            write_template_file,
            write_parse_debug_file
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            app.state::<VideoServerHandle>().start()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
