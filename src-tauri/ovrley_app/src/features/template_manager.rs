//! The template-manager feature: `useTemplateManagement`, `useTemplateFetching`,
//! `useTemplateSaveStatus` and the file helpers of `templateSnapshot.js` and
//! `templateFileUtils.js`. File ingress and egress are `ovrley_core::normalize::template` (D3).
//!
//! Actions take paths (D5): the UI shows a dialog first, agents pass the path. Work that touches
//! the disk runs on a worker; its result comes back as an action, and the continuation the caller
//! left (a restore step, an agent's reply) runs with it.

use crate::features::app_shell::{UnsavedAnswer, UnsavedContinuation};
use crate::features::undo_redo::HistoryMode;
use crate::lib::file_dialog::{open_single_path, save_single_path, DialogOptions};
use crate::lib::fonts::collect_font_ids;
use crate::store::agent::{AgentReply, AgentResult};
use crate::store::services::{DialogFilter, Services};
use crate::store::slices::template::{
    create_new_template, hydrate_template_state, TemplateDescriptor, TemplateKind, TemplateSource,
};
use crate::store::Store;
use ovrley_core::normalize::template::{
    create_durable_template_state, load_template_document, template_file_payload,
    DurableTemplateState,
};
use ovrley_core::paths::AppPaths;
use ovrley_services::raster_resources::RasterResources;
use ovrley_widgets::model::document::OverlayConfig;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// The preference that restores the last template at startup.
pub const LAST_TEMPLATE_KEY: &str = "last-template";

/// What runs on the UI thread when a template operation ends: `Ok` or the error message shown.
pub type OnDone = Box<dyn FnOnce(&mut Store, &mut Services, Result<(), String>)>;

/// An agent's reply as a continuation: success answers with `answer(store)`.
pub fn reply_with(reply: AgentReply, answer: fn(&Store) -> Value) -> OnDone {
    Box::new(move |store, _, result| reply.send(result.map(|()| answer(store))))
}

/// `useTemplateSaveStatus`'s status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Draft,
    Saved,
    Modified,
}

impl SaveStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Saved => "Saved",
            Self::Modified => "Modified",
        }
    }

    /// `showTemplateStatus`.
    pub fn shows(self) -> bool {
        matches!(self, Self::Draft | Self::Modified)
    }
}

/// The durable state of the document on screen, cached by the document and global-default
/// pointers (parent §2: expensive derivations are cached fields).
#[derive(Debug, Default)]
struct DurableCache {
    config: Option<Rc<OverlayConfig>>,
    global_defaults: Option<Rc<Map<String, Value>>>,
    state: Option<Result<DurableTemplateState, String>>,
}

#[derive(Default)]
pub struct TemplateManagerState {
    /// `templateSelectorOpen`.
    pub selector_open: bool,
    /// Callers waiting for the running template list fetch; concurrent fetches share it.
    fetch_waiters: Option<Vec<Box<dyn FnOnce(&mut Store, &mut Services)>>>,
    /// Continuations of running workers, by job.
    continuations: HashMap<u64, OnDone>,
    next_job: u64,
    durable: DurableCache,
}

impl std::fmt::Debug for TemplateManagerState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TemplateManagerState")
            .field("selector_open", &self.selector_open)
            .field("fetching", &self.fetch_waiters.is_some())
            .field("jobs", &self.continuations.len())
            .finish()
    }
}

impl TemplateManagerState {
    fn park(&mut self, on_done: OnDone) -> u64 {
        self.next_job += 1;
        self.continuations.insert(self.next_job, on_done);
        self.next_job
    }

    fn resume(&mut self, job: u64) -> OnDone {
        self.continuations
            .remove(&job)
            .expect("every template job parks its continuation")
    }

    /// The cached durable state of the document on screen.
    pub fn current_durable_state(&self) -> Option<&Result<DurableTemplateState, String>> {
        self.durable.state.as_ref()
    }
}

