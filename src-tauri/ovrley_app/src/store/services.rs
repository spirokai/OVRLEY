//! The side effects an action may ask for (plan D3).
//!
//! Actions stay synchronous. What they cannot do in place, they ask of a service held by the
//! dispatcher. `ovrley_ui` implements the traits with `rfd`, `slint::invoke_from_event_loop`,
//! `arboard` and the winit window; tests use the in-memory fakes at the end of this module, which
//! record every call.

use super::{DispatchHandle, SendAction};
use crate::lib::preferences_store::PreferencesStore;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// A file type a native file dialog offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl DialogFilter {
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            extensions: extensions
                .iter()
                .map(|extension| extension.to_string())
                .collect(),
        }
    }
}

/// A native file dialog to show. Paths are strings, as the React port handles them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogRequest {
    OpenFile {
        filters: Vec<DialogFilter>,
        default_path: Option<String>,
    },
    OpenDirectory {
        default_path: Option<String>,
    },
    Save {
        default_path: String,
        filters: Vec<DialogFilter>,
    },
}

/// Receives the selected path, or `None` when the dialog was cancelled. Called on the UI thread.
pub type DialogReply = Box<dyn FnOnce(Option<String>)>;

/// Native file pickers.
pub trait Dialogs {
    fn show(&self, request: FileDialogRequest, reply: DialogReply);
}

/// Delivers an action to the UI thread from any thread.
pub trait UiPost: Send + Sync {
    fn post(&self, action: SendAction);
}

pub trait Clipboard {
    fn write_text(&self, text: &str) -> Result<(), String>;
}

/// The native window.
pub trait WindowControl {
    /// Hides the window and ends the event loop.
    fn close(&self);
    fn minimize(&self);
    fn toggle_maximize(&self);
}

/// Background jobs on std threads, whose results come back to the UI thread as actions.
#[derive(Clone)]
pub struct Workers {
    ui_post: Arc<dyn UiPost>,
}

impl Workers {
    pub fn new(ui_post: Arc<dyn UiPost>) -> Self {
        Self { ui_post }
    }

    /// Runs `job` on its own thread and dispatches the action it returns.
    pub fn spawn(&self, name: &str, job: impl FnOnce() -> SendAction + Send + 'static) {
        let ui_post = self.ui_post.clone();
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || ui_post.post(job()))
            .expect("spawn a worker thread");
    }
}

/// Everything an action may use besides the store.
pub struct Services {
    /// The runtime paths `ovrley_core` reads templates, fonts and tools from.
    pub paths: Arc<ovrley_core::paths::AppPaths>,
    /// The session's selected raster images, shared with render jobs.
    pub rasters: ovrley_services::raster_resources::RasterResources,
    pub preferences: PreferencesStore,
    pub dialogs: Box<dyn Dialogs>,
    pub workers: Workers,
    pub ui_post: Arc<dyn UiPost>,
    pub clipboard: Box<dyn Clipboard>,
    pub window: Box<dyn WindowControl>,
    /// Dispatches later on the UI thread, for replies that arrive there (dialogs).
    pub dispatch: DispatchHandle,
}

/// The parts of [`Services`] a host provides; the dispatcher adds its own handle.
pub struct ServiceParts {
    pub paths: Arc<ovrley_core::paths::AppPaths>,
    pub preferences: PreferencesStore,
    pub dialogs: Box<dyn Dialogs>,
    pub ui_post: Arc<dyn UiPost>,
    pub clipboard: Box<dyn Clipboard>,
    pub window: Box<dyn WindowControl>,
}

impl ServiceParts {
    pub(super) fn into_services(self, dispatch: DispatchHandle) -> Services {
        Services {
            paths: self.paths,
            rasters: ovrley_services::raster_resources::RasterResources::default(),
            preferences: self.preferences,
            dialogs: self.dialogs,
            workers: Workers::new(self.ui_post.clone()),
            ui_post: self.ui_post,
            clipboard: self.clipboard,
            window: self.window,
            dispatch,
        }
    }
}

