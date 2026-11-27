//! The store and its update loop.
//!
//! [`Store`] holds everything the UI shows. [`Dispatcher`] owns the store, the services and a
//! queue of actions. One dispatch runs one action, then the reconcile steps in their fixed order
//! (`reconcile`), then the history step, then the UI hook that `ovrley_ui` installs, which runs
//! the projections.
//!
//! A dispatch requested while another runs (for example by a Slint `changed` handler a projection
//! fired) is queued and runs right after, projections included. No `RefCell` borrow is ever taken
//! twice.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use editor::EditorState;
use media::MediaState;
use ovrley_core::encode::settings::RenderSettings;
use services::{ServiceParts, Services};
use template::TemplateState;
use videos::VideoImportState;
use crate::features::{canvas, toolbar};
use crate::features::canvas::drafts::WidgetDraftState;
use crate::features::history::{EditorHistory, UndoableState};
use crate::features::shell::{AgentSessionState, AppBootstrapState, EditorShellState, TrafficLightInsetState, UnsavedChangesConfirmState, WindowCloseGuardState, WindowFileDropState};
use crate::features::toolbar::{DrawerPreferenceState, LayoutState};
use crate::i18n::I18nState;
use crate::preferences::PreferencesStore;

pub mod agent;
pub mod services;
pub mod editor;
pub mod media;
pub mod template;
pub mod videos;

/// Everything the UI shows.
#[derive(Debug, Default)]
pub struct Store {
    pub layout: LayoutState,
    pub media: MediaState,
    pub editor: EditorState,
    pub template: TemplateState,
    pub video_import: VideoImportState,
    /// The render frame rate and widget update rate.
    pub render_settings: RenderSettings,
    /// The manual sync's durable state, which projects save.
    pub manual_video_sync: ovrley_core::media::sync::ManualVideoSync,
    /// The activity's events, `None` before the first detection.
    pub manual_video_sync_detection: Option<std::sync::Arc<crate::features::sync::detection::Detection>>,
    /// The manual sync's candidate results.
    pub manual_video_sync_results: crate::features::sync::manual::CandidateResults,
    /// The separate renders' output folder, which projects save, and the running batch.
    pub batch_render: crate::features::render::session::BatchRenderState,
    /// The timeline filmstrips.
    pub filmstrips: crate::features::player::filmstrips::FilmstripsState,
    /// The import review.
    pub video_import_review: crate::features::import::VideoImportReviewState,
    /// The seconds fields of the import dialog, the video drawer and the video panel.
    pub seconds_fields: crate::controls::seconds::SecondsFieldsState,
    /// The open `TransitionSelect` popover.
    pub transition_select: Option<crate::features::transitions::TransitionSelectState>,
    /// The embedded-telemetry extraction queue.
    pub telemetry_queue: crate::features::import::VideoTelemetryQueue,
    /// The filmstrip decode queue.
    pub filmstrip_queue: crate::features::player::filmstrips::FilmstripQueue,
    /// The transitions' rendered blends and their queue.
    pub transition_proxies: crate::features::player::preview::TransitionProxies,
    /// Sync Doctor's workspace.
    pub video_sync: crate::features::sync::VideoSyncWorkspace,
    /// The player's state and cached timeline.
    pub player: crate::features::player::PlayerState,
    /// The editor history.
    pub history: EditorHistory,
    /// The render dialog.
    pub render_dialog: crate::features::render::RenderDialogState,
    /// The project lifecycle and document status.
    pub projects: crate::features::projects::ProjectLifecycleState,
    /// The unsaved-changes question.
    pub unsaved_confirm: UnsavedChangesConfirmState,
    /// The live widget drafts.
    pub drafts: WidgetDraftState,
    /// The widgets tab and the font catalog.
    pub widgets: crate::features::widgets::WidgetsState,
    /// The theme colours widget fallbacks use; `ovrley_ui` reads them from the Slint theme before
    /// the first dispatch.
    pub theme: Option<crate::store::template::ThemeColors>,
    /// The viewport, and later the selection and gestures.
    pub canvas: crate::features::canvas::CanvasState,
    /// The widget preview host.
    pub preview: crate::features::widgets::preview::PreviewHost,
    /// The scene settings' anchors and the pending label-font edit.
    pub scene: crate::features::scene::SceneState,
    /// The maps' styles, adapter and frame thread.
    pub maps: crate::features::maps::MapsState,
    /// Settings → the disk cache rows.
    pub disk_caches: crate::features::caches::DiskCachesState,
    /// The template listing, save status and the cached durable state.
    pub templates: crate::features::templates::TemplatesState,
    pub i18n: I18nState,
    /// The last drawer preference seen.
    pub drawer_preference: DrawerPreferenceState,
    pub bootstrap: AppBootstrapState,
    pub editor_shell: EditorShellState,
    pub close_guard: WindowCloseGuardState,
    pub traffic_light_inset: TrafficLightInsetState,
    pub file_drop: WindowFileDropState,
    pub agent_session: AgentSessionState,
    /// The update prompt.
    pub app_update: crate::features::shell::AppUpdateState,
}