/// Reconcile step: recomputes the durable state when the document or the global defaults changed.
pub fn reconcile_durable_state(store: &mut Store) {
    let cache = &store.template_manager.durable;
    let config_same = cache
        .config
        .as_ref()
        .is_some_and(|config| Rc::ptr_eq(config, &store.editor.config));
    let globals_same = cache
        .global_defaults
        .as_ref()
        .is_some_and(|globals| Rc::ptr_eq(globals, &store.template.global_defaults));
    if config_same && globals_same {
        return;
    }
    let state =
        create_durable_template_state(&store.editor.config, Some(&store.template.global_defaults))
            .map_err(|error| error.to_string());
    if let Err(error) = &state {
        log::error!("the document on screen does not normalize: {error}");
    }
    store.template_manager.durable = DurableCache {
        config: Some(Rc::clone(&store.editor.config)),
        global_defaults: Some(Rc::clone(&store.template.global_defaults)),
        state: Some(state),
    };
}

/// `useTemplateSaveStatus`: Draft until saved, then Saved or Modified. A document that does not
/// normalize (a defect, logged by the reconcile step) counts as modified.
pub fn save_status(store: &Store) -> SaveStatus {
    let Some(saved) = &store.template.last_saved_template_state else {
        return SaveStatus::Draft;
    };
    match store.template_manager.current_durable_state() {
        Some(Ok(current)) if current == saved => SaveStatus::Saved,
        _ => SaveStatus::Modified,
    }
}

/// `sanitizeTemplateFilename`.
pub fn sanitize_template_filename(name: &str) -> String {
    let trimmed = name.trim();
    let stem = match trimmed.rfind('.') {
        Some(dot) if !trimmed[dot + 1..].is_empty() && !trimmed[dot + 1..].contains(['/', '.']) => {
            &trimmed[..dot]
        }
        _ => trimmed,
    };
    let mut normalized = String::new();
    let mut previous_underscore = false;
    for character in stem.chars() {
        let keep = character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-');
        let next = if keep { character } else { '_' };
        if next == '_' {
            if previous_underscore {
                continue;
            }
            previous_underscore = true;
        } else {
            previous_underscore = false;
        }
        normalized.push(next);
    }
    let normalized = normalized.trim_matches('_');
    let normalized = if normalized.is_empty() {
        "ovrley_template"
    } else {
        normalized
    };
    format!("{normalized}.json")
}

/// `getFilenameFromPath`.
pub fn filename_from_path(path: Option<&str>) -> String {
    let last = path
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    if last.is_empty() {
        "ovrley_template.json".into()
    } else {
        last.to_string()
    }
}

/// `getFilenameFromTemplateId`.
pub fn filename_from_template_id(id: &str) -> &str {
    id.strip_prefix("user:")
        .or_else(|| id.strip_prefix("built-in:"))
        .unwrap_or(id)
}

/// The filename a save suggests, from the loaded template.
pub fn suggested_template_filename(source: Option<&TemplateSource>) -> String {
    let source_name = match source {
        Some(TemplateSource::Bundled { template_id }) => Some(template_id.as_str()),
        Some(TemplateSource::File { path }) => Some(path.as_str()),
        None => None,
    };
    let name = filename_from_path(source_name);
    let name = filename_from_template_id(&name);
    sanitize_template_filename(if name.is_empty() { "my_template" } else { name })
}

/// One group of the template select (`getTemplateGroups`).
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateGroup {
    pub label: String,
    pub templates: Vec<TemplateDescriptor>,
}

/// `getTemplateGroups`: templates grouped by resolution, largest first, names sorted
/// case-insensitively. `unknown` labels templates without a valid resolution.
pub fn template_groups(templates: &[TemplateDescriptor], unknown: &str) -> Vec<TemplateGroup> {
    struct Group {
        label: String,
        pixels: i64,
        width: i64,
        height: i64,
        templates: Vec<TemplateDescriptor>,
    }
    let mut groups: Vec<Group> = Vec::new();
    for template in templates {
        let valid = template.width > 0 && template.height > 0;
        let label = if valid {
            format!("{} x {}", template.width, template.height)
        } else {
            unknown.to_string()
        };
        let index = match groups.iter().position(|group| group.label == label) {
            Some(index) => index,
            None => {
                let (width, height) = if valid {
                    (i64::from(template.width), i64::from(template.height))
                } else {
                    (-1, -1)
                };
                groups.push(Group {
                    label,
                    pixels: if valid { width * height } else { -1 },
                    width,
                    height,
                    templates: Vec::new(),
                });
                groups.len() - 1
            }
        };
        groups[index].templates.push(template.clone());
    }
    for group in &mut groups {
        group
            .templates
            .sort_by_key(|template| template.name.to_lowercase());
    }
    groups.sort_by(|left, right| {
        right
            .pixels
            .cmp(&left.pixels)
            .then(right.width.cmp(&left.width))
            .then(right.height.cmp(&left.height))
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });
    groups
        .into_iter()
        .map(|group| TemplateGroup {
            label: group.label,
            templates: group.templates,
        })
        .collect()
}

