//! The app shell: startup, the window-level keyboard scopes and workspaces, the editor shell's
//! view settings, the close guard and unsaved-changes question, the window-wide file drop, the
//! external activity import, agent sessions, the settings sections, and the app update.

use std::sync::{Arc, Mutex};
use ovrley_core::import::ACTIVITY_EXTENSIONS;
use ovrley_services::runtime::Distribution;
use ovrley_services::updater::DownloadProgress;
use ovrley_widgets::model::document::{course_widget_dimensions, number};
use crate::dialogs::{open_single_path, DialogOptions};
use crate::features::templates;
use crate::features::history::HistoryMode;
use crate::features::toolbar::LeftDrawerTool;
use crate::format::format_update_progress;
use crate::i18n::{catalog, Locale};
use crate::shortcuts::{manifest, match_keyboard_shortcut, Binding, KeyInput, Modifier, Scope};
use crate::store::Store;
use crate::store::agent::AgentReply;
use crate::store::media::{ActivitySource, ImportedActivity};
use crate::store::services::{AppUpdate, DialogFilter, Services, UiPost};

/// The app's version: the workspace's.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version label the header shows: [`VERSION`] with a `v` prefix.
pub fn app_version() -> String {
    format!("v{VERSION}")
}

/// Startup progress and failure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppBootstrapState {
    /// Startup work has finished; until then the window shows the startup screen.
    pub startup_ready: bool,
    /// A startup failure once the window exists (a malformed `lng`). The window shows it as a
    /// full-window page.
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

/// A close request is handled once, and asks the unsaved-project check
/// before the window closes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowCloseGuardState {
    /// A second request while one is handled is ignored; it ends when the user
    /// cancels the unsaved question.
    pub handling_close: bool,
}

/// The close request of the window's close button, the title bar or the OS.
pub fn request_close(store: &mut Store, services: &mut Services) {
    crate::features::projects::request_close(store, services);
}

/// On macOS, the header leaves room for the traffic lights while the title
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
        cfg!(target_os = "macos") && !self.fullscreen
    }
}

/// App-scope shortcuts that no focused control used. Returns whether the
/// key is consumed.
///
/// - A repeated key never runs a command.
/// - Open overlays block app shortcuts: the settings dialog, and the unpinned
///   drawer's backdrop, except `drawer.toggle` while the drawer is that overlay. Keys do not reach
///   the window while a select, popover or dialog popup has focus, so those block by themselves.
/// - The render dialog, its questions and the update dialog are overlays too.
pub fn handle_app_key(store: &mut Store, services: &mut Services, input: &KeyInput) -> bool {
    if input.repeat {
        return false;
    }
    let Some(found) = match_keyboard_shortcut(input, Scope::App) else {
        return false;
    };
    let drawer_overlay = store.layout.is_unpinned_overlay();
    let overlay_open = store.editor_shell.settings_open || drawer_overlay || store.render_dialog.is_open() || store.app_update.is_open();
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
            templates::create_new_template_request(store, services, None);
            true
        }
        "template.import" => {
            templates::open_import_template_dialog(store, services);
            true
        }
        "template.save" => {
            if !templates::save_status(store).shows() {
                return false;
            }
            templates::open_save_template_dialog(store, services, Box::new(|_, _, _| {}));
            true
        }
        "template.select" => {
            store.templates.request_selector_open();
            true
        }
        "activity.import" => {
            open_activity_dialog(store, services);
            true
        }
        "video.import" => {
            crate::features::import::open_video_import_dialog(store, services);
            true
        }
        "project.new" => {
            crate::features::projects::new_project(store, services, None, None);
            true
        }
        "project.open" => {
            crate::features::projects::open_project_dialog(store, services);
            true
        }
        "project.save" | "project.saveAs" => {
            let mode = if command == "project.saveAs" { crate::features::projects::SaveMode::SaveAs } else { crate::features::projects::SaveMode::Save };
            crate::features::projects::save_project(store, services, mode, None, Box::new(|_, _, _| {}));
            true
        }
        "render.open" => {
            if crate::features::render::render_disabled_reason(store).is_some() {
                return false;
            }
            crate::features::render::open_render_dialog(store, services);
            true
        }
        "output.open" => {
            open_output_directory(store, services);
            true
        }
        "sync.auto" => {
            if store.media.activity_summary.is_none() || crate::features::sync::work_clip(store).is_none() {
                return false;
            }
            if let Err(error) = crate::features::sync::auto_sync(store) {
                log::error!("video sync: {error}");
            }
            true
        }
        _ => false,
    }
}

