//! The composition root (`useAppShellComposition`): creates the window and the dispatcher, binds
//! the callbacks, installs the projections and runs the event loop.

use crate::platform::gpu::GpuDevice;
use crate::platform::{self, NativeDialogs, SlintUiPost, SlintWindowControl, SystemClipboard};
use crate::{features, i18n_strings, AppWindow};
use ovrley_app::features::app_shell;
use ovrley_app::i18n::{self, Locale};
use ovrley_app::lib::preferences_store::PreferencesStore;
use ovrley_app::store::services::ServiceParts;
use ovrley_app::store::{Dispatcher, Store};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// Runs every feature's projection after each dispatch. Re-applies the whole catalog to the string
/// globals when the language changed.
struct Projections {
    window: slint::Weak<AppWindow>,
    language: Cell<Locale>,
}

impl Projections {
    fn run(&self, store: &Store) {
        let Some(window) = self.window.upgrade() else { return };
        if self.language.replace(store.i18n.language) != store.i18n.language {
            i18n_strings::apply_locale(&window, i18n::catalog(), store.i18n.language);
        }
        features::app_shell::project(&window, store);
    }
}

/// Opens the window over `store` and runs until it closes. `gpu` is the shared device, kept for
/// the compositor (Phase 6).
pub fn run(store: Store, preferences: PreferencesStore, gpu: Option<GpuDevice>) -> Result<(), String> {
    let window = AppWindow::new().map_err(|error| format!("Could not create the window: {error}"))?;
    crate::install_component_logic(&window);
    let language = store.i18n.language;
    i18n_strings::apply_locale(&window, i18n::catalog(), language);

    let dispatcher = Dispatcher::new(
        store,
        ServiceParts {
            preferences,
            dialogs: Box::new(NativeDialogs { window: window.as_weak() }),
            ui_post: Arc::new(SlintUiPost),
            clipboard: Box::new(SystemClipboard::default()),
            window: Box::new(SlintWindowControl { window: window.as_weak() }),
        },
    );
    platform::install_ui_dispatch(dispatcher.handle());
    let projections = Projections {
        window: window.as_weak(),
        language: Cell::new(language),
    };
    dispatcher.set_ui_hook(Rc::new(move |store: &Store| projections.run(store)));
    dispatcher.reproject();

    // The close guard decides; the window stays until it does.
    let close = dispatcher.handle();
    window.window().on_close_requested(move || {
        close.dispatch(app_shell::request_close);
        slint::CloseRequestResponse::KeepWindowShown
    });

    // Leaving fullscreen resizes the window, and AppKit puts the traffic lights back in the corner on
    // every resize, so both are re-read here.
    let resized = dispatcher.handle();
    window.window().on_winit_window_event(move |slint_window, event| {
        if let winit::event::WindowEvent::Resized(_) = event {
            let fullscreen = slint_window
                .with_winit_window(|winit_window| {
                    #[cfg(target_os = "macos")]
                    platform::macos::apply_traffic_light_inset(winit_window);
                    winit_window.fullscreen().is_some()
                })
                .unwrap_or(false);
            resized.dispatch(move |store, _| store.traffic_light_inset.set_fullscreen(fullscreen));
        }
        EventResult::Propagate
    });

    // `useAppBootstrap`'s shell part runs on the first event-loop turn.
    let startup = dispatcher.handle();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || startup.dispatch(|store, _| store.bootstrap.set_startup_ready()));

    // Live preview (D17): Slint gives no reload notification, so the projections run on a short
    // timer. A reload resets the globals to their defaults; the projections compare with the
    // current values and restore the store's state, and the catalog is written again.
    #[cfg(feature = "live-preview")]
    let _live_preview = {
        let timer = slint::Timer::default();
        let dispatcher = dispatcher.clone();
        let window = window.as_weak();
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || {
            if let Some(window) = window.upgrade() {
                let language = dispatcher.with_store(|store| store.i18n.language);
                i18n_strings::apply_locale(&window, i18n::catalog(), language);
            }
            dispatcher.reproject();
        });
        timer
    };

    #[cfg(debug_assertions)]
    crate::snapshot::start_if_requested(&window, &dispatcher);

    let _gpu = gpu;
    let result = window.run().map_err(|error| format!("The window failed: {error}"));
    dispatcher.flush_preferences();
    result
}
