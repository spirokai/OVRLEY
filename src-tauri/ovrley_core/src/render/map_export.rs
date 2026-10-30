//! What an export's maps need before encoding starts, and the one thread that draws their frames.
//!
//! Preparation walks the camera over every rendered frame, downloads the tiles those views see and
//! holds their bytes for the job, so disk eviction or a lost connection cannot affect a running
//! export. It then tessellates each tile once to size the GPU pools for the job's largest view. Any
//! failure here fails the job before FFmpeg starts.
//!
//! A renderer owns one device and draws one frame at a time, so frame workers send their requests
//! to one thread and wait. A request whose tiles cannot all be resident fails the job rather than
//! drawing a blank patch.

use crate::{
    debug::RenderProfiler,
    encode::progress::RenderController,
    error::{CoreError, CoreResult},
    normalize::ValidatedRenderConfig,
};
use ovrley_map::{view_tiles, MapDataSource, MapSprite, MapStyle, TileId, ViewPadding};
use ovrley_map_gpu::{Buildings, GpuMapConfig, GpuMapRenderer, GpuPoolSizes, MapTileSizer};
use ovrley_widgets::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    preview_widget::ValidatedWidget,
    widgets::map::{
        content::{ActivityMap, MapContent},
        frame_provider::{map_frame_image, MapFrame, MapFrameProvider, MapFrameRequest},
        validation::ValidatedMapWidget,
    },
};
use skia_safe::Image;
use std::{
    collections::{BTreeSet, HashMap},
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Instant,
};

/// Connections to the tile server at once.
const FETCH_THREADS: usize = 8;
/// Fill and building edges are jagged without it.
const SAMPLE_COUNT: u32 = 4;
const PREPARING_MESSAGE: &str = "Preparing video assets...";

/// What a job's maps are prepared from.
pub struct ExportMapJob<'a> {
    /// The native tools have none, and fail on a map that needs tiles.
    pub source: Option<&'a dyn MapDataSource>,
    /// The dense frames the export renders.
    pub frame_indices: &'a [usize],
    pub controller: &'a RenderController,
}

struct ExportMap {
    content: MapContent,
    frames: Arc<dyn MapFrameProvider>,
}

/// One entry per config map. `widgets` holds the senders, so it drops first and ends the thread.
#[derive(Default)]
pub(super) struct ExportMaps {
    widgets: Vec<ExportMap>,
    _thread: Option<MapFrameThread>,
}

impl ExportMaps {
    pub(super) fn prepare(
        config: &ValidatedRenderConfig,
        activity: &ActivitySampler,
        host: &WidgetHost,
        seconds: &[f64],
        job: &ExportMapJob<'_>,
        profiler: &mut RenderProfiler,
    ) -> CoreResult<Self> {
        if config.maps.is_empty() {
            return Ok(Self::default());
        }
        let contents = config
            .maps
            .iter()
            .map(|widget| {
                MapContent::for_host(widget, &config.scene.presentation, host, Some(activity))
            })
            .collect::<WidgetResult<Vec<_>>>()?;
        let views = profiler.measure("prepare.maps.walk", || {
            contents
                .iter()
                .map(|content| match content {
                    MapContent::Activity(map) => walk_views(map, seconds, job.controller),
                    MapContent::NoGps => Ok(Vec::new()),
                })
                .collect::<CoreResult<Vec<_>>>()
        })?;
        // Without GPS, or with no frame inside the activity, nothing needs a tile or a GPU.
        if views.iter().all(Vec::is_empty) {
            return Ok(Self {
                widgets: contents
                    .into_iter()
                    .map(|content| ExportMap {
                        content,
                        frames: Arc::new(NoMapFrames),
                    })
                    .collect(),
                _thread: None,
            });
        }
        let source = job.source.ok_or_else(|| {
            CoreError::Config(
                "plots: a map widget needs a map data source, which this tool does not have".into(),
            )
        })?;
        let data = profiler.measure("prepare.maps.fetch", || {
            JobMapData::fetch(source, &config.maps, &views, job.controller)
        })?;
        let (mut renderers, mut slots) = (Vec::new(), Vec::with_capacity(views.len()));
        for ((widget, content), views) in config.maps.iter().zip(&contents).zip(&views) {
            slots.push(match content {
                MapContent::Activity(map) if !views.is_empty() => {
                    renderers.push(profiler.measure("prepare.maps.renderer", || {
                        ExportRenderer::new(widget, map, views, &data, job.controller)
                    })?);
                    Some(renderers.len() - 1)
                }
                _ => None,
            });
        }
        let (sender, thread) = MapFrameThread::start(renderers, data)?;
        Ok(Self {
            widgets: contents
                .into_iter()
                .zip(slots)
                .map(|(content, slot)| ExportMap {
                    content,
                    frames: match slot {
                        Some(renderer) => Arc::new(ExportMapFrames {
                            renderer,
                            sender: sender.clone(),
                        }),
                        None => Arc::new(NoMapFrames),
                    },
                })
                .collect(),
            _thread: Some(thread),
        })
    }

