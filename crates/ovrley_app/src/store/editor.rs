//! The editor state: the document, the selection and unit system, playback, the timeline
//! viewport and mute, the map view mode and the raster images.

use std::rc::Rc;
use ovrley_core::template::template_manifest;
use ovrley_widgets::model::document::{convert_config_units, normalize_selection_ids, primary_selection_id, OverlayConfig, UnitSystem};
use serde_json::Value;
use crate::preferences::PreferencesStore;
use crate::store::Store;
use crate::store::services::Services;
use crate::video::program::PlaybackSource;

/// A non-finite second becomes 0.
fn normalize_preview_second(second: f64) -> f64 {
    if second.is_finite() {
        second
    } else {
        0.0
    }
}

/// The default scene and empty collections.
pub fn default_config() -> OverlayConfig {
    OverlayConfig {
        scene: template_manifest().scene.clone(),
        ..OverlayConfig::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewPlaybackState {
    #[default]
    Paused,
    Playing,
    Scrubbing,
}

/// The visible window of the timeline, `view_start < view_end`, both finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineViewport {
    pub view_start: f64,
    pub view_end: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorState {
    /// The document, copy-on-write: a history snapshot is a pointer copy.
    pub config: Rc<OverlayConfig>,
    /// Incremented when a template load, import or new replaces the document.
    pub editor_document_revision: u64,
    pub selected_widget_id: Option<String>,
    /// In document order.
    pub selected_widget_ids: Vec<String>,
    pub is_marquee_selecting: bool,
    pub fallback_duration_seconds: f64,
    pub selected_second: f64,
    pub timeline_viewport: TimelineViewport,
    /// Set when a project is applied, so the next media change keeps the restored viewport.
    pub skip_next_timeline_viewport_reset: bool,
    pub preview_playback_state: PreviewPlaybackState,
    /// The clock that owns playback.
    pub preview_playback_source: PlaybackSource,
    pub is_video_muted: bool,
    /// The system new widgets start in; not part of the document or its history.
    pub unit_system: UnitSystem,
    /// The map in view mode (double-clicked to pan and zoom), if any.
    pub map_view_widget_id: Option<String>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            config: Rc::new(default_config()),
            editor_document_revision: 0,
            selected_widget_id: None,
            selected_widget_ids: Vec::new(),
            is_marquee_selecting: false,
            fallback_duration_seconds: 73.0,
            selected_second: 0.0,
            timeline_viewport: TimelineViewport { view_start: 0.0, view_end: 73.0 },
            skip_next_timeline_viewport_reset: false,
            preview_playback_state: PreviewPlaybackState::Paused,
            preview_playback_source: PlaybackSource::Timeline,
            is_video_muted: false,
            unit_system: UnitSystem::Metric,
            map_view_widget_id: None,
        }
    }
}

/// After a config replacement, keep the selected ids that still exist;
/// otherwise keep the primary, or fall back to the first widget (when a primary was set) or the
/// last.
fn reconcile_selection(config: &OverlayConfig, selected_ids: &[String], selected_id: Option<&str>) -> (Vec<String>, Option<String>) {
    let ordered = config.widget_ids();
    if ordered.is_empty() {
        return (Vec::new(), None);
    }
    let retained = normalize_selection_ids(selected_ids, &ordered);
    if !retained.is_empty() {
        let primary = primary_selection_id(&retained, selected_id);
        return (retained, primary);
    }
    if let Some(id) = selected_id.filter(|id| ordered.iter().any(|candidate| candidate == id)) {
        return (vec![id.to_string()], Some(id.to_string()));
    }
    let fallback = if selected_id.is_some() { ordered[0].clone() } else { ordered[ordered.len() - 1].clone() };
    (vec![fallback.clone()], Some(fallback))
}

impl EditorState {
    /// Replaces the document and reconciles the selection.
    pub fn set_config(&mut self, config: OverlayConfig) {
        self.replace_config(Rc::new(config));
    }