/// `useTemplateFetching`: lists the templates on a worker. A fetch while one runs waits for it.
pub fn fetch_templates(
    store: &mut Store,
    services: &mut Services,
    then: Option<Box<dyn FnOnce(&mut Store, &mut Services)>>,
) {
    if let Some(waiters) = &mut store.template_manager.fetch_waiters {
        waiters.extend(then);
        return;
    }
    store.template_manager.fetch_waiters = Some(then.into_iter().collect());
    let paths = Arc::clone(&services.paths);
    services.workers.spawn("template-list", move || {
        let listed = ovrley_core::commands::backend_list_templates(&paths)
            .map_err(|error| error.to_string())
            .and_then(|value| {
                serde_json::from_value::<Vec<TemplateDescriptor>>(value)
                    .map_err(|error| error.to_string())
            });
        Box::new(move |store: &mut Store, services: &mut Services| {
            match listed {
                Ok(templates) => store.template.set_templates(templates),
                Err(error) => log::error!("Failed to fetch templates: {error}"),
            }
            let waiters = store
                .template_manager
                .fetch_waiters
                .take()
                .unwrap_or_default();
            for waiter in waiters {
                waiter(store, services);
            }
        })
    });
}

/// A template file prepared for the editor: its durable document with the raster resources
/// attached, its fonts known.
struct PreparedTemplate {
    config: OverlayConfig,
    global_defaults: Map<String, Value>,
}

/// `prepareTemplateFilePayload`: the one ingress, then the load-time resources. A raster that
/// fails to load keeps its error on its widget, so the rest of the template still loads; an
/// unknown font fails the load.
fn prepare_template(
    paths: &AppPaths,
    rasters: &RasterResources,
    file: &Value,
) -> Result<PreparedTemplate, String> {
    let document = load_template_document(file).map_err(|error| error.to_string())?;
    let mut config = document.config;
    for raster in &mut config.rasters {
        let Some(path) = raster
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        match ovrley_services::file_ops::load_selected_raster(rasters, std::path::Path::new(&path))
        {
            Ok(selection) => {
                raster.insert("resourceId".into(), Value::String(selection.resource_id))
            }
            Err(error) => raster.insert(
                "resourceErrorCode".into(),
                Value::String(error.code.to_string()),
            ),
        };
    }
    let fonts = collect_font_ids(
        &json!({ "config": config.to_value(), "settings": { "globalDefaults": document.global_defaults } }),
    );
    for font in fonts {
        ovrley_core::fonts::font_capabilities(&paths.font_dirs, &font)
            .map_err(|error| error.to_string())?;
    }
    Ok(PreparedTemplate {
        config,
        global_defaults: document.global_defaults,
    })
}

/// `applyLoadedTemplateState` under `replaceEditorDocument`: the document replaces the history.
fn apply_prepared(store: &mut Store, prepared: PreparedTemplate, source: Option<TemplateSource>) {
    store.history.set_mode(HistoryMode::Reset);
    let saved = create_durable_template_state(&prepared.config, Some(&prepared.global_defaults))
        .map_err(|error| error.to_string());
    hydrate_template_state(
        &mut store.editor,
        &mut store.template,
        prepared.config,
        prepared.global_defaults,
        source,
    );
    match saved {
        Ok(saved) => store.template.set_last_saved_template_state(Some(saved)),
        Err(error) => log::error!("a loaded template does not normalize again: {error}"),
    }
}

fn spawn_prepare(
    store: &mut Store,
    services: &mut Services,
    read: impl FnOnce(&AppPaths) -> Result<Value, String> + Send + 'static,
    finish: impl FnOnce(&mut Store, &mut Services, Result<PreparedTemplate, String>, OnDone) + Send + 'static,
    on_done: OnDone,
) {
    let job = store.template_manager.park(on_done);
    let paths = Arc::clone(&services.paths);
    let rasters = services.rasters.clone();
    services.workers.spawn("template-load", move || {
        let prepared = read(&paths).and_then(|file| prepare_template(&paths, &rasters, &file));
        Box::new(move |store: &mut Store, services: &mut Services| {
            let on_done = store.template_manager.resume(job);
            finish(store, services, prepared, on_done);
        })
    });
}

