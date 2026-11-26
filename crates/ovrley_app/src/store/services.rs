//! The side effects an action may ask for.
//!
//! Actions stay synchronous. What they cannot do in place, they ask of a service held by the
//! dispatcher. `ovrley_ui` implements the traits with `rfd`, `slint::invoke_from_event_loop`,
//! `arboard` and the winit window; tests use the in-memory fakes at the end of this module, which
//! record every call.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use ovrley_core::encode::ffmpeg::transitions::TransitionProxyRequest;
use ovrley_core::jobs::batch::BatchServiceError;
use ovrley_core::jobs::contracts::{BatchAcceptance, BatchRenderRequest, BatchSnapshot};
use ovrley_core::jobs::execution::RenderAccepted;
use ovrley_services::cache::CacheUsage;
use ovrley_services::maps::{MapAdapterSupport, MapFrameWake, MapPrepareRequest, MapStyleCatalog};
use ovrley_services::updater::{DownloadProgress, ReleaseUpdate};
use ovrley_services::video::{Filmstrip, FilmstripHwaccel, FilmstripRequest};
use ovrley_widgets::widgets::map::preview::MapFrameFactory;
use serde_json::Value;
use crate::preferences::PreferencesStore;
use crate::store::{DispatchHandle, SendAction};

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

/// A native file dialog to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDialogRequest {
    OpenFile {
        filters: Vec<DialogFilter>,
        default_path: Option<String>,
    },
    /// Several files at once.
    OpenFiles {
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

/// Receives the selected paths, none when the dialog was cancelled. Called on the UI thread.
pub type DialogReply = Box<dyn FnOnce(Vec<String>)>;

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

/// The video tools the player uses off the UI thread: filmstrips through `ovrley_services::video`,
/// and codec detection.
pub trait VideoTools: Send + Sync {
    /// The strip from the disk cache, or `None` on a miss. Cheap: no decode.
    fn cached_filmstrip(&self, request: &FilmstripRequest) -> Result<Option<Filmstrip>, String>;
    /// The strip from the cache, or decoded (with `hwaccel`, or in software) and cached. `None`
    /// when FFmpeg decodes no frame.
    fn filmstrip(&self, request: &FilmstripRequest, hwaccel: Option<FilmstripHwaccel>) -> Result<Option<Filmstrip>, String>;
    /// The encoders and decoders the bundled FFmpeg offers.
    fn detect_codecs(&self) -> Result<ovrley_core::encode::ffmpeg::detect::AvailableCodecs, String>;
    /// The file check, then `probe_video_metadata`. Blocking.
    fn probe(&self, path: &str) -> Result<ovrley_core::media::probe::SourceVideoMetadata, String>;
    /// The first frame as a PNG, or `None` when FFmpeg reads no frame. Blocking;
    /// each runs an FFmpeg process.
    fn thumbnail(&self, path: &str) -> Option<Vec<u8>>;
    /// The video's embedded activity, `None` without one. Blocking.
    fn extract_telemetry(&self, path: &str) -> Result<Option<ovrley_core::activity::schema::ParsedActivity>, String>;
    /// Renders a join's blend and returns the file's path. Blocking.
    fn render_transition_proxy(&self, request: &TransitionProxyRequest) -> Result<String, String>;
    fn remove_transition_proxy(&self, path: &str) -> Result<(), String>;
}

/// The video tools: the filmstrip cache in the application cache directory, with its limit kept in
/// the settings file, and the bundled FFmpeg.
pub struct NativeVideoTools {
    paths: Arc<ovrley_core::paths::AppPaths>,
    cache: ovrley_services::cache::FilmstripCache,
}

/// The settings key of the filmstrip cache limit.
pub const FILMSTRIP_CACHE_LIMIT_KEY: &str = "filmstripCacheLimitBytes";

impl NativeVideoTools {
    /// Opens the cache. The saved limit restores state at startup: a malformed one falls back to
    /// the default, so a bad value never keeps OVRLEY from starting.
    pub fn open(paths: Arc<ovrley_core::paths::AppPaths>, app_cache_dir: &std::path::Path, preferences: &PreferencesStore) -> Result<Self, String> {
        use ovrley_services::cache::{CacheLimit, FilmstripCache, DEFAULT_FILMSTRIP_CACHE_LIMIT};
        let limit = preferences
            .get(FILMSTRIP_CACHE_LIMIT_KEY)
            .and_then(Value::as_u64)
            .and_then(|limit| CacheLimit::new(limit).ok())
            .unwrap_or(DEFAULT_FILMSTRIP_CACHE_LIMIT);
        Ok(Self { cache: FilmstripCache::open(app_cache_dir, limit)?, paths })
    }
}

impl VideoTools for NativeVideoTools {
    fn cached_filmstrip(&self, request: &FilmstripRequest) -> Result<Option<Filmstrip>, String> {
        ovrley_services::video::cached_filmstrip(&self.cache, request)
    }

    fn filmstrip(&self, request: &FilmstripRequest, hwaccel: Option<FilmstripHwaccel>) -> Result<Option<Filmstrip>, String> {
        let ffmpeg = ovrley_core::encode::ffmpeg::resolve_ffmpeg_binary(&self.paths.repo_root).map_err(|error| error.to_string())?;
        ovrley_services::video::filmstrip(&self.cache, &ffmpeg, request, hwaccel)
    }

    fn detect_codecs(&self) -> Result<ovrley_core::encode::ffmpeg::detect::AvailableCodecs, String> {
        ovrley_core::encode::ffmpeg::detect::detect_codecs(&self.paths.repo_root).map_err(|error| error.to_string())
    }

    fn probe(&self, path: &str) -> Result<ovrley_core::media::probe::SourceVideoMetadata, String> {
        let metadata = std::fs::metadata(path).map_err(|error| format!("Failed to read video file metadata: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("Video path is not a file: {path}"));
        }
        ovrley_core::media::probe::probe_video_metadata(&self.paths, path).map_err(|error| error.to_string())
    }

    fn thumbnail(&self, path: &str) -> Option<Vec<u8>> {
        let ffmpeg = ovrley_core::encode::ffmpeg::resolve_ffmpeg_binary(&self.paths.repo_root).ok()?;
        ovrley_services::video::thumbnail(&ffmpeg, std::path::Path::new(path))
    }

    fn extract_telemetry(&self, path: &str) -> Result<Option<ovrley_core::activity::schema::ParsedActivity>, String> {
        ovrley_core::media::telemetry::extract_activity(&self.paths.repo_root, path).map_err(|error| error.to_string())
    }

    fn render_transition_proxy(&self, request: &TransitionProxyRequest) -> Result<String, String> {
        ovrley_core::encode::ffmpeg::transitions::render_transition_proxy(&self.paths, request)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| error.to_string())
    }

    fn remove_transition_proxy(&self, path: &str) -> Result<(), String> {
        ovrley_core::encode::ffmpeg::transitions::remove_transition_proxy(std::path::Path::new(path)).map_err(|error| error.to_string())
    }
}

/// The maps' data and renderer: the installed styles, the preview's frame
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
    fn tile_cache_usage(&self) -> Result<CacheUsage, String>;
    /// Applies and saves a new limit, evicting down to it.
    fn set_tile_cache_limit(&self, limit: u64) -> Result<CacheUsage, String>;
    fn clear_tile_cache(&self) -> Result<CacheUsage, String>;
    /// The map data renders draw route maps with; `None` in tests.
    fn map_source(&self) -> Option<ovrley_map::gpu::MapSource>;
}