impl Store {
    /// Reads the preferences the shell needs before its first frame: the language (`lng`,
    /// strict), whether agents may edit (strict), the drawer preference (repaired to the default
    /// when malformed) and the unit system (strict).
    pub fn hydrate(preferences: &PreferencesStore) -> Result<Self, String> {
        let mut store = Self::default();
        crate::i18n::hydrate_language_preference(&mut store, preferences)?;
        crate::features::shell::hydrate_agent_preference(&mut store, preferences)?;
        crate::features::toolbar::hydrate_drawer_preference(&mut store, preferences);
        crate::store::editor::hydrate_unit_system_preference(&mut store, preferences)?;
        Ok(store)
    }
}

/// One action: runs on the UI thread with the store and the services.
pub type Action = Box<dyn FnOnce(&mut Store, &mut Services)>;

/// An action built on another thread (a worker result, an MCP call) and posted to the UI thread.
pub type SendAction = Box<dyn FnOnce(&mut Store, &mut Services) + Send>;

/// Runs after every action and its reconcile steps, with the store read-only. `ovrley_ui` runs the
/// projections here.
pub type UiHook = Rc<dyn Fn(&Store)>;

struct State {
    store: Store,
    services: Services,
}

struct Shared {
    state: RefCell<State>,
    queue: RefCell<VecDeque<Action>>,
    running: Cell<bool>,
    ui_hook: RefCell<Option<UiHook>>,
    #[cfg(feature = "test-util")]
    stats: Cell<DispatchStats>,
}

/// What the update loop did, for the headless benchmarks: the dispatches and the time their
/// actions, reconcile steps and history step took, and the projections and theirs.
#[cfg(feature = "test-util")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DispatchStats {
    pub dispatches: u64,
    pub update: std::time::Duration,
    pub projections: u64,
    pub projection: std::time::Duration,
}

/// Owns the store and the services, and runs actions one at a time.
#[derive(Clone)]
pub struct Dispatcher(Rc<Shared>);

/// Dispatches into a [`Dispatcher`] without keeping it alive. Held by services whose replies come
/// back on the UI thread.
#[derive(Clone)]
pub struct DispatchHandle(Weak<Shared>);

impl DispatchHandle {
    /// Queues an action. It is dropped if the dispatcher no longer exists (the app is closing).
    pub fn dispatch(&self, action: impl FnOnce(&mut Store, &mut Services) + 'static) {
        if let Some(shared) = self.0.upgrade() {
            Dispatcher(shared).dispatch(action);
        }
    }

    /// [`Dispatcher::dispatch_with_result`], if the dispatcher still exists.
    pub fn dispatch_with_result<R: 'static>(&self, action: impl FnOnce(&mut Store, &mut Services) -> R + 'static) -> Option<R> {
        self.0.upgrade().and_then(|shared| Dispatcher(shared).dispatch_with_result(action))
    }

    /// Writes pending preferences out, if the dispatcher still exists.
    pub fn flush_preferences(&self) {
        if let Some(shared) = self.0.upgrade() {
            Dispatcher(shared).flush_preferences();
        }
    }

    /// Reads the store, if the dispatcher still exists and no action runs (a timer outside the
    /// dispatch loop checking whether it has work).
    pub fn with_store<R>(&self, read: impl FnOnce(&Store) -> R) -> Option<R> {
        let shared = self.0.upgrade()?;
        if shared.running.get() {
            return None;
        }
        let state = shared.state.try_borrow().ok()?;
        Some(read(&state.store))
    }
}

impl Dispatcher {
    pub fn new(store: Store, services: ServiceParts) -> Self {
        Self(Rc::new_cyclic(|weak| Shared {
            state: RefCell::new(State {
                store,
                services: services.into_services(DispatchHandle(weak.clone())),
            }),
            queue: RefCell::new(VecDeque::new()),
            running: Cell::new(false),
            ui_hook: RefCell::new(None),
            #[cfg(feature = "test-util")]
            stats: Cell::default(),
        }))
    }

    /// The update loop's counts and times since the last call, which resets them.
    #[cfg(feature = "test-util")]
    pub fn take_stats(&self) -> DispatchStats {
        self.0.stats.take()
    }

    pub fn handle(&self) -> DispatchHandle {
        DispatchHandle(Rc::downgrade(&self.0))
    }

