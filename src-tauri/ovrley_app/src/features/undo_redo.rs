//! Port of `features/undo-redo` (D6): the editor history and the undo and redo actions.
//!
//! zundo records one entry per store `set`; natively the dispatcher records at most one per
//! action. It compares the undoable projection (`partializeUndoableState`) before and after the
//! action and its reconcile steps: pointers first (D1), then values.

use crate::store::Store;
use ovrley_widgets::model::document::OverlayConfig;
use serde_json::{Map, Value};
use std::rc::Rc;

/// `HISTORY_LIMIT`.
pub const HISTORY_LIMIT: usize = 20;

/// `partializeUndoableState`, Phase 3 part. `renderSettings` (Phase 7) and
/// `videoSyncOffsetSeconds` (Phase 8) join with their phases.
#[derive(Clone, Debug)]
pub struct UndoableState {
    pub config: Rc<OverlayConfig>,
    pub global_defaults: Rc<Map<String, Value>>,
    pub aspect_ratio: String,
    pub start_second: f64,
    pub end_second: f64,
}

impl UndoableState {
    pub fn of(store: &Store) -> Self {
        Self {
            config: Rc::clone(&store.editor.config),
            global_defaults: Rc::clone(&store.template.global_defaults),
            aspect_ratio: store.template.aspect_ratio.clone(),
            start_second: store.editor.start_second,
            end_second: store.editor.end_second,
        }
    }

    /// `deepEqual`, with the pointer comparison first.
    pub fn same_as(&self, other: &Self) -> bool {
        (Rc::ptr_eq(&self.config, &other.config) || self.config == other.config)
            && (Rc::ptr_eq(&self.global_defaults, &other.global_defaults) || self.global_defaults == other.global_defaults)
            && self.aspect_ratio == other.aspect_ratio
            && self.start_second == other.start_second
            && self.end_second == other.end_second
    }

    fn restore(self, store: &mut Store) {
        store.template.global_defaults = self.global_defaults;
        store.template.aspect_ratio = self.aspect_ratio;
        store.editor.start_second = self.start_second;
        store.editor.end_second = self.end_second;
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

/// Whether history commands are available: not while a confirmation dialog is open
/// (`useUndoRedo({disabled})`). The render dialog joins in Phase 7.
pub fn history_enabled(store: &Store) -> bool {
    !store.unsaved_confirm.is_open()
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
            store.editor.set_end_second(40.0);
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
