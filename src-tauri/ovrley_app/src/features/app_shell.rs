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
use crate::features::template_manager;
use crate::features::undo_redo::HistoryMode;
use crate::lib::file_dialog::{open_single_path, DialogOptions};
use crate::store::agent::AgentReply;
use crate::store::services::{DialogFilter, Services};
use crate::store::slices::media::{ActivitySource, ImportedActivity};
use crate::store::Store;
use ovrley_widgets::model::document::{course_widget_dimensions, number};

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
/// - Commands whose features have not arrived (projects, video, render, output, sync) are matched
///   but not consumed, so they do not swallow the key.
pub fn handle_app_key(store: &mut Store, services: &mut Services, input: &KeyInput) -> bool {
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
        "template.new" => {
            template_manager::create_new_template_request(store, services, None);
            true
        }
        "template.import" => {
            template_manager::open_import_template_dialog(store, services);
            true
        }
        "template.save" => {
            if !template_manager::save_status(store).shows() {
                return false;
            }
            template_manager::open_save_template_dialog(store, services, Box::new(|_, _, _| {}));
            true
        }
        "template.select" => {
            store.template_manager.request_selector_open();
            true
        }
        "activity.import" => {
            open_activity_dialog(store, services);
            true
        }
        _ => false,
    }
}

/// A key no focused control used, through the window's scopes in turn: app, history, editor,
/// player.
pub fn handle_window_key(store: &mut Store, services: &mut Services, input: &KeyInput) -> bool {
    handle_app_key(store, services, input)
        || crate::features::overlay_editor::keyboard::handle_history_key(store, input)
        || crate::features::player::handle_timeline_clip_key(store, input)
        || crate::features::overlay_editor::keyboard::handle_editor_key(store, input)
        || crate::features::player::handle_player_key(store, input, std::time::Instant::now())
}

/// The capture phase (D14): the drawer's Escape rule, then an active gesture's cancel.
pub fn handle_window_capture_key(store: &mut Store, input: &KeyInput) -> bool {
    crate::features::toolbar::handle_drawer_capture_key(store, input) || crate::features::overlay_editor::keyboard::handle_capture_key(store, input)
}

/// The debug video loader (phase 4 D18): `OVRLEY_DEV_VIDEOS` lists video paths separated by `;`.
/// Each is probed on a worker, becomes an entity with default edits, and is placed end to end in
/// import order, the first at 0, outside the history. There is no review dialog and no telemetry.
/// Debug builds only, until Phase 6 brings the import; Phase 6 deletes it.
pub const DEV_VIDEOS_VARIABLE: &str = "OVRLEY_DEV_VIDEOS";

pub fn load_dev_videos(store: &mut Store, services: &mut Services, paths: Vec<String>) {
    use crate::lib::video_program::{create_imported_video, new_id, VideoEdits};
    let _ = store;
    let app_paths = std::sync::Arc::clone(&services.paths);
    services.workers.spawn("dev-videos", move || {
        let probed: Result<Vec<_>, String> = paths
            .iter()
            .enumerate()
            .map(|(order, path)| {
                let metadata = ovrley_core::media::prepared_video::probe_video_metadata(&app_paths, path).map_err(|error| format!("{path}: {error}"))?;
                create_imported_video(&metadata, new_id(), order as u32, Vec::new()).map_err(|error| format!("{path}: {error}"))
            })
            .collect();
        Box::new(move |store: &mut Store, _: &mut Services| match probed {
            Err(error) => store.media.set_error_message(format!("{DEV_VIDEOS_VARIABLE}: {error}")),
            Ok(videos) => {
                let mut edits = crate::lib::video_program::VideoEditsMap::new();
                let mut offset = 0.0;
                for video in &videos {
                    edits.insert(video.id.clone(), VideoEdits { offset_seconds: offset, ..VideoEdits::default() });
                    offset += video.duration_seconds;
                }
                crate::features::undo_redo::change_video_collection(store, &[], |store| {
                    store.video_import.imported_videos = std::rc::Rc::new(videos);
                    store.video_import.video_edits = std::rc::Rc::new(edits);
                    store.video_import.selected_clip = None;
                    crate::store::slices::video_import::clamp_playhead_to_videos(store, 0.0);
                });
            }
        })
    });
}