    /// [`Self::set_config`] with a shared document (undo and redo restore pointers).
    pub fn replace_config(&mut self, config: Rc<OverlayConfig>) {
        self.config = config;
        let (ids, primary) = reconcile_selection(&self.config, &self.selected_widget_ids, self.selected_widget_id.as_deref());
        self.selected_widget_ids = ids;
        self.selected_widget_id = primary;
    }

    /// Edits the document in place (copy-on-write), with the same follow-up as [`Self::set_config`].
    pub fn edit_config<R>(&mut self, edit: impl FnOnce(&mut OverlayConfig) -> R) -> R {
        let mut next = (*self.config).clone();
        let result = edit(&mut next);
        self.set_config(next);
        result
    }

    /// The validated preference, without touching the document.
    pub fn initialize_unit_system(&mut self, system: UnitSystem) {
        self.unit_system = system;
    }

    /// Converts the open overlay, then switches the system.
    pub fn set_unit_system(&mut self, system: UnitSystem) {
        let converted = convert_config_units(&self.config, system);
        self.set_config(converted);
        self.unit_system = system;
    }

    pub fn set_fallback_duration_seconds(&mut self, duration: f64) {
        self.fallback_duration_seconds = duration;
    }

    /// A non-finite second becomes 0.
    pub fn set_selected_second(&mut self, second: f64) {
        self.selected_second = normalize_preview_second(second);
    }

    /// Requires finite `view_start < view_end`.
    pub fn set_timeline_viewport(&mut self, viewport: TimelineViewport) -> Result<(), String> {
        if !(viewport.view_start.is_finite() && viewport.view_end.is_finite() && viewport.view_start < viewport.view_end) {
            return Err("Timeline viewport requires finite viewStart < viewEnd".into());
        }
        self.timeline_viewport = viewport;
        Ok(())
    }

    pub fn start_preview_playback(&mut self, source: PlaybackSource, second: f64) {
        self.preview_playback_state = PreviewPlaybackState::Playing;
        self.preview_playback_source = source;
        self.selected_second = normalize_preview_second(second);
    }

    pub fn pause_preview_playback(&mut self, second: f64) {
        self.preview_playback_state = PreviewPlaybackState::Paused;
        self.selected_second = normalize_preview_second(second);
    }

    pub fn toggle_video_mute(&mut self) {
        self.is_video_muted = !self.is_video_muted;
    }

    /// Scrubbing at the second; an unchanged scrub sets nothing.
    pub fn update_preview_scrub(&mut self, second: f64) {
        let second = normalize_preview_second(second);
        if self.preview_playback_state == PreviewPlaybackState::Scrubbing && self.selected_second == second {
            return;
        }
        self.preview_playback_state = PreviewPlaybackState::Scrubbing;
        self.selected_second = second;
    }

    /// Paused at the second; an unchanged pause sets nothing.
    pub fn commit_preview_scrub(&mut self, second: f64) {
        let second = normalize_preview_second(second);
        if self.preview_playback_state == PreviewPlaybackState::Paused && self.selected_second == second {
            return;
        }
        self.preview_playback_state = PreviewPlaybackState::Paused;
        self.selected_second = second;
    }

    /// The requested ids that exist, in document order, with the preferred
    /// primary.
    pub fn set_widget_selection(&mut self, widget_ids: &[String], preferred: Option<&str>) {
        let ids = normalize_selection_ids(widget_ids, &self.config.widget_ids());
        self.selected_widget_id = primary_selection_id(&ids, preferred);
        self.selected_widget_ids = ids;
        // A click outside the map selects something else, which ends its view mode.
        self.map_view_widget_id = None;
    }

    pub fn set_selected_widget_id(&mut self, widget_id: Option<&str>) {
        let ids: Vec<String> = widget_id.map(|id| vec![id.to_string()]).unwrap_or_default();
        self.set_widget_selection(&ids, widget_id);
    }