/// Opens the remembered render folder, else Downloads. A failure is reported in English.
pub fn open_output_directory(store: &mut Store, services: &mut Services) {
    let opened = crate::dialogs::load_remembered_render_directory(services).and_then(|directory| services.render.open_output_directory(directory.as_deref()));
    if let Err(error) = opened {
        log::error!("Error opening render output directory: {error}");
        store.media.set_error_message(format!("Failed to open render output folder: {error}"));
    }
}

/// A key no focused control used, through the window's scopes in turn: app, history, editor,
/// player.
pub fn handle_window_key(store: &mut Store, services: &mut Services, input: &KeyInput) -> bool {
    handle_app_key(store, services, input)
        || crate::features::canvas::keyboard::handle_history_key(store, input)
        || crate::features::player::handle_timeline_clip_key(store, input)
        || crate::features::canvas::keyboard::handle_editor_key(store, input)
        || crate::features::player::handle_player_key(store, input, std::time::Instant::now())
}

/// The capture-phase keys: the drawer's Escape rule, then an active gesture's cancel.
pub fn handle_window_capture_key(store: &mut Store, input: &KeyInput) -> bool {
    crate::features::toolbar::handle_drawer_capture_key(store, input) || crate::features::canvas::keyboard::handle_capture_key(store, input)
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

/// The keyboard workspace a press selects: the player while it shows, else the editor or the
/// control panel. Other presses (the header, the drawer) keep the workspace.
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
    // Case-insensitive, so `.FIT` matches `fit`.
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

/// The imports one drop runs, in order, or the one error it shows. At most one template and one
/// activity; several videos open one review; a project must be alone, because opening it replaces
/// everything; a directory or any other file imports nothing. The wording is English.
pub fn route_dropped_files(paths: &[DroppedPath], debug: bool) -> Result<Vec<DropImport>, String> {
    let mut imports: Vec<DropImport> = Vec::with_capacity(paths.len());
    for dropped in paths {
        let name = dropped.path.file_name().map_or_else(|| dropped.path.display().to_string(), |name| name.to_string_lossy().into_owned());
        let kind = classify(dropped, debug).ok_or_else(|| format!("Drop failed: {name} cannot be imported."))?;
        if kind != DropKind::Video && imports.iter().any(|import| import.kind == kind) {
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

/// The window-wide file drag: a file hovers the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFileDropState {
    pub hovering: bool,
}

/// Drops are ignored, and the overlay does not show, while a modal dialog is open or the startup
/// screen shows.
pub fn accepts_file_drops(store: &Store) -> bool {
    !store.editor_shell.settings_open
        && !store.unsaved_confirm.is_open()
        && !crate::features::projects::has_open_dialog(store)
        && !store.render_dialog.is_open()
        && !store.app_update.is_open()
        && !AppBootstrapState::shows_startup_screen(store)
}

pub fn set_file_drag_hovering(store: &mut Store, hovering: bool) {
    store.file_drop.hovering = hovering && accepts_file_drops(store);
}

/// One drop, however many files: each kind's import runs in order (a template, then the activity,
/// then one review of the videos). A project opens as Load Project does.
pub fn drop_files(store: &mut Store, services: &mut Services, paths: &[DroppedPath]) {
    store.file_drop.hovering = false;
    if !accepts_file_drops(store) {
        return;
    }
    let imports = match route_dropped_files(paths, cfg!(debug_assertions)) {
        Err(message) => return store.media.set_error_message(message),
        Ok(imports) => imports,
    };
    let mut videos = Vec::new();
    for import in imports {
        let path = import.path.to_string_lossy().into_owned();
        match import.kind {
            DropKind::Activity => import_activity(store, services, path, None),
            DropKind::Template => crate::features::templates::import_template(store, services, path, None),
            DropKind::Video => videos.push(path),
            DropKind::Project => crate::features::projects::open_project(store, services, path, None, Default::default()),
        }
    }
    if !videos.is_empty() {
        drop_video_files(store, services, videos);
    }
}

/// A single image in debug builds sets the background, else the review.
fn drop_video_files(store: &mut Store, services: &mut Services, paths: Vec<String>) {
    use crate::features::import::{apply_background_image, is_debug_image_path, start_import, ImportRequest};
    if store.editor_shell.debug_mode_enabled && paths.len() == 1 && is_debug_image_path(&paths[0]) {
        return apply_background_image(store, paths.into_iter().next().expect("one path"));
    }
    if let Err(error) = start_import(store, services, paths, ImportRequest::default()) {
        store.media.set_error_message(format!("Video drop failed: {error}"));
    }
}

/// The activity file dialog's filter.
pub fn activity_dialog_filter() -> DialogFilter {
    DialogFilter::new("Activity", &ACTIVITY_EXTENSIONS)
}

/// The directory the activity dialog remembers.
pub const LAST_ACTIVITY_IMPORT_DIR_KEY: &str = "last-activity-import-dir";

/// The dialog, then the import of the chosen file. A cancelled
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

/// The worker parses the file through `ovrley_core`'s one import
/// entry and serializes the preview's copy; its result activates without a history entry.
/// An agent's call answers with the activity, or the error.
pub fn import_activity(store: &mut Store, services: &mut Services, path: String, reply: Option<AgentReply>) {
    store.media.set_processing(true);
    let paths = std::sync::Arc::clone(&services.paths);
    services.workers.spawn("activity-import", move || {
        let result = ovrley_core::import::load_activity(&paths, std::path::Path::new(&path)).map_err(|error| error.to_string()).and_then(ImportedActivity::from_activity);
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

/// The activity, its whole-second duration as the timeline (the video telemetry path does not
/// floor), and course widgets sized to the route. The
/// videos are placed against it.
pub fn activate_parsed_activity(store: &mut Store, source: ActivitySource, imported: ImportedActivity) {
    let activity = std::sync::Arc::clone(&imported.activity);
    store.media.set_activity_source(Some(source));
    store.media.activate_activity_file(imported);
    clear_activity_lane_selection(store);
    crate::store::videos::place_videos_for_activity(store);
    let duration = activity.metadata.get("duration_seconds").and_then(serde_json::Value::as_f64).unwrap_or_default();
    if !(duration.is_finite() && duration > 0.0) {
        return;
    }
    let whole = duration.floor();
    store.editor.set_fallback_duration_seconds(whole);
    store.editor.set_selected_second(0.0);
    apply_course_plot_dimensions(store, &activity.sample_course_points);
}

/// Course widgets sized to a route.
pub fn apply_course_plot_dimensions(store: &mut Store, course_points: &[(Option<f64>, Option<f64>)]) {
    let sizing = ovrley_widgets::model::catalog::catalog()
        .require_widget("course")
        .expect("the catalog defines course")
        .course_sizing
        .expect("course declares its sizing");
    let editor = &mut store.editor;
    if editor.config.plots.iter().any(|plot| plot.get("value").and_then(serde_json::Value::as_str) == Some("course")) {
        if let Some((width, height)) = course_widget_dimensions(course_points, sizing.max_dimension, sizing.min_dimension) {
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
    clear_activity_file(store);
    Ok(())
}

/// The external activity and its source go, and the videos are placed again.
pub fn clear_activity_file(store: &mut Store) {
    if store.media.parsed_activity.is_none() {
        return;
    }
    store.media.clear_activity_file(true);
    clear_activity_lane_selection(store);
    crate::store::videos::place_videos_for_activity(store);
}

/// Changing the activity clears an activity-lane selection, which belongs to the replaced
/// activity.
fn clear_activity_lane_selection(store: &mut Store) {
    use crate::store::videos::ClipLane;
    if store.video_import.selected_clip.as_ref().is_some_and(|selection| selection.lane == ClipLane::Activity) {
        store.video_import.selected_clip = None;
    }
}

/// The preference that lets an agent drive the window over MCP; off when absent.
pub const ALLOW_AGENT_EDITS_KEY: &str = "allowAgentEdits";

/// MCP sessions with the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentSessionState {
    /// The user allows agent edits.
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
pub fn hydrate_agent_preference(store: &mut Store, preferences: &crate::preferences::PreferencesStore) -> Result<(), String> {
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

/// The settings dialog's sections, in the navigation's order.
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

/// The canvas background behind the scene. `Image` is the debug background image; `Video` shows
/// the video preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorBackgroundMode {
    #[default]
    Checker,
    Black,
    White,
    Image,
    Video,
}

/// Editor presentation state, in memory only, with today's launch defaults.
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

/// The zoom range, and the step the zoom buttons move by (rounded to hundredths).
pub const MIN_EDITOR_ZOOM: f64 = 0.35;
pub const MAX_EDITOR_ZOOM: f64 = 4.0;
const ZOOM_STEP: f64 = 0.05;

fn round_to_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

impl EditorShellState {
    pub fn decrease_zoom(&mut self) {
        self.editor_zoom_level = round_to_hundredths(self.editor_zoom_level - ZOOM_STEP).clamp(MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
    }

    pub fn increase_zoom(&mut self) {
        self.editor_zoom_level = round_to_hundredths(self.editor_zoom_level + ZOOM_STEP).clamp(MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
    }

    pub fn reset_editor_zoom(&mut self) {
        self.editor_zoom_level = 1.0;
    }

    /// The wheel zoom's level, clamped like the steps.
    pub fn set_editor_zoom_level(&mut self, zoom: f64) {
        self.editor_zoom_level = zoom.clamp(MIN_EDITOR_ZOOM, MAX_EDITOR_ZOOM);
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

/// What runs once the user answers the unsaved-changes dialog.
#[derive(Debug)]
pub enum UnsavedContinuation {
    /// A new template; an agent's `template_new` waits on its reply.
    NewTemplate { reply: Option<crate::store::agent::AgentReply> },
    /// A new project; an agent's `project_new` waits on its reply.
    NewProject { reply: Option<crate::store::agent::AgentReply> },
    /// Opening the project at `path`.
    OpenProject { path: String, request: crate::features::projects::ProjectOpenRequest },
    /// The window's close request.
    CloseWindow,
}

/// One pending confirmation.
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

    /// Opens the dialog for `continuation`. A request while one is pending replaces it; the
    /// replaced call's agent reply is cancelled.
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

/// The shortcuts reference, one row per description with matching
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
                    let index = shortcuts.iter().position(|row| row.description == description).unwrap_or_else(|| {
                        shortcuts.push(ShortcutRow {
                            description,
                            options: Vec::new(),
                        });
                        shortcuts.len() - 1
                    });
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

/// Where `app-update.versionNowAvailable` puts the version, which the dialog sets apart.
const VERSION_SLOT: &str = "<0>{{version}}</0>";

#[derive(Debug, Default)]
pub enum UpdatePhase {
    #[default]
    Hidden,
    Available(Arc<dyn AppUpdate>),
    /// Until the installed version starts. `progress` is `None` until the first bytes arrive.
    Downloading { update: Arc<dyn AppUpdate>, progress: Option<DownloadProgress> },
    Failed,
}

/// The update prompt's state.
#[derive(Debug, Default)]
pub struct AppUpdateState {
    pub phase: UpdatePhase,
    /// An update is installed: the app starts it as it exits.
    pub relaunch: bool,
    checked: bool,
}

impl AppUpdateState {
    /// Whether the dialog shows.
    pub fn is_open(&self) -> bool {
        !matches!(self.phase, UpdatePhase::Hidden)
    }
}

/// One check per run, never in a development build or in a distribution that
/// is updated from elsewhere. A failed check is only logged.
pub fn check_at_startup(store: &mut Store, services: &mut Services, distribution: Distribution, development: bool) {
    if store.app_update.checked || development || distribution == Distribution::Portable {
        return;
    }
    store.app_update.checked = true;
    let updates = services.updates.clone();
    services.workers.spawn("update-check", move || {
        let checked = updates.check();
        Box::new(move |store, _| match checked {
            Ok(Some(update)) => store.app_update.phase = UpdatePhase::Available(update),
            Ok(None) => {}
            Err(error) => log::error!("Update check failed: {error}"),
        })
    });
}

/// "Later", "Close", Escape and the scrim. A running update cannot be dismissed. The session
/// checks once, so a dismissed update stays away until the next start.
pub fn dismiss(store: &mut Store) {
    if !matches!(store.app_update.phase, UpdatePhase::Downloading { .. }) {
        store.app_update.phase = UpdatePhase::Hidden;
    }
}

/// "Update now": downloads, installs, then closes the app for the relaunch.
pub fn install_update(store: &mut Store, services: &mut Services) {
    let UpdatePhase::Available(update) = &store.app_update.phase else { return };
    let update = update.clone();
    store.app_update.phase = UpdatePhase::Downloading { update: update.clone(), progress: None };
    let ui_post = services.ui_post.clone();
    services.workers.spawn("update-download", move || {
        let latest = Arc::new(Mutex::new(None));
        let downloaded = update.download(&|progress| report_progress(&latest, ui_post.as_ref(), progress));
        Box::new(move |store, services| match downloaded {
            Ok(package) => {
                // Windows installs by ending the process, which skips every drop.
                services.preferences.flush();
                services.workers.spawn("update-install", move || {
                    let installed = update.install(package);
                    Box::new(move |store, services| match installed {
                        Ok(()) => {
                            store.app_update.relaunch = true;
                            services.window.close();
                        }
                        Err(error) => fail(store, &error),
                    })
                });
            }
            Err(error) => fail(store, &error),
        })
    });
}

fn fail(store: &mut Store, error: &str) {
    log::error!("Update installation failed: {error}");
    store.app_update.phase = UpdatePhase::Failed;
}

/// Keeps only the newest progress: one action is posted when the slot goes from empty to full.
fn report_progress(latest: &Arc<Mutex<Option<DownloadProgress>>>, ui_post: &dyn UiPost, progress: DownloadProgress) {
    if latest.lock().expect("update progress poisoned").replace(progress).is_some() {
        return;
    }
    let latest = latest.clone();
    ui_post.post(Box::new(move |store, _| {
        let newest = latest.lock().expect("update progress poisoned").take();
        if let UpdatePhase::Downloading { progress, .. } = &mut store.app_update.phase {
            *progress = newest;
        }
    }));
}

/// The download's percentage, for a download of a known length.
fn progress_percent(progress: DownloadProgress) -> Option<f64> {
    let length = progress.content_length.filter(|length| *length > 0)?;
    Some((progress.downloaded as f64 / length as f64 * 100.0).min(100.0))
}

/// What `UpdatePromptDialog` shows.
#[derive(Debug, PartialEq)]
pub enum UpdatePromptView {
    Hidden,
    /// The announcement's text before and after the version.
    Available { version: String, before: String, after: String },
    /// `progress` is the percentage and its label, `None` while the length is unknown.
    Downloading { version: String, progress: Option<(f64, String)> },
    Failed,
}

pub fn update_prompt_view(store: &Store) -> UpdatePromptView {
    match &store.app_update.phase {
        UpdatePhase::Hidden => UpdatePromptView::Hidden,
        UpdatePhase::Available(update) => {
            let (before, after) = announcement(store.i18n.language);
            UpdatePromptView::Available { version: update.version().into(), before: before.into(), after: after.into() }
        }
        UpdatePhase::Downloading { update, progress } => UpdatePromptView::Downloading {
            version: update.version().into(),
            progress: progress.and_then(progress_percent).map(|percent| (percent, format_update_progress(percent))),
        },
        UpdatePhase::Failed => UpdatePromptView::Failed,
    }
}

fn announcement(locale: Locale) -> (&'static str, &'static str) {
    let text = catalog().t(locale, "app-update.versionNowAvailable");
    let (before, after) = text.split_once(VERSION_SLOT).expect("the announcement has the version's slot");
    (before.trim_end(), after.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Dispatcher;
    use serde_json::json;
    use crate::store::services::fakes::{fake_services, FakeUpdate, Fakes, WindowCall};
    use std::sync::atomic::Ordering;

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
        dispatcher.dispatch(|store, _| crate::features::projects::dismiss_startup_dialog(store));
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
        assert_eq!(state.has_inset(), cfg!(target_os = "macos"));
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
        input.ctrl = ctrl && !cfg!(target_os = "macos");
        input.meta = ctrl && cfg!(target_os = "macos");
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
    fn sync_auto_needs_a_clip_and_an_activity() {
        let (answers, _, _) = press(Store::default(), vec![modified("a", true, true)]);
        assert_eq!(answers, [false], "without a clip the key is not consumed");
    }

    #[test]
    fn mod_i_opens_the_video_dialog() {
        let (answers, _, fakes) = press(Store::default(), vec![modified("i", true, false)]);
        assert_eq!(answers, [true]);
        assert!(matches!(fakes.dialogs.requests()[0], crate::store::services::FileDialogRequest::OpenFiles { .. }));
    }

    #[test]
    fn template_and_activity_commands_run() {
        let (answers, dispatcher, fakes) = press(Store::default(), vec![modified("k", true, false), modified("t", true, false), modified("l", true, true)]);
        assert_eq!(answers, [true, true, true]);
        assert_eq!(fakes.dialogs.requests().len(), 2, "the activity and template dialogs opened");
        assert_eq!(dispatcher.with_store(|store| store.templates.selector_open_serial), 1);
    }

    #[test]
    fn template_save_runs_only_while_the_status_shows() {
        let mut store = Store::default();
        store.template.last_saved_template_state = Some(ovrley_core::template::create_durable_template_state(&store.editor.config, Some(&store.template.global_defaults)).unwrap());
        crate::features::templates::reconcile_durable_state(&mut store);
        let (answers, _, fakes) = press(store, vec![modified("p", true, true)]);
        assert_eq!(answers, [false], "a saved template has nothing to save");
        assert!(fakes.dialogs.requests().is_empty());
        let (answers, _, fakes) = press(Store::default(), vec![modified("p", true, true)]);
        assert_eq!(answers, [true], "a draft saves");
        assert_eq!(fakes.dialogs.requests().len(), 1);
    }

    #[test]
    fn modified_shell_commands_still_run_when_a_field_had_focus() {
        // A focused text field rejects Alt+W, so the window's handler receives it: modified shell
        // commands work in inputs.
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
    fn two_files_of_one_kind_import_nothing_except_videos() {
        assert_eq!(kinds(&["/a/one.fit", "/a/two.gpx"], false), Err("Drop failed: Drop at most one activity file.".into()));
        assert_eq!(kinds(&["/a/one.mp4", "/a/two.MOV", "/a/ride.fit"], false), Ok(vec![DropKind::Activity, DropKind::Video, DropKind::Video]));
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
            drop_files(store, services, &dropped(&["/a/clip.mp4", "/a/other.mp4"]));
            assert_eq!(store.media.error_message, None);
            assert_eq!(store.video_import_review.review.as_ref().unwrap().file_count, 2, "both videos open one review");
        });
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
        fakes.settle(&dispatcher, |store| !store.media.is_processing);
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
        fakes.settle(&dispatcher, |store| !store.media.is_processing);
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
        fakes.settle(&dispatcher, |store| store.media.activity_summary.is_some());
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
        let (preferences, _) = crate::preferences::PreferencesStore::in_memory(json!({}));
        assert!(!Store::hydrate(&preferences).unwrap().agent_session.allow_agent_edits);
        let (preferences, _) = crate::preferences::PreferencesStore::in_memory(json!({"allowAgentEdits": true}));
        assert!(Store::hydrate(&preferences).unwrap().agent_session.allow_agent_edits);
        let (preferences, _) = crate::preferences::PreferencesStore::in_memory(json!({"allowAgentEdits": "yes"}));
        assert!(Store::hydrate(&preferences).is_err());
    }

    fn started(update: FakeUpdate) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(serde_json::json!({}));
        fakes.updates.offer(update);
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| check_at_startup(store, services, Distribution::Installed, false));
        fakes.settle(&dispatcher, |store| store.app_update.is_open());
        (dispatcher, fakes)
    }

    fn progress(downloaded: u64, content_length: Option<u64>) -> DownloadProgress {
        DownloadProgress { downloaded, content_length }
    }

    #[test]
    fn development_and_portable_builds_never_check() {
        let (parts, fakes) = fake_services(serde_json::json!({}));
        let dispatcher = Dispatcher::new(Store::default(), parts);
        dispatcher.dispatch(|store, services| {
            check_at_startup(store, services, Distribution::Installed, true);
            check_at_startup(store, services, Distribution::Portable, false);
            assert!(!store.app_update.checked);
        });
        assert_eq!(fakes.updates.checks.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn later_hides_the_update_for_the_session() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        assert_eq!(
            dispatcher.with_store(update_prompt_view),
            UpdatePromptView::Available { version: "9.8.7".into(), before: "OVRLEY".into(), after: "is now available.".into() }
        );
        dispatcher.dispatch(|store, services| {
            dismiss(store);
            check_at_startup(store, services, Distribution::Installed, false);
        });
        assert!(!dispatcher.with_store(|store| store.app_update.is_open()));
        assert_eq!(fakes.updates.checks.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_update_flushes_the_preferences_then_installs_and_relaunches() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        dispatcher.dispatch(install_update);
        fakes.settle(&dispatcher, |store| store.app_update.relaunch);
        assert!(fakes.updates.flushed_before_install.load(Ordering::SeqCst));
        assert_eq!(*fakes.window.calls.borrow(), [WindowCall::Close]);
    }

    #[test]
    fn a_running_update_cannot_be_dismissed() {
        let (dispatcher, _fakes) = started(FakeUpdate::new("9.8.7"));
        dispatcher.dispatch(|store, services| {
            install_update(store, services);
            dismiss(store);
            assert!(matches!(store.app_update.phase, UpdatePhase::Downloading { .. }));
        });
    }

    #[test]
    fn a_failed_download_and_a_failed_install_leave_the_app_running() {
        let failures = [
            FakeUpdate { download_error: Some("offline".into()), ..FakeUpdate::new("9.8.7") },
            FakeUpdate { install_error: Some("read-only".into()), ..FakeUpdate::new("9.8.7") },
        ];
        for update in failures {
            let (dispatcher, fakes) = started(update);
            dispatcher.dispatch(install_update);
            fakes.settle(&dispatcher, |store| matches!(store.app_update.phase, UpdatePhase::Failed));
            assert!(!dispatcher.with_store(|store| store.app_update.relaunch));
            assert!(fakes.window.calls.borrow().is_empty());
            dispatcher.dispatch(|store, _| dismiss(store));
            assert!(!dispatcher.with_store(|store| store.app_update.is_open()));
        }
    }

    #[test]
    fn progress_is_determinate_only_with_a_length() {
        let (dispatcher, fakes) = started(FakeUpdate::new("9.8.7"));
        let shown = |progress| {
            dispatcher.dispatch(move |store, _| {
                let update = Arc::new(FakeUpdate::new("9.8.7"));
                store.app_update.phase = UpdatePhase::Downloading { update, progress };
            });
            match dispatcher.with_store(update_prompt_view) {
                UpdatePromptView::Downloading { progress, .. } => progress,
                view => panic!("not downloading: {view:?}"),
            }
        };
        assert_eq!(shown(None), None);
        assert_eq!(shown(Some(progress(10, None))), None);
        assert_eq!(shown(Some(progress(10, Some(0)))), None);
        assert_eq!(shown(Some(progress(25, Some(200)))), Some((12.5, "13%".into())));
        assert_eq!(shown(Some(progress(300, Some(200)))), Some((100.0, "100%".into())));

        // The newest of the reports that arrive between two turns of the UI thread.
        let latest = Arc::new(Mutex::new(None));
        report_progress(&latest, &fakes.ui_post, progress(50, Some(200)));
        report_progress(&latest, &fakes.ui_post, progress(150, Some(200)));
        assert_eq!(fakes.ui_post.posted.lock().unwrap().len(), 1);
        fakes.ui_post.drain(&dispatcher);
        assert!(matches!(
            dispatcher.with_store(update_prompt_view),
            UpdatePromptView::Downloading { progress: Some((percent, _)), .. } if percent == 75.0
        ));
    }

    #[test]
    fn every_translation_has_the_versions_slot() {
        for (locale, _) in crate::i18n::LOCALES {
            announcement(locale);
        }
    }
}