/// A rectangle in window coordinates, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Area {
    pub fn contains(&self, point: (f64, f64)) -> bool {
        point.0 >= self.x && point.0 < self.x + self.width && point.1 >= self.y && point.1 < self.y + self.height
    }
}

/// The keyboard workspace a press selects (phase 4 D11), replacing React's
/// `onPointerDownCapture`: the player while it shows, else the editor or the control panel. Other
/// presses (the header, the drawer) keep the workspace.
pub fn workspace_at(point: (f64, f64), editor: Area, control_panel: Area, player: Option<Area>) -> Option<KeyboardWorkspace> {
    if player.is_some_and(|player| player.contains(point)) {
        return Some(KeyboardWorkspace::Player);
    }
    (editor.contains(point) || control_panel.contains(point)).then_some(KeyboardWorkspace::Editor)
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

use ovrley_core::activity::ACTIVITY_EXTENSIONS;
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

/// One drop, however many files: each kind's import runs in order (a template, then the activity).
/// The video and project arms arrive with Phase 6; until then those kinds report that they cannot
/// be opened yet.
pub fn drop_files(store: &mut Store, services: &mut Services, paths: &[DroppedPath]) {
    store.file_drop.hovering = false;
    if !accepts_file_drops(store) {
        return;
    }
    let imports = match route_dropped_files(paths, cfg!(debug_assertions)) {
        Err(message) => return store.media.set_error_message(message),
        Ok(imports) => imports,
    };
    for import in imports {
        let path = import.path.to_string_lossy().into_owned();
        match import.kind {
            DropKind::Activity => import_activity(store, services, path, None),
            DropKind::Template => crate::features::template_manager::import_template(store, services, path, None),
            kind => store.media.set_error_message(format!("Drop failed: {} files cannot be opened yet.", kind.name())),
        }
    }
}

/// The activity file dialog's filter.
pub fn activity_dialog_filter() -> DialogFilter {
    DialogFilter::new("Activity", &ACTIVITY_EXTENSIONS)
}

/// The directory the activity dialog remembers.
pub const LAST_ACTIVITY_IMPORT_DIR_KEY: &str = "last-activity-import-dir";

/// `handleActivityFileOpen`: the dialog, then the import of the chosen file (D5). A cancelled
/// dialog imports nothing.
pub fn open_activity_dialog(store: &mut Store, services: &mut Services) {
    let opened = open_single_path(
        services,
        &[activity_dialog_filter()],
        DialogOptions {
            default_path: None,
            last_directory_key: Some(LAST_ACTIVITY_IMPORT_DIR_KEY),
        },
        Box::new(|store, services, selected| {
            if let Some(path) = selected {
                import_activity(store, services, path, None);
            }
        }),
    );
    if let Err(error) = opened {
        store.media.set_error_message(format!("Activity selection failed: {error}"));
    }
}

/// `useActivityImport`'s import: the worker parses the file through `ovrley_core`'s one import
/// entry and serializes the preview's copy (D4); its result activates without a history entry.
/// An agent's call answers with the activity, or the error.
pub fn import_activity(store: &mut Store, services: &mut Services, path: String, reply: Option<AgentReply>) {
    store.media.set_processing(true);
    let paths = std::sync::Arc::clone(&services.paths);
    services.workers.spawn("activity-import", move || {
        let result = ovrley_core::activity::import_activity_path(&paths, &path).map_err(|error| error.to_string()).and_then(|response| {
            let preview_bytes = serde_json::to_vec(&response.parsed_activity).map_err(|error| error.to_string())?;
            Ok(ImportedActivity {
                activity: std::sync::Arc::new(response.parsed_activity),
                preview_bytes: std::sync::Arc::new(preview_bytes),
            })
        });
        Box::new(move |store: &mut Store, _: &mut Services| finish_activity_import(store, path, result, reply))
    });
}

fn finish_activity_import(store: &mut Store, path: String, result: Result<ImportedActivity, String>, reply: Option<AgentReply>) {
    store.history.set_mode(HistoryMode::Skip);
    let answer = match result {
        Ok(imported) => {
            store.media.clear_activity_summary();
            activate_parsed_activity(store, ActivitySource::File { path }, imported);
            Ok(crate::store::agent::describe_activity(store))
        }
        Err(error) => {
            store.media.set_error_message(format!("Activity selection failed: {error}"));
            Err(error)
        }
    };
    store.media.set_processing(false);
    if let Some(reply) = reply {
        reply.send(answer);
    }
}

/// `activateParsedActivity`: the activity, its whole-second duration as the timeline (an oddity
/// kept: the video telemetry path does not floor), and course widgets sized to the route.
pub fn activate_parsed_activity(store: &mut Store, source: ActivitySource, imported: ImportedActivity) {
    let activity = std::sync::Arc::clone(&imported.activity);
    store.media.set_activity_source(Some(source));
    store.media.activate_activity_file(imported);
    clear_activity_lane_selection(store);
    let duration = activity.metadata.get("duration_seconds").and_then(serde_json::Value::as_f64).unwrap_or_default();
    if !(duration.is_finite() && duration > 0.0) {
        return;
    }
    let whole = duration.floor();
    let editor = &mut store.editor;
    editor.set_fallback_duration_seconds(whole);
    editor.set_selected_second(0.0);
    let sizing = ovrley_widgets::model::catalog::catalog()
        .require_widget("course")
        .expect("the catalog defines course")
        .course_sizing
        .expect("course declares its sizing");
    if editor.config.plots.iter().any(|plot| plot.get("value").and_then(serde_json::Value::as_str) == Some("course")) {
        if let Some((width, height)) = course_widget_dimensions(&activity.sample_course_points, sizing.max_dimension, sizing.min_dimension) {
            let config = std::rc::Rc::make_mut(&mut editor.config);
            for plot in config.plots.iter_mut().filter(|plot| plot.get("value").and_then(serde_json::Value::as_str) == Some("course")) {
                plot.insert("width".into(), number(width));
                plot.insert("height".into(), number(height));
            }
        }
    }
}

/// The activity drawer's delete: offered while there is an external activity.
pub fn delete_activity(store: &mut Store) -> Result<(), String> {
    if store.media.parsed_activity.is_none() {
        return Err("No activity file is loaded.".into());
    }
    store.media.clear_activity_file(true);
    clear_activity_lane_selection(store);
    Ok(())
}

/// `activateActivityFile` and `clearActivityFile`: the activity lane's selection belongs to the
/// replaced activity. Placing the videos again (`computeVideoSync`) arrives with Phase 6.
fn clear_activity_lane_selection(store: &mut Store) {
    use crate::store::slices::video_import::ClipLane;
    if store.video_import.selected_clip.as_ref().is_some_and(|selection| selection.lane == ClipLane::Activity) {
        store.video_import.selected_clip = None;
    }
}

/// The preference that lets an agent drive the window over MCP (D13); off when absent.
pub const ALLOW_AGENT_EDITS_KEY: &str = "allowAgentEdits";

/// MCP sessions with the window (D13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentSessionState {
    /// The user allows agent edits (`allowAgentEdits`).
    pub allow_agent_edits: bool,
    /// Open connections from `ovrley --mcp` processes.
    pub connections: usize,
}

