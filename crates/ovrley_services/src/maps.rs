//! The maps: the installed styles, [`MapData`] (everything a map's bytes come from), the tile
//! download that prepares a project's maps, and the editor preview's map frames.
//!
//! At startup every `*.json` in the build's `map-styles` folder, then in the user's
//! `Documents/OVRLEY/map-styles`, is read with [`read_style_file`], its id the file name in
//! lowercase. Every bundled style must install, the one a new map widget draws among them, or the
//! app does not start: they ship with it. A user style that doesn't install is logged and left out,
//! and so is one whose id is already installed, so a bundled style always means the same map.
//! Nothing is read again until the next start.
//!
//! The default style lists first, so it is also what a widget naming a missing style draws. The rest
//! list by label, bundled before the user's.

/// The map types the app reaches through this crate, so it needs no map dependency.
pub use ovrley_map::gpu::{labels::GlyphService, probe_adapter, MapAdapterSupport};
pub use ovrley_map::{MapStyleCatalog, MapStyleInfo};

use crate::tiles::{TileCache, TileCoordinates};
use ovrley_core::render::download_preview_tiles;
use ovrley_core::CoreError;
use ovrley_map::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes};
use ovrley_map::{read_style_file, MapDataSource, MapSource, MapStyleBuildings, MapStyleId};
use ovrley_widgets::activity::ActivitySampler;
use ovrley_widgets::error::WidgetResult;
use ovrley_widgets::host::WidgetHost;
use ovrley_widgets::model::scene::{validate_scene_presentation, ScenePresentationConfig};
use ovrley_widgets::model::standard_widgets::default_map_style;
use ovrley_widgets::render::preview::PreviewRasterScale;
use ovrley_widgets::widgets::map::config::MapConfig;
use ovrley_widgets::widgets::map::preview::{
    MapFrameFactory, MapFramePixels, MapFrameProvider, MapFrameReply, MapFrameRequest,
};
use ovrley_widgets::widgets::map::validation::validate_map_widget;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Where installed styles are read from.
#[derive(Clone, Debug)]
pub struct MapStyleDirs {
    /// Shipped with the build. It must exist.
    pub bundled: PathBuf,
    /// The user's own. It is created when missing, so there is somewhere to put them.
    pub user: PathBuf,
}

/// The installed styles: their catalog, and each one's document.
#[derive(Debug)]
pub struct MapStyleRegistry {
    catalog: MapStyleCatalog,
    /// Each installed style's document as JSON bytes.
    documents: HashMap<MapStyleId, Vec<u8>>,
}

impl MapStyleRegistry {
    /// Reads the installed styles.
    ///
    /// # Errors
    ///
    /// When a bundled style can't be read or doesn't install, or the default isn't bundled.
    pub fn load(dirs: &MapStyleDirs) -> Result<Self, String> {
        let mut documents = HashMap::new();
        let mut bundled = Vec::new();
        for path in style_files(&dirs.bundled)? {
            let (info, document) = read_style(&path)
                .map_err(|reason| format!("Bundled map style {} does not install: {reason}", path.display()))?;
            documents.insert(info.id.clone(), document);
            bundled.push(info);
        }

        if let Err(error) = fs::create_dir_all(&dirs.user) {
            log::warn!("Map styles in {} are not read: {error}", dirs.user.display());
        }
        let user_files = style_files(&dirs.user).unwrap_or_else(|error| {
            log::warn!("{error}");
            Vec::new()
        });
        let mut user = Vec::new();
        for path in user_files {
            match read_style(&path) {
                Err(reason) => log::warn!("Map style {} is not installed: {reason}", path.display()),
                Ok((info, _)) if documents.contains_key(&info.id) => {
                    log::warn!("Map style {} is not installed: {} is already installed", path.display(), info.id);
                }
                Ok((info, document)) => {
                    documents.insert(info.id.clone(), document);
                    user.push(info);
                }
            }
        }

        let by_label =
            |a: &MapStyleInfo, b: &MapStyleInfo| a.label.cmp(&b.label).then_with(|| a.id.as_str().cmp(b.id.as_str()));
        bundled.sort_by(by_label);
        user.sort_by(by_label);
        let default = bundled
            .iter()
            .position(|style| &style.id == default_map_style())
            .ok_or_else(|| format!("The build has no {} map style, which new map widgets draw", default_map_style()))?;
        let default = bundled.remove(default);
        Ok(Self {
            // Two bundled files may differ only in case: then they share an id, and this fails.
            catalog: MapStyleCatalog::new(std::iter::once(default).chain(bundled).chain(user).collect())?,
            documents,
        })
    }