    pub fn set_marquee_selecting(&mut self, selecting: bool) {
        self.is_marquee_selecting = selecting;
    }

    pub fn set_map_view_widget_id(&mut self, widget_id: Option<&str>) {
        self.map_view_widget_id = widget_id.map(str::to_string);
    }

    /// The chosen image's path, size and resource, replacing a load error, as one
    /// edit.
    pub fn set_raster_image(&mut self, widget_id: &str, path: &str, width: u32, height: u32, resource_id: &str) -> Result<(), String> {
        let raster = self.config.rasters.iter().find(|raster| raster.get("id").and_then(serde_json::Value::as_str) == Some(widget_id));
        let mut raster = raster.ok_or_else(|| format!("Raster widget {widget_id} is unavailable"))?.clone();
        raster.remove("resourceErrorCode");
        raster.insert("path".into(), path.into());
        raster.insert("width".into(), width.into());
        raster.insert("height".into(), height.into());
        raster.insert("resourceId".into(), resource_id.into());
        self.edit_config(|config| config.replace_widget(widget_id, &raster))
    }
}

const UNIT_SYSTEM_PREFERENCE_KEY: &str = "unitSystem";

/// An absent preference keeps metric; any value other than a known system fails startup.
pub fn hydrate_unit_system_preference(store: &mut Store, preferences: &PreferencesStore) -> Result<(), String> {
    match preferences.get(UNIT_SYSTEM_PREFERENCE_KEY) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(system)) => {
            store.editor.initialize_unit_system(system.parse::<UnitSystem>()?);
            Ok(())
        }
        Some(other) => Err(format!("Unknown unit system: {other}")),
    }
}