/// `handleTemplateChange`: loads a listed template (refetching the list once when the id is
/// unknown), replaces the document and remembers it for the next start.
pub fn load_template(store: &mut Store, services: &mut Services, id: String, on_done: OnDone) {
    store.media.set_processing(true);
    if store.template.find_template(&id).is_some() {
        return load_listed_template(store, services, id, on_done);
    }
    fetch_templates(
        store,
        services,
        Some(Box::new(move |store, services| {
            load_listed_template(store, services, id, on_done)
        })),
    );
}

fn load_listed_template(store: &mut Store, services: &mut Services, id: String, on_done: OnDone) {
    let Some(descriptor) = store.template.find_template(&id).cloned() else {
        return fail_load(store, services, format!("Unknown template: {id}"), on_done);
    };
    let source = match descriptor.kind {
        TemplateKind::BuiltIn => Some(TemplateSource::Bundled {
            template_id: id.clone(),
        }),
        TemplateKind::User => descriptor
            .path
            .clone()
            .map(|path| TemplateSource::File { path }),
    };
    let read_id = id.clone();
    spawn_prepare(
        store,
        services,
        move |paths| {
            let text = ovrley_core::commands::backend_get_template(paths, &read_id)
                .map_err(|error| error.to_string())?;
            serde_json::from_str(&text).map_err(|error| error.to_string())
        },
        move |store, services, prepared, on_done| match prepared {
            Ok(prepared) => {
                apply_prepared(store, prepared, source);
                services.preferences.set(
                    LAST_TEMPLATE_KEY,
                    json!({"source": "backend", "filename": id}),
                );
                store.media.set_processing(false);
                on_done(store, services, Ok(()));
            }
            Err(error) => fail_load(store, services, error, on_done),
        },
        on_done,
    );
}

fn fail_load(store: &mut Store, services: &mut Services, error: String, on_done: OnDone) {
    log::error!("Failed to load template: {error}");
    store
        .media
        .set_error_message(format!("Failed to load template: {error}"));
    store.media.set_processing(false);
    on_done(store, services, Err(error));
}

/// The template file dialog's filter.
pub fn template_dialog_filter() -> DialogFilter {
    DialogFilter::new("OVRLEY Template", &["json"])
}

/// `handleImportTemplate`'s UI part: the dialog, then the import.
pub fn open_import_template_dialog(store: &mut Store, services: &mut Services) {
    let opened = open_single_path(
        services,
        &[template_dialog_filter()],
        DialogOptions::default(),
        Box::new(|store, services, selected| {
            if let Some(path) = selected {
                import_template(store, services, path, None);
            }
        }),
    );
    if let Err(error) = opened {
        store
            .media
            .set_error_message(format!("Failed to import template: {error}"));
    }
}

/// `handleImportTemplate`: a template file replaces the document. An import does not change the
/// startup template, as today.
pub fn import_template(
    store: &mut Store,
    services: &mut Services,
    path: String,
    reply: Option<AgentReply>,
) {
    let on_done: OnDone = match reply {
        Some(reply) => reply_with(reply, crate::store::agent::describe_template),
        None => Box::new(|_, _, _| {}),
    };
    import_template_file(store, services, path, "Failed to import template", on_done);
}

fn import_template_file(
    store: &mut Store,
    services: &mut Services,
    path: String,
    error_prefix: &'static str,
    on_done: OnDone,
) {
    let read_path = path.clone();
    spawn_prepare(
        store,
        services,
        move |_| {
            let text = std::fs::read_to_string(&read_path)
                .map_err(|error| format!("{read_path}: {error}"))?;
            serde_json::from_str(&text).map_err(|error| error.to_string())
        },
        move |store, services, prepared, on_done| match prepared {
            Ok(prepared) => {
                apply_prepared(store, prepared, Some(TemplateSource::File { path }));
                on_done(store, services, Ok(()));
            }
            Err(error) => {
                log::error!("{error_prefix}: {error}");
                store
                    .media
                    .set_error_message(format!("{error_prefix}: {error}"));
                on_done(store, services, Err(error));
            }
        },
        on_done,
    );
}

/// `confirmCreateNewTemplate`: the new document replaces the history; the startup template is
/// forgotten and the zoom resets (`onTemplateCreated`).
fn confirm_create_new_template(store: &mut Store, services: &mut Services) {
    store.history.set_mode(HistoryMode::Reset);
    create_new_template(&mut store.editor, &mut store.template);
    services.preferences.delete(LAST_TEMPLATE_KEY);
    store.editor_shell.reset_editor_zoom();
}

