//! The app-shell feature's hooks: `useAppBootstrap` (its shell part), `useWindowCloseGuard` and
//! `useTrafficLightInset`.
//!
//! React keeps each in a hook's state; here each is a struct in the [`Store`], named after its
//! hook, and its actions are methods on it (plan D1, D2).

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
}
