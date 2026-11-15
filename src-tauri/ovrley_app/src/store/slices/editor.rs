//! Port of `store/slices/createEditorSlice.js`, Phase 3 part: the document, selection, scene
//! timing and unit system. Playback and the timeline viewport arrive with Phase 4, render state
//! with Phase 7, the map view mode with Phase 5.

use crate::store::sync::{sync_scene_timing_from_config, sync_scene_timing_to_config};
use ovrley_core::normalize::template::template_manifest;
use ovrley_widgets::model::document::{convert_config_units, normalize_selection_ids, primary_selection_id, OverlayConfig, UnitSystem};
use std::rc::Rc;

/// `DEFAULT_CONFIG`: the default scene and empty collections.
pub fn default_config() -> OverlayConfig {
    OverlayConfig {
        scene: template_manifest().scene.clone(),
        ..OverlayConfig::default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorSlice {
    /// The document (D1), copy-on-write: a history snapshot is a pointer copy.
    pub config: Rc<OverlayConfig>,
    /// Incremented when a template load, import or new replaces the document (D6).
    pub editor_document_revision: u64,
    pub selected_widget_id: Option<String>,
    /// In document order.
    pub selected_widget_ids: Vec<String>,
    pub is_marquee_selecting: bool,
    pub fallback_duration_seconds: f64,
    pub start_second: f64,
    pub end_second: f64,
    pub selected_second: f64,
    /// The system new widgets start in; not part of the document or its history.
    pub unit_system: UnitSystem,
}

impl Default for EditorSlice {
    fn default() -> Self {
        Self {
            config: Rc::new(default_config()),
            editor_document_revision: 0,
            selected_widget_id: None,
            selected_widget_ids: Vec::new(),
            is_marquee_selecting: false,
            fallback_duration_seconds: 73.0,
            start_second: 0.0,
            end_second: 73.0,
            selected_second: 0.0,
            unit_system: UnitSystem::Metric,
        }
    }
}

/// `reconcileSelection`: after a config replacement, keep the selected ids that still exist;
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

impl EditorSlice {
    /// `setConfig`: replaces the document, reconciles the selection and syncs the scene timing.
    pub fn set_config(&mut self, config: OverlayConfig) {
        self.replace_config(Rc::new(config));
    }

    /// [`Self::set_config`] with a shared document (undo and redo restore pointers).
    pub fn replace_config(&mut self, config: Rc<OverlayConfig>) {
        let previous = std::mem::replace(&mut self.config, config);
        let (ids, primary) = reconcile_selection(&self.config, &self.selected_widget_ids, self.selected_widget_id.as_deref());
        self.selected_widget_ids = ids;
        self.selected_widget_id = primary;
        sync_scene_timing_from_config(self, Some(&previous), false);
    }

    /// Edits the document in place (copy-on-write), with `setConfig`'s reconcile steps.
    pub fn edit_config<R>(&mut self, edit: impl FnOnce(&mut OverlayConfig) -> R) -> R {
        let mut next = (*self.config).clone();
        let result = edit(&mut next);
        self.set_config(next);
        result
    }

    /// `initializeUnitSystem`: the validated preference, without touching the document.
    pub fn initialize_unit_system(&mut self, system: UnitSystem) {
        self.unit_system = system;
    }

    /// `setUnitSystem`: converts the open overlay, then switches the system.
    pub fn set_unit_system(&mut self, system: UnitSystem) {
        let converted = convert_config_units(&self.config, system);
        self.set_config(converted);
        self.unit_system = system;
    }

    pub fn set_fallback_duration_seconds(&mut self, duration: f64) {
        self.fallback_duration_seconds = duration;
    }

    pub fn set_start_second(&mut self, second: f64) {
        if self.start_second == second {
            return;
        }
        self.start_second = second;
        sync_scene_timing_to_config(self, Some(second), None);
    }

    pub fn set_end_second(&mut self, second: f64) {
        if self.end_second == second {
            return;
        }
        self.end_second = second;
        sync_scene_timing_to_config(self, None, Some(second));
    }

    /// `setSelectedSecond`; a non-finite second becomes 0.
    pub fn set_selected_second(&mut self, second: f64) {
        self.selected_second = if second.is_finite() { second } else { 0.0 };
    }

    /// `setWidgetSelection`: the requested ids that exist, in document order, with the preferred
    /// primary.
    pub fn set_widget_selection(&mut self, widget_ids: &[String], preferred: Option<&str>) {
        let ids = normalize_selection_ids(widget_ids, &self.config.widget_ids());
        self.selected_widget_id = primary_selection_id(&ids, preferred);
        self.selected_widget_ids = ids;
    }

    /// `setSelectedWidgetId`.
    pub fn set_selected_widget_id(&mut self, widget_id: Option<&str>) {
        let ids: Vec<String> = widget_id.map(|id| vec![id.to_string()]).unwrap_or_default();
        self.set_widget_selection(&ids, widget_id);
    }

    pub fn set_marquee_selecting(&mut self, selecting: bool) {
        self.is_marquee_selecting = selecting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(labels: &[&str]) -> OverlayConfig {
        let labels: Vec<_> = labels.iter().map(|id| json!({"id": id, "text": id})).collect();
        OverlayConfig::from_value(&json!({ "labels": labels })).unwrap()
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn selection_follows_document_order_and_the_preferred_primary() {
        let mut editor = EditorSlice::default();
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
        let mut editor = EditorSlice::default();
        editor.set_config(config(&["a", "b", "c"]));
        editor.set_widget_selection(&ids(&["a", "b"]), Some("b"));
        editor.set_config(config(&["a", "c"]));
        assert_eq!(editor.selected_widget_ids, ids(&["a"]));
        assert_eq!(editor.selected_widget_id.as_deref(), Some("a"));
    }

    #[test]
    fn a_replacement_without_survivors_falls_back() {
        let mut editor = EditorSlice::default();
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
        let mut editor = EditorSlice::default();
        editor.set_config(OverlayConfig::from_value(&json!({"values": [{"id": "s", "value": "speed", "display_unit": "kmh"}]})).unwrap());
        editor.set_unit_system(UnitSystem::Imperial);
        assert_eq!(editor.config.values[0]["display_unit"], "mph");
        assert_eq!(editor.unit_system, UnitSystem::Imperial);
    }

    #[test]
    fn selected_second_rejects_non_finite_values() {
        let mut editor = EditorSlice::default();
        editor.set_selected_second(f64::NAN);
        assert_eq!(editor.selected_second, 0.0);
        editor.set_selected_second(12.5);
        assert_eq!(editor.selected_second, 12.5);
    }
}
