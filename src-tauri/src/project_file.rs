//! Tauri command wrappers for `ovrley_services::project_file`.

use crate::runtime_paths;
use ovrley_services::project_file::{self, ReadProjectResult, WriteProjectResult};
use ovrley_services::video_thumbnail::png_data_url;
use ovrley_services::raster_resources::RasterResources;
use std::collections::HashMap;

#[tauri::command]
pub(crate) fn default_project_directory(app: tauri::AppHandle) -> Result<String, String> {
    project_file::default_project_directory(&runtime_paths::user_data_root(&app)?)
}

/// A project of the startup dialog, its thumbnail as a `data:` URL for the webview.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectFileSummary {
    name: String,
    path: String,
    thumbnail_data_url: Option<String>,
}

#[tauri::command]
pub(crate) fn list_project_files(directory: String) -> Result<Vec<ProjectFileSummary>, String> {
    Ok(project_file::list_project_files(directory)?
        .into_iter()
        .map(|summary| ProjectFileSummary {
            thumbnail_data_url: summary.thumbnail.as_deref().map(png_data_url),
            name: summary.name,
            path: summary.path,
        })
        .collect())
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
) -> Result<WriteProjectResult, String> {
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
