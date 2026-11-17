//! The side effects an action may ask for (plan D3).
//!
//! Actions stay synchronous. What they cannot do in place, they ask of a service held by the
//! dispatcher. `ovrley_ui` implements the traits with `rfd`, `slint::invoke_from_event_loop`,
//! `arboard` and the winit window; tests use the in-memory fakes at the end of this module, which
//! record every call.

use super::{DispatchHandle, SendAction};
use serde_json::Value;
use crate::lib::preferences_store::PreferencesStore;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
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

/// The video tools the player uses off the UI thread (phase 4 D12): filmstrips through
/// `ovrley_services::video_filmstrip`, the code the Tauri commands use, and codec detection.
pub trait VideoTools: Send + Sync {
    /// The strip from the disk cache, or `None` on a miss. Cheap: no decode.
    fn cached_filmstrip(&self, request: &FilmstripRequest) -> Result<Option<Filmstrip>, String>;
    /// The strip from the cache, or decoded (with `hwaccel`, or in software) and cached. `None`
    /// when FFmpeg decodes no frame.
    fn filmstrip(&self, request: &FilmstripRequest, hwaccel: Option<FilmstripHwaccel>) -> Result<Option<Filmstrip>, String>;
    /// `detectCodecs`: the encoders and decoders the bundled FFmpeg offers.
    fn detect_codecs(&self) -> Result<ovrley_core::encode::ffmpeg::detect::AvailableCodecs, String>;
}

pub use ovrley_services::video_filmstrip::{Filmstrip, FilmstripHwaccel, FilmstripRequest, FilmstripWindow};

/// The native video tools: the filmstrip cache in the application cache directory, with the limit
/// the Tauri app keeps in the shared settings file, and the bundled FFmpeg.
pub struct NativeVideoTools {
    paths: Arc<ovrley_core::paths::AppPaths>,
    cache: ovrley_services::filmstrip_cache::FilmstripCache,
}

/// The settings key of the filmstrip cache limit, shared with the Tauri app.
pub const FILMSTRIP_CACHE_LIMIT_KEY: &str = "filmstripCacheLimitBytes";

impl NativeVideoTools {
    /// Opens the cache. The saved limit restores state at startup: a malformed one falls back to
    /// the default, so a bad value never keeps OVRLEY from starting.
    pub fn open(paths: Arc<ovrley_core::paths::AppPaths>, app_cache_dir: &std::path::Path, preferences: &PreferencesStore) -> Result<Self, String> {
        use ovrley_services::filmstrip_cache::{validate_filmstrip_cache_limit, FilmstripCache, DEFAULT_FILMSTRIP_CACHE_LIMIT};
        let limit = preferences
            .get(FILMSTRIP_CACHE_LIMIT_KEY)
            .and_then(Value::as_u64)
            .and_then(|limit| validate_filmstrip_cache_limit(limit).ok())
            .unwrap_or(DEFAULT_FILMSTRIP_CACHE_LIMIT);
        Ok(Self { cache: FilmstripCache::for_application_cache(app_cache_dir, limit)?, paths })
    }
}

impl VideoTools for NativeVideoTools {
    fn cached_filmstrip(&self, request: &FilmstripRequest) -> Result<Option<Filmstrip>, String> {
        ovrley_services::video_filmstrip::cached_filmstrip(&self.cache, request)
    }

    fn filmstrip(&self, request: &FilmstripRequest, hwaccel: Option<FilmstripHwaccel>) -> Result<Option<Filmstrip>, String> {
        let ffmpeg = ovrley_core::encode::ffmpeg::binary::resolve_ffmpeg_binary(&self.paths.repo_root).map_err(|error| error.to_string())?;
        ovrley_services::video_filmstrip::filmstrip(&self.cache, &ffmpeg, request, hwaccel)
    }

    fn detect_codecs(&self) -> Result<ovrley_core::encode::ffmpeg::detect::AvailableCodecs, String> {
        ovrley_core::encode::ffmpeg::detect::detect_codecs(&self.paths.repo_root).map_err(|error| error.to_string())
    }
}

pub use ovrley_services::map_styles::{MapAdapterSupport, MapStyleCatalog};
pub use ovrley_services::map_tile_service::TileCacheUsage;
pub use ovrley_services::widget_preview::{MapFrameWake, MapPrepareRequest};
pub use ovrley_widgets::widgets::map::preview::MapFrameFactory;

/// The maps' data and renderer (phase 5 D9, D11): the installed styles, the preview's frame
/// thread, the background tile download, the tile cache, and the GPU the renderer would use.
pub trait MapServices: Send + Sync {
    /// The installed styles, read at startup.
    fn styles(&self) -> &MapStyleCatalog;
    /// Starts the preview's map frame thread, which calls `wake` whenever a map's frame changed.
    fn frames(&self, wake: MapFrameWake) -> Result<Arc<dyn MapFrameFactory>, String>;
    /// Which adapter the map renderer would open. Blocking.
    fn probe_adapter(&self) -> MapAdapterSupport;
    /// `prepare_map_tiles`: downloads the tiles the maps will show into the disk cache. Blocking;
    /// a cancelled download ends with `Ok`.
    fn prepare_tiles(&self, request: MapPrepareRequest, cancelled: &AtomicBool) -> Result<(), String>;
    fn tile_cache_usage(&self) -> Result<TileCacheUsage, String>;
    /// Applies and saves a new limit, evicting down to it.
    fn set_tile_cache_limit(&self, limit: u64) -> Result<TileCacheUsage, String>;
    fn clear_tile_cache(&self) -> Result<TileCacheUsage, String>;
}

