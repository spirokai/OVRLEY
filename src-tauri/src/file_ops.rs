//! File-system command implementations for the Tauri application shell.
//!
//! Owns: template save-path resolution, template file writes, and parse-debug
//!       file writes, and selected-file reads for frontend import flows.
//! Does not own: template listing, template content retrieval, or rendering —
//!       those live in `ovrley_core::commands` and the render pipeline.
//!
//! Allowed dependencies: `std`, `tauri`, `runtime_paths`, `ovrley_core`
//!       (for template/image validation), `raster_resources` (session ownership).

use crate::raster_resources::RasterResources;
use crate::runtime_paths;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

/// Returns the default save path for a user template under the documents folder.
#[tauri::command]
pub(crate) fn default_template_save_path(
    app: tauri::AppHandle,
    filename: String,
) -> Result<String, String> {
    let mut path = app.path().document_dir().map_err(|e| e.to_string())?;
    path.push("OVRLEY");
    path.push("templates");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);
    Ok(path.to_string_lossy().to_string())
}

/// Writes a user template file, creating parent directories as needed.
///
/// Validates the template content before writing — invalid JSON or config
/// that fails the normalization seam is rejected with an error.
#[tauri::command]
pub(crate) fn write_template_file(path: String, contents: String) -> Result<String, String> {
    ovrley_core::commands::validate_template_contents(&contents).map_err(|e| e.to_string())?;

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
pub(crate) fn write_parse_debug_file(filename: String, contents: String) -> Result<String, String> {
    let mut path = runtime_paths::source_repo_root();
    path.push("debug");
    path.push("activities");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(filename);

    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Reads one user-selected file from an absolute path and returns its raw bytes.
#[tauri::command]
pub(crate) fn read_selected_file_bytes(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| e.to_string())
}

/// Checks one resolved source path without reading or interpreting its contents.
#[tauri::command]
pub(crate) fn selected_path_is_file(path: String) -> bool {
    PathBuf::from(path).is_file()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RasterSelection {
    width: u32,
    height: u32,
    resource_id: String,
}

#[derive(Serialize)]
pub(crate) struct RasterSelectionError {
    code: &'static str,
    message: String,
}

impl From<ovrley_core::raster::RasterError> for RasterSelectionError {
    fn from(error: ovrley_core::raster::RasterError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

/// Validates a user-selected bitmap and registers its immutable oriented preview in Rust.
#[tauri::command]
pub(crate) fn load_selected_raster(
    resources: tauri::State<'_, RasterResources>,
    path: String,
) -> Result<RasterSelection, RasterSelectionError> {
    let raster = ovrley_core::raster::load_selected_raster(PathBuf::from(path).as_path())
        .map_err(RasterSelectionError::from)?;
    let (width, height) = raster.dimensions();
    let resource_id = resources.insert(raster);
    Ok(RasterSelection {
        width,
        height,
        resource_id,
    })
}

/// Returns display bytes from the immutable Rust-owned image snapshot.
#[tauri::command]
pub(crate) fn raster_preview_png(
    resources: tauri::State<'_, RasterResources>,
    resource_id: String,
) -> Result<String, String> {
    resources.preview_png_base64(&resource_id)
}
