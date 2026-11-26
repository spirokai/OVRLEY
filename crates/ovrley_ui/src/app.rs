//! The composition root: creates the window and the dispatcher, binds the callbacks, installs the
//! projections and runs the event loop.

use crate::features::{documents, editor, import, playback, player, render, shell, sync, toolbar, transitions, widgets};
use crate::platform::gpu::GpuDevice;
use crate::platform::{self, NativeDialogs, SlintUiPost, SlintWindowControl, SystemClipboard};
use crate::{AppWindow, Theme};
use ovrley_app::features::sync::maps::WheelDevice;
use ovrley_app::i18n::Locale;
use ovrley_app::preferences::PreferencesStore;
use ovrley_app::store::template::ThemeColors;
use ovrley_app::store::services::{MapServices, NativeRenderService, NativeUpdateService, NativeVideoTools, ServiceParts, UiPost};
use ovrley_app::store::{DispatchHandle, Dispatcher, Store};
use ovrley_core::paths::AppPaths;
use ovrley_mcp::link::SocketLocation;
use ovrley_services::runtime::Distribution;
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Runs every feature's projection after each dispatch, and switches Slint's bundled translation
/// when the language changed (every `@tr` then re-evaluates).
struct Projections {
    window: slint::Weak<AppWindow>,
    documents: documents::Projection,
    editor: editor::Projection,
    player: player::Projection,
    sync: sync::Projection,
    toolbar: toolbar::Projection,
    transitions: transitions::Projection,
    widgets: widgets::Projection,
    /// The language last written; `None` until the first projection.
    language: Cell<Option<Locale>>,
}

impl Projections {
    fn run(&self, store: &Store) {
        let Some(window) = self.window.upgrade() else { return };
        let language = store.i18n.language;
        if self.language.replace(Some(language)) != Some(language) {
            select_language(language);
            shell::project_language(&window, language);
        }
        shell::project(&window, store);
        self.toolbar.project(&window, store);
        self.documents.project(&window, store);
        render::project(&window, store);
        import::project(&window, store);
        self.sync.project(&window, store);
        self.editor.project(&window, store);
        self.player.project(&window, store);
        self.transitions.project(&window, store);
        self.widgets.project(&window, store);
        playback::project(&window, store);
    }
}

/// Wires `window` to a dispatcher over `store` and `services`: the component logic, the features'
/// callbacks and projections, and a first projection. The app and the window tests both start
/// here; the tests pass fake services.
pub fn compose(window: &AppWindow, mut store: Store, services: ServiceParts) -> Dispatcher {
    crate::install_component_logic(window);
    store.theme = Some(theme_colors(window));
    let dispatcher = Dispatcher::new(store, services);
    let projections = Projections {
        window: window.as_weak(),
        documents: documents::Projection::default(),
        editor: editor::Projection::new(dispatcher.handle()),
        player: player::Projection::new(dispatcher.handle()),
        sync: sync::Projection::default(),
        toolbar: toolbar::Projection::default(),
        transitions: transitions::Projection::default(),
        widgets: widgets::Projection::default(),
        language: Cell::new(None),
    };
    shell::initialize(window);
    transitions::initialize(window);
    shell::bind(window, &dispatcher);
    toolbar::bind(window, &dispatcher);
    documents::bind(window, &dispatcher);
    render::bind(window, &dispatcher);
    import::bind(window, &dispatcher);
    sync::bind(window, &dispatcher);
    editor::bind(window, &dispatcher);
    player::bind(window, &dispatcher);
    transitions::bind(window, &dispatcher);
    widgets::bind(window, &dispatcher);
    dispatcher.set_ui_hook(Rc::new(move |store: &Store| projections.run(store)));
    dispatcher.reproject();
    dispatcher
}

/// Why the app ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Closed,
    /// An update is installed. The caller starts it: the agents' socket is free by then and the
    /// preferences are written, so the new instance finds both as it needs them.
    Relaunch,
}

/// What the window starts with, from `main`.
pub struct Startup {
    pub store: Store,
    pub distribution: Distribution,
    pub paths: AppPaths,
    pub app_cache_dir: PathBuf,
    pub preferences: PreferencesStore,
    pub maps: Arc<dyn MapServices>,
    /// The shared device the video players draw on, when there is one.
    pub gpu: Option<GpuDevice>,
    /// Where agents reach the window.
    pub agent_socket: SocketLocation,
}

