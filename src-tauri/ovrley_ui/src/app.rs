//! The composition root (`useAppShellComposition`): creates the window and the dispatcher, binds
//! the callbacks, installs the projections and runs the event loop.

use crate::platform::gpu::GpuDevice;
use crate::platform::{self, NativeDialogs, SlintUiPost, SlintWindowControl, SystemClipboard};
use crate::{features, AppWindow};
use ovrley_app::features::app_shell;
use ovrley_app::i18n::Locale;
use ovrley_app::lib::preferences_store::PreferencesStore;
use ovrley_app::store::services::ServiceParts;
use ovrley_app::store::{Dispatcher, Store};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

/// Runs every feature's projection after each dispatch, and switches Slint's bundled translation
/// when the language changed (every `@tr` then re-evaluates).
struct Projections {
    window: slint::Weak<AppWindow>,
    overlay: features::overlay_editor::Projection,
    player: features::player::Projection,
    transitions: features::transitions::Projection,
    widget_editor: features::widget_editor::Projection,
    widget_drawer: features::widget_drawer::Projection,
    maps: features::maps::Projection,
    scene_settings: features::scene_settings::Projection,
    language: Cell<Locale>,
    /// The first run writes what depends on the language even though it has not changed.
    first: Cell<bool>,
}

impl Projections {
    fn run(&self, store: &Store) {
        let Some(window) = self.window.upgrade() else { return };
        let language_changed = self.language.replace(store.i18n.language) != store.i18n.language || self.first.replace(false);
        if language_changed {
            select_language(store.i18n.language);
        }
        features::app_shell::project(&window, store, language_changed);
        features::toolbar::project(&window, store);
        features::template_manager::project(&window, store);
        features::projects::project(&window, store);
        features::video_preview::project(&window, store);
        features::undo_redo::project(&window, store);
        self.overlay.project(&window, store);
        self.player.project(&window, store);
        self.transitions.project(&window, store);
        self.widget_editor.project(&window, store);
        self.widget_drawer.project(&window, store);
        self.maps.project(store);
        self.scene_settings.project(&window, store);
    }
}

/// Wires `window` to a dispatcher over `store` and `services`: the component logic, the language,
/// the features' callbacks and projections, and a first projection. The app and the window tests
/// both start here; the tests pass fake services.
pub fn compose(window: &AppWindow, mut store: Store, services: ServiceParts) -> Dispatcher {
    crate::install_component_logic(window);
    store.theme = Some(theme_colors(window));
    let language = store.i18n.language;
    select_language(language);
    let dispatcher = Dispatcher::new(store, services);
    let projections = Projections {
        window: window.as_weak(),
        overlay: features::overlay_editor::Projection::new(dispatcher.handle()),
        player: features::player::Projection::new(dispatcher.handle()),
        transitions: features::transitions::Projection::default(),
        widget_editor: features::widget_editor::Projection::default(),
        widget_drawer: features::widget_drawer::Projection::default(),
        maps: features::maps::Projection::new(dispatcher.handle()),
        scene_settings: features::scene_settings::Projection::default(),
        language: Cell::new(language),
        first: Cell::new(true),
    };
    features::app_shell::initialize(window);
    features::transitions::initialize(window);
    features::transitions::bind(window, &dispatcher);
    features::app_shell::bind(window, &dispatcher);
    features::toolbar::bind(window, &dispatcher);
    features::template_manager::bind(window, &dispatcher);
    features::projects::bind(window, &dispatcher);
    features::overlay_editor::bind(window, &dispatcher);
    features::player::bind(window, &dispatcher);
    features::undo_redo::bind(window, &dispatcher);
    features::widget_editor::bind(window, &dispatcher);
    features::widget_drawer::bind(window, &dispatcher);
    features::scene_settings::bind(window, &dispatcher);
    features::video_preview::bind(window, &dispatcher);
    dispatcher.set_ui_hook(Rc::new(move |store: &Store| projections.run(store)));
    dispatcher.reproject();
    dispatcher
}