impl AgentSessionState {
    /// The header's indicator: an agent is connected and allowed.
    pub fn shows_indicator(self) -> bool {
        self.allow_agent_edits && self.connections > 0
    }

    pub fn session_opened(&mut self) {
        self.connections += 1;
    }

    pub fn session_closed(&mut self) {
        self.connections = self.connections.saturating_sub(1);
    }
}

/// Reads `allowAgentEdits`: absent is off; a present value must be a boolean.
pub fn hydrate_agent_preference(store: &mut Store, preferences: &crate::lib::preferences_store::PreferencesStore) -> Result<(), String> {
    match preferences.get(ALLOW_AGENT_EDITS_KEY) {
        None => Ok(()),
        Some(serde_json::Value::Bool(allowed)) => {
            store.agent_session.allow_agent_edits = *allowed;
            Ok(())
        }
        Some(other) => Err(format!("Preference \"{ALLOW_AGENT_EDITS_KEY}\" must be true or false, found {other}")),
    }
}

/// The Settings switch: applies and persists `allowAgentEdits`.
pub fn set_allow_agent_edits(store: &mut Store, services: &mut Services, allowed: bool) {
    store.agent_session.allow_agent_edits = allowed;
    services.preferences.set(ALLOW_AGENT_EDITS_KEY, serde_json::Value::Bool(allowed));
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

/// The canvas background behind the scene. Image and video arrive with Phase 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorBackgroundMode {
    #[default]
    Checker,
    Black,
    White,
}

