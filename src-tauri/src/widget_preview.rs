//! Native ownership of immutable finalized activity snapshots and font resources.
use crate::map_tile_service::MapTileService;
use ovrley_core::{activity::finalize::FinalizeActivityResponse, render::download_preview_tiles, CoreError};
use ovrley_widgets::{
    activity::ActivitySampler,
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    widgets::map::{config::MapConfig, validation::validate_map_widget},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub(crate) struct PreviewActivities(Mutex<BTreeMap<String, Vec<u8>>>);

impl PreviewActivities {
    pub fn retain(
        &self,
        mut response: FinalizeActivityResponse,
    ) -> Result<FinalizeActivityResponse, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&response.parsed_activity).map_err(|e| e.to_string())?;
        self.0
            .lock()
            .expect("preview activities lock")
            .insert(id.clone(), bytes);
        response
            .parsed_activity
            .extra
            .insert("preview_activity_id".into(), id.into());
        Ok(response)
    }
}

#[tauri::command]
pub(crate) fn backend_preview_activity(
    state: tauri::State<'_, PreviewActivities>,
    activity_id: String,
) -> Result<tauri::ipc::Response, String> {
    let snapshots = state.0.lock().expect("preview activities lock");
    let bytes = snapshots
        .get(&activity_id)
        .ok_or_else(|| format!("unknown preview activity: {activity_id}"))?;
    Ok(tauri::ipc::Response::new(bytes.clone()))
}

/// The tiles of a map route being downloaded. A newer request replaces it.
#[derive(Default)]
pub(crate) struct MapDownload(Mutex<Option<Arc<AtomicBool>>>);

const MAP_PREPARE_EVENT: &str = "map-prepare";

#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
enum MapPrepareEvent {
    Needed { total: usize },
    Downloading { done: usize, total: usize },
    Ready,
    Failed { error: String },
}

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
    map_tiles: tauri::State<'_, MapTileService>,
    activity_id: String,
    widgets: Vec<MapConfig>,
    scene: ScenePresentationConfig,
) -> Result<(), String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    if let Some(previous) = download
        .0
        .lock()
        .expect("map download lock")
        .replace(cancelled.clone())
    {
        previous.store(true, Ordering::SeqCst);
    }
    if widgets.is_empty() {
        return Ok(());
    }
    let activity = previews
        .0
        .lock()
        .expect("preview activities lock")
        .get(&activity_id)
        .ok_or_else(|| format!("unknown preview activity: {activity_id}"))?
        .clone();
    let source = map_tiles.inner().clone();
    let events = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let activity = ActivitySampler::from_json(&activity).map_err(CoreError::from)?;
        let host = WidgetHost::from_scene(&scene).map_err(CoreError::from)?;
        let scene = validate_scene_presentation(scene).map_err(CoreError::from)?;
        let widgets = widgets
            .iter()
            .enumerate()
            .map(|(index, widget)| validate_map_widget(widget, index))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CoreError::from)?;
        download_preview_tiles(
            &widgets,
            &scene,
            &host,
            &activity,
            &source,
            &cancelled,
            &|done, total| {
                // Often enough for a progress bar, and quiet on a route of thousands of tiles.
                if done == 0 {
                    emit_map_prepare(&events, MapPrepareEvent::Needed { total });
                } else if done % (total / 50).max(1) == 0 {
                    emit_map_prepare(&events, MapPrepareEvent::Downloading { done, total });
                }
            },
        )
    })
    .await
    .map_err(|error| error.to_string())?;
    match result {
        Ok(()) => {
            emit_map_prepare(&app, MapPrepareEvent::Ready);
            Ok(())
        }
        Err(CoreError::Cancelled) => Ok(()),
        Err(error) => {
            let error = error.to_string();
            emit_map_prepare(&app, MapPrepareEvent::Failed { error: error.clone() });
            Err(error)
        }
    }
}

/// Running splits of one retained activity for a widget's split length.
#[tauri::command]
pub(crate) fn backend_preview_split_table(
    state: tauri::State<'_, PreviewActivities>,
    activity_id: String,
    length: ovrley_widgets::widgets::splits_run::table::SplitLength,
) -> Result<tauri::ipc::Response, String> {
    if !length.distance.is_finite() || length.distance <= 0.0 {
        return Err(format!("split length must be positive, got {}", length.distance));
    }
    let activity: ovrley_core::activity::schema::ParsedActivity = {
        let snapshots = state.0.lock().expect("preview activities lock");
        let bytes = snapshots
            .get(&activity_id)
            .ok_or_else(|| format!("unknown preview activity: {activity_id}"))?;
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?
    };
    let table = ovrley_core::activity::splits::derive_split_table(&activity, length);
    Ok(tauri::ipc::Response::new(
        serde_json::to_vec(&table).map_err(|e| e.to_string())?,
    ))
}

#[tauri::command]
pub(crate) fn backend_release_preview_activity(
    state: tauri::State<'_, PreviewActivities>,
    activity_id: String,
) {
    state
        .0
        .lock()
        .expect("preview activities lock")
        .remove(&activity_id);
}

#[tauri::command]
pub(crate) fn backend_preview_font(
    app: tauri::AppHandle,
    request: ovrley_widgets::preview::FontRequest,
) -> Result<tauri::ipc::Response, String> {
    let paths = crate::runtime_paths::app_paths(&app)?;
    let bytes = ovrley_core::fonts::preview_font_resource(
        &paths.font_dirs,
        &request.font,
        request.font_weight,
        request.italic,
    )
    .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}
