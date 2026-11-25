//! The window's shell: the `AppShell` global (header, settings, startup screen, keys, imports),
//! the error alert, the update prompt and undo and redo.

use crate::platform::keys::Modifiers;
use crate::platform::SlintWindowControl;
use crate::projection::{convert_enum, set_if_changed, strings};
use crate::{
    AlertLayer, AppShell, AppUpdates, AppWindow, Choice, ChoiceKind, SettingsSection,
    ShortcutGroupView, ShortcutOptionView, ShortcutRowView, UndoRedo, UpdatePromptPhase,
};
use ovrley_app::features::shell::{
    self, keyboard_shortcut_groups, AppBootstrapState, SettingsSection as AppSettingsSection, UpdatePromptView,
};
use ovrley_app::features::maps;
use ovrley_app::features::history::{history_enabled, redo, undo};
use ovrley_app::i18n::{translate_backend_error, Locale, LOCALES};
use ovrley_app::format::format_cache_bytes;
use ovrley_app::shortcuts::KeyInput;
use ovrley_app::store::editor::change_unit_system_preference;
use ovrley_app::store::services::WindowControl;
use ovrley_app::store::{Dispatcher, Store};
use ovrley_widgets::model::document::UnitSystem;
use slint::{ComponentHandle, ModelRc, VecModel};

convert_enum!(SettingsSection <=> AppSettingsSection { Settings, Shortcuts });

/// Bytes in a MiB. The cache slider runs in MiB, which a Slint float holds exactly.
const MIB: f64 = 1024.0 * 1024.0;

/// Writes what never changes: the versions, the language list.
pub fn initialize(window: &AppWindow) {
    let shell = window.global::<AppShell>();
    shell.set_version(ovrley_app::features::shell::app_version().into());
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
    window
        .global::<AppUpdates>()
        .set_current_version(ovrley_app::features::shell::VERSION.into());
}

/// Re-translates what Rust chooses at runtime: the shortcut descriptions.
pub fn project_language(window: &AppWindow, language: Locale) {
    let groups: Vec<ShortcutGroupView> = keyboard_shortcut_groups(language)
        .into_iter()
        .map(|group| {
            let rows: Vec<ShortcutRowView> = group
                .shortcuts
                .into_iter()
                .map(|row| {
                    let options: Vec<ShortcutOptionView> = row
                        .options
                        .iter()
                        .map(|option| ShortcutOptionView {
                            modifiers: strings(&option.modifiers),
                            keys: strings(&option.keys),
                        })
                        .collect();
                    ShortcutRowView {
                        description: row.description.into(),
                        options: ModelRc::new(VecModel::from(options)),
                    }
                })
                .collect();
            ShortcutGroupView {
                name: group.name.into(),
                rows: ModelRc::new(VecModel::from(rows)),
            }
        })
        .collect();
    window
        .global::<AppShell>()
        .set_shortcut_groups(ModelRc::new(VecModel::from(groups)));
}

pub fn project(window: &AppWindow, store: &Store) {
    let shell = window.global::<AppShell>();
    set_if_changed(
        shell.get_show_startup_screen(),
        AppBootstrapState::shows_startup_screen(store),
        |value| shell.set_show_startup_screen(value),
    );
    let startup_error = store.bootstrap.startup_error.clone().unwrap_or_default();
    set_if_changed(shell.get_startup_error(), startup_error.into(), |value| {
        shell.set_startup_error(value)
    });
    set_if_changed(
        shell.get_traffic_light_inset(),
        store.traffic_light_inset.has_inset(),
        |value| shell.set_traffic_light_inset(value),
    );
    set_if_changed(
        shell.get_allow_agent_edits(),
        store.agent_session.allow_agent_edits,
        |value| shell.set_allow_agent_edits(value),
    );
    set_if_changed(
        shell.get_map_tile_cache(),
        map_tile_cache_view(store),
        |value| shell.set_map_tile_cache(value),
    );
    set_if_changed(
        shell.get_agent_connected(),
        store.agent_session.shows_indicator(),
        |value| shell.set_agent_connected(value),
    );
    set_if_changed(
        shell.get_file_drag_hovering(),
        store.file_drop.hovering,
        |value| shell.set_file_drag_hovering(value),
    );
    set_if_changed(shell.get_processing(), store.media.is_processing, |value| {
        shell.set_processing(value)
    });
    set_if_changed(
        shell.get_importing_video(),
        store.media.importing_video,
        |value| shell.set_importing_video(value),
    );
    let editor_shell = &store.editor_shell;
    set_if_changed(
        shell.get_settings_open(),
        editor_shell.settings_open,
        |value| shell.set_settings_open(value),
    );
    set_if_changed(
        shell.get_settings_section(),
        editor_shell.settings_section.into(),
        |value| shell.set_settings_section(value),
    );
    let language = store.i18n.language;
    set_if_changed(shell.get_language(), language.code().into(), |value| {
        shell.set_language(value)
    });
    set_if_changed(
        shell.get_unit_system(),
        store.editor.unit_system.name().into(),
        |value| shell.set_unit_system(value),
    );

    let alert = window.global::<AlertLayer>();
    set_if_changed(
        alert.get_shown(),
        store.media.error_message.is_some(),
        |value| alert.set_shown(value),
    );
    if let Some(message) = &store.media.error_message {
        set_if_changed(
            alert.get_message(),
            translate_backend_error(message, language).into(),
            |value| alert.set_message(value),
        );
    }

    project_update_prompt(window, store);

    let history = window.global::<UndoRedo>();
    let enabled = history_enabled(store);
    set_if_changed(
        history.get_can_undo(),
        enabled && store.history.can_undo(),
        |value| history.set_can_undo(value),
    );
    set_if_changed(
        history.get_can_redo(),
        enabled && store.history.can_redo(),
        |value| history.set_can_redo(value),
    );
}

