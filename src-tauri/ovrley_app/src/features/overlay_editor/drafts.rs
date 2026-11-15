//! Port of `useWidgetDraftState` (D7): live widget drafts from the inspector, and the patches of a
//! canvas gesture. Both are hook state, outside the history; global-default drafts arrive with
//! scene settings in Phase 5.
//!
//! React publishes drafts once per animation frame. Natively each draft is a dispatch, and the
//! preview frame coalesces them (D9).

use crate::store::Store;
use ovrley_widgets::model::document::{merge_widget_data, OverlayConfig, WidgetData};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct WidgetDraftState {
    /// Data patches by widget id, merged as `mergeWidgetData` merges them.
    drafts: BTreeMap<String, Map<String, Value>>,
    /// The running canvas gesture's resolved patches by widget id; only the open inspector reads
    /// them (`useGestureWidget`).
    gesture_patches: BTreeMap<String, Map<String, Value>>,
    /// The document and selection the drafts were made against (the reconcile step's memory).
    seen_config: Option<Rc<OverlayConfig>>,
    seen_selection: Vec<String>,
    /// Counts changes to the drafts and patches, for the preview's dirty marking.
    revision: u64,
}

impl WidgetDraftState {
    /// Changes whenever a draft or a gesture patch changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// `setLiveWidgetDraft`.
    pub fn set_widget_draft(&mut self, widget_id: &str, updates: &Map<String, Value>) {
        let merged = merge_widget_data(self.drafts.get(widget_id), updates);
        self.drafts.insert(widget_id.to_string(), merged);
        self.touch();
    }

    /// `setLiveWidgetDrafts`.
    pub fn set_widget_drafts(&mut self, updates: &[(String, Map<String, Value>)]) {
        for (widget_id, patch) in updates {
            self.set_widget_draft(widget_id, patch);
        }
    }

    pub fn widget_draft(&self, widget_id: &str) -> Option<&Map<String, Value>> {
        self.drafts.get(widget_id)
    }

    pub fn drafts(&self) -> &BTreeMap<String, Map<String, Value>> {
        &self.drafts
    }

    /// `clearWidgetDrafts`.
    pub fn clear_widget_drafts(&mut self, widget_ids: &[String]) {
        for widget_id in widget_ids {
            if self.drafts.remove(widget_id).is_some() {
                self.touch();
            }
        }
    }

    /// `resetWidgetDrafts`.
    pub fn reset_widget_drafts(&mut self) {
        if !self.drafts.is_empty() {
            self.drafts.clear();
            self.touch();
        }
    }

    /// `setGesturePatches`.
    pub fn set_gesture_patches(&mut self, patches: BTreeMap<String, Map<String, Value>>) {
        self.gesture_patches = patches;
        self.touch();
    }

    /// `clearGesturePatches`.
    pub fn clear_gesture_patches(&mut self) {
        if !self.gesture_patches.is_empty() {
            self.gesture_patches.clear();
            self.touch();
        }
    }

    pub fn gesture_patch(&self, widget_id: &str) -> Option<&Map<String, Value>> {
        self.gesture_patches.get(widget_id)
    }

    /// A widget's data with its live draft applied (`applyWidgetDrafts`, before resolving the
    /// active display, which the effective data does).
    pub fn apply_draft(&self, widget_id: &str, data: &WidgetData) -> WidgetData {
        match self.drafts.get(widget_id) {
            Some(draft) => merge_widget_data(Some(data), draft),
            None => data.clone(),
        }
    }

    /// A widget's data with the running gesture's patch applied (`useGestureWidget`).
    pub fn apply_gesture(&self, widget_id: &str, data: &WidgetData) -> WidgetData {
        match self.gesture_patches.get(widget_id) {
            Some(patch) => merge_widget_data(Some(data), patch),
            None => data.clone(),
        }
    }
}

/// Reconcile step 2: a new document pointer clears the drafts (`useEffect(resetWidgetDrafts,
/// [config])`); a new document or selection ends the gesture's patches. The gesture itself is
/// cancelled by the overlay editor (step 7). Returns whether the document or selection changed.
pub fn reconcile_widget_drafts(store: &mut Store) -> bool {
    let config_changed = !store.drafts.seen_config.as_ref().is_some_and(|seen| Rc::ptr_eq(seen, &store.editor.config));
    let selection_changed = store.drafts.seen_selection != store.editor.selected_widget_ids;
    if config_changed {
        store.drafts.reset_widget_drafts();
        store.drafts.seen_config = Some(Rc::clone(&store.editor.config));
    }
    if config_changed || selection_changed {
        store.drafts.clear_gesture_patches();
        store.drafts.seen_selection = store.editor.selected_widget_ids.clone();
    }
    config_changed || selection_changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn drafts_merge_variants_and_apply_over_widget_data() {
        let mut drafts = WidgetDraftState::default();
        drafts.set_widget_draft("w", &map(json!({"display_variants": {"linear": {"width": 300}}})));
        drafts.set_widget_draft("w", &map(json!({"display_variants": {"linear": {"height": 40}}, "x": 5})));
        let data = map(json!({"id": "w", "value": "speed", "display_type": "linear", "display_variants": {"linear": {"width": 200, "rotation": 0}}}));
        let applied = drafts.apply_draft("w", &data);
        assert_eq!(applied["display_variants"]["linear"], json!({"width": 300, "rotation": 0, "height": 40}));
        assert_eq!(applied["x"], 5);
        assert_eq!(drafts.apply_draft("other", &data), data);
    }

    #[test]
    fn a_new_document_clears_drafts_and_a_new_selection_clears_gesture_patches() {
        let mut store = Store::default();
        reconcile_widget_drafts(&mut store);
        store.drafts.set_widget_draft("a", &map(json!({"x": 1})));
        store.drafts.set_gesture_patches(BTreeMap::from([("a".to_string(), map(json!({"x": 2})))]));
        assert!(!reconcile_widget_drafts(&mut store), "nothing changed");
        assert!(store.drafts.widget_draft("a").is_some());

        store.editor.set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "a"}, {"id": "b"}]})).unwrap());
        assert!(reconcile_widget_drafts(&mut store));
        assert!(store.drafts.widget_draft("a").is_none());
        assert!(store.drafts.gesture_patch("a").is_none());

        store.drafts.set_gesture_patches(BTreeMap::from([("a".to_string(), map(json!({"x": 2})))]));
        store.drafts.set_widget_draft("a", &map(json!({"x": 1})));
        store.editor.set_selected_widget_id(Some("a"));
        assert!(reconcile_widget_drafts(&mut store));
        assert!(store.drafts.gesture_patch("a").is_none());
        assert!(store.drafts.widget_draft("a").is_some(), "a selection change keeps the drafts");
    }
}
