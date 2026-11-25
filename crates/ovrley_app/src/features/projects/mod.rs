//! Projects: new, open, save and close, the project status, and the recovery of missing sources.
//!
//! `.oly` files are read, migrated, validated and written only by `ovrley_services::project`; the
//! editor part goes through the template ingress. Disk work runs on workers; each result comes
//! back as an action. Opening, starting new and closing the window ask about unsaved changes
//! first.

pub mod apply;
pub mod snapshot;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use apply::{apply_new_project, apply_project, create_project_manual_video_sync, create_project_video_edits, resolve_disabled_laps, PreparedProject};
use ovrley_core::normalize::template::create_durable_template_state;
use ovrley_services::project::{default_project_directory, list_project_files, read_project, write_project, OpenedProject, ProjectEditor, ProjectError, ProjectFileSummary};
use ovrley_widgets::model::document::{canonical_numbers, OverlayConfig};
use ovrley_widgets::widgets::raster::RasterError;
use serde_json::{json, Map, Value};
use snapshot::{create_project_snapshot, project_content, project_timeline, LAST_PROJECT_DIRECTORY_KEY, ProjectBaseline};
use crate::dialogs::{filename_from_selected_path, open_single_path, path_in_directory, save_single_path, DialogOptions};
use crate::features::history::HistoryMode;
use crate::features::import::{probe_videos, queue_telemetry_extraction};
use crate::features::shell::{UnsavedAnswer, UnsavedContinuation};
use crate::features::templates::check_document_fonts;
use crate::i18n::catalog;
use crate::store::Store;
use crate::store::agent::{AgentReply, AgentResult};
use crate::store::media::{ActivitySource, ImportedActivity};
use crate::store::services::{DialogFilter, Services};
use crate::video::{create_missing_video, video_metadata_snapshot, ImportedVideo};
use crate::video::collection::sort_clips_by_timeline;

/// The project status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectStatus {
    /// A pristine project never saved.
    New,
    /// Changes in a project never saved, or no baseline yet.
    Unsaved,
    Saved,
    Modified,
}

impl ProjectStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Unsaved => "Unsaved",
            Self::Saved => "Saved",
            Self::Modified => "Modified",
        }
    }

    pub fn has_unsaved_changes(self) -> bool {
        matches!(self, Self::Unsaved | Self::Modified)
    }
}

/// A missing source the user is asked about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceRole {
    Activity,
    Video,
}

/// A one-time notice about a project from an older version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectNotice {
    /// Saving replaced an older-format file, which was kept as this backup.
    FormatUpgraded { backup_path: String },
    /// A version 3 to 5 batch project rendered this folder's videos.
    LegacyBatchFolder { folder: String },
}

/// `StartupProjectsDialog`'s state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartupDialogState {
    pub open: bool,
    pub projects: Vec<ProjectFileSummary>,
    /// The project the dialog is opening; its cards are disabled meanwhile.
    pub opening_path: Option<String>,
}

/// How an open was asked for: an agent's choice about missing sources and its reply, and whether
/// the startup dialog asked (it reopens when the open fails).
#[derive(Debug, Default)]
pub struct ProjectOpenRequest {
    /// `missingSources: skip`: a missing activity or single video loads as absent without asking.
    pub skip_missing_sources: bool,
    pub reply: Option<AgentReply>,
    pub from_startup: bool,
}

/// A project read from disk with its editor normalized, waiting for its sources.
struct ReadProject {
    read: OpenedProject,
    config: OverlayConfig,
    global_defaults: Map<String, Value>,
    activity_exists: bool,
    videos_exist: Vec<bool>,
}

/// A load waiting while its sources are resolved.
struct PendingLoad {
    path: String,
    request: ProjectOpenRequest,
    project: ReadProject,
    /// The activity's resolved path once decided.
    activity: Option<Option<String>>,
    /// The videos' resolved paths, decided in order.
    videos: Vec<Option<String>>,
}

/// `MissingSourceDialog`: the role and file name asked about, and the load waiting on the answer.
pub struct MissingSourceState {
    pub role: SourceRole,
    pub file_name: String,
    pending: PendingLoad,
}

impl std::fmt::Debug for MissingSourceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("MissingSourceState").field("role", &self.role).field("file_name", &self.file_name).finish()
    }
}

/// What the status was last computed from: the pointers of the copy-on-write state and the small
/// project-owned values.
#[derive(Clone, Debug)]
struct StatusKey {
    config: Rc<OverlayConfig>,
    global_defaults: Rc<Map<String, Value>>,
    videos: Rc<Vec<ImportedVideo>>,
    edits: Rc<crate::video::VideoEditsMap>,
    values: StatusValues,
}

/// The small project-owned values of a `StatusKey`, compared by value.
#[derive(Clone, Debug, PartialEq)]
struct StatusValues {
    activity_source: Option<ActivitySource>,
    activity_trim: Option<ovrley_core::activity::trim::ActivityTrim>,
    disabled_laps: Vec<usize>,
    transition_defaults: ovrley_core::encode::ffmpeg::transitions::TransitionDefaults,
    render_settings: ovrley_core::encode::render_settings::RenderSettings,
    manual_video_sync: ovrley_core::media::manual_video_sync::ManualVideoSync,
    batch_output_folder: Option<String>,
    loaded_project_path: Option<String>,
    baseline_serial: u64,
}

