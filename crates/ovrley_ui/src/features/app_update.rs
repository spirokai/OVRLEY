//! The app-update feature: its projection into the `AppUpdates` global and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppUpdates, AppWindow, UpdatePromptPhase};
use ovrley_app::features::app_update::{self, UpdatePromptView};
use ovrley_app::store::{Dispatcher, Store};
use slint::ComponentHandle;

pub fn project(window: &AppWindow, store: &Store) {
    let global = window.global::<AppUpdates>();
    let phase = match app_update::update_prompt_view(store) {
        UpdatePromptView::Hidden => UpdatePromptPhase::Hidden,
        UpdatePromptView::Available { version, before, after } => {
            set_if_changed(global.get_version(), version.into(), |value| global.set_version(value));
            set_if_changed(global.get_announcement_before(), before.into(), |value| global.set_announcement_before(value));
            set_if_changed(global.get_announcement_after(), after.into(), |value| global.set_announcement_after(value));
            UpdatePromptPhase::Available
        }
        UpdatePromptView::Downloading { version, progress } => {
            set_if_changed(global.get_version(), version.into(), |value| global.set_version(value));
            set_if_changed(global.get_determinate(), progress.is_some(), |value| global.set_determinate(value));
            if let Some((percent, label)) = progress {
                set_if_changed(global.get_progress(), percent as f32, |value| global.set_progress(value));
                set_if_changed(global.get_progress_label(), label.into(), |value| global.set_progress_label(value));
            }
            UpdatePromptPhase::Downloading
        }
        UpdatePromptView::Failed => UpdatePromptPhase::Failed,
    };
    set_if_changed(global.get_phase(), phase, |value| global.set_phase(value));
}

/// Writes what never changes: the running version.
pub fn initialize(window: &AppWindow) {
    window.global::<AppUpdates>().set_current_version(ovrley_app::lib::utils::VERSION.into());
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let global = window.global::<AppUpdates>();
    let dispatch = dispatcher.handle();
    global.on_update_now(move || dispatch.dispatch(app_update::install_update));
    let dispatch = dispatcher.handle();
    global.on_dismiss(move || dispatch.dispatch(|store, _| app_update::dismiss(store)));
}