/// The settings key of the map tile cache limit, shared with the Tauri app.
pub const MAP_TILE_CACHE_LIMIT_KEY: &str = "mapTileCacheLimitBytes";

/// The native maps over `MapData`, which the Tauri shell and export use.
pub struct NativeMapServices {
    data: ovrley_services::map_styles::MapData,
}

impl NativeMapServices {
    /// Reads the installed styles and opens the tile cache, as the Tauri shell does. The saved
    /// limit is a behaviour preference, so it stays strict: absent means the default, anything but
    /// a valid whole number of bytes fails (D11).
    pub fn open(roots: &ovrley_services::runtime_paths::RuntimeRoots, preferences: &PreferencesStore) -> Result<Self, String> {
        use ovrley_services::map_tile_service::{validate_tile_cache_limit, MapTileService, DEFAULT_TILE_CACHE_LIMIT};
        let limit = match preferences.get(MAP_TILE_CACHE_LIMIT_KEY) {
            None => DEFAULT_TILE_CACHE_LIMIT,
            Some(value) => validate_tile_cache_limit(
                value.as_u64().ok_or_else(|| format!("Setting {MAP_TILE_CACHE_LIMIT_KEY} must be a whole number of bytes: {value}"))?,
            )?,
        };
        Ok(Self {
            data: ovrley_services::map_styles::MapData::new(
                ovrley_services::map_styles::MapStyleRegistry::load(&roots.map_style_dirs())?,
                MapTileService::for_application_cache(roots.app_cache_dir.clone(), limit)?,
                Arc::new(ovrley_services::map_styles::GlyphService::open(&roots.map_font_dir())?),
            ),
        })
    }
}

impl MapServices for NativeMapServices {
    fn styles(&self) -> &MapStyleCatalog {
        self.data.registry().catalog()
    }

    fn frames(&self, wake: MapFrameWake) -> Result<Arc<dyn MapFrameFactory>, String> {
        Ok(Arc::new(ovrley_services::widget_preview::PreviewMapFrames::start(self.data.clone(), wake)?))
    }

    fn probe_adapter(&self) -> MapAdapterSupport {
        ovrley_services::map_styles::probe_adapter()
    }

    fn prepare_tiles(&self, request: MapPrepareRequest, cancelled: &AtomicBool) -> Result<(), String> {
        ovrley_services::widget_preview::prepare_map_tiles(request, &self.data, cancelled, &|_| {})
    }

    fn tile_cache_usage(&self) -> Result<TileCacheUsage, String> {
        self.data.tiles().usage()
    }

    fn set_tile_cache_limit(&self, limit: u64) -> Result<TileCacheUsage, String> {
        self.data.tiles().set_limit(limit)
    }

    fn clear_tile_cache(&self) -> Result<TileCacheUsage, String> {
        self.data.tiles().clear()?;
        self.data.tiles().usage()
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
    pub video_tools: Arc<dyn VideoTools>,
    pub maps: Arc<dyn MapServices>,
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
    pub video_tools: Arc<dyn VideoTools>,
    pub maps: Arc<dyn MapServices>,
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
            video_tools: self.video_tools,
            maps: self.maps,
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
        pub video_tools: Arc<FakeVideoTools>,
        pub maps: Arc<FakeMapServices>,
    }

    /// Maps over the checkout's bundled styles, with frames from a test's factory, a fixed adapter
    /// and an in-memory tile cache. Every call is recorded.
    pub struct FakeMapServices {
        pub styles: MapStyleCatalog,
        pub adapter: Mutex<MapAdapterSupport>,
        pub frames: Mutex<Arc<dyn MapFrameFactory>>,
        pub wake: Mutex<Option<MapFrameWake>>,
        pub prepared: Mutex<Vec<usize>>,
        pub usage: Mutex<TileCacheUsage>,
        pub calls: Mutex<Vec<String>>,
    }