/// The settings key of the map tile cache limit.
pub const MAP_TILE_CACHE_LIMIT_KEY: &str = "mapTileCacheLimitBytes";

/// The native maps over `MapData`, which export uses too.
pub struct NativeMapServices {
    data: ovrley_services::maps::MapData,
}

impl NativeMapServices {
    /// Reads the installed styles and opens the tile cache. The saved limit is a behaviour
    /// preference, so it stays strict: absent means the default, anything but a valid whole number
    /// of bytes fails.
    pub fn open(roots: &ovrley_services::runtime::RuntimeRoots, preferences: &PreferencesStore) -> Result<Self, String> {
        use ovrley_services::cache::CacheLimit;
        use ovrley_services::tiles::{TileCache, DEFAULT_TILE_CACHE_LIMIT};
        let limit = match preferences.get(MAP_TILE_CACHE_LIMIT_KEY) {
            None => DEFAULT_TILE_CACHE_LIMIT,
            Some(value) => CacheLimit::new(
                value.as_u64().ok_or_else(|| format!("Setting {MAP_TILE_CACHE_LIMIT_KEY} must be a whole number of bytes: {value}"))?,
            )?,
        };
        Ok(Self {
            data: ovrley_services::maps::MapData::new(
                ovrley_services::maps::MapStyleRegistry::load(&roots.map_style_dirs())?,
                TileCache::open(&roots.app_cache_dir, limit)?,
                Arc::new(ovrley_services::maps::GlyphService::open(&roots.map_font_dir())?),
            ),
        })
    }
}

impl MapServices for NativeMapServices {
    fn styles(&self) -> &MapStyleCatalog {
        self.data.registry().catalog()
    }

