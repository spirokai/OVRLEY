//! The undo-redo feature: its projection into the `UndoRedo` global and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppWindow, UndoRedo};
use ovrley_app::features::undo_redo::{history_enabled, redo, undo};
use ovrley_app::store::{Dispatcher, Store};
use slint::ComponentHandle;

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<UndoRedo>();
    let enabled = history_enabled(store);
    set_if_changed(global.get_can_undo(), enabled && store.history.can_undo(), |value| global.set_can_undo(value));
    set_if_changed(global.get_can_redo(), enabled && store.history.can_redo(), |value| global.set_can_redo(value));
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<UndoRedo>();
    let dispatch = dispatcher.handle();
    global.on_undo(move || {
        dispatch.dispatch(|store, _| {
            if history_enabled(store) {
                undo(store);
            }
        })
    });
    let dispatch = dispatcher.handle();
    global.on_redo(move || {
        dispatch.dispatch(|store, _| {
            if history_enabled(store) {
                redo(store);
            }
        })
    });
}