    impl Default for FakeMapServices {
        fn default() -> Self {
            let paths = repository_paths();
            let dirs = ovrley_services::map_styles::MapStyleDirs {
                bundled: paths.repo_root.join("assets").join("map-styles"),
                user: std::env::temp_dir().join(format!("ovrley-test-map-styles-{}", std::process::id())),
            };
            let registry = ovrley_services::map_styles::MapStyleRegistry::load(&dirs).expect("the bundled map styles install");
            Self {
                styles: registry.catalog().clone(),
                adapter: Mutex::new(MapAdapterSupport::Hardware),
                frames: Mutex::new(Arc::new(NoFrames)),
                wake: Mutex::new(None),
                prepared: Mutex::new(Vec::new()),
                usage: Mutex::new(TileCacheUsage { used_bytes: 0, limit_bytes: 512 << 20, min_limit_bytes: 128 << 20, max_limit_bytes: 40 * (128 << 20), limit_step_bytes: 128 << 20 }),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    /// Frames that never draw: each fails at once, so a map shows its placeholder.
    pub struct NoFrames;

    impl MapFrameFactory for NoFrames {
        fn provider(&self, _style: &ovrley_services::map_styles::MapStyleInfo, _sample_count: u32) -> Arc<dyn ovrley_widgets::widgets::map::preview::MapFrameProvider> {
            Arc::new(NoFrames)
        }
    }

    impl ovrley_widgets::widgets::map::preview::MapFrameProvider for NoFrames {
        fn frame(
            &self,
            _request: &ovrley_widgets::widgets::map::preview::MapFrameRequest,
            _spare: Option<Vec<u8>>,
        ) -> ovrley_widgets::error::WidgetResult<ovrley_widgets::widgets::map::preview::MapFrameReply> {
            let (reply, answer) = std::sync::mpsc::channel();
            reply.send(Err("test maps draw no frames".into())).expect("the answer is held");
            Ok(answer)
        }
    }

    impl MapServices for FakeMapServices {
        fn styles(&self) -> &MapStyleCatalog {
            &self.styles
        }

        fn frames(&self, wake: MapFrameWake) -> Result<Arc<dyn MapFrameFactory>, String> {
            *self.wake.lock().unwrap() = Some(wake);
            Ok(self.frames.lock().unwrap().clone())
        }

        fn probe_adapter(&self) -> MapAdapterSupport {
            *self.adapter.lock().unwrap()
        }

        fn prepare_tiles(&self, request: MapPrepareRequest, _cancelled: &AtomicBool) -> Result<(), String> {
            self.prepared.lock().unwrap().push(request.widgets.len());
            Ok(())
        }

        fn tile_cache_usage(&self) -> Result<TileCacheUsage, String> {
            self.calls.lock().unwrap().push("usage".into());
            Ok(*self.usage.lock().unwrap())
        }

        fn set_tile_cache_limit(&self, limit: u64) -> Result<TileCacheUsage, String> {
            self.calls.lock().unwrap().push(format!("limit {limit}"));
            let mut usage = self.usage.lock().unwrap();
            usage.limit_bytes = ovrley_services::map_tile_service::validate_tile_cache_limit(limit)?;
            Ok(*usage)
        }

        fn clear_tile_cache(&self) -> Result<TileCacheUsage, String> {
            self.calls.lock().unwrap().push("clear".into());
            let mut usage = self.usage.lock().unwrap();
            usage.used_bytes = 0;
            Ok(*usage)
        }
    }

    /// Video tools answering from a table: a strip per window key (cached or decoded), and
    /// the codecs.
    #[derive(Default)]
    pub struct FakeVideoTools {
        pub cached: Mutex<std::collections::HashMap<String, Filmstrip>>,
        pub decoded: Mutex<std::collections::HashMap<String, Filmstrip>>,
        pub decodes: Mutex<Vec<(String, Option<FilmstripHwaccel>)>>,
        pub codecs: Mutex<ovrley_core::encode::ffmpeg::detect::AvailableCodecs>,
    }

    /// A strip's table key: its window as `crate::features::video_preview::filmstrips` keys it.
    fn key(request: &FilmstripRequest) -> String {
        crate::features::video_preview::filmstrips::request_key(request)
    }

    impl VideoTools for FakeVideoTools {
        fn cached_filmstrip(&self, request: &FilmstripRequest) -> Result<Option<Filmstrip>, String> {
            Ok(self.cached.lock().unwrap().get(&key(request)).cloned())
        }

        fn filmstrip(&self, request: &FilmstripRequest, hwaccel: Option<FilmstripHwaccel>) -> Result<Option<Filmstrip>, String> {
            self.decodes.lock().unwrap().push((key(request), hwaccel));
            Ok(self.decoded.lock().unwrap().get(&key(request)).cloned())
        }

        fn detect_codecs(&self) -> Result<ovrley_core::encode::ffmpeg::detect::AvailableCodecs, String> {
            Ok(self.codecs.lock().unwrap().clone())
        }
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
            video_tools: Arc::new(FakeVideoTools::default()),
            maps: Arc::new(FakeMapServices::default()),
        };
        let parts = ServiceParts {
            paths: Arc::new(repository_paths()),
            preferences,
            dialogs: Box::new(fakes.dialogs.clone()),
            ui_post: Arc::new(fakes.ui_post.clone()),
            clipboard: Box::new(fakes.clipboard.clone()),
            window: Box::new(fakes.window.clone()),
            video_tools: fakes.video_tools.clone(),
            maps: fakes.maps.clone(),
        };
        (parts, fakes)
    }
}