    #[must_use]
    pub fn catalog(&self) -> &MapStyleCatalog {
        &self.catalog
    }
}

/// Every `*.json` in `dir`, by file name.
fn style_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        fs::read_dir(dir).map_err(|error| format!("Map styles in {} are not readable: {error}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")))
        .collect();
    files.sort();
    Ok(files)
}

/// The style in `path` with its document, or why it doesn't install.
fn read_style(path: &Path) -> Result<(MapStyleInfo, Vec<u8>), String> {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).ok_or("its file name is not UTF-8")?;
    let id = MapStyleId::new(&stem.to_ascii_lowercase())?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let (info, document) = read_style_file(id, &bytes).map_err(|errors| errors.join("; "))?;
    Ok((info, serde_json::to_vec(&document).expect("a JSON document serializes")))
}

/// Everything a map's bytes come from: the installed styles, the tile cache and the bundled fonts'
/// glyphs. Clones share all three.
#[derive(Clone)]
pub struct MapData {
    styles: Arc<MapStyleRegistry>,
    tiles: TileCache,
    glyphs: Arc<GlyphService>,
}

impl MapData {
    #[must_use]
    pub fn new(styles: MapStyleRegistry, tiles: TileCache, glyphs: Arc<GlyphService>) -> Self {
        Self { styles: Arc::new(styles), tiles, glyphs }
    }

    #[must_use]
    pub fn registry(&self) -> &MapStyleRegistry {
        &self.styles
    }

    #[must_use]
    pub fn tiles(&self) -> &TileCache {
        &self.tiles
    }

    /// What a render's maps draw from.
    #[must_use]
    pub fn map_source(&self) -> MapSource {
        MapSource { styles: self.styles.catalog.clone(), data: Arc::new(self.clone()) }
    }
}

impl MapDataSource for MapData {
    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String> {
        self.styles.documents.get(style).cloned().ok_or_else(|| format!("Map style {style} is not installed"))
    }

    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String> {
        self.tiles.resolve(TileCoordinates { zoom: z, x, y }).map(|payload| payload.bytes)
    }

    fn glyphs(&self) -> Arc<GlyphService> {
        self.glyphs.clone()
    }
}

/// The tiles of a map route being downloaded. A newer download cancels it.
#[derive(Debug, Default)]
pub struct MapDownload(Mutex<Option<Arc<AtomicBool>>>);

impl MapDownload {
    /// Cancels the running download, if any, and returns the cancellation flag of a new one.
    pub fn restart(&self) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.0.lock().expect("map download lock poisoned").replace(cancelled.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        cancelled
    }
}

/// What to download tiles for: the serialized activity, and the map widgets on the scene at the
/// preview's raster scale.
#[derive(Debug)]
pub struct MapPrepareRequest {
    pub activity: Vec<u8>,
    pub widgets: Vec<MapConfig>,
    pub scene: ScenePresentationConfig,
    pub raster_scale: f32,
}