impl StatusKey {
    fn of(store: &Store) -> Self {
        Self {
            config: Rc::clone(&store.editor.config),
            global_defaults: Rc::clone(&store.template.global_defaults),
            videos: Rc::clone(&store.video_import.imported_videos),
            edits: Rc::clone(&store.video_import.video_edits),
            values: StatusValues {
                activity_source: store.media.activity_source.clone(),
                activity_trim: store.media.activity_trim,
                disabled_laps: store.media.disabled_laps.clone(),
                transition_defaults: store.video_import.transition_defaults.clone(),
                render_settings: store.render_settings.clone(),
                manual_video_sync: store.manual_video_sync.clone(),
                batch_output_folder: store.batch_render.batch_output_folder.clone(),
                loaded_project_path: store.projects.loaded_project_path.clone(),
                baseline_serial: store.projects.baseline_serial,
            },
        }
    }

    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.config, &other.config)
            && Rc::ptr_eq(&self.global_defaults, &other.global_defaults)
            && Rc::ptr_eq(&self.videos, &other.videos)
            && Rc::ptr_eq(&self.edits, &other.edits)
            && self.values == other.values
    }
}

/// The project lifecycle's state.
#[derive(Debug, Default)]
pub struct ProjectLifecycleState {
    pub loaded_project_path: Option<String>,
    baseline: Option<ProjectBaseline>,
    /// Increments with every new baseline, so the status cache sees it.
    baseline_serial: u64,
    /// A project operation runs (its file dialog included).
    pub running: bool,
    /// A project load runs.
    pub loading: bool,
    pub startup: StartupDialogState,
    startup_requested: bool,
    pub notice: Option<ProjectNotice>,
    pub missing_source: Option<MissingSourceState>,
    /// Whether the content equals the baseline, with what it was computed from.
    status_cache: Option<(StatusKey, bool)>,
    save_jobs: SaveJobs,
}

/// What a busy project command answers an agent.
pub const BUSY: &str = "OVRLEY is busy with another project operation, an import or a render. Try again when it has finished.";

/// What an agent reads when Save cannot pick a file for it.
pub const AGENT_SAVE_NEEDS_PATH: &str =
    "The project has never been saved to a file, so it cannot be saved without a path. Call project_save with a path first.";

/// The project file dialogs' filter.
fn project_filter() -> DialogFilter {
    DialogFilter::new("OVRLEY Project", &["oly"])
}

/// A project operation's translated name, for its error ("open project: …").
fn operation_name(store: &Store, key: &str) -> String {
    catalog().t(store.i18n.language, key).to_string()
}

/// A failed project operation: logged and shown as "{operation}: {message}".
fn report(store: &mut Store, key: &str, error: &str) {
    let operation = operation_name(store, key);
    log::error!("Failed to {operation}: {error}");
    store.media.set_error_message(format!("{operation}: {error}"));
}

/// Why a project operation failed.
#[derive(Debug)]
enum Failure {
    /// A project file could not be read or written: the UI shows the translated message of `key`,
    /// agents read `detail`.
    File { key: &'static str, detail: String },
    /// Any other failure, shown after the operation's name.
    Other(String),
}

impl Failure {
    /// A project file error; `invalid_key` names what a malformed project or a failed file
    /// operation means to this operation.
    fn file(error: &ProjectError, invalid_key: &'static str) -> Self {
        let key = match error {
            ProjectError::ArchiveSize => "projects.archiveTooLarge",
            ProjectError::RasterUnavailable(_) => "projects.rasterUnavailable",
            ProjectError::Invalid(_) => invalid_key,
        };
        Self::File { key, detail: error.to_string() }
    }

    fn into_detail(self) -> String {
        match self {
            Self::File { detail, .. } | Self::Other(detail) => detail,
        }
    }

