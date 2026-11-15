//! Native ownership of immutable finalized activity snapshots, map tile preparation and preview
//! font resources.
use crate::map_styles::MapData;
use ovrley_core::{
    activity::finalize::FinalizeActivityResponse, activity::schema::ParsedActivity, paths::AppPaths, render::download_preview_tiles,
    CoreError,
};
use ovrley_widgets::{
    activity::ActivitySampler,
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::FontRequest,
    render::preview::PreviewRasterScale,
    widgets::{
        map::{config::MapConfig, validation::validate_map_widget},
        splits_run::table::SplitLength,
    },
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// Finalized activities retained for the preview, keyed by the id handed to the frontend.
#[derive(Default)]
pub struct PreviewActivities(Mutex<BTreeMap<String, Vec<u8>>>);

impl PreviewActivities {
    pub fn retain(&self, mut response: FinalizeActivityResponse) -> Result<FinalizeActivityResponse, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&response.parsed_activity).map_err(|e| e.to_string())?;
        self.0.lock().expect("preview activities lock").insert(id.clone(), bytes);
        response.parsed_activity.extra.insert("preview_activity_id".into(), id.into());
        Ok(response)
    }

    /// The serialized activity retained under `activity_id`.
    pub fn snapshot(&self, activity_id: &str) -> Result<Vec<u8>, String> {
        self.0
            .lock()
            .expect("preview activities lock")
            .get(activity_id)
            .cloned()
            .ok_or_else(|| format!("unknown preview activity: {activity_id}"))
    }

    pub fn release(&self, activity_id: &str) {
        self.0.lock().expect("preview activities lock").remove(activity_id);
    }

    /// Running splits of one retained activity for a widget's split length, serialized.
    pub fn split_table(&self, activity_id: &str, length: SplitLength) -> Result<Vec<u8>, String> {
        if !length.distance.is_finite() || length.distance <= 0.0 {
            return Err(format!("split length must be positive, got {}", length.distance));
        }
        let activity: ParsedActivity = {
            let snapshots = self.0.lock().expect("preview activities lock");
            let bytes = snapshots
                .get(activity_id)
                .ok_or_else(|| format!("unknown preview activity: {activity_id}"))?;
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?
        };
        let table = ovrley_core::activity::splits::derive_split_table(&activity, length);
        serde_json::to_vec(&table).map_err(|e| e.to_string())
    }
}

/// The tiles of a map route being downloaded. A newer request replaces it.
#[derive(Default)]
pub struct MapDownload(Mutex<Option<Arc<AtomicBool>>>);

impl MapDownload {
    /// Cancels the running download, if any, and returns the cancellation flag of a new one.
    pub fn restart(&self) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.0.lock().expect("map download lock").replace(cancelled.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        cancelled
    }
}

/// Progress of a map tile preparation, as the preview shows it.
#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum MapPrepareEvent {
    Needed { total: usize },
    Downloading { done: usize, total: usize },
    Ready,
    Failed { error: String },
}

/// A map tile preparation request: the retained activity and the map widgets to prepare for.
pub struct MapPrepareRequest {
    pub activity: Vec<u8>,
    pub widgets: Vec<MapConfig>,
    pub scene: ScenePresentationConfig,
    pub raster_scale: f32,
}

/// Downloads the tiles a project's maps will show over the activity, so scrubbing mostly hits the
/// disk cache. Blocking; the preview fetches what it needs either way. A cancelled download ends
/// quietly with `Ok`.
pub fn prepare_map_tiles(
    request: MapPrepareRequest,
    source: &MapData,
    cancelled: &AtomicBool,
    on_event: &(dyn Fn(MapPrepareEvent) + Sync),
) -> Result<(), String> {
    let MapPrepareRequest {
        activity,
        widgets,
        scene,
        raster_scale,
    } = request;
    let result = (|| {
        let activity = ActivitySampler::from_json(&activity).map_err(CoreError::from)?;
        // The preview's density sets the map's pixel ratio and so the tile zoom it needs.
        let host = PreviewRasterScale::new(raster_scale)
            .and_then(|raster_scale| WidgetHost::from_scene(&scene, raster_scale))
            .map_err(CoreError::from)?;
        let scene = validate_scene_presentation(scene).map_err(CoreError::from)?;
        let widgets = widgets
            .iter()
            .enumerate()
            .map(|(index, widget)| validate_map_widget(widget, index))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CoreError::from)?;
        download_preview_tiles(&widgets, &scene, &host, &activity, source, cancelled, &|done, total| {
            // Often enough for a progress bar, and quiet on a route of thousands of tiles.
            if done == 0 {
                on_event(MapPrepareEvent::Needed { total });
            } else if done % (total / 50).max(1) == 0 {
                on_event(MapPrepareEvent::Downloading { done, total });
            }
        })
    })();
    match result {
        Ok(()) => {
            on_event(MapPrepareEvent::Ready);
            Ok(())
        }
        Err(CoreError::Cancelled) => Ok(()),
        Err(error) => {
            let error = error.to_string();
            on_event(MapPrepareEvent::Failed { error: error.clone() });
            Err(error)
        }
    }
}

/// The font file a preview widget asks for.
pub fn preview_font(paths: &AppPaths, request: &FontRequest) -> Result<Vec<u8>, String> {
    ovrley_core::fonts::preview_font_resource(&paths.font_dirs, &request.font, request.font_weight, request.italic).map_err(|e| e.to_string())
}
