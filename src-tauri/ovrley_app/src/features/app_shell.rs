//! The app-shell feature's hooks: `useAppBootstrap` (its shell part), `useEditorShellState` (its
//! shell fields), `useWindowCloseGuard` and `useTrafficLightInset`; `getKeyboardShortcutGroups`;
//! and the window-wide file drop (plan D10).
//!
//! React keeps each in a hook's state; here each is a struct in the [`Store`], named after its
//! hook, and its actions are methods on it (plan D1, D2).

use crate::i18n::{catalog, Locale};
use crate::lib::keyboard_shortcuts::{manifest, match_keyboard_shortcut, Binding, KeyInput, Modifier, Scope};
use crate::store::slices::layout::LeftDrawerTool;
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

/// `useAppShellKeyboard`: app-scope shortcuts that no focused control used. Returns whether the
/// key is consumed.
///
/// - A repeated key never runs a command.
/// - Open overlays block app shortcuts (`hasOpenOverlay`): the settings dialog, and the unpinned
///   drawer's backdrop, except `drawer.toggle` while the drawer is that overlay. Keys do not reach
///   the window while a select, popover or dialog popup has focus, so those block by themselves.
/// - Phase 2 runs only `drawer.toggle`. Every other app command is matched but not consumed until
///   its feature arrives, so it does not swallow the key.
pub fn handle_app_key(store: &mut Store, input: &KeyInput) -> bool {
    if input.repeat {
        return false;
    }
    let Some(found) = match_keyboard_shortcut(input, Scope::App) else {
        return false;
    };
    let drawer_overlay = store.layout.is_unpinned_overlay();
    let overlay_open = store.editor_shell.settings_open || drawer_overlay;
    let command = found.command.id.as_str();
    if overlay_open && !(command == "drawer.toggle" && drawer_overlay && !store.editor_shell.settings_open) {
        return false;
    }
    match command {
        "drawer.toggle" => {
            store.layout.select_left_drawer_tool(LeftDrawerTool::Widgets);
            true
        }
        _ => false,
    }
}

/// What a dropped file is, by extension, using the lists the file dialogs use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DropKind {
    // In import order: a template first, then the activity, then the video.
    Template,
    Activity,
    Video,
    Project,
}

impl DropKind {
    fn name(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Activity => "activity",
            Self::Video => "video",
            Self::Project => "project",
        }
    }
}

const ACTIVITY_EXTENSIONS: [&str; 10] = ["gpx", "fit", "tcx", "srt", "igc", "csv", "vbo", "uddf", "ssrf", "xml"];
const VIDEO_EXTENSIONS: [&str; 3] = ["mp4", "mov", "mkv"];
/// Background images, in debug builds only, as the video import accepts them today.
const DEBUG_IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// A dropped path, as the platform layer saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedPath {
    pub path: std::path::PathBuf,
    pub is_dir: bool,
}

/// One import a drop runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropImport {
    pub kind: DropKind,
    pub path: std::path::PathBuf,
}

fn classify(dropped: &DroppedPath, debug: bool) -> Option<DropKind> {
    if dropped.is_dir {
        return None;
    }
    // Case-insensitive, as `caseInsensitiveFilters` makes the file dialogs.
    let extension = dropped.path.extension()?.to_str()?.to_ascii_lowercase();
    let extension = extension.as_str();
    if ACTIVITY_EXTENSIONS.contains(&extension) {
        Some(DropKind::Activity)
    } else if VIDEO_EXTENSIONS.contains(&extension) || (debug && DEBUG_IMAGE_EXTENSIONS.contains(&extension)) {
        Some(DropKind::Video)
    } else if extension == "oly" {
        Some(DropKind::Project)
    } else if extension == "json" {
        Some(DropKind::Template)
    } else {
        None
    }
}

