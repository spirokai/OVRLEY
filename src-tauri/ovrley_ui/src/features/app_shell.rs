//! The app-shell feature: its projection into the `AppShell` and `AlertLayer` globals, and its
//! callbacks.

use crate::hooks::app_store_selectors::set_if_changed;
use crate::platform::SlintWindowControl;
use crate::{AlertLayer, AppShell, AppWindow, Choice, ChoiceKind, SettingsSection, ShortcutGroupView, ShortcutOptionView, ShortcutRowView};
use ovrley_app::features::app_shell::{self, keyboard_shortcut_groups, AppBootstrapState, ShortcutGroup};
use ovrley_app::i18n::{translate_backend_error, Locale, LOCALES};
use ovrley_app::store::services::WindowControl;
use ovrley_app::store::{Dispatcher, Store};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// `v<VITE_OVRLEY_VERSION>`, read at compile time from the variable CI already sets, with React's
/// fallback.
fn app_version() -> String {
    let raw = option_env!("VITE_OVRLEY_VERSION").map(str::trim).filter(|version| !version.is_empty()).unwrap_or("0.00.0");
    if raw.starts_with('v') {
        raw.to_string()
    } else {
        format!("v{raw}")
    }
}

fn to_slint_section(section: app_shell::SettingsSection) -> SettingsSection {
    match section {
        app_shell::SettingsSection::Settings => SettingsSection::Settings,
        app_shell::SettingsSection::Shortcuts => SettingsSection::Shortcuts,
    }
}

fn from_slint_section(section: SettingsSection) -> app_shell::SettingsSection {
    match section {
        SettingsSection::Settings => app_shell::SettingsSection::Settings,
        SettingsSection::Shortcuts => app_shell::SettingsSection::Shortcuts,
    }
}

fn strings(values: &[String]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values.iter().map(SharedString::from).collect::<Vec<_>>()))
}

fn shortcut_groups_model(groups: Vec<ShortcutGroup>) -> ModelRc<ShortcutGroupView> {
    let groups: Vec<ShortcutGroupView> = groups
        .into_iter()
        .map(|group| ShortcutGroupView {
            name: group.name.into(),
            rows: ModelRc::new(VecModel::from(
                group
                    .shortcuts
                    .into_iter()
                    .map(|row| ShortcutRowView {
                        description: row.description.into(),
                        options: ModelRc::new(VecModel::from(
                            row.options
                                .iter()
                                .map(|option| ShortcutOptionView {
                                    modifiers: strings(&option.modifiers),
                                    keys: strings(&option.keys),
                                })
                                .collect::<Vec<_>>(),
                        )),
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect();
    ModelRc::new(VecModel::from(groups))
}

/// Writes what never changes: the version and the language list.
pub fn initialize(window: &AppWindow) {
    let shell = window.global::<AppShell>();
    shell.set_version(app_version().into());
    let languages: Vec<Choice> = LOCALES
        .iter()
        .map(|(locale, label)| Choice {
            kind: ChoiceKind::Item,
            value: locale.code().into(),
            label: (*label).into(),
            ..Default::default()
        })
        .collect();
    shell.set_languages(ModelRc::new(VecModel::from(languages)));
}

/// `language_changed` re-translates what Rust chooses at runtime: the shortcut descriptions.
pub fn project(window: &AppWindow, store: &Store, language_changed: bool) {
    let shell = window.global::<AppShell>();
    set_if_changed(shell.get_show_startup_screen(), AppBootstrapState::shows_startup_screen(store), |value| {
        shell.set_show_startup_screen(value)
    });
    let startup_error = store.bootstrap.startup_error.clone().unwrap_or_default();
    set_if_changed(shell.get_startup_error(), startup_error.into(), |value| shell.set_startup_error(value));
    set_if_changed(shell.get_traffic_light_inset(), store.traffic_light_inset.has_inset(), |value| {
        shell.set_traffic_light_inset(value)
    });

    let editor_shell = &store.editor_shell;
    set_if_changed(shell.get_settings_open(), editor_shell.settings_open, |value| shell.set_settings_open(value));
    set_if_changed(shell.get_settings_section(), to_slint_section(editor_shell.settings_section), |value| {
        shell.set_settings_section(value)
    });
    let language = store.i18n.language;
    set_if_changed(shell.get_language(), language.code().into(), |value| shell.set_language(value));
    if language_changed {
        shell.set_shortcut_groups(shortcut_groups_model(keyboard_shortcut_groups(language)));
    }

    let alert = window.global::<AlertLayer>();
    set_if_changed(alert.get_shown(), store.media.error_message.is_some(), |value| alert.set_shown(value));
    if let Some(message) = &store.media.error_message {
        set_if_changed(alert.get_message(), translate_backend_error(message, language).into(), |value| alert.set_message(value));
    }
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let shell = window.global::<AppShell>();

    let dispatch = dispatcher.handle();
    shell.on_open_settings(move || dispatch.dispatch(|store, _| store.editor_shell.open_settings()));
    let dispatch = dispatcher.handle();
    shell.on_close_settings(move || dispatch.dispatch(|store, _| store.editor_shell.close_settings()));
    let dispatch = dispatcher.handle();
    shell.on_select_settings_section(move |section| {
        let section = from_slint_section(section);
        dispatch.dispatch(move |store, _| store.editor_shell.set_settings_section(section));
    });
    let dispatch = dispatcher.handle();
    shell.on_change_language(move |code| {
        // The select lists only `LOCALES`, so an unknown code is a bug, not user input.
        let locale = Locale::require(&serde_json::Value::String(code.to_string())).expect("the language select offers supported locales");
        dispatch.dispatch(move |store, services| ovrley_app::i18n::change_language_preference(store, services, locale));
    });

    // Minimise and maximise change no app state: they call the window directly (D3).
    let window_control = SlintWindowControl { window: window.as_weak() };
    shell.on_minimize_window(move || window_control.minimize());
    let window_control = SlintWindowControl { window: window.as_weak() };
    shell.on_toggle_maximize_window(move || window_control.toggle_maximize());
    let dispatch = dispatcher.handle();
    shell.on_close_window(move || dispatch.dispatch(app_shell::request_close));

    let alert = window.global::<AlertLayer>();
    let dispatch = dispatcher.handle();
    alert.on_copy_requested(move |text| {
        let text = text.to_string();
        dispatch.dispatch(move |_, services| {
            if let Err(error) = services.clipboard.write_text(&text) {
                log::warn!("could not copy the error: {error}");
            }
        });
    });
    let dispatch = dispatcher.handle();
    alert.on_dismiss_requested(move || dispatch.dispatch(|store, _| store.media.clear_error()));
}