fn project_update_prompt(window: &AppWindow, store: &Store) {
    let global = window.global::<AppUpdates>();
    let phase = match shell::update_prompt_view(store) {
        UpdatePromptView::Hidden => UpdatePromptPhase::Hidden,
        UpdatePromptView::Available {
            version,
            before,
            after,
        } => {
            set_if_changed(global.get_version(), version.into(), |value| {
                global.set_version(value)
            });
            set_if_changed(global.get_announcement_before(), before.into(), |value| {
                global.set_announcement_before(value)
            });
            set_if_changed(global.get_announcement_after(), after.into(), |value| {
                global.set_announcement_after(value)
            });
            UpdatePromptPhase::Available
        }
        UpdatePromptView::Downloading { version, progress } => {
            set_if_changed(global.get_version(), version.into(), |value| {
                global.set_version(value)
            });
            set_if_changed(global.get_determinate(), progress.is_some(), |value| {
                global.set_determinate(value)
            });
            if let Some((percent, label)) = progress {
                set_if_changed(global.get_progress(), percent as f32, |value| {
                    global.set_progress(value)
                });
                set_if_changed(global.get_progress_label(), label.into(), |value| {
                    global.set_progress_label(value)
                });
            }
            UpdatePromptPhase::Downloading
        }
        UpdatePromptView::Failed => UpdatePromptPhase::Failed,
    };
    set_if_changed(global.get_phase(), phase, |value| global.set_phase(value));
}

/// Settings → Map tile cache, hidden until its usage has loaded.
fn map_tile_cache_view(store: &Store) -> crate::MapTileCacheView {
    let cache = &store.maps.tile_cache;
    let Some(usage) = cache.usage else {
        return crate::MapTileCacheView::default();
    };
    let limit = cache.limit().expect("a loaded cache has a limit");
    let mib = |bytes: u64| (bytes as f64 / MIB) as f32;
    crate::MapTileCacheView {
        shown: true,
        used: format_cache_bytes(usage.used_bytes).into(),
        limit: format_cache_bytes(usage.limit_bytes).into(),
        used_percent: (usage.used_bytes as f64 / usage.limit_bytes as f64 * 100.0) as f32,
        value_mib: mib(limit),
        min_mib: mib(usage.min_limit_bytes),
        max_mib: mib(usage.max_limit_bytes),
        step_mib: mib(usage.limit_step_bytes),
        value_display: format_cache_bytes(limit).into(),
        clear_disabled: cache.pending || usage.used_bytes == 0,
    }
}

fn mib_to_bytes(mib: f32) -> u64 {
    (f64::from(mib) * MIB).round() as u64
}