    fn report(&self, store: &mut Store, operation_key: &str) {
        match self {
            Self::File { key, detail } => {
                log::error!("Failed to {}: {detail}", operation_name(store, operation_key));
                store.media.set_error_message(catalog().t(store.i18n.language, key).to_string());
            }
            Self::Other(message) => report(store, operation_key, message),
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

/// Busy: a project operation, an activity import, a quiet video import or the renderer runs. Project commands are ignored and their buttons disabled meanwhile.
pub fn is_busy(store: &Store) -> bool {
    store.projects.running
        || store.projects.loading
        || store.media.is_processing
        || store.media.importing_video
        || crate::features::render::session::is_renderer_busy(store)
}

/// The user data root (the project and template folders' parent).
fn user_data_root(services: &Services) -> std::path::PathBuf {
    services.paths.user_templates_dir.parent().map(std::path::Path::to_path_buf).unwrap_or_default()
}

// --- status -------------------------------------------------------------------------------------

fn content_unchanged(store: &Store, baseline: &ProjectBaseline) -> bool {
    match project_content(store, store.projects.loaded_project_path.as_deref()) {
        Ok(content) => content == baseline.content,
        Err(error) => {
            log::error!("the project content does not snapshot: {error}");
            false
        }
    }
}

/// The content compared with the baseline, and the playhead
/// and viewport with the saved timeline. Uses the cached comparison when its inputs are unchanged.
pub fn project_status(store: &Store) -> ProjectStatus {
    let Some(baseline) = &store.projects.baseline else { return ProjectStatus::Unsaved };
    let content_same = match &store.projects.status_cache {
        Some((key, same)) if key.same(&StatusKey::of(store)) => *same,
        _ => content_unchanged(store, baseline),
    };
    let unchanged = content_same && project_timeline(store) == baseline.timeline;
    match (store.projects.loaded_project_path.is_some(), unchanged) {
        (false, true) => ProjectStatus::New,
        (false, false) => ProjectStatus::Unsaved,
        (true, true) => ProjectStatus::Saved,
        (true, false) => ProjectStatus::Modified,
    }
}

/// Reconcile step: recomputes the content comparison when the project-owned state changed.
pub fn reconcile_project_status(store: &mut Store) {
    let Some(baseline) = &store.projects.baseline else {
        store.projects.status_cache = None;
        return;
    };
    let key = StatusKey::of(store);
    if store.projects.status_cache.as_ref().is_some_and(|(cached, _)| cached.same(&key)) {
        return;
    }
    let same = content_unchanged(store, baseline);
    store.projects.status_cache = Some((key, same));
}

fn mark_saved(store: &mut Store, path: String, baseline: ProjectBaseline) {
    store.projects.loaded_project_path = Some(path);
    store.projects.baseline = Some(baseline);
    store.projects.baseline_serial += 1;
}

/// The session as a pristine project never saved.
fn mark_new(store: &mut Store) {
    store.projects.loaded_project_path = None;
    store.projects.baseline = match create_project_snapshot(store, None) {
        Ok(document) => Some(ProjectBaseline::of(document)),
        Err(error) => {
            log::error!("the new project does not snapshot: {error}");
            None
        }
    };
    store.projects.baseline_serial += 1;
}

/// The project as agents read it: its file name, path and status.
pub fn describe_project(store: &Store) -> Value {
    let path = store.projects.loaded_project_path.as_deref();
    json!({
        "name": path.and_then(filename_from_selected_path),
        "path": path,
        "status": project_status(store).name(),
    })
}

// --- unsaved changes ----------------------------------------------------------------------------

/// Runs `proceed` now without unsaved changes, else with the agent's
/// answer, else after the user's (the dialog opens with `continuation`).
fn confirm_transition(store: &mut Store, services: &mut Services, agent_answer: Option<UnsavedAnswer>, continuation: UnsavedContinuation) {
    if !project_status(store).has_unsaved_changes() {
        return continue_transition(store, services, UnsavedAnswer::Discard, continuation);
    }
    match agent_answer {
        Some(answer) => continue_transition(store, services, answer, continuation),
        None => store.unsaved_confirm.request_confirm(continuation),
    }
}

/// The project unsaved-changes dialog's answer.
pub fn answer_unsaved_confirm(store: &mut Store, services: &mut Services, answer: UnsavedAnswer) {
    let is_project = matches!(
        store.unsaved_confirm.continuation(),
        Some(UnsavedContinuation::NewProject { .. } | UnsavedContinuation::OpenProject { .. } | UnsavedContinuation::CloseWindow)
    );
    if !is_project {
        return;
    }
    let continuation = store.unsaved_confirm.take().expect("a project confirmation is pending");
    continue_transition(store, services, answer, continuation);
}

fn continue_transition(store: &mut Store, services: &mut Services, answer: UnsavedAnswer, continuation: UnsavedContinuation) {
    match answer {
        UnsavedAnswer::Discard => proceed(store, services, continuation),
        UnsavedAnswer::Cancel => cancel(store, continuation),
        UnsavedAnswer::Save => {
            // An agent cannot answer a file dialog: its Save writes to the loaded project only.
            let from_agent = match &continuation {
                UnsavedContinuation::NewProject { reply } => reply.is_some(),
                UnsavedContinuation::OpenProject { request, .. } => request.reply.is_some(),
                _ => false,
            };
            if from_agent && store.projects.loaded_project_path.is_none() {
                return fail(store, continuation, AGENT_SAVE_NEEDS_PATH.into());
            }
            save_project(
                store,
                services,
                SaveMode::Save,
                None,
                Box::new(move |store, services, saved| match saved {
                    Ok(true) => proceed(store, services, continuation),
                    Ok(false) => cancel(store, continuation),
                    Err(error) => fail(store, continuation, error),
                }),
            );
        }
    }
}

/// What waited on the question runs.
fn proceed(store: &mut Store, services: &mut Services, continuation: UnsavedContinuation) {
    match continuation {
        UnsavedContinuation::NewTemplate { .. } => unreachable!("template confirmations are answered by the template manager"),
        UnsavedContinuation::NewProject { reply } => create_project(store, services, reply),
        UnsavedContinuation::OpenProject { path, request } => load_project(store, services, path, request),
        UnsavedContinuation::CloseWindow => crate::features::render::close_window(store, services),
    }
}

/// Cancelled (or a save that was): nothing happens; an agent reads that nothing was made.
fn cancel(store: &mut Store, continuation: UnsavedContinuation) {
    store.close_guard.handling_close = false;
    match continuation {
        UnsavedContinuation::NewProject { reply: Some(reply) } => reply.send(Ok(json!({ "created": false }))),
        UnsavedContinuation::OpenProject { request, .. } => {
            if let Some(reply) = request.reply {
                reply.send(Ok(json!({ "opened": false })));
            }
        }
        _ => {}
    }
}

/// The save before the transition failed: nothing happens; an agent reads the error.
fn fail(store: &mut Store, continuation: UnsavedContinuation, error: String) {
    store.close_guard.handling_close = false;
    let reply = match continuation {
        UnsavedContinuation::NewProject { reply } => reply,
        UnsavedContinuation::OpenProject { request, .. } => request.reply,
        _ => None,
    };
    if let Some(reply) = reply {
        reply.send(Err(error));
    }
}

// --- new ----------------------------------------------------------------------------------------

/// Asks about unsaved changes first; an agent passes its answer, or waits for
/// the user's.
pub fn new_project(store: &mut Store, services: &mut Services, agent_answer: Option<UnsavedAnswer>, reply: Option<AgentReply>) {
    if is_busy(store) {
        if let Some(reply) = reply {
            reply.send(Err(BUSY.into()));
        }
        return;
    }
    confirm_transition(store, services, agent_answer, UnsavedContinuation::NewProject { reply });
}

/// The new session replaces the history, then becomes the baseline.
fn create_project(store: &mut Store, _services: &mut Services, reply: Option<AgentReply>) {
    if is_busy(store) {
        if let Some(reply) = reply {
            reply.send(Err(BUSY.into()));
        }
        return;
    }
    store.history.set_mode(HistoryMode::Reset);
    let result = apply_new_project(store);
    match &result {
        Ok(()) => {
            mark_new(store);
            store.projects.startup.open = false;
        }
        Err(error) => report(store, "projects.createProject", error),
    }
    if let Some(reply) = reply {
        reply.send(result.map(|()| json!({ "created": true, "project": describe_project(store) })));
    }
}

// --- save ---------------------------------------------------------------------------------------

/// What runs when a save ends: `Ok(true)` saved, `Ok(false)` cancelled or ignored, `Err` failed
/// (already reported).
pub type SaveDone = Box<dyn FnOnce(&mut Store, &mut Services, Result<bool, String>)>;

/// Save, or Save As.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMode {
    /// Writes to the loaded project.
    Save,
    /// Writes to a new path.
    SaveAs,
}

/// Save writes to the loaded project; Save As, or a project never saved, asks for a path (the
/// loaded path, else `project.oly` in the project folder). An agent passes `path` instead.
pub fn save_project(store: &mut Store, services: &mut Services, mode: SaveMode, path: Option<String>, then: SaveDone) {
    if is_busy(store) {
        return then(store, services, Err(BUSY.into()));
    }
    let target = path.or_else(|| (mode == SaveMode::Save).then(|| store.projects.loaded_project_path.clone()).flatten());
    if let Some(target) = target {
        return write_session(store, services, target, then);
    }
    let default_path = match &store.projects.loaded_project_path {
        Some(path) => path.clone(),
        None => match default_project_directory(&user_data_root(services)) {
            Ok(directory) => path_in_directory(&directory.to_string_lossy(), "project.oly"),
            Err(error) => {
                report(store, "projects.saveProject", &error);
                return then(store, services, Err(error));
            }
        },
    };
    store.projects.running = true;
    let shared = Rc::new(std::cell::RefCell::new(Some(then)));
    let fallback = Rc::clone(&shared);
    let opened = save_single_path(
        services,
        &default_path,
        "oly",
        Some("OVRLEY Project"),
        Some(LAST_PROJECT_DIRECTORY_KEY),
        Box::new(move |store, services, selected| {
            store.projects.running = false;
            let then = shared.borrow_mut().take().expect("the dialog answers once");
            match selected {
                Some(path) => write_session(store, services, path, then),
                None => then(store, services, Ok(false)),
            }
        }),
    );
    if let Err(error) = opened {
        store.projects.running = false;
        report(store, "projects.saveProject", &error);
        if let Some(then) = fallback.borrow_mut().take() {
            then(store, services, Err(error));
        }
    }
}

/// The session's snapshot, written on a worker with its raster images and a
/// thumbnail; then the baseline, and the notice when an older-format file was kept as a backup.
fn write_session(store: &mut Store, services: &mut Services, path: String, then: SaveDone) {
    let prepared = create_project_snapshot(store, Some(&path)).map_err(Failure::from).and_then(|document| {
        let handles: HashMap<String, String> = store
            .editor
            .config
            .rasters
            .iter()
            .filter(|raster| raster.get("path").is_some_and(|path| !path.is_null()))
            .map(|raster| {
                let id = raster.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                let resource = raster.get("resourceId").and_then(Value::as_str).ok_or_else(|| format!("Raster {id} has no loaded image resource"))?;
                Ok((id, resource.to_string()))
            })
            .collect::<Result<_, String>>()?;
        let images = services.rasters.owned_assets(handles).map_err(|error| Failure::File { key: "projects.rasterUnavailable", detail: error.clone() })?;
        Ok((document, images))
    });
    let (document, images) = match prepared {
        Ok(prepared) => prepared,
        Err(failure) => {
            failure.report(store, "projects.saveProject");
            return then(store, services, Err(failure.into_detail()));
        }
    };
    store.projects.running = true;
    let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&services.paths.repo_root).ok();
    let job = store.projects.park(then);
    services.workers.spawn("project-save", move || {
        let written = write_project(std::path::Path::new(&path), document.clone(), ffmpeg.as_deref(), images)
            .map_err(|error| Failure::file(&error, "projects.saveFailed"));
        Box::new(move |store: &mut Store, services: &mut Services| {
            store.projects.running = false;
            let then = store.projects.resume(job);
            match written {
                Ok(backup) => {
                    mark_saved(store, path, ProjectBaseline::of(document));
                    if let Some(backup) = backup {
                        store.projects.notice = Some(ProjectNotice::FormatUpgraded { backup_path: backup.to_string_lossy().into_owned() });
                    }
                    then(store, services, Ok(true));
                }
                Err(failure) => {
                    failure.report(store, "projects.saveProject");
                    then(store, services, Err(failure.into_detail()));
                }
            }
        })
    });
}

/// Continuations of running save workers, by job: UI-thread closures, which a worker result
/// (sent across threads) cannot carry.
#[derive(Default)]
struct SaveJobs {
    next: u64,
    waiting: HashMap<u64, SaveDone>,
}

impl std::fmt::Debug for SaveJobs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SaveJobs").field("waiting", &self.waiting.len()).finish()
    }
}

