//! Port of `features/undo-redo` (D6): the editor history and the undo and redo actions.
//!
//! zundo records one entry per store `set`; natively the dispatcher records at most one per
//! action. It compares the undoable projection (`partializeUndoableState`) before and after the
//! action and its reconcile steps: pointers first (D1), then values.

use ovrley_core::activity::trim::ActivityTrim;
use crate::lib::video_program::VideoEditsMap;
use ovrley_core::encode::render_settings::RenderSettings;
use crate::store::Store;
use ovrley_widgets::model::document::OverlayConfig;
use serde_json::{Map, Value};
use std::rc::Rc;

/// `HISTORY_LIMIT`.
pub const HISTORY_LIMIT: usize = 20;

/// `partializeUndoableState`.
#[derive(Clone, Debug)]
pub struct UndoableState {
    pub config: Rc<OverlayConfig>,
    pub global_defaults: Rc<Map<String, Value>>,
    pub aspect_ratio: String,
    pub render_settings: RenderSettings,
    pub video_edits: Rc<VideoEditsMap>,
    pub activity_trim: Option<ActivityTrim>,
}

impl UndoableState {
    pub fn of(store: &Store) -> Self {
        Self {
            config: Rc::clone(&store.editor.config),
            global_defaults: Rc::clone(&store.template.global_defaults),
            aspect_ratio: store.template.aspect_ratio.clone(),
            render_settings: store.render_settings.clone(),
            video_edits: Rc::clone(&store.video_import.video_edits),
            activity_trim: store.media.activity_trim,
        }
    }

    /// `deepEqual`, with the pointer comparison first.
    pub fn same_as(&self, other: &Self) -> bool {
        (Rc::ptr_eq(&self.config, &other.config) || self.config == other.config)
            && (Rc::ptr_eq(&self.global_defaults, &other.global_defaults) || self.global_defaults == other.global_defaults)
            && self.aspect_ratio == other.aspect_ratio
            && self.render_settings == other.render_settings
            && (Rc::ptr_eq(&self.video_edits, &other.video_edits) || self.video_edits == other.video_edits)
            && self.activity_trim == other.activity_trim
    }

    fn restore(self, store: &mut Store) {
        store.template.global_defaults = self.global_defaults;
        store.template.aspect_ratio = self.aspect_ratio;
        store.render_settings = self.render_settings;
        store.video_import.video_edits = self.video_edits;
        store.media.activity_trim = self.activity_trim;
        // zundo writes the fields back without `setConfig`. The selection is trimmed to widgets that
        // still exist, so it never names a removed widget (React leaves it stale).
        store.editor.config = self.config;
        let ids = store.editor.selected_widget_ids.clone();
        let primary = store.editor.selected_widget_id.clone();
        store.editor.set_widget_selection(&ids, primary.as_deref());
    }
}

/// How the dispatcher records the running action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HistoryMode {
    /// One entry when the undoable state changed (the default).
    #[default]
    Record,
    /// `runWithoutEditorHistory`: the change is not recorded.
    Skip,
    /// `replaceEditorDocument`: the history is cleared and the document revision increments.
    Reset,
}

#[derive(Clone, Debug, Default)]
pub struct EditorHistory {
    pub past: Vec<UndoableState>,
    pub future: Vec<UndoableState>,
    /// The running action's mode; back to `Record` after each dispatch.
    mode: HistoryMode,
}

