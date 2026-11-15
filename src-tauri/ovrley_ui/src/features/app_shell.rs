//! The app-shell feature: its projection into the `AppShell` global and its callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::{AppShell, AppWindow};
use ovrley_app::features::app_shell::AppBootstrapState;
use ovrley_app::store::Store;
use slint::ComponentHandle;

pub fn project(window: &AppWindow, store: &Store) {
    let shell = window.global::<AppShell>();
    set_if_changed(shell.get_show_startup_screen(), AppBootstrapState::shows_startup_screen(store), |value| {
        shell.set_show_startup_screen(value)
    });
    let startup_error = store.bootstrap.startup_error.clone().unwrap_or_default();
    set_if_changed(shell.get_startup_error(), startup_error.into(), |value| shell.set_startup_error(value));
    set_if_changed(shell.get_traffic_light_inset(), store.traffic_light_inset.has_inset(), |value| {
        shell.set_traffic_light_inset(value)
    });
}
