//! The composed app window with fake services: keyboard routing and the drawer through the real
//! projections.

mod common;

use common::*;
use ovrley_app::store::services::fakes::fake_services;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_ui::{AppShell, AppWindow, Toolbar};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};

fn window() -> (AppWindow, Dispatcher) {
    let window = AppWindow::new().unwrap();
    let (parts, _fakes) = fake_services(serde_json::json!({}));
    let store = Store::hydrate(&parts.preferences).unwrap();
    let dispatcher = ovrley_ui::app::compose(&window, store, parts);
    dispatcher.dispatch(|store, _| store.bootstrap.set_startup_ready());
    window.show().unwrap();
    settle();
    (window, dispatcher)
}

/// Alt held around `text`, as the platform reports a chord.
fn alt_key(window: &AppWindow, text: &str) {
    let alt = SharedString::from(Key::Alt);
    window.window().dispatch_event(WindowEvent::KeyPressed { text: alt.clone() });
    key(window, text);
    window.window().dispatch_event(WindowEvent::KeyReleased { text: alt });
}

fn alt_w_toggles_the_drawer_and_escape_dismisses_it() {
    let (window, _dispatcher) = window();
    alt_key(&window, "w");
    settle();
    assert!(window.global::<Toolbar>().get_visible(), "Alt+W opens the widgets drawer");
    special_key(&window, Key::Escape);
    settle();
    assert!(!window.global::<Toolbar>().get_visible(), "Escape dismisses the unpinned drawer");
}

fn the_settings_dialog_blocks_shortcuts_and_returns_the_focus() {
    let (window, dispatcher) = window();
    dispatcher.dispatch(|store, _| store.editor_shell.open_settings());
    settle();
    alt_key(&window, "w");
    settle();
    assert!(!window.global::<Toolbar>().get_visible(), "the dialog blocks app shortcuts");

    special_key(&window, Key::Escape);
    settle();
    assert!(!window.global::<AppShell>().get_settings_open(), "Escape closes the dialog");
    alt_key(&window, "w");
    settle();
    assert!(window.global::<Toolbar>().get_visible(), "the window has the focus again");
}

fn the_update_dialog_hides_the_startup_dialog_and_blocks_shortcuts() {
    use ovrley_app::features::shell::UpdatePhase;
    use ovrley_ui::{AppUpdates, Projects};
    let (window, dispatcher) = window();
    dispatcher.dispatch(|store, _| {
        store.projects.startup.open = true;
        store.app_update.phase = UpdatePhase::Failed;
    });
    settle();
    assert!(!window.global::<Projects>().get_startup_open(), "the update dialog comes first");
    alt_key(&window, "w");
    settle();
    assert!(!window.global::<Toolbar>().get_visible(), "the dialog blocks app shortcuts");

    special_key(&window, Key::Escape);
    settle();
    assert!(!window.global::<AppUpdates>().get_open(), "Escape closes a failed update");
    assert!(window.global::<Projects>().get_startup_open(), "the startup dialog returns");
}

fn a_pinned_drawer_ignores_escape() {
    let (window, dispatcher) = window();
    dispatcher.dispatch(|store, _| store.layout.set_left_drawer_pinned(true));
    settle();
    special_key(&window, Key::Escape);
    settle();
    assert!(window.global::<Toolbar>().get_visible());
}

#[test]
fn cases() {
    run_cases(&[
        ("alt_w_toggles_the_drawer_and_escape_dismisses_it", alt_w_toggles_the_drawer_and_escape_dismisses_it),
        ("the_settings_dialog_blocks_shortcuts_and_returns_the_focus", the_settings_dialog_blocks_shortcuts_and_returns_the_focus),
        ("the_update_dialog_hides_the_startup_dialog_and_blocks_shortcuts", the_update_dialog_hides_the_startup_dialog_and_blocks_shortcuts),
        ("a_pinned_drawer_ignores_escape", a_pinned_drawer_ignores_escape),
    ]);
}