/// What a new-template request answers an agent.
fn new_template_answer(created: bool) -> AgentResult {
    Ok(json!({ "created": created }))
}

/// `handleCreateNewTemplate`: with unsaved changes (Draft or Modified) the user is asked first;
/// an agent's call waits for the answer (D15).
pub fn create_new_template_request(
    store: &mut Store,
    services: &mut Services,
    reply: Option<AgentReply>,
) {
    if !save_status(store).shows() {
        confirm_create_new_template(store, services);
        if let Some(reply) = reply {
            reply.send(new_template_answer(true));
        }
        return;
    }
    store
        .unsaved_confirm
        .request_confirm(UnsavedContinuation::NewTemplate { reply });
}

/// The unsaved-changes dialog's answer. Save runs the save first (in the UI, its dialog); a
/// cancelled save abandons the new template.
pub fn answer_unsaved_confirm(store: &mut Store, services: &mut Services, answer: UnsavedAnswer) {
    let Some(UnsavedContinuation::NewTemplate { reply }) = store.unsaved_confirm.take() else {
        return;
    };
    match answer {
        UnsavedAnswer::Cancel => {
            if let Some(reply) = reply {
                reply.send(new_template_answer(false));
            }
        }
        UnsavedAnswer::Discard => {
            confirm_create_new_template(store, services);
            if let Some(reply) = reply {
                reply.send(new_template_answer(true));
            }
        }
        UnsavedAnswer::Save => {
            let from_agent = reply.is_some();
            let then: OnDone = Box::new(move |store, services, saved| {
                let created = saved.is_ok();
                if created {
                    confirm_create_new_template(store, services);
                }
                if let Some(reply) = reply {
                    reply.send(match saved {
                        Ok(()) => new_template_answer(true),
                        Err(error) => Err(error),
                    });
                }
            });
            if !from_agent {
                return open_save_template_dialog(store, services, then);
            }
            // An agent cannot answer a file dialog: it saves to the loaded template's file (D15).
            match store.template.loaded_template_source.clone() {
                Some(TemplateSource::File { path }) => save_template(store, services, path, then),
                _ => then(store, services, Err(AGENT_SAVE_NEEDS_PATH.into())),
            }
        }
    }
}

/// What an agent reads when the dialog's Save cannot pick a file for it.
pub const AGENT_SAVE_NEEDS_PATH: &str =
    "The template has never been saved to a file, so it cannot be saved without a path. Call template_save with a path, then template_new again.";

/// `handleSaveTemplate`'s UI part: the native save dialog, then the save. A cancelled dialog ends
/// with an error the caller ignores (`saved === undefined`).
pub fn open_save_template_dialog(store: &mut Store, services: &mut Services, then: OnDone) {
    let filename = suggested_template_filename(store.template.loaded_template_source.as_ref());
    let user_data_root = services
        .paths
        .user_templates_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let default_path =
        match ovrley_services::file_ops::default_template_save_path(&user_data_root, &filename) {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(error) => return fail_save(store, services, error, then),
        };
    let shared = std::rc::Rc::new(std::cell::RefCell::new(Some(then)));
    let fallback = std::rc::Rc::clone(&shared);
    let opened = save_single_path(
        services,
        &default_path,
        "json",
        Some("OVRLEY Template"),
        None,
        Box::new(move |store, services, selected| {
            let then = shared.borrow_mut().take().expect("the dialog answers once");
            match selected {
                Some(path) => save_template(store, services, path, then),
                None => then(store, services, Err(SAVE_CANCELLED.into())),
            }
        }),
    );
    if let Err(error) = opened {
        if let Some(then) = fallback.borrow_mut().take() {
            fail_save(store, services, error, then);
        }
    }
}

/// The error a cancelled save dialog ends with.
pub const SAVE_CANCELLED: &str = "The save was cancelled.";

fn fail_save(store: &mut Store, services: &mut Services, error: String, then: OnDone) {
    log::error!("Failed to save template: {error}");
    store
        .media
        .set_error_message(format!("Failed to save template: {error}"));
    then(store, services, Err(error));
}