    fn frames(&self, wake: MapFrameWake) -> Result<Arc<dyn MapFrameFactory>, String> {
        Ok(Arc::new(ovrley_services::maps::PreviewMapFrames::start(self.data.clone(), wake)?))
    }

    fn probe_adapter(&self) -> MapAdapterSupport {
        ovrley_services::maps::probe_adapter()
    }

    fn prepare_tiles(&self, request: MapPrepareRequest, cancelled: &AtomicBool) -> Result<(), String> {
        ovrley_services::maps::prepare_map_tiles(request, &self.data, cancelled)
    }

    fn tile_cache_usage(&self) -> Result<CacheUsage, String> {
        Ok(self.data.tiles().usage())
    }

    fn set_tile_cache_limit(&self, limit: u64) -> Result<CacheUsage, String> {
        self.data.tiles().set_limit(ovrley_services::cache::CacheLimit::new(limit)?)
    }

    fn clear_tile_cache(&self) -> Result<CacheUsage, String> {
        self.data.tiles().clear()
    }

    fn map_source(&self) -> Option<ovrley_map::gpu::MapSource> {
        Some(self.data.map_source())
    }
}

/// The render services: one execution
/// service and one inspection service, called with typed values. Submission, inspection and the
/// preview frame block, so they run on workers; progress arrives through the mailbox.
pub trait RenderService: Send + Sync {
    fn submit_single(
        &self,
        resources: &ovrley_services::rasters::RasterResources,
        config: ovrley_core::config::RenderConfig,
        activity: Arc<ovrley_core::activity::schema::ParsedActivity>,
        output_path: &str,
        overwrite: bool,
    ) -> ovrley_core::error::CoreResult<RenderAccepted>;
    fn submit_batch(&self, resources: &ovrley_services::rasters::RasterResources, request: BatchRenderRequest) -> Result<BatchAcceptance, BatchServiceError>;
    /// Cancels the single render; whether one was active.
    fn cancel(&self) -> bool;
    fn cancel_batch(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError>;
    /// Whether the renderer is reserved: a render, its preparation or its cleanup runs.
    fn busy(&self) -> bool;
    /// The single render's latest progress, read once after acceptance: events published before
    /// the session started in the store were dropped as another render's.
    fn progress(&self) -> ovrley_core::encode::progress::RenderProgress;
    /// A batch's latest snapshot, read once after acceptance for the same reason.
    fn batch_snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError>;
    /// A new inspection session's id.
    fn create_inspection(&self) -> String;
    fn dispose_inspection(&self, inspection_id: &str);
    /// Inspects one source in the session: its native source id, or why it failed. Blocking.
    fn inspect_source(&self, inspection_id: &str, path: &str) -> Result<String, String>;
    /// The debug PNG frame at `second`, in Downloads. Blocking.
    fn render_preview_frame(
        &self,
        resources: &ovrley_services::rasters::RasterResources,
        config: ovrley_core::config::RenderConfig,
        activity: &ovrley_core::activity::schema::ParsedActivity,
        second: f64,
    ) -> Result<String, String>;
    /// A fresh file name in `directory`, else Downloads.
    fn suggest_output_path(&self, kind: ovrley_core::encode::settings::ExportMode, directory: Option<&str>) -> Result<String, String>;
    fn open_output_directory(&self, directory: Option<&str>) -> Result<(), String>;
    /// Opens a rendered file in the system player.
    fn open_file(&self, path: &str) -> Result<(), String>;
}

/// The native render services, publishing progress through a [`RenderMailbox`].
///
/// [`RenderMailbox`]: crate::features::render::session::RenderMailbox
pub struct NativeRenderService {
    paths: Arc<ovrley_core::paths::AppPaths>,
    execution: ovrley_core::jobs::execution::RenderExecutionService,
    inspection: ovrley_core::jobs::inspection::VideoInspectionService,
    map_source: Option<ovrley_map::gpu::MapSource>,
}

impl NativeRenderService {
    pub fn new(paths: Arc<ovrley_core::paths::AppPaths>, map_source: Option<ovrley_map::gpu::MapSource>, ui_post: Arc<dyn UiPost>) -> Self {
        let mailbox = crate::features::render::session::RenderMailbox::new(ui_post);
        Self {
            paths,
            execution: ovrley_core::jobs::execution::RenderExecutionService::with_sink(mailbox),
            inspection: ovrley_core::jobs::inspection::VideoInspectionService::default(),
            map_source,
        }
    }
}

impl RenderService for NativeRenderService {
    fn submit_single(
        &self,
        resources: &ovrley_services::rasters::RasterResources,
        config: ovrley_core::config::RenderConfig,
        activity: Arc<ovrley_core::activity::schema::ParsedActivity>,
        output_path: &str,
        overwrite: bool,
    ) -> ovrley_core::error::CoreResult<RenderAccepted> {
        self.execution.submit_single_config(&self.paths, config, activity, output_path, overwrite, Some(resources), self.map_source.clone())
    }

