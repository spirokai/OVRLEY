//! Tauri command wrappers for `ovrley_services::project_file`.

use crate::runtime_paths;
use ovrley_services::project_file::{self, ProjectFileSummary, ReadProjectResult};
use ovrley_services::raster_resources::RasterResources;
use std::collections::HashMap;

#[tauri::command]
pub(crate) fn default_project_directory(app: tauri::AppHandle) -> Result<String, String> {
    project_file::default_project_directory(&runtime_paths::user_data_root(&app)?)
}

#[tauri::command]
pub(crate) fn list_project_files(directory: String) -> Result<Vec<ProjectFileSummary>, String> {
    project_file::list_project_files(directory)
}

#[tauri::command]
pub(crate) fn read_project_file(resources: tauri::State<'_, RasterResources>, path: String) -> Result<ReadProjectResult, String> {
    project_file::read_project_file(&resources, path)
}

#[tauri::command]
pub(crate) async fn write_project_file(
    app: tauri::AppHandle,
    resources: tauri::State<'_, RasterResources>,
    path: String,
    project_json: String,
    raster_resource_ids: HashMap<String, String>,
) -> Result<String, String> {
    let images = resources
        .owned_assets(raster_resource_ids)
        .map_err(|error| format!("[project_error:raster_unavailable] {error}"))?;
    let ffmpeg = runtime_paths::app_paths(&app)
        .ok()
        .and_then(|paths| ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&paths.repo_root).ok());
    tauri::async_runtime::spawn_blocking(move || project_file::write_project_file(path, project_json, ffmpeg.as_deref(), images))
        .await
        .map_err(|error| format!("[project_error:save_failed] Project save task failed: {error}"))?
}