impl ProjectLifecycleState {
    fn park(&mut self, then: SaveDone) -> u64 {
        self.save_jobs.next += 1;
        self.save_jobs.waiting.insert(self.save_jobs.next, then);
        self.save_jobs.next
    }

    fn resume(&mut self, job: u64) -> SaveDone {
        self.save_jobs.waiting.remove(&job).expect("every save job parks its continuation")
    }
}

/// What a save answers an agent.
pub fn save_answer(store: &Store, saved: Result<bool, String>) -> AgentResult {
    match saved {
        Ok(true) => {
            let backup = match &store.projects.notice {
                Some(ProjectNotice::FormatUpgraded { backup_path }) => Some(backup_path.clone()),
                _ => None,
            };
            Ok(json!({ "project": describe_project(store), "backupPath": backup }))
        }
        Ok(false) => Err("The save was cancelled.".into()),
        Err(error) => Err(error),
    }
}

// --- open ---------------------------------------------------------------------------------------

/// The file dialog, then the open of the chosen project, which asks about
/// unsaved changes. A cancelled dialog asks nothing.
pub fn open_project_dialog(store: &mut Store, services: &mut Services) {
    if is_busy(store) {
        return;
    }
    let default_path = match default_project_directory(&user_data_root(services)) {
        Ok(directory) => directory.to_string_lossy().into_owned(),
        Err(error) => return report(store, "projects.openProject", &error),
    };
    store.projects.running = true;
    let opened = open_single_path(
        services,
        &[project_filter()],
        DialogOptions {
            default_path: Some(&default_path),
            last_directory_key: Some(LAST_PROJECT_DIRECTORY_KEY),
        },
        Box::new(|store, services, selected| {
            store.projects.running = false;
            if let Some(path) = selected {
                open_project(store, services, path, None, ProjectOpenRequest::default());
            }
        }),
    );
    if let Err(error) = opened {
        store.projects.running = false;
        report(store, "projects.openProject", &error);
    }
}