pub fn bind(window: &AppWindow, dispatcher: &Dispatcher) {
    let shell = window.global::<AppShell>();
    let d = dispatcher.handle();
    shell.on_open_settings(move || d.dispatch(|store, _| store.editor_shell.open_settings()));
    let d = dispatcher.handle();
    shell.on_close_settings(move || d.dispatch(|store, _| store.editor_shell.close_settings()));
    let d = dispatcher.handle();
    shell.on_select_settings_section(move |section| {
        d.dispatch(move |store, _| store.editor_shell.set_settings_section(section.into()))
    });
    let d = dispatcher.handle();
    shell.on_change_language(move |code| {
        // The select lists only `LOCALES`, so an unknown code is a bug, not user input.
        let (locale, _) = LOCALES
            .iter()
            .find(|(locale, _)| locale.code() == code.as_str())
            .expect("the language select offers supported locales");
        let locale = *locale;
        d.dispatch(move |store, services| {
            ovrley_app::i18n::change_language_preference(store, services, locale)
        });
    });
    let d = dispatcher.handle();
    shell.on_change_unit_system(move |system| {
        // The select lists only the two systems, so another value is a bug, not user input.
        let system =
            UnitSystem::require(&system).expect("the units select offers the unit systems");
        d.dispatch(move |store, services| change_unit_system_preference(store, services, system));
    });

    let d = dispatcher.handle();
    shell.on_map_tile_cache_opened(move || d.dispatch(maps::load_tile_cache));
    let d = dispatcher.handle();
    shell.on_map_tile_cache_draft(move |mib| {
        d.dispatch(move |store, _| maps::draft_tile_cache_limit(store, mib_to_bytes(mib)))
    });
    let d = dispatcher.handle();
    shell.on_map_tile_cache_commit(move |mib| {
        d.dispatch(move |store, services| {
            maps::commit_tile_cache_limit(store, services, mib_to_bytes(mib))
        })
    });
    let d = dispatcher.handle();
    shell.on_map_tile_cache_clear(move || d.dispatch(maps::clear_tile_cache));
    let d = dispatcher.handle();
    shell.on_set_allow_agent_edits(move |allowed| {
        d.dispatch(move |store, services| {
            shell::set_allow_agent_edits(store, services, allowed)
        })
    });

    // Minimise and maximise change no app state: they call the window directly.
    let window_control = SlintWindowControl {
        window: window.as_weak(),
    };
    shell.on_minimize_window(move || window_control.minimize());
    let window_control = SlintWindowControl {
        window: window.as_weak(),
    };
    shell.on_toggle_maximize_window(move || window_control.toggle_maximize());
    let d = dispatcher.handle();
    shell.on_close_window(move || d.dispatch(shell::request_close));

    let d = dispatcher.handle();
    shell.on_app_key(move |text, alt, control, shift, meta, repeat| {
        let input = KeyInput {
            repeat,
            ..Modifiers {
                alt,
                control,
                shift,
                meta,
            }
            .key_input(&text)
        };
        d.dispatch_with_result(move |store, services| {
            shell::handle_window_key(store, services, &input)
        })
        .unwrap_or(false)
    });
    let d = dispatcher.handle();
    shell.on_app_key_released(move |text, alt, control, shift, meta| {
        let input = Modifiers {
            alt,
            control,
            shift,
            meta,
        }
        .key_input(&text);
        d.dispatch_with_result(move |store, _| {
            ovrley_app::features::player::handle_key_release(store, &input)
        })
        .unwrap_or(false)
    });
    let d = dispatcher.handle();
    shell.on_capture_key(move |text, alt, control, shift, meta, repeat| {
        let input = KeyInput {
            repeat,
            ..Modifiers {
                alt,
                control,
                shift,
                meta,
            }
            .key_input(&text)
        };
        d.dispatch_with_result(move |store, _| shell::handle_window_capture_key(store, &input))
            .unwrap_or(false)
    });

    let d = dispatcher.handle();
    shell.on_import_activity(move || d.dispatch(shell::open_activity_dialog));
    let d = dispatcher.handle();
    shell.on_import_videos(move || {
        d.dispatch(ovrley_app::features::import::open_video_import_dialog)
    });

    let alert = window.global::<AlertLayer>();
    let d = dispatcher.handle();
    alert.on_copy_requested(move |text| {
        let text = text.to_string();
        d.dispatch(move |_, services| {
            if let Err(error) = services.clipboard.write_text(&text) {
                log::warn!("could not copy the error: {error}");
            }
        });
    });
    let d = dispatcher.handle();
    alert.on_dismiss_requested(move || d.dispatch(|store, _| store.media.clear_error()));

    let updates = window.global::<AppUpdates>();
    let d = dispatcher.handle();
    updates.on_update_now(move || d.dispatch(shell::install_update));
    let d = dispatcher.handle();
    updates.on_dismiss(move || d.dispatch(|store, _| shell::dismiss(store)));

    let history = window.global::<UndoRedo>();
    let d = dispatcher.handle();
    history.on_undo(move || {
        d.dispatch(|store, _| {
            if history_enabled(store) {
                undo(store);
            }
        });
    });
    let d = dispatcher.handle();
    history.on_redo(move || {
        d.dispatch(|store, _| {
            if history_enabled(store) {
                redo(store);
            }
        });
    });
}
