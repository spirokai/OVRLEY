//! Tauri command wrappers for the application shell.
//!
//! Owns: all `#[tauri::command]` functions that delegate to `ovrley_core::commands`,
//!       plus the shared serializer helper that eliminates repeated JSON-string
//!       serialization boilerplate.
//! Does not own: file-system commands — those live in `file_ops.rs`.
//!       Domain logic, path construction, and preview import helpers live in
//!       `runtime_paths`, `preview_import`, and `ovrley_core` respectively.
//!
//! Allowed dependencies: `ovrley_core::commands`, `runtime_paths`, `preview_import`,
//!       `tauri`, `serde_json`.
//! Forbidden dependencies: none (this is the Tauri boundary layer and may import
//!       from any shell module).

use crate::preview_import::{content_type_for_path, preview_warnings_for_metadata};
use crate::runtime_paths;
use crate::video_server::VideoServerHandle;
use crate::BackendState;
use ovrley_core::commands;
use std::path::PathBuf;
use tauri::AppHandle;

/// Serializes a `Serialize` value into a JSON string or maps an error to a
/// `String`, consolidating the repeated `.map_err(|e| e.to_string())?;
/// serde_json::to_string(...).map_err(...)` pattern used by most commands.
fn serialize_command_result<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

/// Helper: calls a core command returning a `Serialize` value, maps errors
/// through `.to_string()`, then serializes the result into a JSON string.
fn call_and_serialize<T: serde::Serialize>(
    result: Result<T, impl ToString>,
) -> Result<String, String> {
    serialize_command_result(&result.map_err(|e| e.to_string())?)
}

/// Returns a basic backend health payload for the frontend runtime check.
#[tauri::command]
pub(crate) async fn backend_health(app: AppHandle) -> Result<String, String> {
    serialize_command_result(&commands::backend_health(&runtime_paths::app_paths(&app)?))
}

/// Returns the current operating system identifier used for platform-specific UI.
#[tauri::command]
pub(crate) async fn backend_current_os() -> Result<String, String> {
    serialize_command_result(&commands::backend_current_os())
}

/// Lists bundled and system fonts available to the backend renderer.
#[tauri::command]
pub(crate) async fn backend_list_system_fonts(app: AppHandle) -> Result<String, String> {
    serialize_command_result(&commands::backend_list_system_fonts(
        &runtime_paths::app_paths(&app)?,
    ))
}

/// Starts an overlay video render from serialized scene config and activity data.
///
/// The render controller in managed state tracks progress and cancellation for
/// the long-running encoder task.
#[tauri::command]
pub(crate) async fn backend_render(
    app: AppHandle,
    state: tauri::State<'_, BackendState>,
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    call_and_serialize(commands::backend_render(
        &runtime_paths::app_paths(&app)?,
        &state.render_controller,
        &config_json,
        &parsed_activity_json,
    ))
}

/// Renders one transparent preview PNG for the requested second.
#[tauri::command]
pub(crate) async fn backend_render_preview_frame(
    app: AppHandle,
    config_json: String,
    parsed_activity_json: String,
    second: f64,
) -> Result<String, String> {
    #[cfg(debug_assertions)]
    {
        return call_and_serialize(commands::backend_render_preview_frame(
            &runtime_paths::app_paths(&app)?,
            &config_json,
            &parsed_activity_json,
            second,
        ));
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        let _ = config_json;
        let _ = parsed_activity_json;
        let _ = second;
        Err("Preview-frame rendering is only available in debug builds.".to_string())
    }
}

/// Returns progress for the currently active or most recent render job.
#[tauri::command]
pub(crate) async fn backend_progress(
    state: tauri::State<'_, BackendState>,
) -> Result<String, String> {
    serialize_command_result(&commands::backend_progress(&state.render_controller))
}

/// Opens the application's downloads/output directory in the platform file manager.
#[tauri::command]
pub(crate) async fn backend_open_downloads(app: AppHandle) -> Result<String, String> {
    call_and_serialize(commands::backend_open_downloads(&runtime_paths::app_paths(
        &app,
    )?))
}

/// Opens a rendered video file from the output directory.
#[tauri::command]
pub(crate) async fn backend_open_video(app: AppHandle, filename: String) -> Result<String, String> {
    call_and_serialize(commands::backend_open_video(
        &runtime_paths::app_paths(&app)?,
        &filename,
    ))
}

/// Lists bundled and user-created overlay templates.
#[tauri::command]
pub(crate) async fn backend_list_templates(app: AppHandle) -> Result<String, String> {
    call_and_serialize(commands::backend_list_templates(&runtime_paths::app_paths(
        &app,
    )?))
}

/// Reads one overlay template by filename.
#[tauri::command]
pub(crate) async fn backend_get_template(
    app: AppHandle,
    filename: String,
) -> Result<String, String> {
    commands::backend_get_template(&runtime_paths::app_paths(&app)?, &filename)
        .map_err(|e| e.to_string())
}

/// Requests cancellation for the active render job.
#[tauri::command]
pub(crate) async fn backend_cancel(
    state: tauri::State<'_, BackendState>,
) -> Result<String, String> {
    serialize_command_result(&commands::backend_cancel(&state.render_controller))
}

/// Probes a video file with ffprobe and returns serialized metadata.
///
/// This command is retained for diagnostics; the normal import path uses the
/// same core probe through `backend_import_preview_video`.
#[tauri::command]
pub(crate) async fn backend_probe_video(
    app: AppHandle,
    file_path: String,
) -> Result<String, String> {
    call_and_serialize(commands::backend_probe_video(
        &runtime_paths::app_paths(&app)?,
        &file_path,
    ))
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
pub(crate) async fn backend_import_preview_video(
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

    let video_metadata = commands::backend_probe_video(&runtime_paths::app_paths(&app)?, &path)
        .map_err(|e| e.to_string())?;
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

    serialize_command_result(&response)
}

/// Clears the currently registered local HTTP preview video.
///
/// Any previously issued `/video/<import_id>` URL becomes invalid after this
/// command because the server only serves the current import.
#[tauri::command]
pub(crate) async fn backend_clear_preview_video(
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
pub(crate) async fn backend_get_video_state(
    state: tauri::State<'_, VideoServerHandle>,
) -> Result<String, String> {
    serialize_command_result(&state.current_state())
}

/// Detects available ffmpeg encoders and hardware acceleration paths.
#[tauri::command]
pub(crate) async fn backend_detect_codecs(app: AppHandle) -> Result<String, String> {
    call_and_serialize(commands::backend_detect_codecs(&runtime_paths::app_paths(
        &app,
    )?))
}
