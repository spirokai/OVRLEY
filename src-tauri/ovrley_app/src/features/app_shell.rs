//! The app-shell feature's hooks: `useAppBootstrap` (its shell part), `useEditorShellState` (its
//! shell fields), `useWindowCloseGuard` and `useTrafficLightInset`; and `getKeyboardShortcutGroups`.
//!
//! React keeps each in a hook's state; here each is a struct in the [`Store`], named after its
//! hook, and its actions are methods on it (plan D1, D2).

use crate::i18n::{catalog, Locale};
use crate::lib::keyboard_shortcuts::{manifest, Binding, Modifier};
use crate::lib::utils::IS_MACOS;
use crate::store::services::Services;
use crate::store::Store;

/// `useAppBootstrap`'s shell part. Phase 3 adds template fetching and restoring, Phase 7 codec
/// detection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppBootstrapState {
    /// Startup work has finished; until then the window shows the startup screen.
    pub startup_ready: bool,
    /// A startup failure once the window exists (a malformed `lng`). The window shows it as a
    /// full-window page, as `main.jsx` does.
    pub startup_error: Option<String>,
}

impl AppBootstrapState {
    /// Runs on the first event-loop turn.
    pub fn set_startup_ready(&mut self) {
        self.startup_ready = true;
    }

    /// The startup screen shows while the drawer preference is not hydrated or startup work runs
    /// (`!toolbarDrawer.initialized || !startupReady`).
    pub fn shows_startup_screen(store: &Store) -> bool {
        !store.layout.left_drawer_initialized || !store.bootstrap.startup_ready
    }
}

/// `useWindowCloseGuard`: a close request is handled once, and asks the guards before the window
/// closes. Phase 2 has no guard that objects; Phase 6 adds the unsaved-project check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowCloseGuardState {
    /// `handlingClose`: a second request while one is handled is ignored.
    pub handling_close: bool,
}

/// The close request of the window's close button, the title bar or the OS.
pub fn request_close(store: &mut Store, services: &mut Services) {
    if store.close_guard.handling_close {
        return;
    }
    store.close_guard.handling_close = true;
    services.window.close();
}

/// `useTrafficLightInset`: on macOS, the header leaves room for the traffic lights while the title
/// bar overlays it, that is while the window is not fullscreen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficLightInsetState {
    pub fullscreen: bool,
}

impl TrafficLightInsetState {
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    pub fn has_inset(self) -> bool {
        IS_MACOS && !self.fullscreen
    }
}

/// The settings dialog's sections (`data/settingsSections.js`), in the navigation's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSection {
    #[default]
    Settings,
    Shortcuts,
}

/// Which workspace receives workspace shortcuts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardWorkspace {
    #[default]
    Editor,
    Player,
}

/// `useEditorShellState`'s shell fields. Phase 3 adds zoom, background, grid and snap. In memory
/// only, as in React.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorShellState {
    pub settings_open: bool,
    pub settings_section: SettingsSection,
    pub active_keyboard_workspace: KeyboardWorkspace,
    /// `import.meta.env.DEV`.
    pub debug_mode_enabled: bool,
}

impl Default for EditorShellState {
    fn default() -> Self {
        Self {
            settings_open: false,
            settings_section: SettingsSection::default(),
            active_keyboard_workspace: KeyboardWorkspace::default(),
            debug_mode_enabled: cfg!(debug_assertions),
        }
    }
}

impl EditorShellState {
    pub fn open_settings(&mut self) {
        self.settings_open = true;
    }

    pub fn close_settings(&mut self) {
        self.settings_open = false;
    }

    pub fn set_settings_section(&mut self, section: SettingsSection) {
        self.settings_section = section;
    }

    pub fn set_active_keyboard_workspace(&mut self, workspace: KeyboardWorkspace) {
        self.active_keyboard_workspace = workspace;
    }
}

/// One way to press a shortcut: modifier labels joined by `+`, then one or more alternative keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutOption {
    pub group_key: String,
    pub keys: Vec<String>,
    pub modifiers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub description: String,
    pub options: Vec<ShortcutOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub name: String,
    pub shortcuts: Vec<ShortcutRow>,
}

fn key_label(key: &str) -> String {
    match key {
        "arrowdown" => "↓".into(),
        "arrowleft" => "←".into(),
        "arrowright" => "→".into(),
        "arrowup" => "↑".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Del".into(),
        "end" => "End".into(),
        "escape" => "Esc".into(),
        "home" => "Home".into(),
        "space" => "Space".into(),
        "wheel" => "Scroll".into(),
        other => other.to_uppercase(),
    }
}

fn modifier_label(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Alt => "Alt | OPT",
        Modifier::Ctrl => "Ctrl",
        Modifier::Mod => "Ctrl | CMD",
        Modifier::Shift => "Shift",
    }
}

fn modifier_name(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Alt => "alt",
        Modifier::Ctrl => "ctrl",
        Modifier::Mod => "mod",
        Modifier::Shift => "shift",
    }
}

fn binding_parts(binding: &Binding) -> Vec<ShortcutOption> {
    if let Some(label) = &binding.label {
        return std::iter::once(label)
            .chain(&binding.display_aliases)
            .map(|label| ShortcutOption {
                group_key: format!("label:{label}"),
                keys: vec![label.replace("Mod", "Ctrl/⌘")],
                modifiers: Vec::new(),
            })
            .collect();
    }
    vec![ShortcutOption {
        group_key: binding.modifiers.iter().map(|modifier| modifier_name(*modifier)).collect::<Vec<_>>().join("|"),
        keys: vec![key_label(&binding.key)],
        modifiers: binding.modifiers.iter().map(|modifier| modifier_label(*modifier).to_string()).collect(),
    }]
}