/// Opens the project at `path` (the dialog's choice, a drop, an agent's path) after the unsaved
/// question; an agent passes its answer, or waits for the user's.
pub fn open_project(store: &mut Store, services: &mut Services, path: String, agent_answer: Option<UnsavedAnswer>, request: ProjectOpenRequest) {
    if is_busy(store) {
        if let Some(reply) = request.reply {
            reply.send(Err(BUSY.into()));
        }
        return;
    }
    confirm_transition(store, services, agent_answer, UnsavedContinuation::OpenProject { path, request });
}

/// Read on a worker, resolve the sources (asking about a missing activity or single
/// video), prepare the activity and the videos on a worker, then commit in one action that resets
/// the history. Any failure before the commit leaves the session as it was.
fn load_project(store: &mut Store, services: &mut Services, path: String, request: ProjectOpenRequest) {
    if is_busy(store) {
        return end_load(store, request, Err(BUSY.into()));
    }
    store.projects.loading = true;
    let rasters = services.rasters.clone();
    let paths = Arc::clone(&services.paths);
    let read_path = path.clone();
    services.workers.spawn("project-read", move || {
        let read = read_and_normalize(&rasters, &paths, &read_path);
        Box::new(move |store: &mut Store, services: &mut Services| match read {
            Ok(project) => {
                let pending = PendingLoad {
                    path,
                    request,
                    project,
                    activity: None,
                    videos: Vec::new(),
                };
                resolve_sources(store, services, pending);
            }
            Err(failure) => fail_load(store, request, failure),
        })
    });
}

