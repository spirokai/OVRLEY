//! Native ownership of immutable finalized activity snapshots and font resources.
use ovrley_core::activity::finalize::FinalizeActivityResponse;
use std::{collections::BTreeMap, sync::Mutex};

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
    font_id: String,
) -> Result<tauri::ipc::Response, String> {
    let paths = crate::runtime_paths::app_paths(&app)?;
    let bytes = ovrley_core::fonts::preview_font_resource(&paths.font_dirs, &font_id)
        .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}
