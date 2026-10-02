//! File-system command implementations for the Tauri application shell.
//!
//! Owns: template save-path resolution, template file writes, and parse-debug
//!       file writes, and selected-file reads for frontend import flows.
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

const BATCH_VIDEO_EXTENSIONS: [&str; 3] = ["mp4", "mov", "mkv"];

/// Lists supported video files directly inside a directory, sorted by name.
///
/// Used by the batch-render folder picker — does not recurse into
/// subdirectories.
#[tauri::command]
pub(crate) fn list_directory_video_files(directory: String) -> Result<Vec<String>, String> {
    let directory = PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Video directory must be an absolute path".into());
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let is_video = entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    BATCH_VIDEO_EXTENSIONS
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                });
        if is_video {
            paths.push(path.to_string_lossy().to_string());
        }
    }

    paths.sort_by(|left, right| left.to_lowercase().cmp(&right.to_lowercase()));
    Ok(paths)
}