/// The read, the editor through the template ingress with its raster results attached, the fonts
/// checked, and which source files exist.
fn read_and_normalize(rasters: &ovrley_services::rasters::RasterResources, paths: &ovrley_core::paths::AppPaths, path: &str) -> Result<ReadProject, Failure> {
    let mut read = read_project(rasters, std::path::Path::new(path)).map_err(|error| Failure::file(&error, "projects.invalidArchive"))?;
    let editor = &read.project.editor;
    let config = OverlayConfig::from_value(&editor.config)?;
    let durable = create_durable_template_state(&config, Some(&editor.global_defaults)).map_err(|error| error.to_string())?;
    check_document_fonts(paths, &durable.config, &durable.global_defaults)?;
    // The baseline compares with the session's snapshot; JSON has one number type, so whole
    // floats the file holds (`"fps": 30.0`) read as the integers the session writes.
    let mut baseline_config = durable.config.to_value();
    canonical_numbers(baseline_config.as_object_mut().expect("a config is an object"));
    read.project.editor = ProjectEditor {
        config: baseline_config,
        global_defaults: durable.global_defaults.clone(),
    };
    let mut config = durable.config;
    attach_raster_load_results(&mut config, &read.rasters)?;
    let sources = &read.resolved_sources;
    let activity_exists = sources.activity_path.as_deref().is_some_and(std::path::Path::is_file);
    let videos_exist = sources.video_paths.iter().map(|path| path.is_file()).collect();
    Ok(ReadProject {
        read,
        config,
        global_defaults: durable.global_defaults,
        activity_exists,
        videos_exist,
    })
}

/// Each raster with an image gets its resource or its error.
fn attach_raster_load_results(config: &mut OverlayConfig, results: &HashMap<String, Result<String, RasterError>>) -> Result<(), String> {
    for raster in &mut config.rasters {
        if raster.get("path").is_none_or(Value::is_null) {
            continue;
        }
        let id = raster.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
        match results.get(&id).ok_or_else(|| format!("Raster {id} has no resource result"))? {
            Ok(resource_id) => raster.insert("resourceId".into(), Value::String(resource_id.clone())),
            Err(error) => raster.insert("resourceErrorCode".into(), Value::String(error.code().into())),
        };
    }
    Ok(())
}

/// The activity, then the videos. A missing activity, or the video of a
/// single-video project, asks to be located, loaded as absent, or the load cancelled; the missing
/// videos of a project with several load as missing without asking.
fn resolve_sources(store: &mut Store, services: &mut Services, mut pending: PendingLoad) {
    let sources = &pending.project.read.resolved_sources;
    if pending.activity.is_none() {
        match sources.activity_path.as_deref().map(|path| path.to_string_lossy().into_owned()) {
            None => pending.activity = Some(None),
            Some(path) if pending.project.activity_exists => pending.activity = Some(Some(path)),
            Some(path) => return ask_missing_source(store, services, pending, SourceRole::Activity, &path),
        }
    }
    let sources = &pending.project.read.resolved_sources;
    let video_paths: Vec<String> = sources.video_paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    if video_paths.len() > 1 {
        pending.videos = video_paths.iter().cloned().zip(&pending.project.videos_exist).map(|(path, exists)| exists.then_some(path)).collect();
    }
    while pending.videos.len() < video_paths.len() {
        let index = pending.videos.len();
        if pending.project.videos_exist[index] {
            pending.videos.push(Some(video_paths[index].clone()));
        } else {
            return ask_missing_source(store, services, pending, SourceRole::Video, &video_paths[index].clone());
        }
    }
    prepare_project(store, services, pending);
}

fn ask_missing_source(store: &mut Store, services: &mut Services, mut pending: PendingLoad, role: SourceRole, path: &str) {
    if pending.request.skip_missing_sources {
        record_source(&mut pending, role, None);
        return resolve_sources(store, services, pending);
    }
    store.projects.missing_source = Some(MissingSourceState {
        role,
        file_name: filename_from_selected_path(path).unwrap_or(path).to_string(),
        pending,
    });
}

fn record_source(pending: &mut PendingLoad, role: SourceRole, path: Option<String>) {
    match role {
        SourceRole::Activity => pending.activity = Some(path),
        SourceRole::Video => pending.videos.push(path),
    }
}

/// The missing-source dialog's Locate: a file dialog for the role; a cancelled dialog keeps the
/// question open. A file of another type fails the load.
pub fn locate_missing_source(store: &mut Store, services: &mut Services) {
    let Some(missing) = &store.projects.missing_source else { return };
    let filter = match missing.role {
        SourceRole::Activity => crate::features::shell::activity_dialog_filter(),
        SourceRole::Video => DialogFilter::new("Video", &crate::features::import::VIDEO_EXTENSIONS),
    };
    let accepted = filter.extensions.clone();
    let opened = open_single_path(
        services,
        std::slice::from_ref(&filter),
        DialogOptions::default(),
        Box::new(move |store, services, selected| {
            let Some(path) = selected else { return };
            let Some(missing) = store.projects.missing_source.take() else { return };
            let extension = path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
            if !accepted.contains(&extension) {
                let role = match missing.role {
                    SourceRole::Activity => "activity",
                    SourceRole::Video => "video",
                };
                return fail_load(store, missing.pending.request, format!("Selected file is not a supported {role} source").into());
            }
            let mut pending = missing.pending;
            record_source(&mut pending, missing.role, Some(path));
            resolve_sources(store, services, pending);
        }),
    );
    if let Err(error) = opened {
        report(store, "projects.openProject", &error);
    }
}