/// `handleSaveTemplate`: writes the template file through the validating writer, then refetches
/// the list, makes the file the source, records the saved state and remembers the file.
pub fn save_template(store: &mut Store, services: &mut Services, path: String, then: OnDone) {
    let name = suggested_template_filename(store.template.loaded_template_source.as_ref());
    let name = name.strip_suffix(".json").unwrap_or(&name).to_string();
    let payload = match template_file_payload(
        &store.editor.config,
        &store.template.global_defaults,
        Some(&name),
        chrono::Utc::now(),
    ) {
        Ok(payload) => payload,
        Err(error) => return fail_save(store, services, error.to_string(), then),
    };
    let saved_state = match create_durable_template_state(
        &store.editor.config,
        Some(&store.template.global_defaults),
    ) {
        Ok(state) => state,
        Err(error) => return fail_save(store, services, error.to_string(), then),
    };
    let job = store.template_manager.park(then);
    let write_path = path.clone();
    services.workers.spawn("template-save", move || {
        let contents =
            serde_json::to_string_pretty(&payload).expect("a template payload serializes");
        let written = ovrley_services::file_ops::write_template_file(
            std::path::Path::new(&write_path),
            &contents,
        );
        Box::new(move |store: &mut Store, services: &mut Services| {
            let then = store.template_manager.resume(job);
            match written {
                Ok(()) => {
                    store
                        .template
                        .set_loaded_template_source(Some(TemplateSource::File {
                            path: path.clone(),
                        }));
                    store
                        .template
                        .set_last_saved_template_state(Some(saved_state));
                    services
                        .preferences
                        .set(LAST_TEMPLATE_KEY, json!({"source": "file", "path": path}));
                    fetch_templates(
                        store,
                        services,
                        Some(Box::new(move |store, services| {
                            then(store, services, Ok(()))
                        })),
                    );
                }
                Err(error) => fail_save(store, services, error, then),
            }
        })
    });
}

