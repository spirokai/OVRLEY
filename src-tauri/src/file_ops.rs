//! Tauri command wrappers for `ovrley_services::file_ops` and the selected-file reads of the
//! frontend import flows.

use crate::runtime_paths;
use ovrley_services::file_ops::{self, RasterSelection, RasterSelectionError};
use ovrley_services::raster_resources::RasterResources;
use std::path::PathBuf;

/// Returns the default save path for a user template under the documents folder.
#[tauri::command]
pub(crate) fn default_template_save_path(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let path = file_ops::default_template_save_path(&runtime_paths::user_data_root(&app)?, &filename)?;
    Ok(path.to_string_lossy().to_string())
}

/// Writes a user template file, creating parent directories as needed.
#[tauri::command]
pub(crate) fn write_template_file(path: String, contents: String) -> Result<String, String> {
    file_ops::write_template_file(&PathBuf::from(&path), &contents)?;
    Ok(path)
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

/// Validates a user-selected bitmap and registers its immutable oriented preview in Rust.
#[tauri::command]
pub(crate) fn load_selected_raster(
    resources: tauri::State<'_, RasterResources>,
    path: String,
) -> Result<RasterSelection, RasterSelectionError> {
    file_ops::load_selected_raster(&resources, &PathBuf::from(path))
}

/// Returns display bytes from the immutable Rust-owned image snapshot.
#[tauri::command]
pub(crate) fn raster_preview_png(resources: tauri::State<'_, RasterResources>, resource_id: String) -> Result<String, String> {
    resources.preview_png_base64(&resource_id)
}
