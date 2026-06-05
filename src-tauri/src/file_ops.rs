//! File-system command implementations for the Tauri application shell.
//!
//! Owns: template save-path resolution, template file writes, and parse-debug
//!       file writes.
//! Does not own: template listing, template content retrieval, or rendering —
//!       those live in `ovrley_core::commands` and the render pipeline.
//!
//! Allowed dependencies: `std`, `tauri`, `runtime_paths`, `ovrley_core`
//!       (for template write validation through the normalization seam).

use crate::runtime_paths;
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
    ovrley_core::commands::validate_template_contents(&contents)
        .map_err(|e| e.to_string())?;

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