    pub(super) fn widget<'a>(
        &self,
        index: usize,
        widget: &'a ValidatedMapWidget,
    ) -> ValidatedWidget<'a> {
        let map = &self.widgets[index];
        ValidatedWidget::Map {
            widget,
            frames: map.frames.clone(),
            content: map.content.clone(),
        }
    }
}

/// A map that draws no frame in this export: no GPS, or no frame inside the activity.
struct NoMapFrames;

impl MapFrameProvider for NoMapFrames {
    fn frame(&self, _request: &MapFrameRequest) -> WidgetResult<MapFrame> {
        Err(WidgetError::Render(
            "A map asked for a frame that its export did not prepare".into(),
        ))
    }
}

/// Every view `map` shows over `seconds`, in order.
fn walk_views(
    map: &ActivityMap,
    seconds: &[f64],
    controller: &RenderController,
) -> CoreResult<Vec<Vec<TileId>>> {
    let threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let chunks: Vec<&[f64]> = seconds.chunks(seconds.len().div_ceil(threads).max(1)).collect();
    let walked = for_each_parallel(&chunks, threads, controller, |_| {}, |chunk| {
        Ok(map.views(chunk.iter().copied())?)
    })?;
    let mut views: Vec<Vec<TileId>> = Vec::new();
    for view in walked.into_iter().flatten() {
        if views.last() != Some(&view) {
            views.push(view);
        }
    }
    Ok(views)
}