/// The missing-source dialog's Load Anyway: the source loads as absent.
pub fn skip_missing_source(store: &mut Store, services: &mut Services) {
    let Some(missing) = store.projects.missing_source.take() else { return };
    let mut pending = missing.pending;
    record_source(&mut pending, missing.role, None);
    resolve_sources(store, services, pending);
}

/// The missing-source dialog's Cancel: the load ends and the session stays as it was.
pub fn cancel_missing_source(store: &mut Store) {
    let Some(missing) = store.projects.missing_source.take() else { return };
    end_load(store, missing.pending.request, Ok(json!({ "opened": false })));
}

/// The sources prepared on a worker: the activity through the one activity import, each found
/// video probed with its thumbnail, each missing one created from its saved metadata (an older
/// project's missing video, which has none, is left out), and the saved state fitted to them.
fn prepare_project(store: &mut Store, services: &mut Services, pending: PendingLoad) {
    let PendingLoad { path, request, project, activity, videos } = pending;
    let activity_path = activity.expect("the activity was resolved");
    let tools = Arc::clone(&services.video_tools);
    let paths = Arc::clone(&services.paths);
    let unsupported = catalog().t_or(store.i18n.language, "video-preview.unsupportedVideoFile", "Not a supported video file (MP4, MOV, MKV).").to_string();
    services.workers.spawn("project-prepare", move || {
        let prepared = (|| -> Result<PreparedProject, String> {
            let ReadProject { read, config, global_defaults, .. } = project;
            let mut document = read.project;
            let legacy_batch_video_folder = read.legacy_batch_video_folder;
            let saved_paths: Vec<String> = read.resolved_sources.video_paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
            let activity = match activity_path {
                Some(path) => Some((path.clone(), ovrley_core::activity::import_activity_path(&paths, &path).map_err(|error| error.to_string()).and_then(ImportedActivity::from_response)?)),
                None => None,
            };
            let found: Vec<(String, String, u32)> = document
                .sources
                .videos
                .iter()
                .zip(&videos)
                .enumerate()
                .filter_map(|(index, (video, path))| path.clone().map(|path| (path, video.id.clone(), index as u32)))
                .collect();
            let mut probed: HashMap<String, ImportedVideo> = HashMap::new();
            for (_, result) in probe_videos(&*tools, found, &unsupported) {
                let video = result?;
                probed.insert(video.id.clone(), video);
            }
            let loaded: Vec<ImportedVideo> = document
                .sources
                .videos
                .iter()
                .enumerate()
                .filter_map(|(index, video)| match probed.remove(&video.id) {
                    Some(found) => Some(found),
                    None => video.metadata.as_ref().map(|metadata| create_missing_video(metadata, video.id.clone(), saved_paths[index].clone(), index as u32)),
                })
                .collect();
            let edits = create_project_video_edits(&loaded, &document.sources.videos)?;
            let manual_video_sync = create_project_manual_video_sync(&loaded, &document.sync.manual);
            let disabled_laps = resolve_disabled_laps(&document.laps.disabled, activity.as_ref().map(|(_, activity)| activity))?;
            // Metadata learned from a probe is not a change the user made.
            for video in &mut document.sources.videos {
                if let Some(loaded) = loaded.iter().find(|candidate| candidate.id == video.id) {
                    video.metadata = Some(video_metadata_snapshot(loaded));
                }
            }
            Ok(PreparedProject {
                document,
                config,
                global_defaults,
                activity,
                videos: loaded,
                edits,
                manual_video_sync,
                disabled_laps,
                legacy_batch_video_folder,
            })
        })();
        Box::new(move |store: &mut Store, services: &mut Services| match prepared {
            Ok(prepared) => commit_project(store, services, path, request, prepared),
            Err(error) => fail_load(store, request, error.into()),
        })
    });
}

/// Applies a prepared project as the new document, then the background, telemetry, the baseline
/// and the legacy folder's notice.
fn commit_project(store: &mut Store, services: &mut Services, path: String, request: ProjectOpenRequest, prepared: PreparedProject) {
    let legacy_folder = prepared.legacy_batch_video_folder.clone();
    let baseline = ProjectBaseline::of(prepared.document.clone());
    let found_ids: Vec<String> = prepared.videos.iter().filter(|video| !video.missing).map(|video| video.id.clone()).collect();
    let has_videos = !prepared.videos.is_empty();
    store.history.set_mode(HistoryMode::Reset);
    apply_project(store, prepared);
    if has_videos {
        store.editor_shell.set_editor_background_mode(crate::features::shell::EditorBackgroundMode::Video);
    }
    queue_telemetry_extraction(store, services, &found_ids);
    mark_saved(store, path, baseline);
    if let Some(folder) = &legacy_folder {
        store.projects.notice = Some(ProjectNotice::LegacyBatchFolder { folder: folder.clone() });
    }
    let answer = json!({ "project": describe_project(store), "legacyBatchVideoFolder": legacy_folder });
    end_load(store, request, Ok(answer));
}

fn fail_load(store: &mut Store, request: ProjectOpenRequest, failure: Failure) {
    failure.report(store, "projects.openProject");
    end_load(store, request, Err(failure.into_detail()));
}