    fn submit_batch(&self, resources: &ovrley_services::rasters::RasterResources, request: BatchRenderRequest) -> Result<BatchAcceptance, BatchServiceError> {
        self.execution.submit_batch(&self.paths, &self.inspection, request, Some(resources), self.map_source.clone())
    }

    fn cancel(&self) -> bool {
        self.execution.cancel()
    }

    fn cancel_batch(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
        self.execution.cancel_batch(batch_id)
    }

    fn busy(&self) -> bool {
        self.execution.progress().busy
    }

    fn progress(&self) -> ovrley_core::encode::progress::RenderProgress {
        self.execution.progress()
    }

    fn batch_snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
        self.execution.batch_snapshot(batch_id)
    }

    fn create_inspection(&self) -> String {
        self.inspection.create_session().inspection_id
    }

    fn dispose_inspection(&self, inspection_id: &str) {
        self.inspection.dispose_session(inspection_id);
    }

    fn inspect_source(&self, inspection_id: &str, path: &str) -> Result<String, String> {
        self.inspection.inspect_source(&self.paths, inspection_id, path).map(|source| source.source_id).map_err(|error| error.to_string())
    }

    fn render_preview_frame(
        &self,
        resources: &ovrley_services::rasters::RasterResources,
        config: ovrley_core::config::RenderConfig,
        activity: &ovrley_core::activity::schema::ParsedActivity,
        second: f64,
    ) -> Result<String, String> {
        ovrley_core::render::render_preview_frame(&self.paths, config, activity, second, Some(resources), self.map_source.as_ref())
            .map(|path| path.display().to_string())
            .map_err(|error| error.to_string())
    }

    fn suggest_output_path(&self, kind: ovrley_core::encode::settings::ExportMode, directory: Option<&str>) -> Result<String, String> {
        ovrley_core::output::suggest_output_path(&self.paths, kind, directory.map(std::path::Path::new))
            .map(|path| path.display().to_string())
            .map_err(|error| error.to_string())
    }

    fn open_output_directory(&self, directory: Option<&str>) -> Result<(), String> {
        ovrley_core::output::open_output_directory(&self.paths, directory).map_err(|error| error.to_string())
    }

    fn open_file(&self, path: &str) -> Result<(), String> {
        ovrley_core::output::open_rendered_video(path).map_err(|error| error.to_string())
    }
}

/// An update the check found.
pub trait AppUpdate: std::fmt::Debug + Send + Sync {
    fn version(&self) -> &str;
    /// The verified package. Blocking.
    fn download(&self, on_progress: &dyn Fn(DownloadProgress)) -> Result<Vec<u8>, String>;
    /// Installs the package. Blocking. On Windows it ends the process.
    fn install(&self, package: Vec<u8>) -> Result<(), String>;
}

/// The release channel.
pub trait UpdateService: Send + Sync {
    /// The update to this version, `None` when it is the newest. Blocking.
    fn check(&self) -> Result<Option<Arc<dyn AppUpdate>>, String>;
}

/// `ovrley_services::updater`.
pub struct NativeUpdateService;

impl UpdateService for NativeUpdateService {
    fn check(&self) -> Result<Option<Arc<dyn AppUpdate>>, String> {
        let update = ovrley_services::updater::check(crate::features::shell::VERSION)?;
        Ok(update.map(|update| Arc::new(update) as Arc<dyn AppUpdate>))
    }
}

impl AppUpdate for ReleaseUpdate {
    fn version(&self) -> &str {
        self.version()
    }

    fn download(&self, on_progress: &dyn Fn(DownloadProgress)) -> Result<Vec<u8>, String> {
        self.download(on_progress)
    }

    fn install(&self, package: Vec<u8>) -> Result<(), String> {
        self.install(package)
    }
}

/// Everything an action may use besides the store.
pub struct Services {
    /// The runtime paths `ovrley_core` reads templates, fonts and tools from.
    pub paths: Arc<ovrley_core::paths::AppPaths>,
    /// The session's selected raster images, shared with render jobs.
    pub rasters: ovrley_services::rasters::RasterResources,
    pub preferences: PreferencesStore,
    pub dialogs: Box<dyn Dialogs>,
    pub workers: Workers,
    pub ui_post: Arc<dyn UiPost>,
    pub clipboard: Box<dyn Clipboard>,
    pub window: Box<dyn WindowControl>,
    pub video_tools: Arc<dyn VideoTools>,
    pub maps: Arc<dyn MapServices>,
    pub render: Arc<dyn RenderService>,
    pub updates: Arc<dyn UpdateService>,
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
    pub render: Arc<dyn RenderService>,
    pub updates: Arc<dyn UpdateService>,
}

