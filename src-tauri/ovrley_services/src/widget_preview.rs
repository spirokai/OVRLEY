//! Native ownership of immutable finalized activity snapshots, map tile preparation, the native
//! editor preview's map frames and preview font resources.
use crate::map_styles::MapData;
use ovrley_core::{
    activity::finalize::FinalizeActivityResponse, activity::schema::ParsedActivity, paths::AppPaths, render::download_preview_tiles,
    CoreError,
};
use ovrley_widgets::{
    activity::ActivitySampler,
    host::WidgetHost,
    model::scene::{validate_scene_presentation, ScenePresentationConfig},
    preview::FontRequest,
    render::preview::PreviewRasterScale,
    widgets::{
        map::{
            config::MapConfig,
            preview::{MapFrameFactory, MapFramePixels, MapFrameProvider, MapFrameReply, MapFrameRequest},
            validation::validate_map_widget,
        },
        splits_run::table::SplitLength,
    },
};
use ovrley_map::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes};
use ovrley_map::{MapStyleBuildings, MapStyleInfo};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// Finalized activities retained for the preview, keyed by the id handed to the frontend.
#[derive(Default)]
pub struct PreviewActivities(Mutex<BTreeMap<String, Vec<u8>>>);

impl PreviewActivities {
    pub fn retain(&self, mut response: FinalizeActivityResponse) -> Result<FinalizeActivityResponse, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&response.parsed_activity).map_err(|e| e.to_string())?;
        self.0.lock().expect("preview activities lock").insert(id.clone(), bytes);
        response.parsed_activity.extra.insert("preview_activity_id".into(), id.into());
        Ok(response)
    }

    /// The serialized activity retained under `activity_id`.
    pub fn snapshot(&self, activity_id: &str) -> Result<Vec<u8>, String> {
        self.0
            .lock()
            .expect("preview activities lock")
            .get(activity_id)
            .cloned()
            .ok_or_else(|| format!("unknown preview activity: {activity_id}"))
    }

    pub fn release(&self, activity_id: &str) {
        self.0.lock().expect("preview activities lock").remove(activity_id);
    }

    /// Running splits of one retained activity for a widget's split length, serialized.
    pub fn split_table(&self, activity_id: &str, length: SplitLength) -> Result<Vec<u8>, String> {
        if !length.distance.is_finite() || length.distance <= 0.0 {
            return Err(format!("split length must be positive, got {}", length.distance));
        }
        let activity: ParsedActivity = {
            let snapshots = self.0.lock().expect("preview activities lock");
            let bytes = snapshots
                .get(activity_id)
                .ok_or_else(|| format!("unknown preview activity: {activity_id}"))?;
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?
        };
        let table = ovrley_core::activity::splits::derive_split_table(&activity, length);
        serde_json::to_vec(&table).map_err(|e| e.to_string())
    }
}

/// The tiles of a map route being downloaded. A newer request replaces it.
#[derive(Default)]
pub struct MapDownload(Mutex<Option<Arc<AtomicBool>>>);

impl MapDownload {
    /// Cancels the running download, if any, and returns the cancellation flag of a new one.
    pub fn restart(&self) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(previous) = self.0.lock().expect("map download lock").replace(cancelled.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        cancelled
    }
}

/// Progress of a map tile preparation, as the preview shows it.
#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum MapPrepareEvent {
    Needed { total: usize },
    Downloading { done: usize, total: usize },
    Ready,
    Failed { error: String },
}

/// A map tile preparation request: the retained activity and the map widgets to prepare for.
pub struct MapPrepareRequest {
    pub activity: Vec<u8>,
    pub widgets: Vec<MapConfig>,
    pub scene: ScenePresentationConfig,
    pub raster_scale: f32,
}

/// Downloads the tiles a project's maps will show over the activity, so scrubbing mostly hits the
/// disk cache. Blocking; the preview fetches what it needs either way. A cancelled download ends
/// quietly with `Ok`.
pub fn prepare_map_tiles(
    request: MapPrepareRequest,
    source: &MapData,
    cancelled: &AtomicBool,
    on_event: &(dyn Fn(MapPrepareEvent) + Sync),
) -> Result<(), String> {
    let MapPrepareRequest {
        activity,
        widgets,
        scene,
        raster_scale,
    } = request;
    let result = (|| {
        let activity = ActivitySampler::from_json(&activity).map_err(CoreError::from)?;
        // The preview's density sets the map's pixel ratio and so the tile zoom it needs.
        let host = PreviewRasterScale::new(raster_scale)
            .and_then(|raster_scale| WidgetHost::from_scene(&scene, raster_scale))
            .map_err(CoreError::from)?;
        let scene = validate_scene_presentation(scene).map_err(CoreError::from)?;
        let widgets = widgets
            .iter()
            .enumerate()
            .map(|(index, widget)| validate_map_widget(widget, index))
            .collect::<Result<Vec<_>, _>>()
            .map_err(CoreError::from)?;
        download_preview_tiles(&widgets, &scene, &host, &activity, source, cancelled, &|done, total| {
            // Often enough for a progress bar, and quiet on a route of thousands of tiles.
            if done == 0 {
                on_event(MapPrepareEvent::Needed { total });
            } else if done % (total / 50).max(1) == 0 {
                on_event(MapPrepareEvent::Downloading { done, total });
            }
        })
    })();
    match result {
        Ok(()) => {
            on_event(MapPrepareEvent::Ready);
            Ok(())
        }
        Err(CoreError::Cancelled) => Ok(()),
        Err(error) => {
            let error = error.to_string();
            on_event(MapPrepareEvent::Failed { error: error.clone() });
            Err(error)
        }
    }
}

