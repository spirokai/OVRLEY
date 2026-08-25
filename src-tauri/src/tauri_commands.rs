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

use crate::distribution::DistributionKind;
use crate::preview_import::{content_type_for_path, preview_warnings_for_metadata};
use crate::runtime_paths;
use crate::video_server::VideoServerHandle;
use crate::BackendState;
use ovrley_core::activity::finalize::FinalizeActivityResponse;
use ovrley_core::commands;
use ovrley_core::error::CoreError;
use ovrley_core::output::RenderOutputKind;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const WINDOWS_HEVC_EXTENSION_URL: &str = "https://apps.microsoft.com/detail/9nmzlz57r3t7";

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

#[derive(Debug, Serialize)]
#[serde(tag = "code")]
pub(crate) enum BackendRenderError {
    #[serde(rename = "already_exists")]
    AlreadyExists { message: String },
    #[serde(rename = "render_error")]
    RenderError { message: String },
}

impl BackendRenderError {
    fn from_core(error: CoreError) -> Self {
        match error {
            CoreError::OutputExists(message) => Self::AlreadyExists { message },
            CoreError::OutputIo { path, source } => Self::RenderError {
                message: output_io_message(&path, &source),
            },
            error => Self::RenderError {
                message: error.to_string(),
            },
        }
    }
}

fn output_io_message(path: &Path, source: &std::io::Error) -> String {
    let directory = path
        .parent()
        .map(|value| value.display().to_string())
        .unwrap_or_else(|| path.display().to_string());

    match source.kind() {
        std::io::ErrorKind::NotFound => {
            format!("The output directory does not exist: {directory}")
        }
        std::io::ErrorKind::PermissionDenied => {
            format!("The output directory is not writable: {directory}")
        }
        _ => format!("IO error at {}: {source}", path.display()),
    }
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

/// Returns the distribution mode detected during Rust startup.
#[tauri::command]
pub(crate) async fn backend_distribution_kind(
    state: tauri::State<'_, DistributionKind>,
) -> Result<DistributionKind, String> {
    Ok(*state)
}

/// Opens the official Microsoft Store page for Windows HEVC playback support.
#[tauri::command]
pub(crate) async fn backend_open_hevc_support() -> Result<(), String> {
    open::that(WINDOWS_HEVC_EXTENSION_URL).map_err(|error| error.to_string())
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
    output_path: String,
    output_kind: RenderOutputKind,
    overwrite: bool,
) -> Result<String, BackendRenderError> {
    let paths = runtime_paths::app_paths(&app)
        .map_err(|message| BackendRenderError::RenderError { message })?;
    let result = commands::backend_render(
        &paths,
        &state.render_controller,
        &config_json,
        &parsed_activity_json,
        &output_path,
        output_kind,
        overwrite,
    )
    .map_err(BackendRenderError::from_core)?;
    serialize_command_result(&result).map_err(|message| BackendRenderError::RenderError { message })
}

/// Finalizes frontend-extracted raw samples into a parsed activity payload.
///
/// This wrapper intentionally does no activity work; it preserves the existing
/// command convention by delegating to `ovrley_core` and serializing the result
/// as a JSON string for JavaScript.
#[tauri::command]
pub(crate) async fn backend_finalize_activity(
    app: AppHandle,
    raw_activity_json: String,
) -> Result<String, String> {
    call_and_serialize(commands::backend_finalize_activity(
        &runtime_paths::app_paths(&app)?,
        &raw_activity_json,
    ))
}

/// Parses a native CSV path through the core columnar activity pipeline.
#[tauri::command]
pub(crate) async fn backend_parse_csv_activity(
    app: AppHandle,
    path: String,
) -> Result<FinalizeActivityResponse, String> {
    commands::backend_parse_csv_activity(&runtime_paths::app_paths(&app)?, &path)
        .map_err(|error| error.to_string())
}

/// Parses a native VBO path through the core columnar activity pipeline.
#[tauri::command]
pub(crate) async fn backend_parse_vbo_activity(
    app: AppHandle,
    path: String,
) -> Result<FinalizeActivityResponse, String> {
    commands::backend_parse_vbo_activity(&runtime_paths::app_paths(&app)?, &path)
        .map_err(|error| error.to_string())
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

/// Opens the remembered render output directory in the platform file manager.
#[tauri::command]
pub(crate) async fn backend_open_output_directory(
    app: AppHandle,
    directory: Option<String>,
) -> Result<String, String> {
    call_and_serialize(commands::backend_open_output_directory(
        &runtime_paths::app_paths(&app)?,
        directory.as_deref(),
    ))
}

/// Returns a fresh suggested render output path.
#[tauri::command]
pub(crate) async fn backend_suggest_output_path(
    app: AppHandle,
    output_kind: RenderOutputKind,
    remembered_directory: Option<String>,
) -> Result<String, String> {
    let path = commands::backend_suggest_output_path(
        &runtime_paths::app_paths(&app)?,
        output_kind,
        remembered_directory.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Opens the application's templates directory in the platform file manager.
#[tauri::command]
pub(crate) async fn backend_open_templates(app: AppHandle) -> Result<String, String> {
    call_and_serialize(commands::backend_open_templates(&runtime_paths::app_paths(
        &app,
    )?))
}

/// Opens a rendered video file from the output directory.
#[tauri::command]
pub(crate) async fn backend_open_video(output_path: String) -> Result<String, String> {
    call_and_serialize(commands::backend_open_video(&output_path))
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

/// Extracts embedded telemetry from an imported video source.
///
/// This is intentionally separate from preview import so a failed telemetry
/// parse does not prevent video playback; the frontend can call it
/// opportunistically after the preview video is registered.
#[tauri::command]
pub(crate) async fn backend_extract_video_telemetry(
    app: AppHandle,
    file_path: String,
) -> Result<Option<FinalizeActivityResponse>, String> {
    commands::backend_extract_video_telemetry(&runtime_paths::app_paths(&app)?, &file_path)
        .map_err(|error| error.to_string())
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

/// Builds elevation widget geometry from config and activity data.
///
/// Returns simplified, projected geometry points for the elevation preview
/// without rendering a Skia surface.
#[tauri::command]
pub(crate) async fn backend_build_elevation_geometry(
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    call_and_serialize(
        commands::elevation_geometry::build_elevation_geometry_command(
            &config_json,
            &parsed_activity_json,
        ),
    )
}

/// Builds route widget geometry from config and activity data.
///
/// Returns simplified, projected geometry points for the route preview
/// without rendering a Skia surface.
#[tauri::command]
pub(crate) async fn backend_build_route_geometry(
    config_json: String,
    parsed_activity_json: String,
) -> Result<String, String> {
    call_and_serialize(commands::route_geometry::build_route_geometry_command(
        &config_json,
        &parsed_activity_json,
    ))
}