/// Opens the window and runs until it closes.
///
/// # Errors
///
/// When the window, the scratch folder or the video tools cannot be set up, or the event loop fails.
pub fn run(startup: Startup) -> Result<Exit, String> {
    let Startup { store, distribution, paths, app_cache_dir, preferences, maps, gpu, agent_socket } = startup;
    let window = AppWindow::new().map_err(|error| format!("Could not create the window: {error}"))?;
    // Window chrome, not app state: written to the global directly, like the maximised state.
    #[cfg(windows)]
    window.global::<crate::AppShell>().set_windows_10(platform::is_windows_10());
    let paths = Arc::new(paths);
    // The transition blends of earlier sessions.
    ovrley_core::encode::ffmpeg::transitions::clear_transition_proxies(&paths).map_err(|error| format!("Could not clear the transition previews: {error}"))?;
    let video_tools = Arc::new(NativeVideoTools::open(Arc::clone(&paths), &app_cache_dir, &preferences)?);
    let ui_post: Arc<dyn UiPost> = Arc::new(SlintUiPost);
    let dispatcher = compose(
        &window,
        store,
        ServiceParts {
            paths: Arc::clone(&paths),
            preferences,
            dialogs: Box::new(NativeDialogs { window: window.as_weak() }),
            ui_post: Arc::clone(&ui_post),
            clipboard: Box::new(SystemClipboard::default()),
            window: Box::new(SlintWindowControl { window: window.as_weak() }),
            video_tools,
            render: Arc::new(NativeRenderService::new(Arc::clone(&paths), maps.map_source(), ui_post)),
            maps,
            updates: Arc::new(NativeUpdateService),
        },
    );
    platform::install_ui_dispatch(dispatcher.handle(), window.as_weak());

    // The first window serves agents; a later one logs that it does not.
    let _agent_link = ovrley_mcp::link::serve(agent_socket, platform::AgentWindow).unwrap_or_else(|error| {
        log::warn!("agents cannot reach this window: {error}");
        None
    });

    // The close guard decides; the window stays until it does.
    let close = dispatcher.handle();
    window.window().on_close_requested(move || {
        close.dispatch(ovrley_app::features::shell::request_close);
        slint::CloseRequestResponse::KeepWindowShown
    });

    follow_window_events(&window);
    let startup = dispatcher.handle();
    let shown = window.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(window) = shown.upgrade() {
            when_winit_window_exists(&window, startup.clone());
        }
        startup.dispatch(move |store, services| bootstrap(store, services, distribution));
    });

    // Live preview: Slint gives no reload notification, so the projections run on a short timer. A
    // reload resets the globals to their defaults; the projections compare with the current values
    // and restore the store's state.
    #[cfg(feature = "live-preview")]
    let _live_preview = {
        let timer = slint::Timer::default();
        let dispatcher = dispatcher.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(500), move || dispatcher.reproject());
        timer
    };

    #[cfg(debug_assertions)]
    crate::snapshot::start_if_requested(&window, &dispatcher);

    let _frame_rate = std::env::var_os("OVRLEY_FRAME_RATE").map(|_| count_frames(&window));

    playback::start(&window, gpu.as_ref(), &dispatcher.handle());
    let result = window.run().map_err(|error| format!("The window failed: {error}"));
    playback::shutdown();
    // The players are gone; the device may go now.
    drop(gpu);
    dispatcher.flush_preferences();
    let relaunch = dispatcher.with_store(|store| store.app_update.relaunch);
    result.map(|()| if relaunch { Exit::Relaunch } else { Exit::Closed })
}

/// What starts on the first event-loop turn: the update check, the template list, the font
/// catalog, the codecs, the maps, and the last template's restore. The startup screen shows until
/// the restore has finished, then the startup projects dialog lists the project folder.
fn bootstrap(store: &mut Store, services: &mut ovrley_app::store::services::Services, distribution: Distribution) {
    use ovrley_app::features::{maps, player, projects, shell, templates, widgets};
    shell::check_at_startup(store, services, distribution, cfg!(debug_assertions));
    templates::fetch_templates(store, services, None);
    widgets::load_font_catalog(store, services);
    player::filmstrips::fetch_available_codecs(store, services);
    maps::initialize(store, services);
    templates::restore_last_loaded_template(
        store,
        services,
        Box::new(|store, services| {
            store.bootstrap.set_startup_ready();
            projects::load_startup_projects(store, services);
        }),
    );
}