/// Ends a load: the startup dialog reopens when its open did not succeed.
fn end_load(store: &mut Store, request: ProjectOpenRequest, answer: AgentResult) {
    store.projects.loading = false;
    if request.from_startup {
        store.projects.startup.opening_path = None;
        let opened = answer.as_ref().is_ok_and(|value| value.get("opened") != Some(&Value::Bool(false)));
        store.projects.startup.open = !opened;
    }
    if let Some(reply) = request.reply {
        reply.send(answer);
    }
}

// --- startup ------------------------------------------------------------------------------------

/// The projects of `directory`, else of the remembered folder, else of the default one (created
/// when absent), listed on a worker; `then` runs with them on the UI thread.
pub fn list_projects(
    services: &mut Services,
    directory: Option<String>,
    then: impl FnOnce(&mut Store, Result<Vec<ProjectFileSummary>, String>) + Send + 'static,
) {
    let remembered = match directory {
        Some(directory) => Ok(Some(directory)),
        None => services.preferences.optional_path(LAST_PROJECT_DIRECTORY_KEY).map(|path| path.map(|path| path.to_string_lossy().into_owned())),
    };
    let root = user_data_root(services);
    services.workers.spawn("project-list", move || {
        let listed = remembered
            .and_then(|directory| directory.map_or_else(|| default_project_directory(&root), |directory| Ok(directory.into())))
            .and_then(|directory| list_project_files(&directory));
        Box::new(move |store: &mut Store, _: &mut Services| then(store, listed))
    });
}

/// Once startup is ready: the projects of the remembered folder, or the default one. The dialog
/// opens even when listing failed; the error goes to the alert.
pub fn load_startup_projects(store: &mut Store, services: &mut Services) {
    if store.projects.startup_requested {
        return;
    }
    store.projects.startup_requested = true;
    list_projects(services, None, finish_startup_listing);
}

fn finish_startup_listing(store: &mut Store, listed: Result<Vec<ProjectFileSummary>, String>) {
    match listed {
        Ok(projects) => store.projects.startup.projects = projects,
        Err(error) => {
            log::error!("Failed to list startup projects: {error}");
            store.media.set_error_message(format!("Failed to list projects: {error}"));
        }
    }
    store.projects.startup.open = true;
}

/// The startup dialog's dismissal: the session becomes a pristine new project.
pub fn dismiss_startup_dialog(store: &mut Store) {
    mark_new(store);
    store.projects.startup.open = false;
}

/// The startup dialog's New Project: no question, the dialog closes once it is made.
pub fn startup_new_project(store: &mut Store, services: &mut Services) {
    create_project(store, services, None);
}

/// The startup dialog's project: the dialog closes while it opens, without the unsaved question
/// (the session has no baseline yet), and reopens when the open fails.
pub fn startup_open_project(store: &mut Store, services: &mut Services, path: String) {
    if is_busy(store) {
        return;
    }
    store.projects.startup.opening_path = Some(path.clone());
    store.projects.startup.open = false;
    load_project(
        store,
        services,
        path,
        ProjectOpenRequest {
            from_startup: true,
            ..ProjectOpenRequest::default()
        },
    );
}

// --- close --------------------------------------------------------------------------------------

/// The window's close request with the project check: unsaved changes ask first; a request while one is
/// handled, or while a confirmation is open, is ignored.
pub fn request_close(store: &mut Store, services: &mut Services) {
    if store.close_guard.handling_close || store.unsaved_confirm.is_open() {
        return;
    }
    store.close_guard.handling_close = true;
    confirm_transition(store, services, None, UnsavedContinuation::CloseWindow);
}

// --- views --------------------------------------------------------------------------------------

/// A source of the projects drawer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSourceRow {
    pub filename: String,
    pub path: String,
}

/// `ProjectsDrawerContent`'s view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectsDrawerView {
    pub busy: bool,
    pub save_disabled: bool,
    /// The loaded project's file name and path; the sources show only with one.
    pub project: Option<ProjectSourceRow>,
    pub activity: Option<ProjectSourceRow>,
    /// The videos in timeline order.
    pub videos: Vec<ProjectSourceRow>,
}

fn source_row(path: &str) -> ProjectSourceRow {
    ProjectSourceRow {
        filename: filename_from_selected_path(path).unwrap_or(path).to_string(),
        path: path.to_string(),
    }
}

pub fn projects_drawer_view(store: &Store) -> ProjectsDrawerView {
    let clips = store.video_import.clips();
    ProjectsDrawerView {
        busy: is_busy(store),
        save_disabled: project_status(store) == ProjectStatus::Saved,
        project: store.projects.loaded_project_path.as_deref().map(source_row),
        activity: store.media.activity_source.as_ref().map(|ActivitySource::File { path }| source_row(path)),
        videos: sort_clips_by_timeline(&clips).into_iter().map(|clip| source_row(&clip.video.path)).collect(),
    }
}

/// Whether a project dialog is open: the startup projects, a missing source or a notice.
pub fn has_open_dialog(store: &Store) -> bool {
    store.projects.startup.open || store.projects.missing_source.is_some() || store.projects.notice.is_some()
}

#[cfg(test)]
mod tests;
