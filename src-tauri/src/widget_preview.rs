//! Tauri command wrappers for `ovrley_services::widget_preview`.
use ovrley_services::map_styles::MapData;
use ovrley_services::widget_preview::{self, MapDownload, MapPrepareEvent, MapPrepareRequest, PreviewActivities};
use ovrley_widgets::model::scene::ScenePresentationConfig;
use ovrley_widgets::widgets::map::config::MapConfig;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub(crate) fn backend_preview_activity(
    state: tauri::State<'_, PreviewActivities>,
    activity_id: String,
) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(state.snapshot(&activity_id)?))
}

const MAP_PREPARE_EVENT: &str = "map-prepare";

fn emit_map_prepare(app: &AppHandle, event: MapPrepareEvent) {
    if let Err(error) = app.emit(MAP_PREPARE_EVENT, event) {
        log::warn!("failed to emit {MAP_PREPARE_EVENT} event: {error}");
    }
}

/// Downloads the tiles a project's maps will show over the activity, so scrubbing mostly hits the disk
/// cache. It runs in the background: the preview fetches what it needs either way. No widgets just
/// stops the last download.
#[tauri::command]
pub(crate) async fn backend_map_prepare(
    app: AppHandle,
    previews: tauri::State<'_, PreviewActivities>,
    download: tauri::State<'_, MapDownload>,
    map_data: tauri::State<'_, MapData>,
    activity_id: String,
    widgets: Vec<MapConfig>,
    scene: ScenePresentationConfig,
    raster_scale: f32,
) -> Result<(), String> {
    let cancelled = download.restart();
    if widgets.is_empty() {
        return Ok(());
    }
    let request = MapPrepareRequest {
        activity: previews.snapshot(&activity_id)?,
        widgets,
        scene,
        raster_scale,
    };
    let source = map_data.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        widget_preview::prepare_map_tiles(request, &source, &cancelled, &|event| emit_map_prepare(&app, event))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Running splits of one retained activity for a widget's split length.
#[tauri::command]
pub(crate) fn backend_preview_split_table(
    state: tauri::State<'_, PreviewActivities>,
    activity_id: String,
    length: ovrley_widgets::widgets::splits_run::table::SplitLength,
) -> Result<tauri::ipc::Response, String> {
    Ok(tauri::ipc::Response::new(state.split_table(&activity_id, length)?))
}

#[tauri::command]
pub(crate) fn backend_release_preview_activity(state: tauri::State<'_, PreviewActivities>, activity_id: String) {
    state.release(&activity_id);
}

#[tauri::command]
pub(crate) fn backend_preview_font(
    app: tauri::AppHandle,
    request: ovrley_widgets::preview::FontRequest,
) -> Result<tauri::ipc::Response, String> {
    let paths = crate::runtime_paths::app_paths(&app)?;
    Ok(tauri::ipc::Response::new(widget_preview::preview_font(&paths, &request)?))
}