impl EditorHistory {
    /// Asks the dispatcher to record the running action this way. `Reset` wins over `Skip`, which
    /// wins over `Record`, when an action asks more than once.
    pub fn set_mode(&mut self, mode: HistoryMode) {
        self.mode = match (self.mode, mode) {
            (HistoryMode::Reset, _) | (_, HistoryMode::Reset) => HistoryMode::Reset,
            (HistoryMode::Skip, _) | (_, HistoryMode::Skip) => HistoryMode::Skip,
            _ => HistoryMode::Record,
        };
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

/// The dispatcher's history step, after an action and its reconcile steps.
pub(crate) fn record(store: &mut Store, before: UndoableState) {
    let mode = std::mem::take(&mut store.history.mode);
    match mode {
        HistoryMode::Skip => {}
        HistoryMode::Reset => {
            store.history.past.clear();
            store.history.future.clear();
            store.editor.editor_document_revision += 1;
        }
        HistoryMode::Record => {
            if before.same_as(&UndoableState::of(store)) {
                return;
            }
            store.history.past.push(before);
            if store.history.past.len() > HISTORY_LIMIT {
                store.history.past.remove(0);
            }
            store.history.future.clear();
        }
    }
}

/// `changeVideoCollection` (D17): changes the imported-video collection outside the history, then
/// rebases every past and future snapshot: edits of videos that are gone are dropped, and videos
/// that are new (or `replaced`, a relinked file) take their current edits, so undo never brings
/// back a removed clip or edits that do not fit a file.
pub fn change_video_collection<R>(store: &mut Store, replaced_video_ids: &[&str], operation: impl FnOnce(&mut Store) -> R) -> R {
    let result = operation(store);
    store.history.set_mode(HistoryMode::Skip);
    let current = Rc::clone(&store.video_import.video_edits);
    let rebase = |snapshot: &mut UndoableState| {
        let rebased: VideoEditsMap = current
            .iter()
            .map(|(video_id, edits)| {
                let kept = (!replaced_video_ids.contains(&video_id.as_str())).then(|| snapshot.video_edits.get(video_id)).flatten();
                (video_id.clone(), kept.unwrap_or(edits).clone())
            })
            .collect();
        snapshot.video_edits = Rc::new(rebased);
    };
    store.history.past.iter_mut().for_each(rebase);
    store.history.future.iter_mut().for_each(rebase);
    result
}

/// Whether history commands are available: not while a confirmation dialog or the cut removal
/// question is open (`useUndoRedo({disabled})`). The render dialog joins in Phase 7.
pub fn history_enabled(store: &Store) -> bool {
    !store.unsaved_confirm.is_open() && store.video_import.cut_removal_request.is_none()
}

/// `undoHistory`. Returns whether a state was restored.
pub fn undo(store: &mut Store) -> bool {
    let Some(previous) = store.history.past.pop() else { return false };
    store.history.future.push(UndoableState::of(store));
    previous.restore(store);
    store.history.set_mode(HistoryMode::Skip);
    true
}

/// `redoHistory`. Returns whether a state was restored.
pub fn redo(store: &mut Store) -> bool {
    let Some(next) = store.history.future.pop() else { return false };
    store.history.past.push(UndoableState::of(store));
    next.restore(store);
    store.history.set_mode(HistoryMode::Skip);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::fake_services;
    use crate::store::Dispatcher;
    use serde_json::json;

    fn dispatcher() -> Dispatcher {
        let (parts, _) = fake_services(json!({}));
        Dispatcher::new(Store::default(), parts)
    }

    fn label_config(text: &str) -> OverlayConfig {
        OverlayConfig::from_value(&json!({"labels": [{"id": "a", "text": text}]})).unwrap()
    }

    fn text(dispatcher: &Dispatcher) -> String {
        dispatcher.with_store(|store| store.editor.config.labels.first().map(|label| label["text"].as_str().unwrap().to_string()).unwrap_or_default())
    }

    #[test]
    fn one_step_per_action_and_undo_redo_restore_it() {
        let dispatcher = dispatcher();
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("one")));
        dispatcher.dispatch(|store, _| {
            // Several edits in one action are one step.
            store.editor.set_config(label_config("x"));
            store.editor.set_config(label_config("two"));
        });
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2);
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(text(&dispatcher), "one");
        dispatcher.dispatch(|store, _| assert!(redo(store)));
        assert_eq!(text(&dispatcher), "two");
        assert_eq!(dispatcher.with_store(|store| (store.history.past.len(), store.history.future.len())), (2, 0));
    }

    #[test]
    fn an_action_that_changes_nothing_records_nothing() {
        let dispatcher = dispatcher();
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("one")));
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("one")));
        dispatcher.dispatch(|store, _| store.editor.set_selected_widget_id(Some("a")));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1);
    }

    #[test]
    fn a_new_edit_clears_the_future_and_the_limit_drops_the_oldest() {
        let dispatcher = dispatcher();
        for index in 0..25 {
            dispatcher.dispatch(move |store, _| store.editor.set_config(label_config(&index.to_string())));
        }
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), HISTORY_LIMIT);
        dispatcher.dispatch(|store, _| {
            undo(store);
        });
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("new")));
        assert!(!dispatcher.with_store(|store| store.history.can_redo()));
    }

    #[test]
    fn skip_does_not_record_and_reset_clears_and_bumps_the_revision() {
        let dispatcher = dispatcher();
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("one")));
        dispatcher.dispatch(|store, _| {
            store.history.set_mode(HistoryMode::Skip);
            store.media.set_activity_trim(Some(ActivityTrim { start_second: 1.0, end_second: 4.0 }));
        });
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1);
        dispatcher.dispatch(|store, _| {
            store.history.set_mode(HistoryMode::Reset);
            store.editor.set_config(label_config("loaded"));
        });
        dispatcher.with_store(|store| {
            assert!(!store.history.can_undo());
            assert_eq!(store.editor.editor_document_revision, 1);
        });
        dispatcher.dispatch(|store, _| store.editor.set_config(label_config("after")));
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 1, "the mode resets after each dispatch");
    }

    #[test]
    fn removing_a_video_rebases_the_history_so_undo_never_resurrects_it() {
        use crate::lib::video_program::fixtures::simple;
        use crate::store::slices::video_import::{remove_imported_video, set_video_offset, tests::store_with};
        let (parts, _) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(store_with(vec![simple("kept", 60.0, 0), simple("removed", 60.0, 1)], &[]), parts);
        dispatcher.dispatch(|store, _| set_video_offset(store, "kept", 5.0, false).unwrap());
        dispatcher.dispatch(|store, _| set_video_offset(store, "removed", 50.0, false).unwrap());
        dispatcher.dispatch(|store, _| {
            change_video_collection(store, &[], |store| {
                remove_imported_video(store, "removed").unwrap();
                set_video_offset(store, "kept", 7.0, false).unwrap();
            })
        });
        let offsets = |dispatcher: &Dispatcher| dispatcher.with_store(|store| store.video_import.video_edits.iter().map(|(id, edits)| (id.clone(), edits.offset_seconds)).collect::<Vec<_>>());
        assert_eq!(offsets(&dispatcher), [("kept".to_string(), 7.0)]);
        assert_eq!(dispatcher.with_store(|store| store.history.past.len()), 2, "the collection change is no step");
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(offsets(&dispatcher), [("kept".to_string(), 5.0)], "the removed clip stays removed");
        dispatcher.dispatch(|store, _| assert!(undo(store)));
        assert_eq!(offsets(&dispatcher), [("kept".to_string(), 0.0)]);
        dispatcher.dispatch(|store, _| assert!(!undo(store)));
    }

    #[test]
    fn undo_trims_the_selection_to_restored_widgets() {
        let dispatcher = dispatcher();
        dispatcher.dispatch(|store, _| store.editor.set_config(OverlayConfig::default()));
        dispatcher.dispatch(|store, _| {
            store.editor.set_config(label_config("one"));
            store.editor.set_selected_widget_id(Some("a"));
        });
        dispatcher.dispatch(|store, _| {
            undo(store);
        });
        dispatcher.with_store(|store| {
            assert!(store.editor.selected_widget_ids.is_empty());
            assert_eq!(store.editor.selected_widget_id, None);
        });
    }
}