    /// Installs the hook that runs after every dispatch.
    pub fn set_ui_hook(&self, hook: UiHook) {
        *self.0.ui_hook.borrow_mut() = Some(hook);
    }

    /// Runs `action`, its reconcile steps and the projections before returning, unless another
    /// dispatch is running: then it is queued and runs right after that one.
    pub fn dispatch(&self, action: impl FnOnce(&mut Store, &mut Services) + 'static) {
        self.0.queue.borrow_mut().push_back(Box::new(action));
        self.drain();
    }

    /// Runs `action` now and returns its result, for callers that answer from it (a key handler
    /// accepting or rejecting a key). Returns `None`, and runs nothing, when called from inside a
    /// running dispatch, which can only queue.
    pub fn dispatch_with_result<R: 'static>(&self, action: impl FnOnce(&mut Store, &mut Services) -> R + 'static) -> Option<R> {
        if self.0.running.get() {
            log::error!("a dispatch that needs its result was requested inside a running dispatch");
            return None;
        }
        let result = Rc::new(RefCell::new(None));
        let slot = result.clone();
        self.dispatch(move |store, services| *slot.borrow_mut() = Some(action(store, services)));
        let value = result.borrow_mut().take();
        value
    }

    /// Reads the store. Must not be called while an action runs.
    pub fn with_store<R>(&self, read: impl FnOnce(&Store) -> R) -> R {
        read(&self.0.state.borrow().store)
    }

    /// Runs the projections again without an action, for example after a live-preview reload
    /// rebuilt the UI.
    pub fn reproject(&self) {
        if self.0.running.get() {
            return;
        }
        self.0.running.set(true);
        let _running = RunningGuard(&self.0.running);
        self.project();
    }

    fn drain(&self) {
        if self.0.running.get() {
            return;
        }
        self.0.running.set(true);
        let _running = RunningGuard(&self.0.running);
        loop {
            let Some(action) = self.0.queue.borrow_mut().pop_front() else {
                break;
            };
            #[cfg(feature = "test-util")]
            let started = std::time::Instant::now();
            {
                let mut state = self.0.state.borrow_mut();
                let State { store, services } = &mut *state;
                let before = UndoableState::of(store);
                action(store, services);
                reconcile(store, services);
                // The history step: after the reconcile steps, before the projections.
                crate::features::history::record(store, before);
            }
            #[cfg(feature = "test-util")]
            self.count(|stats| {
                stats.dispatches += 1;
                stats.update += started.elapsed();
            });
            self.project();
        }
    }

    fn project(&self) {
        let hook = self.0.ui_hook.borrow().clone();
        if let Some(hook) = hook {
            #[cfg(feature = "test-util")]
            let started = std::time::Instant::now();
            hook(&self.0.state.borrow().store);
            #[cfg(feature = "test-util")]
            self.count(|stats| {
                stats.projections += 1;
                stats.projection += started.elapsed();
            });
        }
    }

    #[cfg(feature = "test-util")]
    fn count(&self, update: impl FnOnce(&mut DispatchStats)) {
        let mut stats = self.0.stats.get();
        update(&mut stats);
        self.0.stats.set(stats);
    }

    /// Writes pending preferences out, before the app exits.
    pub fn flush_preferences(&self) {
        self.0.state.borrow_mut().services.preferences.flush();
    }
}

/// Clears the running flag when a dispatch ends, also by a panic.
struct RunningGuard<'a>(&'a Cell<bool>);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

fn reconcile(store: &mut Store, services: &mut Services) {
    // 1. The drawer preference: persist `{pinned, activeTool}` when it changed.
    toolbar::persist_drawer_preference(store, services);
    // 2. The widget drafts: a new document clears the drafts, and a new document or selection
    //    ends the gesture's patches.
    canvas::interaction::reconcile_gesture(store);
    canvas::drafts::reconcile_widget_drafts(store);
    //    The map view mode ends when its map is gone or deselected, committing its patch.
    crate::features::maps::reconcile_map_view(store);
    // 3. The widgets tab's open item holds while a marquee runs.
    crate::features::widgets::reconcile_open_widget(store);
    //    The open label's font capabilities, for its weight and italic controls.
    crate::features::widgets::reconcile_label_typography(store, services);
    //    A label-font edit of the global defaults applies once its family is known.
    crate::features::scene::apply_pending_font_edit(store);
    // 4. The durable template state behind the save status, cached by pointers.
    crate::features::templates::reconcile_durable_state(store);
    //    The project status, from the durable state and the project-owned state.
    crate::features::projects::reconcile_project_status(store);
    // 5. The player: the cached timeline, the playback rules and the viewport rules.
    crate::features::player::reconcile_player(store);
    //    The filmstrips of the clips and the zoom windows the view wants.
    crate::features::player::filmstrips::reconcile_filmstrips(store, services);
    //    The blends of the program's transitions.
    crate::features::player::preview::reconcile_transition_proxies(store, services);
    //    Sync Doctor: detection and the reruns of a search.
    crate::features::sync::reconcile_video_sync(store, services);
    // 6. The preview: marked dirty when one of its inputs changed; `ovrley_ui` runs the frame.
    crate::features::widgets::preview::reconcile_widget_preview(store);
    // 7. The render dialog: its codec and update-rate rules, the inspection and the export review.
    crate::features::render::reconcile_render_dialog(store, services);
}