/// Converts the open overlay (one undo step) and persists the choice.
pub fn change_unit_system_preference(store: &mut Store, services: &mut Services, system: UnitSystem) {
    store.editor.set_unit_system(system);
    services.preferences.set(UNIT_SYSTEM_PREFERENCE_KEY, Value::String(system.name().to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use ovrley_widgets::model::document::OverlayConfig;

    fn config(labels: &[&str]) -> OverlayConfig {
        let labels: Vec<_> = labels.iter().map(|id| json!({"id": id, "text": id})).collect();
        OverlayConfig::from_value(&json!({ "labels": labels })).unwrap()
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn selection_follows_document_order_and_the_preferred_primary() {
        let mut editor = EditorState::default();
        editor.set_config(config(&["a", "b", "c"]));
        editor.set_widget_selection(&ids(&["c", "a", "x"]), Some("c"));
        assert_eq!(editor.selected_widget_ids, ids(&["a", "c"]));
        assert_eq!(editor.selected_widget_id.as_deref(), Some("c"));
        editor.set_selected_widget_id(None);
        assert!(editor.selected_widget_ids.is_empty());
        assert_eq!(editor.selected_widget_id, None);
    }

    #[test]
    fn a_replacement_keeps_surviving_selection() {
        let mut editor = EditorState::default();
        editor.set_config(config(&["a", "b", "c"]));
        editor.set_widget_selection(&ids(&["a", "b"]), Some("b"));
        editor.set_config(config(&["a", "c"]));
        assert_eq!(editor.selected_widget_ids, ids(&["a"]));
        assert_eq!(editor.selected_widget_id.as_deref(), Some("a"));
    }

    #[test]
    fn a_replacement_without_survivors_falls_back() {
        let mut editor = EditorState::default();
        editor.set_config(config(&["a", "b"]));
        assert_eq!(editor.selected_widget_ids, ids(&["b"]), "no previous selection selects the last widget");
        editor.set_selected_widget_id(Some("b"));
        editor.set_config(config(&["c", "d"]));
        assert_eq!(editor.selected_widget_ids, ids(&["c"]), "a lost primary falls back to the first widget");
        editor.set_config(config(&[]));
        assert!(editor.selected_widget_ids.is_empty());
    }

    #[test]
    fn the_unit_system_converts_the_open_overlay() {
        let mut editor = EditorState::default();
        editor.set_config(OverlayConfig::from_value(&json!({"values": [{"id": "s", "value": "speed", "display_unit": "kmh"}]})).unwrap());
        editor.set_unit_system(UnitSystem::Imperial);
        assert_eq!(editor.config.values[0]["display_unit"], "mph");
        assert_eq!(editor.unit_system, UnitSystem::Imperial);
    }

    #[test]
    fn scrubs_and_commits_set_only_what_changed() {
        let mut editor = EditorState::default();
        editor.update_preview_scrub(4.0);
        assert_eq!((editor.preview_playback_state, editor.selected_second), (PreviewPlaybackState::Scrubbing, 4.0));
        let before = editor.clone();
        editor.update_preview_scrub(4.0);
        assert_eq!(editor, before, "an unchanged scrub sets nothing");
        editor.commit_preview_scrub(f64::NAN);
        assert_eq!((editor.preview_playback_state, editor.selected_second), (PreviewPlaybackState::Paused, 0.0));
        editor.start_preview_playback(PlaybackSource::Video, 7.0);
        assert_eq!((editor.preview_playback_state, editor.preview_playback_source, editor.selected_second), (PreviewPlaybackState::Playing, PlaybackSource::Video, 7.0));
        editor.pause_preview_playback(9.0);
        assert_eq!((editor.preview_playback_state, editor.selected_second), (PreviewPlaybackState::Paused, 9.0));
    }

    #[test]
    fn the_viewport_must_be_a_finite_ordered_range() {
        let mut editor = EditorState::default();
        assert!(editor.set_timeline_viewport(TimelineViewport { view_start: 5.0, view_end: 5.0 }).is_err());
        assert!(editor.set_timeline_viewport(TimelineViewport { view_start: 0.0, view_end: f64::INFINITY }).is_err());
        editor.set_timeline_viewport(TimelineViewport { view_start: -2.0, view_end: 10.0 }).unwrap();
        assert_eq!(editor.timeline_viewport.view_start, -2.0);
    }

    #[test]
    fn selected_second_rejects_non_finite_values() {
        let mut editor = EditorState::default();
        editor.set_selected_second(f64::NAN);
        assert_eq!(editor.selected_second, 0.0);
        editor.set_selected_second(12.5);
        assert_eq!(editor.selected_second, 12.5);
    }

    fn hydrated(preferences: Value) -> Result<Store, String> {
        let (parts, _) = fake_services(preferences);
        Store::hydrate(&parts.preferences)
    }

    #[test]
    fn startup_reads_the_unit_system_strictly() {
        assert_eq!(hydrated(json!({})).unwrap().editor.unit_system, UnitSystem::Metric);
        assert_eq!(hydrated(json!({"unitSystem": "imperial"})).unwrap().editor.unit_system, UnitSystem::Imperial);
        assert_eq!(hydrated(json!({"unitSystem": "nautical"})).unwrap_err(), "Unknown unit system: nautical");
        assert!(hydrated(json!({"unitSystem": 3})).is_err());
    }

    #[test]
    fn a_change_converts_the_overlay_as_one_undo_step_and_persists() {
        let (parts, fakes) = fake_services(json!({}));
        let store = Store::hydrate(&parts.preferences).unwrap();
        let dispatcher = Dispatcher::new(store, parts);
        dispatcher.dispatch(|store, _| {
            store.editor.set_config(OverlayConfig::from_value(&json!({"values": [{"id": "s", "value": "speed", "display_unit": "kmh"}]})).unwrap());
        });
        dispatcher.dispatch(|store, _| store.history.past.clear());
        dispatcher.dispatch(|store, services| change_unit_system_preference(store, services, UnitSystem::Imperial));
        dispatcher.with_store(|store| {
            assert_eq!(store.editor.config.values[0]["display_unit"], "mph");
            assert_eq!(store.history.past.len(), 1);
        });
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()["unitSystem"], "imperial");
    }
}