/// `useEditorShellState`: editor presentation state, in memory only, with today's launch defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorShellState {
    pub settings_open: bool,
    pub settings_section: SettingsSection,
    pub active_keyboard_workspace: KeyboardWorkspace,
    /// `import.meta.env.DEV`.
    pub debug_mode_enabled: bool,
    pub editor_zoom_level: f64,
    pub editor_background_mode: EditorBackgroundMode,
    pub editor_grid_visible: bool,
    pub editor_snap_to_grid: bool,
}

impl Default for EditorShellState {
    fn default() -> Self {
        Self {
            settings_open: false,
            settings_section: SettingsSection::default(),
            active_keyboard_workspace: KeyboardWorkspace::default(),
            debug_mode_enabled: cfg!(debug_assertions),
            editor_zoom_level: 1.0,
            editor_background_mode: EditorBackgroundMode::default(),
            editor_grid_visible: false,
            editor_snap_to_grid: false,
        }
    }
}

/// The zoom range and step (`clamp(Number((zoom ± 0.05).toFixed(2)), 0.35, 4)`).
pub const MIN_EDITOR_ZOOM: f64 = 0.35;
pub const MAX_EDITOR_ZOOM: f64 = 4.0;
const ZOOM_STEP: f64 = 0.05;

fn round_to_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