/// `getKeyboardShortcutGroups`: the shortcuts reference, one row per description with matching
/// bindings merged. It lists every displayed shortcut, including those whose features arrive later.
pub fn keyboard_shortcut_groups(locale: Locale) -> Vec<ShortcutGroup> {
    let catalog = catalog();
    manifest()
        .categories
        .iter()
        .map(|category| {
            let mut shortcuts: Vec<ShortcutRow> = Vec::new();
            for command in category.commands.iter().filter(|command| command.display) {
                for binding in command.bindings.iter().filter(|binding| binding.is_displayed()) {
                    let description = catalog
                        .t(locale, binding.description_key.as_deref().unwrap_or(&command.description_key))
                        .to_string();
                    let index = match shortcuts.iter().position(|row| row.description == description) {
                        Some(index) => index,
                        None => {
                            shortcuts.push(ShortcutRow {
                                description,
                                options: Vec::new(),
                            });
                            shortcuts.len() - 1
                        }
                    };
                    let row = &mut shortcuts[index];
                    for parts in binding_parts(binding) {
                        match row.options.iter_mut().find(|option| option.group_key == parts.group_key) {
                            Some(option) => {
                                if !option.keys.contains(&parts.keys[0]) {
                                    option.keys.push(parts.keys[0].clone());
                                }
                            }
                            None => row.options.push(parts),
                        }
                    }
                }
            }
            ShortcutGroup {
                name: catalog.t(locale, &category.name_key).to_string(),
                shortcuts,
            }
        })
        .filter(|group| !group.shortcuts.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, WindowCall};
    use crate::store::Dispatcher;
    use serde_json::json;

    #[test]
    fn a_close_request_closes_the_window_once() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(request_close);
        dispatcher.dispatch(request_close);
        assert_eq!(*fakes.window.calls.borrow(), [WindowCall::Close]);
    }

    #[test]
    fn the_startup_screen_shows_until_the_drawer_and_startup_are_ready() {
        let mut store = Store::default();
        assert!(AppBootstrapState::shows_startup_screen(&store));
        store.bootstrap.set_startup_ready();
        assert!(
            AppBootstrapState::shows_startup_screen(&store),
            "the drawer is not hydrated"
        );
        store.layout.initialize_left_drawer(Default::default());
        assert!(!AppBootstrapState::shows_startup_screen(&store));
    }

    #[test]
    fn the_traffic_light_inset_follows_fullscreen_on_macos_only() {
        let mut state = TrafficLightInsetState::default();
        assert_eq!(state.has_inset(), IS_MACOS);
        state.set_fullscreen(true);
        assert!(!state.has_inset());
    }

    fn group(name: &str) -> ShortcutGroup {
        keyboard_shortcut_groups(Locale::En).into_iter().find(|group| group.name == name).unwrap()
    }

    fn row(group: &ShortcutGroup, description: &str) -> Vec<ShortcutOption> {
        group.shortcuts.iter().find(|row| row.description == description).unwrap().options.clone()
    }

    fn option(group_key: &str, keys: &[&str], modifiers: &[&str]) -> ShortcutOption {
        ShortcutOption {
            group_key: group_key.into(),
            keys: keys.iter().map(|key| key.to_string()).collect(),
            modifiers: modifiers.iter().map(|modifier| modifier.to_string()).collect(),
        }
    }

    #[test]
    fn combines_bindings_with_the_same_description_into_one_row() {
        assert_eq!(row(&group("Widget editor"), "Delete selected widgets"), [option("", &["Del", "Backspace"], &[])]);
    }

    #[test]
    fn uses_ctrl_cmd_labels_and_hides_non_display_binding_aliases() {
        assert_eq!(row(&group("Widget editor"), "Canvas zoom in"), [option("mod", &["+"], &["Ctrl | CMD"])]);
    }

    #[test]
    fn renders_one_modifier_for_a_group_of_matching_keys() {
        let sync = group("Timeline and sync");
        assert_eq!(row(&sync, "Nudge selected clip sync offset by 0.1 seconds"), [option("", &["←", "→"], &[])]);
        assert_eq!(row(&sync, "Nudge selected clip sync offset by 1 second"), [option("shift", &["←", "→"], &["Shift"])]);
    }

    #[test]
    fn does_not_leave_an_empty_row_when_every_binding_is_hidden() {
        assert!(!group("General commands").shortcuts.iter().any(|row| row.description == "Close widget drawer"));
    }

    #[test]
    fn shows_home_as_the_display_option_for_rewinding_to_the_start() {
        assert_eq!(row(&group("Video player"), "Rewind to start"), [option("label:Home", &["Home"], &[])]);
    }

    #[test]
    fn editor_shell_state_defaults_and_settings_actions() {
        let mut state = EditorShellState::default();
        assert!(!state.settings_open);
        assert_eq!(state.settings_section, SettingsSection::Settings);
        assert_eq!(state.active_keyboard_workspace, KeyboardWorkspace::Editor);
        state.open_settings();
        state.set_settings_section(SettingsSection::Shortcuts);
        assert!(state.settings_open);
        state.close_settings();
        assert!(!state.settings_open);
        assert_eq!(state.settings_section, SettingsSection::Shortcuts, "the section survives closing");
    }
}