/// `route_dropped_files` (Q1): the imports one drop runs, in order, or the one error it shows.
/// At most one file of each kind; a project must be alone, because opening it replaces everything;
/// a directory or any other file imports nothing. The wording is English, like today's drop errors.
pub fn route_dropped_files(paths: &[DroppedPath], debug: bool) -> Result<Vec<DropImport>, String> {
    let mut imports: Vec<DropImport> = Vec::with_capacity(paths.len());
    for dropped in paths {
        let name = dropped.path.file_name().map_or_else(|| dropped.path.display().to_string(), |name| name.to_string_lossy().into_owned());
        let kind = classify(dropped, debug).ok_or_else(|| format!("Drop failed: {name} cannot be imported."))?;
        if imports.iter().any(|import| import.kind == kind) {
            return Err(format!("Drop failed: Drop at most one {} file.", kind.name()));
        }
        imports.push(DropImport {
            kind,
            path: dropped.path.clone(),
        });
    }
    if imports.len() > 1 && imports.iter().any(|import| import.kind == DropKind::Project) {
        return Err("Drop failed: Drop a project file on its own.".into());
    }
    imports.sort_by_key(|import| import.kind);
    Ok(imports)
}

/// The window-wide file drag (D10): a file hovers the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFileDropState {
    pub hovering: bool,
}

/// Drops are ignored, and the overlay does not show, while a modal dialog is open or the startup
/// screen shows.
pub fn accepts_file_drops(store: &Store) -> bool {
    !store.editor_shell.settings_open && !AppBootstrapState::shows_startup_screen(store)
}

pub fn set_file_drag_hovering(store: &mut Store, hovering: bool) {
    store.file_drop.hovering = hovering && accepts_file_drops(store);
}

