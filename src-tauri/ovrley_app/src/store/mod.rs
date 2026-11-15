//! The store and its update loop (parent plan §2; phase 2 D1, D2).
//!
//! [`Store`] holds everything the UI shows: the Zustand slices, and the state React keeps in hooks
//! beyond one render. [`Dispatcher`] owns the store, the services and a queue of actions. One
//! dispatch runs one action, then the reconcile steps in their fixed order (`reconcile.rs`), then
//! the UI hook that `ovrley_ui` installs, which runs the projections.
//!
//! A dispatch requested while another runs (for example by a Slint `changed` handler a projection
//! fired) is queued and runs right after, projections included. No `RefCell` borrow is ever taken
//! twice.

pub mod agent;
mod reconcile;
pub mod services;
pub mod slices;
pub mod sync;

use crate::features::app_shell::{
    AgentSessionState, AppBootstrapState, EditorShellState, TrafficLightInsetState, UnsavedChangesConfirmState, WindowCloseGuardState, WindowFileDropState,
};
use crate::features::undo_redo::{EditorHistory, UndoableState};
use crate::features::overlay_editor::drafts::WidgetDraftState;
use crate::features::toolbar::DrawerPreferenceState;
use crate::i18n::I18nState;
use crate::lib::preferences_store::PreferencesStore;
use services::{ServiceParts, Services};
use slices::editor::EditorSlice;
use slices::layout::LayoutSlice;
use slices::media::MediaSlice;
use slices::template::TemplateSlice;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

/// Everything the UI shows.
#[derive(Debug, Default)]
pub struct Store {
    pub layout: LayoutSlice,
    pub media: MediaSlice,
    pub editor: EditorSlice,
    pub template: TemplateSlice,
    /// The editor history (D6).
    pub history: EditorHistory,
    /// `useUnsavedChangesConfirm` (D15).
    pub unsaved_confirm: UnsavedChangesConfirmState,
    /// `useWidgetDraftState` (D7).
    pub drafts: WidgetDraftState,
    /// The theme colours widget fallbacks use; `ovrley_ui` reads them from the Slint theme before
    /// the first dispatch.
    pub theme: Option<crate::lib::template::ThemeColors>,
    /// `useOverlayEditorState`'s state: the viewport, and later the selection and gestures.
    pub overlay_editor: crate::features::overlay_editor::OverlayEditorState,
    /// The native preview host (D8).
    pub preview: crate::features::widget_preview::PreviewHost,
    /// `useTemplateManagement`'s state and the cached durable state.
    pub template_manager: crate::features::template_manager::TemplateManagerState,
    pub i18n: I18nState,
    /// `useDrawerPreference`'s last observed preference.
    pub drawer_preference: DrawerPreferenceState,
    pub bootstrap: AppBootstrapState,
    pub editor_shell: EditorShellState,
    pub close_guard: WindowCloseGuardState,
    pub traffic_light_inset: TrafficLightInsetState,
    pub file_drop: WindowFileDropState,
    pub agent_session: AgentSessionState,
}

impl Store {
    /// Reads the preferences the shell needs before its first frame, as `main.jsx` and the startup
    /// hooks do: the language (`lng`, strict), whether agents may edit (strict), and the drawer
    /// preference (normalized, Q5).
    pub fn hydrate(preferences: &PreferencesStore) -> Result<Self, String> {
        let mut store = Self::default();
        crate::i18n::hydrate_language_preference(&mut store, preferences)?;
        crate::features::app_shell::hydrate_agent_preference(&mut store, preferences)?;
        crate::features::toolbar::hydrate_drawer_preference(&mut store, preferences);
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
        }))
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
    /// rebuilt the UI (D17).
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
            {
                let mut state = self.0.state.borrow_mut();
                let State { store, services } = &mut *state;
                let before = UndoableState::of(store);
                action(store, services);
                reconcile::run(store, services);
                // The history step (D6): after the reconcile steps, before the projections.
                crate::features::undo_redo::record(store, before);
            }
            self.project();
        }
    }

    fn project(&self) {
        let hook = self.0.ui_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(&self.0.state.borrow().store);
        }
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

#[cfg(test)]
mod tests {
    use super::services::fakes::fake_services;
    use super::*;
    use crate::store::slices::layout::LeftDrawerTool;
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