/// Downloads the tiles a project's maps will show over the activity, so scrubbing mostly hits the
/// disk cache. Blocking; the preview fetches what it needs either way.
///
/// # Errors
///
/// When the request is invalid or a tile cannot be fetched. A cancelled download ends with `Ok`.
pub fn prepare_map_tiles(request: MapPrepareRequest, source: &MapData, cancelled: &AtomicBool) -> Result<(), String> {
    match download_map_tiles(request, source, cancelled) {
        Ok(()) | Err(CoreError::Cancelled) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn download_map_tiles(request: MapPrepareRequest, source: &MapData, cancelled: &AtomicBool) -> Result<(), CoreError> {
    let MapPrepareRequest { activity, widgets, scene, raster_scale } = request;
    let activity = ActivitySampler::from_json(&activity)?;
    // The preview's density sets the map's pixel ratio and so the tile zoom it needs.
    let host = PreviewRasterScale::new(raster_scale).and_then(|raster_scale| WidgetHost::from_scene(&scene, raster_scale))?;
    let scene = validate_scene_presentation(scene)?;
    let widgets = widgets
        .iter()
        .enumerate()
        .map(|(index, widget)| validate_map_widget(widget, index))
        .collect::<Result<Vec<_>, _>>()?;
    download_preview_tiles(&widgets, &scene, &host, &activity, source, cancelled, &|_, _| {})
}

/// A run of zoom edits draws on the current renderer, and only the zoom it settles on is rebuilt,
/// because a renderer's tiles load against its style zoom.
const STYLE_ZOOM_SETTLE: Duration = Duration::from_millis(300);

/// Wakes the editor preview after its maps' frames changed.
pub type MapFrameWake = Arc<dyn Fn() + Send + Sync>;

/// The editor preview's map frames: one thread, which owns a renderer per map on its own device,
/// as export's frame thread does. Unlike export it loads each view's tiles through [`MapData`] when
/// a frame needs them, from the disk cache or the network, and nothing it does fails the preview: a
/// failed frame is logged and answered with its error, and the map shows its placeholder.
///
/// After every frame, and after a renderer rebuilt for a settled zoom, the thread calls `wake`.
pub struct PreviewMapFrames {
    sender: mpsc::Sender<FrameJob>,
    next_slot: AtomicU64,
}

enum FrameJob {
    Open { slot: u64, buildings: MapStyleBuildings, sample_count: u32, generation: Arc<AtomicU64> },
    Frame { slot: u64, request: Box<MapFrameRequest>, reply: mpsc::Sender<Result<MapFramePixels, String>> },
    Release { slot: u64 },
}

impl PreviewMapFrames {
    /// Starts the frame thread.
    ///
    /// # Errors
    ///
    /// When the thread cannot be spawned.
    pub fn start(data: MapData, wake: MapFrameWake) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("preview-map-frames".into())
            .spawn(move || run_map_frames(&receiver, &data, &*wake))
            .map_err(|error| format!("Could not start the map frame thread: {error}"))?;
        Ok(Self { sender, next_slot: AtomicU64::new(0) })
    }
}

impl MapFrameFactory for PreviewMapFrames {
    fn provider(&self, style: &MapStyleInfo, sample_count: u32) -> Arc<dyn MapFrameProvider> {
        let slot = self.next_slot.fetch_add(1, Ordering::Relaxed);
        let generation = Arc::new(AtomicU64::new(0));
        let sender = self.sender.clone();
        // A stopped thread answers every frame with its error, through the disconnected reply.
        let _ = sender.send(FrameJob::Open { slot, buildings: style.buildings.clone(), sample_count, generation: generation.clone() });
        Arc::new(SlotFrames { slot, sender, generation })
    }
}

/// One map's view of the frame thread.
struct SlotFrames {
    slot: u64,
    sender: mpsc::Sender<FrameJob>,
    generation: Arc<AtomicU64>,
}

impl MapFrameProvider for SlotFrames {
    fn frame(&self, request: &MapFrameRequest, _spare: Option<Vec<u8>>) -> WidgetResult<MapFrameReply> {
        let (reply, answer) = mpsc::channel();
        // A stopped thread drops the reply, which the widget reads as a failed frame.
        let _ = self.sender.send(FrameJob::Frame { slot: self.slot, request: Box::new(request.clone()), reply });
        Ok(answer)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

impl Drop for SlotFrames {
    fn drop(&mut self) {
        let _ = self.sender.send(FrameJob::Release { slot: self.slot });
    }
}

/// A map's renderer on the frame thread, and what it was built for.
struct SlotRenderer {
    buildings: MapStyleBuildings,
    sample_count: u32,
    generation: Arc<AtomicU64>,
    renderer: Option<(GpuMapRenderer, GpuPoolSizes)>,
    /// The last frame's request, which a settled zoom draws again.
    last: Option<MapFrameRequest>,
    /// A zoom the renderer is not built for, and when it will have settled.
    settling: Option<(f64, Instant)>,
}

impl SlotRenderer {
    fn config(&self, request: &MapFrameRequest, style_zoom: f64) -> GpuMapConfig {
        GpuMapConfig {
            spec: request.spec.clone(),
            viewport: request.viewport,
            sample_count: self.sample_count,
            style_zoom,
            buildings: self.buildings.clone(),
        }
    }

    /// Builds the renderer anew with `pools`, which the next frame fills.
    fn rebuild(&mut self, config: GpuMapConfig, pools: GpuPoolSizes, data: &MapData) -> Result<(), String> {
        self.renderer = None;
        let renderer = GpuMapRenderer::new(config, pools, data)?;
        log::info!("Preview map renderer on {}, pools {} bytes", renderer.adapter_name(), renderer.pool_bytes());
        self.renderer = Some((renderer, pools));
        self.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    /// A renderer that can draw `request`: built at its zoom when there is none, or when its style,
    /// layers or pixel density changed; resized when only its size did. A zoom edit draws on the
    /// current renderer and starts the settle.
    fn prepare(&mut self, request: &MapFrameRequest, data: &MapData) -> Result<(), String> {
        let zoom = request.camera.zoom();
        let Some((renderer, _)) = self.renderer.as_mut().filter(|(renderer, _)| {
            let config = renderer.config();
            config.spec == request.spec && config.viewport.pixel_ratio() == request.viewport.pixel_ratio()
        }) else {
            self.settling = None;
            return self.rebuild(self.config(request, zoom), GpuPoolSizes::DEFAULT, data);
        };
        if renderer.config().viewport != request.viewport {
            renderer.resize(request.viewport.width_px(), request.viewport.height_px())?;
        }
        if renderer.config().style_zoom == zoom {
            self.settling = None;
        } else if self.settling.is_none_or(|(settling, _)| settling != zoom) {
            self.settling = Some((zoom, Instant::now() + STYLE_ZOOM_SETTLE));
        }
        Ok(())
    }

    /// Draws `request`, growing the pools while its view does not fit them.
    fn frame(&mut self, request: &MapFrameRequest, data: &MapData) -> Result<MapFramePixels, String> {
        self.prepare(request, data)?;
        let tiles = request.tiles();
        let started = Instant::now();
        loop {
            let (renderer, pools) = self.renderer.as_mut().expect("prepare built a renderer");
            if renderer.try_load_view(&tiles, data)? {
                break;
            }
            let (config, pools) = (renderer.config().clone(), pools.doubled());
            log::info!("A preview map view of {} tiles outgrew its pools; doubling them", tiles.len());
            self.rebuild(config, pools, data)?;
        }
        let loaded = started.elapsed();
        let (renderer, _) = self.renderer.as_mut().expect("prepare built a renderer");
        let mut pixels = Vec::new();
        renderer.render(&request.camera, request.labels.as_ref(), request.route.as_ref(), request.corner_radius_px, &mut pixels)?;
        log::debug!(
            "Preview map frame: load {:.1} ms, render {:.1} ms",
            loaded.as_secs_f64() * 1000.0,
            (started.elapsed() - loaded).as_secs_f64() * 1000.0
        );
        self.last = Some(request.clone());
        MapFramePixels::new(request.viewport.width_px(), request.viewport.height_px(), pixels).map_err(|error| error.to_string())
    }

    /// Rebuilds at the settled zoom, keeping the pools' size.
    fn settle(&mut self, data: &MapData) -> Result<(), String> {
        let (Some((zoom, _)), Some(request), Some(&(_, pools))) = (self.settling.take(), self.last.clone(), self.renderer.as_ref())
        else {
            return Ok(());
        };
        self.rebuild(self.config(&request, zoom), pools, data)
    }
}

fn run_map_frames(receiver: &mpsc::Receiver<FrameJob>, data: &MapData, wake: &(dyn Fn() + Send + Sync)) {
    let mut slots: HashMap<u64, SlotRenderer> = HashMap::new();
    loop {
        let deadline = slots.values().filter_map(|slot| slot.settling.map(|(_, at)| at)).min();
        let job = match deadline {
            Some(at) => match receiver.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(job) => Some(job),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            },
            None => match receiver.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            },
        };
        match job {
            Some(FrameJob::Open { slot, buildings, sample_count, generation }) => {
                slots.insert(slot, SlotRenderer { buildings, sample_count, generation, renderer: None, last: None, settling: None });
            }
            Some(FrameJob::Release { slot }) => {
                slots.remove(&slot);
            }
            Some(FrameJob::Frame { slot, request, reply }) => {
                let result = match slots.get_mut(&slot) {
                    Some(renderer) => renderer.frame(&request, data),
                    None => Err("The map's renderer was released".into()),
                };
                if let Err(error) = &result {
                    log::warn!("A preview map frame failed: {error}");
                }
                // A widget that stopped waiting has moved on.
                let _ = reply.send(result);
                wake();
            }
            None => {
                let now = Instant::now();
                for slot in slots.values_mut().filter(|slot| slot.settling.is_some_and(|(_, at)| at <= now)) {
                    if let Err(error) = slot.settle(data) {
                        log::warn!("A preview map renderer could not rebuild: {error}");
                    }
                }
                wake();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value as Json};
    use uuid::Uuid;

    fn bundled_dir() -> PathBuf {
        crate::runtime::source_repo_root().join("assets").join("map-styles")
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ovrley-map-styles-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn style(name: Option<&str>) -> Json {
        let mut style = json!({
            "version": 8,
            "sources": { "openmaptiles": { "type": "vector", "url": "https://example.com/tiles.json" } },
            "layers": [{ "id": "bg", "type": "background", "paint": { "background-color": "#102030" } }],
        });
        if let Some(name) = name {
            style["name"] = json!(name);
        }
        style
    }

    fn write(dir: &Path, file: &str, style: &Json) {
        fs::write(dir.join(file), serde_json::to_vec(style).unwrap()).unwrap();
    }

    fn load(bundled: &Path, user: &Path) -> Result<MapStyleRegistry, String> {
        MapStyleRegistry::load(&MapStyleDirs { bundled: bundled.to_path_buf(), user: user.to_path_buf() })
    }

    fn ids(registry: &MapStyleRegistry) -> Vec<&str> {
        registry.catalog().styles().iter().map(|style| style.id.as_str()).collect()
    }

    #[test]
    fn the_bundled_styles_install_with_the_default_first() {
        let user = temp_dir("bundled");
        let registry = load(&bundled_dir(), &user).unwrap();
        assert_eq!(default_map_style().as_str(), "liberty");
        assert_eq!(ids(&registry), ["liberty", "bright", "dark", "fiord", "positron"]);
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn user_styles_install_after_the_bundled_ones_unless_they_fail_or_collide() {
        let (bundled, user) = (temp_dir("ordered-bundled"), temp_dir("ordered-user"));
        write(&bundled, "zulu.json", &style(Some("Alpha")));
        write(&bundled, "liberty.json", &style(Some("Liberty")));
        write(&user, "Beta.json", &style(None));
        write(&user, "Liberty.json", &style(Some("Mine")));
        write(&user, "my style.json", &style(None));
        write(&user, "broken.json", &json!({ "version": 8 }));
        fs::write(user.join("notes.txt"), "not a style").unwrap();

        let registry = load(&bundled, &user).unwrap();
        assert_eq!(ids(&registry), ["liberty", "zulu", "beta"]);
        assert_eq!(registry.catalog().styles()[2].label, "beta");
        assert!(registry.catalog().styles().iter().all(|style| style.background == [0x10, 0x20, 0x30]));
        fs::remove_dir_all(bundled).unwrap();
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn a_bundled_style_that_does_not_install_or_a_missing_default_stops_the_load() {
        let (bundled, user) = (temp_dir("strict-bundled"), temp_dir("strict-user"));
        write(&bundled, "zulu.json", &style(None));
        assert!(load(&bundled, &user).err().unwrap().contains("no liberty map style"));

        write(&bundled, "liberty.json", &style(None));
        write(&bundled, "broken.json", &json!({ "version": 8 }));
        assert!(load(&bundled, &user).err().unwrap().contains("broken.json does not install"));
        fs::remove_dir_all(bundled).unwrap();
        fs::remove_dir_all(user).unwrap();
    }

    #[test]
    fn a_missing_user_folder_is_created_and_a_missing_bundled_one_fails() {
        let root = temp_dir("folders");
        let user = root.join("user");
        load(&bundled_dir(), &user).unwrap();
        assert!(user.is_dir());
        assert!(load(&root.join("missing"), &user).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