/// The raw winit events the window follows: the wheel's device for Sync Doctor's maps, and on every
/// resize a restored window that fits its screen and, on macOS, the traffic lights back at their
/// inset (AppKit puts them in the corner).
fn follow_window_events(window: &AppWindow) {
    #[cfg(target_os = "macos")]
    let header = window.as_weak();
    window.window().on_winit_window_event(move |slint_window, event| {
        match event {
            // Line deltas come from a notched wheel, pixel deltas from a trackpad; Slint's scroll
            // event that follows does not say which.
            winit::event::WindowEvent::MouseWheel { delta, .. } => sync::record_wheel_device(match delta {
                winit::event::MouseScrollDelta::LineDelta(..) => WheelDevice::Wheel,
                winit::event::MouseScrollDelta::PixelDelta(..) => WheelDevice::Trackpad,
            }),
            winit::event::WindowEvent::Resized(_) => {
                slint_window.with_winit_window(platform::fit_restored_window);
                #[cfg(target_os = "macos")]
                if let Some(header) = header.upgrade() {
                    let header_height = f64::from(header.get_header_height());
                    slint_window.with_winit_window(|winit_window| platform::macos::apply_traffic_light_inset(winit_window, header_height));
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });

    // A resize reports before the header is laid out at its new size, so the traffic lights follow
    // the header's height itself.
    #[cfg(target_os = "macos")]
    {
        let header = window.as_weak();
        window.on_header_height_changed(move |height| {
            if let Some(app) = header.upgrade() {
                app.window().with_winit_window(|winit_window| platform::macos::apply_traffic_light_inset(winit_window, f64::from(height)));
            }
        });
    }
}

/// `OVRLEY_FRAME_RATE=1` logs how many frames the window rendered each second, a measurement aid.
fn count_frames(window: &AppWindow) -> slint::Timer {
    let frames = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&frames);
    window
        .window()
        .set_rendering_notifier(move |state, _| {
            if matches!(state, slint::RenderingState::AfterRendering) {
                counted.set(counted.get() + 1);
            }
        })
        .expect("no other rendering notifier is set");
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        log::info!("window: {} frames rendered this second", frames.replace(0));
    });
    timer
}

/// The theme colours widget fallbacks use, from the `Theme` global, their one source.
fn theme_colors(window: &AppWindow) -> ThemeColors {
    let theme = window.global::<Theme>();
    let hex = |color: slint::Color| format!("#{:02x}{:02x}{:02x}", color.red(), color.green(), color.blue());
    ThemeColors {
        ice: hex(theme.get_theme_color_ice()),
        aqua: hex(theme.get_theme_color_aqua()),
        teal: hex(theme.get_theme_color_teal()),
        accent: hex(theme.get_theme_color_accent()),
    }
}

/// Selects the bundled translation of `locale`.
///
/// # Panics
///
/// When the translation is not bundled: every locale is bundled at build time, and English is the
/// untranslated text, so that is a build that lost its catalogs. Live preview interprets the
/// `.slint` sources without the bundled catalogs, so there the UI stays in English.
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
/// on macOS the traffic lights, which follow AppKit's full-screen transitions (the header gives up
/// its inset as the window starts entering full screen and takes it back as it starts leaving); on
/// Linux the Wayland drop listener; everywhere the window's starting maximised state.
fn when_winit_window_exists(window: &AppWindow, dispatch: DispatchHandle) {
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
        platform::events::set_window_maximized(winit_window.is_maximized() || winit_window.fullscreen().is_some());
        #[cfg(target_os = "macos")]
        {
            use platform::macos::FullscreenTransition;
            platform::macos::apply_traffic_light_inset(&winit_window, f64::from(app.get_header_height()));
            let header = app.as_weak();
            let header_height = move || header.upgrade().map_or(0.0, |app| f64::from(app.get_header_height()));
            platform::macos::observe_fullscreen(&winit_window, header_height, move |transition| {
                let fullscreen = matches!(transition, FullscreenTransition::WillEnter | FullscreenTransition::DidEnter);
                dispatch.dispatch(move |store, _| store.traffic_light_inset.set_fullscreen(fullscreen));
            });
        }
        #[cfg(target_os = "linux")]
        platform::events::start_wayland_drop(&winit_window);
        #[cfg(not(target_os = "macos"))]
        drop(dispatch);
    });
    if let Err(error) = spawned {
        log::warn!("could not start the platform integration: {error}");
    }
}