/// A run of zoom edits draws on the current renderer, and only the zoom it settles on is rebuilt,
/// because a renderer's tiles load against its style zoom (React's `STYLE_ZOOM_SETTLE_MS`).
const STYLE_ZOOM_SETTLE: Duration = Duration::from_millis(300);

/// Wakes the editor preview after its maps' frames changed.
pub type MapFrameWake = Arc<dyn Fn() + Send + Sync>;

/// The native editor preview's map frames (plan D9): one thread, which owns a renderer per map on
/// its own device, as export's frame thread does. Unlike export it loads each view's tiles through
/// [`MapData`] when a frame needs them, from the disk cache or the network, and nothing it does
/// fails the preview: a failed frame is logged and answered with its error, and the map shows its
/// placeholder.
///
/// After every frame, and after a renderer rebuilt for a settled zoom, the thread calls `wake`.
pub struct PreviewMapFrames {
    sender: Mutex<mpsc::Sender<FrameJob>>,
    next_slot: AtomicU64,
}

enum FrameJob {
    Open { slot: u64, buildings: MapStyleBuildings, sample_count: u32, generation: Arc<AtomicU64> },
    Frame { slot: u64, request: Box<MapFrameRequest>, reply: mpsc::Sender<Result<MapFramePixels, String>> },
    Release { slot: u64 },
}

impl PreviewMapFrames {
    pub fn start(data: MapData, wake: MapFrameWake) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("preview-map-frames".into())
            .spawn(move || run_map_frames(&receiver, &data, &*wake))
            .map_err(|error| format!("Could not start the map frame thread: {error}"))?;
        Ok(Self { sender: Mutex::new(sender), next_slot: AtomicU64::new(0) })
    }
}

impl MapFrameFactory for PreviewMapFrames {
    fn provider(&self, style: &MapStyleInfo, sample_count: u32) -> Arc<dyn MapFrameProvider> {
        let slot = self.next_slot.fetch_add(1, Ordering::Relaxed);
        let generation = Arc::new(AtomicU64::new(0));
        let sender = self.sender.lock().expect("map frame sender lock").clone();
        // A stopped thread answers every frame with its error, through the disconnected reply.
        let _ = sender.send(FrameJob::Open { slot, buildings: style.buildings.clone(), sample_count, generation: generation.clone() });
        Arc::new(SlotFrames { slot, sender: Mutex::new(sender), generation })
    }
}

/// One map's view of the frame thread.
struct SlotFrames {
    slot: u64,
    sender: Mutex<mpsc::Sender<FrameJob>>,
    generation: Arc<AtomicU64>,
}

impl MapFrameProvider for SlotFrames {
    fn frame(&self, request: &MapFrameRequest, _spare: Option<Vec<u8>>) -> ovrley_widgets::error::WidgetResult<MapFrameReply> {
        let (reply, answer) = mpsc::channel();
        let job = FrameJob::Frame { slot: self.slot, request: Box::new(request.clone()), reply };
        // A stopped thread drops the reply, which the widget reads as a failed frame.
        let _ = self.sender.lock().expect("map frame sender lock").send(job);
        Ok(answer)
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

impl Drop for SlotFrames {
    fn drop(&mut self) {
        let _ = self.sender.lock().expect("map frame sender lock").send(FrameJob::Release { slot: self.slot });
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
        let rebuild = match &self.renderer {
            None => true,
            Some((renderer, _)) => {
                let config = renderer.config();
                config.spec != request.spec || config.viewport.pixel_ratio() != request.viewport.pixel_ratio()
            }
        };
        if rebuild {
            self.settling = None;
            return self.rebuild(self.config(request, zoom), GpuPoolSizes::DEFAULT, data);
        }
        let (renderer, _) = self.renderer.as_mut().expect("a renderer exists");
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
            let (renderer, pools) = self.renderer.as_mut().expect("prepared");
            if renderer.try_load_view(&tiles, data)? {
                break;
            }
            let (config, pools) = (renderer.config().clone(), pools.doubled());
            log::info!("A preview map view of {} tiles outgrew its pools; doubling them", tiles.len());
            self.rebuild(config, pools, data)?;
        }
        let loaded = started.elapsed();
        let (renderer, _) = self.renderer.as_mut().expect("prepared");
        let mut pixels = Vec::new();
        renderer.render(&request.camera, request.labels.as_ref(), request.route.as_ref(), request.corner_radius_px, &mut pixels)?;
        log::debug!("Preview map frame: load {:.1} ms, render {:.1} ms", loaded.as_secs_f64() * 1000.0, (started.elapsed() - loaded).as_secs_f64() * 1000.0);
        self.last = Some(request.clone());
        MapFramePixels::new(request.viewport.width_px(), request.viewport.height_px(), pixels).map_err(|error| error.to_string())
    }

    /// Rebuilds at the settled zoom, keeping the pools' size.
    fn settle(&mut self, data: &MapData) -> Result<(), String> {
        let (Some((zoom, _)), Some(request), Some((_, pools))) = (self.settling.take(), self.last.clone(), &self.renderer) else {
            return Ok(());
        };
        let pools = *pools;
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

/// The font file a preview widget asks for.
pub fn preview_font(paths: &AppPaths, request: &FontRequest) -> Result<Vec<u8>, String> {
    ovrley_core::fonts::preview_font_resource(&paths.font_dirs, &request.font, request.font_weight, request.italic).map_err(|e| e.to_string())
}