/// Runs `work` over `items` on `threads` threads, stopping at the first failure or cancellation.
/// Results keep the order of `items`. `done` hears how many have finished.
fn for_each_parallel<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    controller: &RenderController,
    done: impl Fn(usize) + Sync,
    work: impl Fn(&T) -> CoreResult<R> + Sync,
) -> CoreResult<Vec<R>> {
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let failure = Mutex::new(None);
    let results = Mutex::new(Vec::with_capacity(items.len()));
    thread::scope(|scope| {
        for _ in 0..threads.min(items.len()) {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        return;
                    };
                    let result = controller.check_cancelled().and_then(|()| work(item));
                    match result {
                        Ok(result) => {
                            results
                                .lock()
                                .expect("map preparation results poisoned")
                                .push((index, result));
                            done(finished.fetch_add(1, Ordering::Relaxed) + 1);
                        }
                        Err(error) => {
                            stop.store(true, Ordering::Relaxed);
                            failure
                                .lock()
                                .expect("map preparation failure poisoned")
                                .get_or_insert(error);
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = failure
        .into_inner()
        .expect("map preparation failure poisoned")
    {
        return Err(error);
    }
    let mut results = results
        .into_inner()
        .expect("map preparation results poisoned");
    results.sort_unstable_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

/// The job's map bytes, held so nothing outside can take them away while it runs.
struct JobMapData {
    styles: HashMap<MapStyle, Vec<u8>>,
    tiles: HashMap<TileId, Vec<u8>>,
}

impl JobMapData {
    fn fetch(
        source: &dyn MapDataSource,
        maps: &[ValidatedMapWidget],
        views: &[Vec<Vec<TileId>>],
        controller: &RenderController,
    ) -> CoreResult<Self> {
        let mut styles = HashMap::new();
        for (map, _) in maps.iter().zip(views).filter(|(_, views)| !views.is_empty()) {
            if !styles.contains_key(&map.spec.style) {
                let style = source
                    .style(map.spec.style)
                    .map_err(|error| CoreError::Render(format!("Map style unavailable: {error}")))?;
                styles.insert(map.spec.style, style);
            }
        }
        let tiles: Vec<TileId> = views
            .iter()
            .flatten()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let total = tiles.len();
        let fetched = for_each_parallel(
            &tiles,
            FETCH_THREADS,
            controller,
            |finished| {
                controller.set_preparing_message(format!(
                    "{PREPARING_MESSAGE} map tiles {finished} of {total}"
                ));
            },
            |tile| {
                source
                    .tile(tile.z, tile.x as u32, tile.y as u32)
                    .map(|bytes| (*tile, bytes))
                    .map_err(|error| {
                        CoreError::Render(format!(
                            "Map tile {}/{}/{} is unavailable: {error}",
                            tile.z, tile.x, tile.y
                        ))
                    })
            },
        )?;
        controller.set_preparing_message(PREPARING_MESSAGE.to_string());
        let tiles: HashMap<TileId, Vec<u8>> = fetched.into_iter().collect();
        log::info!(
            "Map export holds {} tiles, {} bytes",
            tiles.len(),
            tiles.values().map(Vec::len).sum::<usize>()
        );
        Ok(Self { styles, tiles })
    }
}

impl MapDataSource for JobMapData {
    fn style(&self, style: MapStyle) -> Result<Vec<u8>, String> {
        self.styles
            .get(&style)
            .cloned()
            .ok_or_else(|| format!("Map style {} was not prepared", style.name()))
    }

    fn sprite(&self, style: MapStyle) -> Result<MapSprite, String> {
        Err(format!(
            "The export renderer draws no sprites, so {} has none prepared",
            style.name()
        ))
    }

    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String> {
        self.tiles
            .get(&TileId::new(z, x as i32, y as i32))
            .cloned()
            .ok_or_else(|| format!("Map tile {z}/{x}/{y} was not prepared"))
    }
}

/// One map's renderer on the frame thread, and what its frames cost.
struct ExportRenderer {
    renderer: GpuMapRenderer,
    frames: u64,
    load_ms: f64,
    render_ms: f64,
}

impl ExportRenderer {
    /// Fails without a usable GPU, or when the job's largest view cannot fit.
    fn new(
        widget: &ValidatedMapWidget,
        map: &ActivityMap,
        views: &[Vec<TileId>],
        data: &JobMapData,
        controller: &RenderController,
    ) -> CoreResult<Self> {
        let buildings = Buildings::for_style(widget.spec.style);
        let sizer = MapTileSizer::new(
            widget.spec,
            map.viewport(),
            map.zoom(),
            &buildings,
            data,
        )
        .map_err(CoreError::Render)?;
        let tiles: Vec<TileId> = views
            .iter()
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let sizes: HashMap<TileId, GpuPoolSizes> =
            for_each_parallel(&tiles, threads, controller, |_| {}, |tile| {
                let bytes = data
                    .tile(tile.z, tile.x as u32, tile.y as u32)
                    .map_err(CoreError::Render)?;
                let size = sizer.measure(&bytes).map_err(|error| {
                    CoreError::Render(format!(
                        "Map tile {}/{}/{} is unusable: {error}",
                        tile.z, tile.x, tile.y
                    ))
                })?;
                Ok((*tile, size))
            })?
            .into_iter()
            .collect();
        let pools = GpuPoolSizes::for_views(&sizes, views);
        let renderer = GpuMapRenderer::new(
            GpuMapConfig {
                spec: widget.spec,
                viewport: map.viewport(),
                sample_count: SAMPLE_COUNT,
                style_zoom: map.zoom(),
                pools,
                buildings,
            },
            data,
        )
        .map_err(CoreError::Render)?;
        log::info!(
            "Map export renders {} tiles in {} views on {}, pools {} bytes: {pools:?}",
            tiles.len(),
            views.len(),
            renderer.adapter_name(),
            renderer.pool_bytes()
        );
        Ok(Self {
            renderer,
            frames: 0,
            load_ms: 0.0,
            render_ms: 0.0,
        })
    }

    fn frame(&mut self, request: &MapFrameRequest, data: &JobMapData) -> Result<Image, String> {
        let tiles = view_tiles(&request.camera, request.viewport, ViewPadding::Tight);
        let started = Instant::now();
        self.renderer.load_view(&tiles, data)?;
        self.load_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let pixels = self.renderer.render(&request.camera, Some(&request.route))?;
        let image = map_frame_image(
            request.viewport.width_px(),
            request.viewport.height_px(),
            pixels,
        )
        .map_err(|error| error.to_string())?;
        self.render_ms += started.elapsed().as_secs_f64() * 1000.0;
        self.frames += 1;
        Ok(image)
    }
}

struct FrameJob {
    renderer: usize,
    request: MapFrameRequest,
    reply: mpsc::Sender<Result<Image, String>>,
}

/// A worker's view of the frame thread. It never returns `Pending`.
struct ExportMapFrames {
    renderer: usize,
    sender: mpsc::Sender<FrameJob>,
}

impl MapFrameProvider for ExportMapFrames {
    fn frame(&self, request: &MapFrameRequest) -> WidgetResult<MapFrame> {
        let (reply, answer) = mpsc::channel();
        self.sender
            .send(FrameJob {
                renderer: self.renderer,
                request: request.clone(),
                reply,
            })
            .map_err(|_| WidgetError::Render("The map frame thread has stopped".into()))?;
        answer
            .recv()
            .map_err(|_| WidgetError::Render("The map frame thread stopped mid-frame".into()))?
            .map(MapFrame::Ready)
            .map_err(WidgetError::Render)
    }
}

/// Ends when every worker's sender is gone, and joins on drop.
struct MapFrameThread(Option<thread::JoinHandle<()>>);

impl MapFrameThread {
    fn start(
        mut renderers: Vec<ExportRenderer>,
        data: JobMapData,
    ) -> CoreResult<(mpsc::Sender<FrameJob>, Self)> {
        let (sender, receiver) = mpsc::channel::<FrameJob>();
        let handle = thread::Builder::new()
            .name("map-frames".into())
            .spawn(move || {
                for job in receiver {
                    let result = renderers[job.renderer].frame(&job.request, &data);
                    // A worker that stopped waiting has already failed the job.
                    let _ = job.reply.send(result);
                }
                for renderer in &renderers {
                    log::info!(
                        "Map export drew {} frames: load {:.0} ms, render {:.0} ms, {} layers evicted, {} pool clears",
                        renderer.frames,
                        renderer.load_ms,
                        renderer.render_ms,
                        renderer.renderer.evicted_layers(),
                        renderer.renderer.pool_clears()
                    );
                }
            })
            .map_err(|error| {
                CoreError::Render(format!("Could not start the map frame thread: {error}"))
            })?;
        Ok((sender, Self(Some(handle))))
    }
}

impl Drop for MapFrameThread {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            if handle.join().is_err() {
                log::error!("The map frame thread panicked");
            }
        }
    }
}