/// One drop, however many files. No importer exists in Phase 2, so each kind reports that it
/// cannot be opened yet; each feature replaces its kind's arm when it lands (activity and template
/// in Phase 3, video and project in Phase 6).
pub fn drop_files(store: &mut Store, paths: &[DroppedPath]) {
    store.file_drop.hovering = false;
    if !accepts_file_drops(store) {
        return;
    }
    match route_dropped_files(paths, cfg!(debug_assertions)) {
        Err(message) => store.media.set_error_message(message),
        Ok(imports) => {
            if let Some(import) = imports.first() {
                let kind = import.kind.name();
                store.media.set_error_message(format!("Drop failed: {kind} files cannot be opened yet."));
            }
        }
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

    fn alt(key: &str) -> KeyInput {
        let mut input = KeyInput::new(key);
        input.alt = true;
        input
    }

    #[test]
    fn alt_w_toggles_the_widgets_drawer() {
        let mut store = Store::default();
        assert!(handle_app_key(&mut store, &alt("w")));
        assert!(store.layout.left_drawer_visible);
        assert_eq!(store.layout.active_left_drawer_tool, LeftDrawerTool::Widgets);
        assert!(handle_app_key(&mut store, &alt("w")), "allowed while the drawer is the overlay");
        assert!(!store.layout.left_drawer_visible);
    }

    #[test]
    fn a_repeated_key_never_runs_a_command() {
        let mut store = Store::default();
        let mut repeated = alt("w");
        repeated.repeat = true;
        assert!(!handle_app_key(&mut store, &repeated));
        assert!(!store.layout.left_drawer_visible);
    }

    #[test]
    fn the_settings_dialog_blocks_app_shortcuts() {
        let mut store = Store::default();
        store.editor_shell.open_settings();
        assert!(!handle_app_key(&mut store, &alt("w")));
        assert!(!store.layout.left_drawer_visible);
    }

    #[test]
    fn commands_of_later_phases_are_matched_but_not_consumed() {
        let mut store = Store::default();
        let mut save = KeyInput::new("s");
        save.ctrl = true;
        assert!(!handle_app_key(&mut store, &save));
    }

    #[test]
    fn modified_shell_commands_still_run_when_a_field_had_focus() {
        // A focused text field rejects Alt+W, so the window's handler receives it (React's
        // "allows modified shell commands in inputs").
        let mut store = Store::default();
        assert!(handle_app_key(&mut store, &alt("w")));
    }

    fn dropped(paths: &[&str]) -> Vec<DroppedPath> {
        paths
            .iter()
            .map(|path| DroppedPath {
                path: (*path).into(),
                is_dir: path.ends_with('/'),
            })
            .collect()
    }

    fn kinds(paths: &[&str], debug: bool) -> Result<Vec<DropKind>, String> {
        route_dropped_files(&dropped(paths), debug).map(|imports| imports.into_iter().map(|import| import.kind).collect())
    }

    #[test]
    fn classifies_every_extension_in_both_cases() {
        for extension in ACTIVITY_EXTENSIONS {
            assert_eq!(kinds(&[&format!("/a/ride.{extension}")], false), Ok(vec![DropKind::Activity]));
            assert_eq!(kinds(&[&format!("/a/ride.{}", extension.to_uppercase())], false), Ok(vec![DropKind::Activity]));
        }
        for extension in VIDEO_EXTENSIONS {
            assert_eq!(kinds(&[&format!("/a/CLIP.{}", extension.to_uppercase())], false), Ok(vec![DropKind::Video]));
        }
        assert_eq!(kinds(&["/a/Race.OLY"], false), Ok(vec![DropKind::Project]));
        assert_eq!(kinds(&["/a/theme.Json"], false), Ok(vec![DropKind::Template]));
    }

    #[test]
    fn mixed_drops_run_template_then_activity_then_video() {
        assert_eq!(
            kinds(&["/a/clip.mp4", "/a/ride.fit", "/a/look.json"], false),
            Ok(vec![DropKind::Template, DropKind::Activity, DropKind::Video])
        );
    }

    #[test]
    fn two_files_of_one_kind_import_nothing() {
        assert_eq!(kinds(&["/a/one.fit", "/a/two.gpx"], false), Err("Drop failed: Drop at most one activity file.".into()));
    }

    #[test]
    fn a_project_must_be_alone() {
        assert_eq!(kinds(&["/a/race.oly", "/a/ride.fit"], false), Err("Drop failed: Drop a project file on its own.".into()));
    }

    #[test]
    fn directories_and_other_files_are_unsupported() {
        assert_eq!(kinds(&["/a/folder.fit/"], false), Err("Drop failed: folder.fit cannot be imported.".into()));
        assert_eq!(kinds(&["/a/ride.fit", "/a/notes.txt"], false), Err("Drop failed: notes.txt cannot be imported.".into()));
        assert_eq!(kinds(&["/a/README"], false), Err("Drop failed: README cannot be imported.".into()));
    }

    #[test]
    fn images_are_backgrounds_in_debug_builds_only() {
        assert_eq!(kinds(&["/a/bg.PNG"], true), Ok(vec![DropKind::Video]));
        assert!(kinds(&["/a/bg.png"], false).is_err());
    }

    #[test]
    fn drops_are_ignored_behind_a_dialog_and_the_startup_screen() {
        let mut store = Store::default();
        set_file_drag_hovering(&mut store, true);
        assert!(!store.file_drop.hovering, "the startup screen shows");
        store.bootstrap.set_startup_ready();
        store.layout.initialize_left_drawer(Default::default());
        set_file_drag_hovering(&mut store, true);
        assert!(store.file_drop.hovering);
        store.editor_shell.open_settings();
        drop_files(&mut store, &dropped(&["/a/notes.txt"]));
        assert_eq!(store.media.error_message, None);
        assert!(!store.file_drop.hovering);
        store.editor_shell.close_settings();
        drop_files(&mut store, &dropped(&["/a/ride.fit"]));
        assert_eq!(store.media.error_message.as_deref(), Some("Drop failed: activity files cannot be opened yet."));
    }
}