/// `restoreLastLoadedTemplate`: the template the last session used. A bundled template that
/// fails to load, or a file that fails, starts a new template and forgets the preference. A value
/// of neither shape is ignored (Q5: a bad preference never stops the app from starting).
pub fn restore_last_loaded_template(
    store: &mut Store,
    services: &mut Services,
    done: Box<dyn FnOnce(&mut Store, &mut Services)>,
) {
    let saved = services.preferences.get(LAST_TEMPLATE_KEY).cloned();
    let source = saved
        .as_ref()
        .and_then(|value| value.get("source"))
        .and_then(Value::as_str);
    let text = |key: &str| {
        saved
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    match (source, text("filename"), text("path")) {
        (Some("backend"), Some(filename), _) => load_template(
            store,
            services,
            filename,
            Box::new(move |store, services, result| {
                if result.is_err() {
                    start_over(store, services);
                }
                done(store, services);
            }),
        ),
        (Some("file"), _, Some(path)) => import_template_file(
            store,
            services,
            path,
            "Failed to load template",
            Box::new(move |store, services, result| {
                if result.is_err() {
                    start_over(store, services);
                }
                done(store, services);
            }),
        ),
        _ => done(store, services),
    }
}

fn start_over(store: &mut Store, services: &mut Services) {
    store.history.set_mode(HistoryMode::Reset);
    create_new_template(&mut store.editor, &mut store.template);
    services.preferences.delete(LAST_TEMPLATE_KEY);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::services::fakes::{fake_services, Fakes};
    use crate::store::Dispatcher;

    fn dispatcher(preferences: Value) -> (Dispatcher, Fakes) {
        let (parts, fakes) = fake_services(preferences);
        (Dispatcher::new(Store::default(), parts), fakes)
    }

    /// Runs posted worker results until `done` holds.
    fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
        for _ in 0..400 {
            fakes.ui_post.drain(dispatcher);
            if dispatcher.with_store(&done) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out");
    }

    fn descriptor(id: &str, width: u32, height: u32, kind: TemplateKind) -> TemplateDescriptor {
        TemplateDescriptor {
            id: id.into(),
            name: id.trim_end_matches(".json").to_uppercase(),
            kind,
            width,
            height,
            path: None,
        }
    }

    #[test]
    fn filenames_sanitize_as_react_does() {
        assert_eq!(
            sanitize_template_filename("My Template!.json"),
            "My_Template.json"
        );
        assert_eq!(sanitize_template_filename("  "), "ovrley_template.json");
        assert_eq!(sanitize_template_filename("a  b__c"), "a_b_c.json");
        assert_eq!(sanitize_template_filename("_x_"), "x.json");
        assert_eq!(suggested_template_filename(None), "ovrley_template.json");
        assert_eq!(
            suggested_template_filename(Some(&TemplateSource::Bundled {
                template_id: "user:ride.json".into()
            })),
            "ride.json"
        );
        assert_eq!(
            suggested_template_filename(Some(&TemplateSource::File {
                path: "C:\\t\\race day.json".into()
            })),
            "race_day.json"
        );
    }

    #[test]
    fn groups_sort_by_resolution_then_name() {
        let groups = template_groups(
            &[
                descriptor("b.json", 1920, 1080, TemplateKind::BuiltIn),
                descriptor("x.json", 0, 0, TemplateKind::User),
                descriptor("a.json", 1920, 1080, TemplateKind::User),
                descriptor("c.json", 3840, 2160, TemplateKind::BuiltIn),
            ],
            "Unknown Resolution",
        );
        assert_eq!(
            groups
                .iter()
                .map(|group| group.label.as_str())
                .collect::<Vec<_>>(),
            ["3840 x 2160", "1920 x 1080", "Unknown Resolution"]
        );
        assert_eq!(groups[1].templates[0].id, "a.json");
    }

    #[test]
    fn a_bundled_template_loads_replaces_the_history_and_is_remembered() {
        let (dispatcher, fakes) = dispatcher(json!({}));
        dispatcher.dispatch(|store, _| {
            store
                .editor
                .set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "x"}]})).unwrap())
        });
        let loaded = Rc::new(std::cell::Cell::new(false));
        let flag = Rc::clone(&loaded);
        dispatcher.dispatch(move |store, services| {
            load_template(
                store,
                services,
                "breeze-blue.json".into(),
                Box::new(move |_, _, result| flag.set(result.is_ok())),
            );
        });
        let flag = Rc::clone(&loaded);
        settle(&dispatcher, &fakes, move |_| flag.get());
        dispatcher.with_store(|store| {
            assert_eq!(
                store.template.loaded_template_source,
                Some(TemplateSource::Bundled {
                    template_id: "breeze-blue.json".into()
                })
            );
            assert!(!store.history.can_undo());
            assert_eq!(store.editor.editor_document_revision, 1);
            assert!(!store.media.is_processing);
            assert_eq!(save_status(store), SaveStatus::Saved);
            assert!(
                !store.template.templates.is_empty(),
                "the list was fetched for the unknown id"
            );
        });
        assert_eq!(
            fakes.preferences.saves.borrow().last().unwrap()[LAST_TEMPLATE_KEY],
            json!({"source": "backend", "filename": "breeze-blue.json"})
        );
    }

    #[test]
    fn an_unknown_template_fails_with_the_load_message() {
        let (dispatcher, fakes) = dispatcher(json!({}));
        let failed = Rc::new(std::cell::Cell::new(false));
        let flag = Rc::clone(&failed);
        dispatcher.dispatch(move |store, services| {
            load_template(
                store,
                services,
                "nope.json".into(),
                Box::new(move |_, _, result| flag.set(result.is_err())),
            )
        });
        let flag = Rc::clone(&failed);
        settle(&dispatcher, &fakes, move |_| flag.get());
        assert_eq!(
            dispatcher
                .with_store(|store| store.media.error_message.clone())
                .as_deref(),
            Some("Failed to load template: Unknown template: nope.json")
        );
    }

    #[test]
    fn saving_writes_the_file_and_marks_it_saved() {
        let directory = std::env::temp_dir().join(format!("ovrley-save-{}", std::process::id()));
        let path = directory.join("ride.json").to_string_lossy().into_owned();
        let (dispatcher, fakes) = dispatcher(json!({}));
        let saved = Rc::new(std::cell::Cell::new(false));
        let flag = Rc::clone(&saved);
        let target = path.clone();
        dispatcher.dispatch(move |store, services| {
            assert_eq!(save_status(store), SaveStatus::Draft);
            save_template(
                store,
                services,
                target,
                Box::new(move |_, _, result| flag.set(result.is_ok())),
            );
        });
        let flag = Rc::clone(&saved);
        settle(&dispatcher, &fakes, move |_| flag.get());
        dispatcher.with_store(|store| {
            assert_eq!(save_status(store), SaveStatus::Saved);
            assert_eq!(
                store.template.loaded_template_source,
                Some(TemplateSource::File { path: path.clone() })
            );
        });
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["format"], "ovrley-template");
        assert!(
            load_template_document(&written).is_ok(),
            "the saved file loads"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_new_template_asks_first_with_unsaved_changes() {
        let (dispatcher, _) = dispatcher(json!({}));
        let answers = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = Arc::clone(&answers);
        dispatcher.dispatch(move |store, services| {
            store
                .editor
                .set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "x"}]})).unwrap());
            create_new_template_request(
                store,
                services,
                Some(AgentReply::new(move |result| {
                    log.lock().unwrap().push(result)
                })),
            );
        });
        assert!(
            dispatcher.with_store(|store| store.unsaved_confirm.is_open()),
            "a draft asks"
        );
        dispatcher.dispatch(|store, services| {
            answer_unsaved_confirm(store, services, UnsavedAnswer::Discard)
        });
        dispatcher.with_store(|store| {
            assert!(!store.unsaved_confirm.is_open());
            assert!(store.editor.config.labels.is_empty());
        });
        assert_eq!(*answers.lock().unwrap(), [Ok(json!({"created": true}))]);
    }

    #[test]
    fn an_agents_save_from_the_dialog_needs_a_template_file() {
        let (dispatcher, _) = dispatcher(json!({}));
        let answers = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = Arc::clone(&answers);
        dispatcher.dispatch(move |store, services| {
            store
                .editor
                .set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "x"}]})).unwrap());
            create_new_template_request(
                store,
                services,
                Some(AgentReply::new(move |result| {
                    log.lock().unwrap().push(result)
                })),
            );
        });
        dispatcher.dispatch(|store, services| {
            answer_unsaved_confirm(store, services, UnsavedAnswer::Save)
        });
        assert_eq!(
            *answers.lock().unwrap(),
            [Err(AGENT_SAVE_NEEDS_PATH.to_string())]
        );
        assert_eq!(
            dispatcher.with_store(|store| store.editor.config.labels.len()),
            1,
            "nothing was discarded"
        );
    }

    #[test]
    fn cancel_keeps_the_document_and_answers_not_created() {
        let (dispatcher, fakes) = dispatcher(json!({}));
        let answers = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = Arc::clone(&answers);
        dispatcher.dispatch(move |store, services| {
            store
                .editor
                .set_config(OverlayConfig::from_value(&json!({"labels": [{"id": "x"}]})).unwrap());
            create_new_template_request(
                store,
                services,
                Some(AgentReply::new(move |result| {
                    log.lock().unwrap().push(result)
                })),
            );
        });
        dispatcher.dispatch(|store, services| {
            answer_unsaved_confirm(store, services, UnsavedAnswer::Cancel)
        });
        assert_eq!(*answers.lock().unwrap(), [Ok(json!({"created": false}))]);
        assert!(fakes.dialogs.requests().is_empty());
    }

    #[test]
    fn restore_ignores_a_malformed_preference_and_starts_over_after_a_failed_load() {
        let (dispatcher, fakes) = dispatcher(json!({"last-template": {"source": "elsewhere"}}));
        let done = Rc::new(std::cell::Cell::new(false));
        let flag = Rc::clone(&done);
        dispatcher.dispatch(move |store, services| {
            restore_last_loaded_template(store, services, Box::new(move |_, _| flag.set(true)))
        });
        assert!(done.get(), "a malformed value is ignored at once");

        let (dispatcher, fakes2) = dispatcher_with(
            json!({"last-template": {"source": "file", "path": "/nowhere/missing.json"}}),
        );
        let done = Rc::new(std::cell::Cell::new(false));
        let flag = Rc::clone(&done);
        dispatcher.dispatch(move |store, services| {
            restore_last_loaded_template(store, services, Box::new(move |_, _| flag.set(true)))
        });
        let flag = Rc::clone(&done);
        settle(&dispatcher, &fakes2, move |_| flag.get());
        assert!(dispatcher
            .with_store(|store| store.media.error_message.clone())
            .unwrap()
            .starts_with("Failed to load template: "));
        assert!(fakes2
            .preferences
            .saves
            .borrow()
            .last()
            .unwrap()
            .get(LAST_TEMPLATE_KEY)
            .is_none());
        drop(fakes);
    }

    fn dispatcher_with(preferences: Value) -> (Dispatcher, Fakes) {
        dispatcher(preferences)
    }
}