impl ServiceParts {
    pub(super) fn into_services(self, dispatch: DispatchHandle) -> Services {
        Services {
            paths: self.paths,
            rasters: ovrley_services::rasters::RasterResources::default(),
            preferences: self.preferences,
            dialogs: self.dialogs,
            workers: Workers::new(self.ui_post.clone()),
            ui_post: self.ui_post,
            clipboard: self.clipboard,
            video_tools: self.video_tools,
            maps: self.maps,
            render: self.render,
            updates: self.updates,
            window: self.window,
            dispatch,
        }
    }
}

/// In-memory services that record their calls.
#[cfg(any(test, feature = "test-util"))]
pub mod fakes {
    use super::{FileDialogRequest, DialogReply, Dialogs, MapFrameFactory, UiPost, Clipboard, WindowControl, VideoTools, MapServices, RenderService, AppUpdate, UpdateService, Arc, SendAction, MapStyleCatalog, MapAdapterSupport, MapFrameWake, CacheUsage, MapPrepareRequest, AtomicBool, RenderAccepted, BatchAcceptance, BatchServiceError, BatchRenderRequest, BatchSnapshot, Filmstrip, FilmstripHwaccel, FilmstripRequest, TransitionProxyRequest, DownloadProgress, ServiceParts, PreferencesStore};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;
    use crate::preferences::MemorySink;
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
            self.answer_many(&selected.into_iter().collect::<Vec<_>>());
        }

        /// Answers the oldest open dialog with several paths.
        pub fn answer_many(&self, selected: &[&str]) {
            let (_, reply) = self.pending.borrow_mut().remove(0);
            reply(selected.iter().map(|path| path.to_string()).collect());
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
        pub render: Arc<FakeRenderService>,
        pub updates: Arc<FakeUpdateService>,
    }

    impl Fakes {
        /// Runs the posted worker results until `done` holds; panics after five seconds.
        pub fn settle(&self, dispatcher: &super::super::Dispatcher, done: impl Fn(&super::super::Store) -> bool) {
            for _ in 0..1000 {
                self.ui_post.drain(dispatcher);
                if dispatcher.with_store(&done) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("the store did not reach the expected state");
        }
    }

    /// Maps over the checkout's bundled styles, with frames from a test's factory, a fixed adapter
    /// and an in-memory tile cache. Every call is recorded.
    pub struct FakeMapServices {
        pub styles: MapStyleCatalog,
        pub adapter: Mutex<MapAdapterSupport>,
        pub frames: Mutex<Arc<dyn MapFrameFactory>>,
        pub wake: Mutex<Option<MapFrameWake>>,
        pub prepared: Mutex<Vec<usize>>,
        pub usage: Mutex<CacheUsage>,
        pub calls: Mutex<Vec<String>>,
    }

    impl Default for FakeMapServices {
        fn default() -> Self {
            let paths = repository_paths();
            let dirs = ovrley_services::maps::MapStyleDirs {
                bundled: paths.repo_root.join("assets").join("map-styles"),
                user: std::env::temp_dir().join(format!("ovrley-test-map-styles-{}", std::process::id())),
            };
            let registry = ovrley_services::maps::MapStyleRegistry::load(&dirs).expect("the bundled map styles install");
            Self {
                styles: registry.catalog().clone(),
                adapter: Mutex::new(MapAdapterSupport::Hardware),
                frames: Mutex::new(Arc::new(NoFrames)),
                wake: Mutex::new(None),
                prepared: Mutex::new(Vec::new()),
                usage: Mutex::new(CacheUsage { used_bytes: 0, limit_bytes: 512 << 20, min_limit_bytes: 128 << 20, max_limit_bytes: 40 * (128 << 20), limit_step_bytes: 128 << 20 }),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    /// Frames that never draw: each fails at once, so a map shows its placeholder.
    pub struct NoFrames;

    impl MapFrameFactory for NoFrames {
        fn provider(&self, _style: &ovrley_services::maps::MapStyleInfo, _sample_count: u32) -> Arc<dyn ovrley_widgets::widgets::map::preview::MapFrameProvider> {
            Arc::new(NoFrames)
        }
    }

    impl ovrley_widgets::widgets::map::preview::MapFrameProvider for NoFrames {
        fn frame(
            &self,
            _request: &ovrley_widgets::widgets::map::preview::MapFrameRequest,
            _spare: Option<Vec<u8>>,
        ) -> ovrley_widgets::WidgetResult<ovrley_widgets::widgets::map::preview::MapFrameReply> {
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

        fn tile_cache_usage(&self) -> Result<CacheUsage, String> {
            self.calls.lock().unwrap().push("usage".into());
            Ok(*self.usage.lock().unwrap())
        }

        fn set_tile_cache_limit(&self, limit: u64) -> Result<CacheUsage, String> {
            self.calls.lock().unwrap().push(format!("limit {limit}"));
            let mut usage = self.usage.lock().unwrap();
            usage.limit_bytes = ovrley_services::cache::CacheLimit::new(limit)?.bytes();
            Ok(*usage)
        }

        fn clear_tile_cache(&self) -> Result<CacheUsage, String> {
            self.calls.lock().unwrap().push("clear".into());
            let mut usage = self.usage.lock().unwrap();
            usage.used_bytes = 0;
            Ok(*usage)
        }

        fn map_source(&self) -> Option<ovrley_map::gpu::MapSource> {
            None
        }
    }

    /// A call the fake render service received.
    #[derive(Clone, Debug, PartialEq)]
    pub enum FakeRenderCall {
        SubmitSingle { output_path: String, overwrite: bool },
        SubmitBatch { inspection_id: String, outputs: usize },
        Cancel,
        CancelBatch(String),
        CreateInspection(String),
        DisposeInspection(String),
        Inspect(String),
        PreviewFrame { second: f64 },
        OpenDirectory(Option<String>),
        OpenFile(String),
    }

    /// Records each call and answers from its queues; the test sends progress through `mailbox`,
    /// as core would.
    pub struct FakeRenderService {
        pub mailbox: Arc<crate::features::render::session::RenderMailbox>,
        pub calls: Mutex<Vec<FakeRenderCall>>,
        /// The single submissions' answers; an empty queue accepts with the next render id.
        pub single_results: Mutex<std::collections::VecDeque<ovrley_core::error::CoreResult<RenderAccepted>>>,
        pub batch_results: Mutex<std::collections::VecDeque<Result<BatchAcceptance, BatchServiceError>>>,
        /// The submitted single configs and batch requests, newest last.
        pub singles: Mutex<Vec<ovrley_core::config::RenderConfig>>,
        pub batches: Mutex<Vec<BatchRenderRequest>>,
        /// Inspections by path; an unknown path fails.
        pub sources: Mutex<std::collections::HashMap<String, Result<String, String>>>,
        pub cancel_snapshot: Mutex<Option<BatchSnapshot>>,
        pub busy: std::sync::atomic::AtomicBool,
        /// What `progress` and `batch_snapshot` read.
        pub progress: Mutex<ovrley_core::encode::progress::RenderProgress>,
        pub batch_snapshots: Mutex<std::collections::HashMap<String, BatchSnapshot>>,
        next_id: std::sync::atomic::AtomicU64,
    }

    impl FakeRenderService {
        pub fn new(ui_post: Arc<dyn UiPost>) -> Self {
            Self {
                mailbox: crate::features::render::session::RenderMailbox::new(ui_post),
                calls: Mutex::default(),
                single_results: Mutex::default(),
                batch_results: Mutex::default(),
                singles: Mutex::default(),
                batches: Mutex::default(),
                sources: Mutex::default(),
                cancel_snapshot: Mutex::default(),
                busy: Default::default(),
                progress: Mutex::default(),
                batch_snapshots: Mutex::default(),
                next_id: std::sync::atomic::AtomicU64::new(1),
            }
        }

        pub fn calls(&self) -> Vec<FakeRenderCall> {
            self.calls.lock().unwrap().clone()
        }

        fn record(&self, call: FakeRenderCall) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl RenderService for FakeRenderService {
        fn submit_single(
            &self,
            _resources: &ovrley_services::rasters::RasterResources,
            config: ovrley_core::config::RenderConfig,
            _activity: Arc<ovrley_core::activity::schema::ParsedActivity>,
            output_path: &str,
            overwrite: bool,
        ) -> ovrley_core::error::CoreResult<RenderAccepted> {
            use std::sync::atomic::Ordering;
            self.record(FakeRenderCall::SubmitSingle { output_path: output_path.into(), overwrite });
            self.singles.lock().unwrap().push(config);
            self.single_results.lock().unwrap().pop_front().unwrap_or_else(|| {
                Ok(RenderAccepted { started: true, render_id: self.next_id.fetch_add(1, Ordering::SeqCst), output_path: output_path.into() })
            })
        }

        fn submit_batch(&self, _resources: &ovrley_services::rasters::RasterResources, request: BatchRenderRequest) -> Result<BatchAcceptance, BatchServiceError> {
            self.record(FakeRenderCall::SubmitBatch { inspection_id: request.inspection_id.clone(), outputs: request.outputs.len() });
            self.batches.lock().unwrap().push(request);
            self.batch_results.lock().unwrap().pop_front().unwrap_or_else(|| Err(BatchServiceError::DispatchFailed { message: "no batch answer queued".into() }))
        }

        fn cancel(&self) -> bool {
            self.record(FakeRenderCall::Cancel);
            true
        }

        fn cancel_batch(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
            self.record(FakeRenderCall::CancelBatch(batch_id.into()));
            self.cancel_snapshot.lock().unwrap().clone().ok_or_else(|| BatchServiceError::UnknownBatch { message: batch_id.into() })
        }

        fn busy(&self) -> bool {
            self.busy.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn progress(&self) -> ovrley_core::encode::progress::RenderProgress {
            self.progress.lock().unwrap().clone()
        }

        fn batch_snapshot(&self, batch_id: &str) -> Result<BatchSnapshot, BatchServiceError> {
            self.batch_snapshots.lock().unwrap().get(batch_id).cloned().ok_or_else(|| BatchServiceError::UnknownBatch { message: batch_id.into() })
        }

        fn create_inspection(&self) -> String {
            use std::sync::atomic::Ordering;
            let id = format!("inspection-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
            self.record(FakeRenderCall::CreateInspection(id.clone()));
            id
        }

        fn dispose_inspection(&self, inspection_id: &str) {
            self.record(FakeRenderCall::DisposeInspection(inspection_id.into()));
        }

        fn inspect_source(&self, _inspection_id: &str, path: &str) -> Result<String, String> {
            self.record(FakeRenderCall::Inspect(path.into()));
            self.sources.lock().unwrap().get(path).cloned().unwrap_or_else(|| Err(format!("Video path is not a file: {path}")))
        }

        fn render_preview_frame(
            &self,
            _resources: &ovrley_services::rasters::RasterResources,
            _config: ovrley_core::config::RenderConfig,
            _activity: &ovrley_core::activity::schema::ParsedActivity,
            second: f64,
        ) -> Result<String, String> {
            self.record(FakeRenderCall::PreviewFrame { second });
            Ok("/downloads/preview_frame.png".into())
        }

        fn suggest_output_path(&self, kind: ovrley_core::encode::settings::ExportMode, directory: Option<&str>) -> Result<String, String> {
            Ok(format!("{}/{}_1.{}", directory.unwrap_or("/downloads"), kind.filename_prefix(), kind.extension()))
        }

        fn open_output_directory(&self, directory: Option<&str>) -> Result<(), String> {
            self.record(FakeRenderCall::OpenDirectory(directory.map(str::to_string)));
            Ok(())
        }

        fn open_file(&self, path: &str) -> Result<(), String> {
            self.record(FakeRenderCall::OpenFile(path.into()));
            Ok(())
        }
    }

    /// Video tools answering from a table: a strip per window key (cached or decoded), the
    /// codecs, a probe per path (an unknown path is not a file), thumbnails and telemetry. The
    /// thumbnails in flight are counted, and telemetry extractions recorded in order.
    #[derive(Default)]
    pub struct FakeVideoTools {
        pub cached: Mutex<std::collections::HashMap<String, Filmstrip>>,
        pub decoded: Mutex<std::collections::HashMap<String, Filmstrip>>,
        pub decodes: Mutex<Vec<(String, Option<FilmstripHwaccel>)>>,
        pub codecs: Mutex<ovrley_core::encode::ffmpeg::detect::AvailableCodecs>,
        pub probes: Mutex<std::collections::HashMap<String, Result<ovrley_core::media::probe::SourceVideoMetadata, String>>>,
        pub thumbnails: Mutex<std::collections::HashMap<String, Vec<u8>>>,
        /// How long a thumbnail takes, so a test sees them overlap.
        pub thumbnail_delay: Mutex<std::time::Duration>,
        pub thumbnails_running: std::sync::atomic::AtomicUsize,
        pub thumbnails_peak: std::sync::atomic::AtomicUsize,
        pub telemetry: Mutex<std::collections::HashMap<String, Result<Option<ovrley_core::activity::schema::ParsedActivity>, String>>>,
        pub telemetry_calls: Mutex<Vec<String>>,
    }

    impl FakeVideoTools {
        /// A probe of `path` that succeeds.
        pub fn add_video(&self, path: &str, metadata: ovrley_core::media::probe::SourceVideoMetadata) {
            self.probes.lock().unwrap().insert(path.to_string(), Ok(metadata));
        }
    }

    /// A strip's table key: its window as `crate::features::player::filmstrips` keys it.
    fn key(request: &FilmstripRequest) -> String {
        crate::features::player::filmstrips::request_key(request)
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

        fn probe(&self, path: &str) -> Result<ovrley_core::media::probe::SourceVideoMetadata, String> {
            self.probes.lock().unwrap().get(path).cloned().unwrap_or_else(|| Err(format!("Video path is not a file: {path}")))
        }

        fn thumbnail(&self, path: &str) -> Option<Vec<u8>> {
            use std::sync::atomic::Ordering;
            let running = self.thumbnails_running.fetch_add(1, Ordering::SeqCst) + 1;
            self.thumbnails_peak.fetch_max(running, Ordering::SeqCst);
            let delay = *self.thumbnail_delay.lock().unwrap();
            std::thread::sleep(delay);
            self.thumbnails_running.fetch_sub(1, Ordering::SeqCst);
            self.thumbnails.lock().unwrap().get(path).cloned()
        }

        fn extract_telemetry(&self, path: &str) -> Result<Option<ovrley_core::activity::schema::ParsedActivity>, String> {
            self.telemetry_calls.lock().unwrap().push(path.to_string());
            self.telemetry.lock().unwrap().get(path).cloned().unwrap_or(Ok(None))
        }

        fn render_transition_proxy(&self, request: &TransitionProxyRequest) -> Result<String, String> {
            Ok(format!("/proxies/{}-{}.mp4", request.transition, request.outgoing.start_second))
        }

        fn remove_transition_proxy(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }
    }

    /// Offers at most one update, and records the check and whether the preferences were flushed
    /// before the install.
    #[derive(Default)]
    pub struct FakeUpdateService {
        offered: Mutex<Option<FakeUpdate>>,
        pub checks: std::sync::atomic::AtomicUsize,
        pub flushed_before_install: Arc<AtomicBool>,
        flushes: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeUpdateService {
        pub fn offer(&self, update: FakeUpdate) {
            *self.offered.lock().unwrap() = Some(FakeUpdate {
                flushes: self.flushes.clone(),
                flushed_before_install: self.flushed_before_install.clone(),
                ..update
            });
        }
    }

    impl UpdateService for FakeUpdateService {
        fn check(&self) -> Result<Option<Arc<dyn AppUpdate>>, String> {
            self.checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.offered.lock().unwrap().clone().map(|update| Arc::new(update) as Arc<dyn AppUpdate>))
        }
    }

    /// An update that downloads and installs at once, or fails as told.
    #[derive(Clone, Debug)]
    pub struct FakeUpdate {
        pub version: String,
        pub download_error: Option<String>,
        pub install_error: Option<String>,
        pub flushes: Arc<std::sync::atomic::AtomicUsize>,
        pub flushed_before_install: Arc<AtomicBool>,
    }

    impl FakeUpdate {
        pub fn new(version: &str) -> Self {
            Self {
                version: version.into(),
                download_error: None,
                install_error: None,
                flushes: Arc::default(),
                flushed_before_install: Arc::default(),
            }
        }
    }

    impl AppUpdate for FakeUpdate {
        fn version(&self) -> &str {
            &self.version
        }

        fn download(&self, on_progress: &dyn Fn(DownloadProgress)) -> Result<Vec<u8>, String> {
            on_progress(DownloadProgress { downloaded: 1, content_length: Some(1) });
            self.download_error.clone().map_or(Ok(vec![0]), Err)
        }

        fn install(&self, _package: Vec<u8>) -> Result<(), String> {
            use std::sync::atomic::Ordering;
            self.flushed_before_install.store(self.flushes.load(Ordering::SeqCst) > 0, Ordering::SeqCst);
            self.install_error.clone().map_or(Ok(()), Err)
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
        let ui_post = FakeUiPost::default();
        let fakes = Fakes {
            updates: Arc::new(FakeUpdateService { flushes: sink.flushes.clone(), ..FakeUpdateService::default() }),
            preferences: sink,
            dialogs: FakeDialogs::default(),
            ui_post: ui_post.clone(),
            clipboard: FakeClipboard::default(),
            window: FakeWindow::default(),
            video_tools: Arc::new(FakeVideoTools::default()),
            maps: Arc::new(FakeMapServices::default()),
            render: Arc::new(FakeRenderService::new(Arc::new(ui_post.clone()))),
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
            render: fakes.render.clone(),
            updates: fakes.updates.clone(),
        };
        (parts, fakes)
    }
}