/// In-memory services that record their calls.
pub mod fakes {
    use super::*;
    use crate::lib::preferences_store::MemorySink;
    use serde_json::Value;

    #[derive(Clone, Default)]
    pub struct FakeDialogs {
        pub pending: Rc<RefCell<Vec<(FileDialogRequest, DialogReply)>>>,
    }

    impl FakeDialogs {
        /// The requests shown so far and not answered.
        pub fn requests(&self) -> Vec<FileDialogRequest> {
            self.pending
                .borrow()
                .iter()
                .map(|(request, _)| request.clone())
                .collect()
        }

        /// Answers the oldest open dialog.
        pub fn answer(&self, selected: Option<&str>) {
            let (_, reply) = self.pending.borrow_mut().remove(0);
            reply(selected.map(str::to_string));
        }
    }

    impl Dialogs for FakeDialogs {
        fn show(&self, request: FileDialogRequest, reply: DialogReply) {
            self.pending.borrow_mut().push((request, reply));
        }
    }

    /// Holds posted actions until the test runs them with [`FakeUiPost::drain`].
    #[derive(Clone, Default)]
    pub struct FakeUiPost {
        pub posted: Arc<Mutex<Vec<SendAction>>>,
    }

    impl FakeUiPost {
        pub fn drain(&self, dispatcher: &super::super::Dispatcher) {
            let actions: Vec<_> = self.posted.lock().unwrap().drain(..).collect();
            for action in actions {
                dispatcher.dispatch(action);
            }
        }
    }

    impl UiPost for FakeUiPost {
        fn post(&self, action: SendAction) {
            self.posted.lock().unwrap().push(action);
        }
    }

    #[derive(Clone, Default)]
    pub struct FakeClipboard {
        pub written: Rc<RefCell<Vec<String>>>,
    }

    impl Clipboard for FakeClipboard {
        fn write_text(&self, text: &str) -> Result<(), String> {
            self.written.borrow_mut().push(text.to_string());
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum WindowCall {
        Close,
        Minimize,
        ToggleMaximize,
    }

    #[derive(Clone, Default)]
    pub struct FakeWindow {
        pub calls: Rc<RefCell<Vec<WindowCall>>>,
    }

    impl WindowControl for FakeWindow {
        fn close(&self) {
            self.calls.borrow_mut().push(WindowCall::Close);
        }
        fn minimize(&self) {
            self.calls.borrow_mut().push(WindowCall::Minimize);
        }
        fn toggle_maximize(&self) {
            self.calls.borrow_mut().push(WindowCall::ToggleMaximize);
        }
    }

    /// Handles on every fake, to inspect after dispatching.
    pub struct Fakes {
        pub preferences: MemorySink,
        pub dialogs: FakeDialogs,
        pub ui_post: FakeUiPost,
        pub clipboard: FakeClipboard,
        pub window: FakeWindow,
    }

    /// The checkout's runtime paths: its bundled templates, fonts and tools.
    pub fn repository_paths() -> ovrley_core::paths::AppPaths {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        ovrley_core::paths::AppPaths::from_repo_root(root.canonicalize().expect("the checkout exists"))
    }

    /// Fake services over the given preferences object.
    pub fn fake_services(preferences: Value) -> (ServiceParts, Fakes) {
        let (preferences, sink) = PreferencesStore::in_memory(preferences);
        let fakes = Fakes {
            preferences: sink,
            dialogs: FakeDialogs::default(),
            ui_post: FakeUiPost::default(),
            clipboard: FakeClipboard::default(),
            window: FakeWindow::default(),
        };
        let parts = ServiceParts {
            paths: Arc::new(repository_paths()),
            preferences,
            dialogs: Box::new(fakes.dialogs.clone()),
            ui_post: Arc::new(fakes.ui_post.clone()),
            clipboard: Box::new(fakes.clipboard.clone()),
            window: Box::new(fakes.window.clone()),
        };
        (parts, fakes)
    }
}