impl EditorShellState {
    pub fn decrease_zoom(&mut self) {
        self.editor_zoom_level = crate::lib::utils::clamp(round_to_hundredths(self.editor_zoom_level - ZOOM_STEP), MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
    }

    pub fn increase_zoom(&mut self) {
        self.editor_zoom_level = crate::lib::utils::clamp(round_to_hundredths(self.editor_zoom_level + ZOOM_STEP), MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
    }

    pub fn reset_editor_zoom(&mut self) {
        self.editor_zoom_level = 1.0;
    }

    /// `setEditorZoomLevel`, which the wheel zoom uses; it clamps like the steps.
    pub fn set_editor_zoom_level(&mut self, zoom: f64) {
        self.editor_zoom_level = crate::lib::utils::clamp(zoom, MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
    }

    pub fn set_editor_background_mode(&mut self, mode: EditorBackgroundMode) {
        self.editor_background_mode = mode;
    }

    pub fn set_editor_grid_visible(&mut self, visible: bool) {
        self.editor_grid_visible = visible;
    }

    pub fn set_editor_snap_to_grid(&mut self, snap: bool) {
        self.editor_snap_to_grid = snap;
    }

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

/// How the user answers the unsaved-changes dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedAnswer {
    Save,
    Discard,
    Cancel,
}

/// What runs once the user answers the unsaved-changes dialog. Phase 6 adds the project cases.
#[derive(Debug)]
pub enum UnsavedContinuation {
    /// `handleCreateNewTemplate`; an agent's `template_new` waits on its reply.
    NewTemplate { reply: Option<crate::store::agent::AgentReply> },
}

/// `useUnsavedChangesConfirm`: one pending confirmation (D15).
#[derive(Debug, Default)]
pub struct UnsavedChangesConfirmState {
    pending: Option<UnsavedContinuation>,
}

impl UnsavedChangesConfirmState {
    pub fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    pub fn continuation(&self) -> Option<&UnsavedContinuation> {
        self.pending.as_ref()
    }

    /// Opens the dialog for `continuation`. A request while one is pending replaces it, as the
    /// React hook's single resolver does; the replaced call's agent reply is cancelled.
    pub fn request_confirm(&mut self, continuation: UnsavedContinuation) {
        self.pending = Some(continuation);
    }

    /// Closes the dialog and hands back what was waiting.
    pub fn take(&mut self) -> Option<UnsavedContinuation> {
        self.pending.take()
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
    fn presses_select_the_player_or_the_editor_workspace() {
        let editor = Area { x: 0.0, y: 40.0, width: 800.0, height: 400.0 };
        let panel = Area { x: 800.0, y: 40.0, width: 300.0, height: 600.0 };
        let player = Area { x: 0.0, y: 440.0, width: 800.0, height: 200.0 };
        assert_eq!(workspace_at((10.0, 500.0), editor, panel, Some(player)), Some(KeyboardWorkspace::Player));
        assert_eq!(workspace_at((10.0, 100.0), editor, panel, Some(player)), Some(KeyboardWorkspace::Editor));
        assert_eq!(workspace_at((900.0, 100.0), editor, panel, Some(player)), Some(KeyboardWorkspace::Editor));
        assert_eq!(workspace_at((10.0, 10.0), editor, panel, Some(player)), None, "the header keeps the workspace");
        assert_eq!(workspace_at((10.0, 500.0), editor, panel, None), None, "a hidden player takes no press");
    }

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
    fn zoom_steps_round_and_clamp() {
        let mut state = EditorShellState::default();
        state.increase_zoom();
        assert_eq!(state.editor_zoom_level, 1.05);
        for _ in 0..100 {
            state.decrease_zoom();
        }
        assert_eq!(state.editor_zoom_level, MIN_EDITOR_ZOOM);
        for _ in 0..100 {
            state.increase_zoom();
        }
        assert_eq!(state.editor_zoom_level, MAX_EDITOR_ZOOM);
        state.reset_editor_zoom();
        assert_eq!(state.editor_zoom_level, 1.0);
        assert_eq!(state.editor_background_mode, EditorBackgroundMode::Checker);
        assert!(!state.editor_grid_visible && !state.editor_snap_to_grid);
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

    /// Runs `keys` through `handle_app_key` in one dispatch and returns what each answered.
    fn press(store: Store, keys: Vec<KeyInput>) -> (Vec<bool>, Dispatcher, crate::store::services::fakes::Fakes) {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(store, parts);
        let answers = dispatcher.dispatch_with_result(move |store, services| keys.iter().map(|key| handle_app_key(store, services, key)).collect()).unwrap();
        (answers, dispatcher, fakes)
    }

    fn modified(key: &str, ctrl: bool, shift: bool) -> KeyInput {
        let mut input = KeyInput::new(key);
        input.ctrl = ctrl && !IS_MACOS;
        input.meta = ctrl && IS_MACOS;
        input.shift = shift;
        input
    }

    #[test]
    fn alt_w_toggles_the_widgets_drawer() {
        let (answers, dispatcher, _) = press(Store::default(), vec![alt("w")]);
        assert_eq!(answers, [true]);
        dispatcher.with_store(|store| {
            assert!(store.layout.left_drawer_visible);
            assert_eq!(store.layout.active_left_drawer_tool, LeftDrawerTool::Widgets);
        });
        let (answers, dispatcher, _) = press(Store::default(), vec![alt("w"), alt("w")]);
        assert_eq!(answers, [true, true], "allowed while the drawer is the overlay");
        assert!(!dispatcher.with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn a_repeated_key_never_runs_a_command() {
        let mut repeated = alt("w");
        repeated.repeat = true;
        let (answers, dispatcher, _) = press(Store::default(), vec![repeated]);
        assert_eq!(answers, [false]);
        assert!(!dispatcher.with_store(|store| store.layout.left_drawer_visible));
    }

    #[test]
    fn the_settings_dialog_blocks_app_shortcuts() {
        let mut store = Store::default();
        store.editor_shell.open_settings();
        let (answers, _, _) = press(store, vec![alt("w")]);
        assert_eq!(answers, [false]);
    }

    #[test]
    fn commands_of_later_phases_are_matched_but_not_consumed() {
        let (answers, _, _) = press(Store::default(), vec![modified("s", true, false), modified("i", true, false)]);
        assert_eq!(answers, [false, false]);
    }

    #[test]
    fn template_and_activity_commands_run() {
        let (answers, dispatcher, fakes) = press(Store::default(), vec![modified("k", true, false), modified("t", true, false), modified("l", true, true)]);
        assert_eq!(answers, [true, true, true]);
        assert_eq!(fakes.dialogs.requests().len(), 2, "the activity and template dialogs opened");
        assert_eq!(dispatcher.with_store(|store| store.template_manager.selector_open_serial), 1);
    }

    #[test]
    fn template_save_runs_only_while_the_status_shows() {
        let mut store = Store::default();
        store.template.last_saved_template_state = Some(ovrley_core::normalize::template::create_durable_template_state(&store.editor.config, Some(&store.template.global_defaults)).unwrap());
        crate::features::template_manager::reconcile_durable_state(&mut store);
        let (answers, _, fakes) = press(store, vec![modified("p", true, true)]);
        assert_eq!(answers, [false], "a saved template has nothing to save");
        assert!(fakes.dialogs.requests().is_empty());
        let (answers, _, fakes) = press(Store::default(), vec![modified("p", true, true)]);
        assert_eq!(answers, [true], "a draft saves");
        assert_eq!(fakes.dialogs.requests().len(), 1);
    }

    #[test]
    fn modified_shell_commands_still_run_when_a_field_had_focus() {
        // A focused text field rejects Alt+W, so the window's handler receives it (React's
        // "allows modified shell commands in inputs").
        let (answers, _, _) = press(Store::default(), vec![alt("w")]);
        assert_eq!(answers, [true]);
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
        let (parts, _) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| {
            set_file_drag_hovering(store, true);
            assert!(!store.file_drop.hovering, "the startup screen shows");
            store.bootstrap.set_startup_ready();
            store.layout.initialize_left_drawer(Default::default());
            set_file_drag_hovering(store, true);
            assert!(store.file_drop.hovering);
            store.editor_shell.open_settings();
            drop_files(store, services, &dropped(&["/a/notes.txt"]));
            assert_eq!(store.media.error_message, None);
            assert!(!store.file_drop.hovering);
            store.editor_shell.close_settings();
            drop_files(store, services, &dropped(&["/a/clip.mp4"]));
            assert_eq!(store.media.error_message.as_deref(), Some("Drop failed: video files cannot be opened yet."));
        });
    }

    fn settle(dispatcher: &Dispatcher, fakes: &crate::store::services::fakes::Fakes, done: impl Fn(&Store) -> bool) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(&done) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out");
    }

    fn fixture(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ovrley_core/tests/fixtures/activity").join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn an_activity_import_activates_without_a_history_entry() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, _| {
            store.editor.set_config(ovrley_widgets::model::document::OverlayConfig::from_value(&json!({"plots": [{"id": "c", "value": "course", "width": 1, "height": 1}]})).unwrap());
        });
        let path = fixture("test-ride.fit");
        let replies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = std::sync::Arc::clone(&replies);
        let target = path.clone();
        dispatcher.dispatch(move |store, services| import_activity(store, services, target, Some(AgentReply::new(move |result| log.lock().unwrap().push(result)))));
        assert!(dispatcher.with_store(|store| store.media.is_processing));
        settle(&dispatcher, &fakes, |store| !store.media.is_processing);
        dispatcher.with_store(|store| {
            assert_eq!(store.media.activity_source, Some(ActivitySource::File { path: path.clone() }));
            let duration = store.media.parsed_activity.as_ref().unwrap().activity.metadata["duration_seconds"].as_f64().unwrap();
            assert_eq!(store.editor.fallback_duration_seconds, duration.floor());
            assert_ne!(store.editor.config.plots[0]["width"], json!(1), "course widgets are sized to the route");
            assert_eq!(store.history.past.len(), 1, "only the config edit before the import is recorded");
            assert!(!store.media.preview_bytes_empty());
        });
        let replies = replies.lock().unwrap();
        assert_eq!(replies[0].as_ref().unwrap()["file"], json!(path));
    }

    #[test]
    fn a_failed_import_shows_the_selection_error() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| import_activity(store, services, "/nowhere/ride.txt".into(), None));
        settle(&dispatcher, &fakes, |store| !store.media.is_processing);
        let message = dispatcher.with_store(|store| store.media.error_message.clone()).unwrap();
        assert!(message.starts_with("Activity selection failed: ") && message.contains("Unsupported activity file format: ride.txt"), "{message}");
    }

    #[test]
    fn the_activity_dialog_remembers_its_directory_and_imports_the_choice() {
        let (parts, fakes) = fake_services(json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(open_activity_dialog);
        let requests = fakes.dialogs.requests();
        let crate::store::services::FileDialogRequest::OpenFile { filters, .. } = &requests[0] else { panic!("an open dialog") };
        assert!(filters[0].extensions.contains(&"FIT".to_string()), "case-insensitive filters");
        fakes.dialogs.answer(Some(&fixture("test.gpx")));
        assert_eq!(fakes.preferences.saves.borrow().last().unwrap()[LAST_ACTIVITY_IMPORT_DIR_KEY], json!(std::path::Path::new(&fixture("test.gpx")).parent().unwrap().to_string_lossy()));
        settle(&dispatcher, &fakes, |store| store.media.activity_summary.is_some());
    }

    #[test]
    fn deleting_needs_an_activity_file() {
        let mut store = Store::default();
        assert!(delete_activity(&mut store).is_err());
    }

    #[test]
    fn the_agent_indicator_needs_a_connection_and_the_preference() {
        let mut state = AgentSessionState::default();
        state.session_opened();
        assert!(!state.shows_indicator(), "edits are off");
        state.allow_agent_edits = true;
        assert!(state.shows_indicator());
        state.session_closed();
        state.session_closed();
        assert_eq!(state.connections, 0);
        assert!(!state.shows_indicator());
    }

    #[test]
    fn the_agent_preference_is_off_when_absent_and_strict_when_present() {
        let (preferences, _) = crate::lib::preferences_store::PreferencesStore::in_memory(json!({}));
        assert!(!Store::hydrate(&preferences).unwrap().agent_session.allow_agent_edits);
        let (preferences, _) = crate::lib::preferences_store::PreferencesStore::in_memory(json!({"allowAgentEdits": true}));
        assert!(Store::hydrate(&preferences).unwrap().agent_session.allow_agent_edits);
        let (preferences, _) = crate::lib::preferences_store::PreferencesStore::in_memory(json!({"allowAgentEdits": "yes"}));
        assert!(Store::hydrate(&preferences).is_err());
    }
}