/// Opens the window over `store` and runs until it closes. `gpu` is the shared device, kept for
/// the compositor (Phase 6). Agents reach the window through `agent_socket`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    store: Store,
    paths: ovrley_core::paths::AppPaths,
    app_cache_dir: &std::path::Path,
    preferences: PreferencesStore,
    maps: Arc<dyn ovrley_app::store::services::MapServices>,
    gpu: Option<GpuDevice>,
    agent_socket: ovrley_mcp::link::SocketLocation,
) -> Result<(), String> {
    let window = AppWindow::new().map_err(|error| format!("Could not create the window: {error}"))?;
    let paths = Arc::new(paths);
    let video_tools = Arc::new(ovrley_app::store::services::NativeVideoTools::open(Arc::clone(&paths), app_cache_dir, &preferences)?);
    let dispatcher = compose(
        &window,
        store,
        ServiceParts {
            paths,
            preferences,
            dialogs: Box::new(NativeDialogs { window: window.as_weak() }),
            ui_post: Arc::new(SlintUiPost),
            clipboard: Box::new(SystemClipboard::default()),
            window: Box::new(SlintWindowControl { window: window.as_weak() }),
            video_tools,
            maps,
        },
    );
    platform::install_ui_dispatch(dispatcher.handle(), window.as_weak());

    // The first window serves agents; a later one logs that it does not (Q6).
    let _agent_link = match ovrley_mcp::link::serve(agent_socket, platform::agent_bridge_hooks()) {
        Ok(server) => server,
        Err(error) => {
            log::warn!("agents cannot reach this window: {error}");
            None
        }
    };

    // The close guard decides; the window stays until it does.
    let close = dispatcher.handle();
    window.window().on_close_requested(move || {
        close.dispatch(app_shell::request_close);
        slint::CloseRequestResponse::KeepWindowShown
    });

    // On every resize: a restored window fits its screen, and on macOS the traffic lights return
    // to their inset (AppKit puts them back in the corner).
    window.window().on_winit_window_event(move |slint_window, event| {
        if let winit::event::WindowEvent::Resized(_) = event {
            slint_window.with_winit_window(platform::fit_restored_window);
            #[cfg(target_os = "macos")]
            slint_window.with_winit_window(platform::macos::apply_traffic_light_inset);
        }
        EventResult::Propagate
    });

    // `useAppBootstrap`'s shell part runs on the first event-loop turn.
    let startup = dispatcher.handle();
    let shown = window.as_weak();
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        if let Some(window) = shown.upgrade() {
            when_winit_window_exists(&window, startup.clone());
        }
        // `useAppBootstrap`: the template list and the last template, with the font catalog
        // loading in parallel (D16); the startup screen shows until the restore has finished.
        startup.dispatch(|store, services| {
            ovrley_app::features::template_manager::fetch_templates(store, services, None);
            ovrley_app::features::widget_editor::load_font_catalog(store, services);
            ovrley_app::features::maps::initialize(store, services);
            // Once startup is ready, the startup projects dialog lists the project folder (D11).
            ovrley_app::features::template_manager::restore_last_loaded_template(
                store,
                services,
                Box::new(|store, services| {
                    store.bootstrap.set_startup_ready();
                    ovrley_app::features::projects::load_startup_projects(store, services);
                }),
            );
        });
    });

    // Live preview (D17): Slint gives no reload notification, so the projections run on a short
    // timer. A reload resets the globals to their defaults; the projections compare with the
    // current values and restore the store's state.
    #[cfg(feature = "live-preview")]
    let _live_preview = {
        let timer = slint::Timer::default();
        let dispatcher = dispatcher.clone();
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || dispatcher.reproject());
        timer
    };

    #[cfg(debug_assertions)]
    crate::snapshot::start_if_requested(&window, &dispatcher);

    let _gpu = gpu;
    let result = window.run().map_err(|error| format!("The window failed: {error}"));
    dispatcher.flush_preferences();
    result
}

/// The theme colours widget fallbacks use, from the `Theme` global, their one source.
fn theme_colors(window: &AppWindow) -> ovrley_app::lib::template::ThemeColors {
    let theme = window.global::<crate::Theme>();
    let hex = |color: slint::Color| format!("#{:02x}{:02x}{:02x}", color.red(), color.green(), color.blue());
    ovrley_app::lib::template::ThemeColors {
        ice: hex(theme.get_theme_color_ice()),
        aqua: hex(theme.get_theme_color_aqua()),
        teal: hex(theme.get_theme_color_teal()),
        accent: hex(theme.get_theme_color_accent()),
    }
}

/// Every locale is bundled at build time, and English is the untranslated text, so a failure is a
/// build that lost its catalogs. Live preview interprets the `.slint` sources without the bundled
/// catalogs, so there the UI stays in English.
fn select_language(locale: Locale) {
    let selected = slint::select_bundled_translation(locale.code());
    #[cfg(feature = "live-preview")]
    if let Err(error) = selected {
        log::warn!("live preview has no bundled translations; showing English instead of {}: {error:?}", locale.code());
        return;
    }
    selected.unwrap_or_else(|error| panic!("the {} translation is not bundled: {error:?}", locale.code()));
}

/// Platform work that needs the winit window, which exists only after the first event-loop turn:
/// on macOS the traffic lights and `useTrafficLightInset`, which follows AppKit's full-screen
/// transitions (the header gives up its inset as the window starts entering full screen and takes
/// it back as it starts leaving); on Linux the Wayland drop listener; everywhere the window's
/// starting maximised state.
fn when_winit_window_exists(window: &AppWindow, dispatch: ovrley_app::store::DispatchHandle) {
    let window = window.as_weak();
    let spawned = slint::spawn_local(async move {
        let Some(app) = window.upgrade() else { return };
        let winit_window = match app.window().winit_window().await {
            Ok(winit_window) => winit_window,
            Err(error) => {
                log::warn!("no winit window for the platform integration: {error}");
                return;
            }
        };
        platform::window_events::set_window_maximized(winit_window.is_maximized() || winit_window.fullscreen().is_some());
        #[cfg(target_os = "macos")]
        {
            use platform::macos::FullscreenTransition;
            platform::macos::apply_traffic_light_inset(&winit_window);
            platform::macos::observe_fullscreen(&winit_window, move |transition| {
                let fullscreen = matches!(transition, FullscreenTransition::WillEnter | FullscreenTransition::DidEnter);
                dispatch.dispatch(move |store, _| store.traffic_light_inset.set_fullscreen(fullscreen));
            });
        }
        #[cfg(target_os = "linux")]
        platform::window_events::start_wayland_drop(&winit_window);
        #[cfg(not(target_os = "macos"))]
        drop(dispatch);
    });
    if let Err(error) = spawned {
        log::warn!("could not start the platform integration: {error}");
    }
}