#[cfg(test)]
mod tests {
    use super::services::fakes::fake_services;
    use super::*;
    use crate::features::toolbar::LeftDrawerTool;
    use serde_json::json;

    fn dispatcher(preferences: serde_json::Value) -> (Dispatcher, services::fakes::Fakes) {
        let (parts, fakes) = fake_services(preferences);
        let store = Store::hydrate(&parts.preferences).unwrap();
        (Dispatcher::new(store, parts), fakes)
    }

    #[test]
    fn exposes_the_canonical_shared_drawer_state_and_actions() {
        let store = Store::default();
        assert_eq!(store.layout.active_left_drawer_tool, LeftDrawerTool::Widgets);
        assert!(!store.layout.left_drawer_pinned);
        assert!(!store.layout.left_drawer_visible);
    }

    #[test]
    fn a_dispatch_runs_its_projection_before_returning() {
        let (dispatcher, _) = dispatcher(json!({}));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        dispatcher.set_ui_hook(Rc::new(move |store: &Store| log.borrow_mut().push(store.layout.left_drawer_visible)));

        dispatcher.dispatch(|store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Video));

        assert_eq!(*seen.borrow(), vec![true]);
    }

    #[test]
    fn a_reentrant_dispatch_is_queued_and_runs_after_the_projection() {
        let (dispatcher, _) = dispatcher(json!({}));
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = events.clone();
        let handle = dispatcher.handle();
        dispatcher.set_ui_hook(Rc::new(move |store: &Store| {
            log.borrow_mut().push(format!("project visible={}", store.layout.left_drawer_visible));
            if store.layout.left_drawer_visible {
                let queued_log = log.clone();
                // As a Slint `changed` handler would, while the projection runs.
                handle.dispatch(move |store, _| {
                    queued_log.borrow_mut().push("queued action".into());
                    store.layout.dismiss_left_drawer_overlay();
                });
                log.borrow_mut().push("projection done".into());
            }
        }));

        dispatcher.dispatch(|store, _| store.layout.select_left_drawer_tool(LeftDrawerTool::Video));

        assert_eq!(
            *events.borrow(),
            ["project visible=true", "projection done", "queued action", "project visible=false"]
        );
    }

    #[test]
    fn dispatch_with_result_answers_synchronously_and_not_from_inside_a_dispatch() {
        let (dispatcher, _) = dispatcher(json!({}));
        assert_eq!(dispatcher.dispatch_with_result(|store, _| store.layout.active_left_drawer_tool), Some(LeftDrawerTool::Widgets));

        let inner = Rc::new(RefCell::new(None));
        let slot = inner.clone();
        let nested = dispatcher.clone();
        dispatcher.set_ui_hook(Rc::new(move |_: &Store| {
            if slot.borrow().is_none() {
                *slot.borrow_mut() = Some(nested.dispatch_with_result(|_, _| 1));
            }
        }));
        dispatcher.dispatch(|_, _| {});
        assert_eq!(*inner.borrow(), Some(None));
    }

    #[test]
    fn reconcile_runs_after_the_action_and_before_the_projection() {
        let (dispatcher, fakes) = dispatcher(json!({}));
        let saves = fakes.preferences.saves.clone();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = seen.clone();
        dispatcher.set_ui_hook(Rc::new(move |_: &Store| log.borrow_mut().push(saves.borrow().len())));

        dispatcher.dispatch(|store, _| store.layout.set_left_drawer_pinned(true));

        assert_eq!(*seen.borrow(), vec![1], "the drawer preference was saved before the projection ran");
    }

    #[test]
    fn worker_results_come_back_as_actions() {
        let (dispatcher, fakes) = dispatcher(json!({}));
        dispatcher.dispatch(|_, services| {
            services.workers.spawn("test-worker", || {
                let message = String::from("worker failed");
                Box::new(move |store: &mut Store, _: &mut Services| store.media.set_error_message(message))
            });
        });
        for _ in 0..200 {
            if !fakes.ui_post.posted.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        fakes.ui_post.drain(&dispatcher);
        assert_eq!(dispatcher.with_store(|store| store.media.error_message.clone()), Some("worker failed".into()));
    }
}
