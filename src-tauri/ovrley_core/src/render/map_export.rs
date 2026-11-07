//! What an export's maps need before encoding starts, and the one thread that draws their frames.
//! The preview's background download walks and fetches the same way, without holding the bytes.
//!
//! Preparation walks the camera over every rendered frame, downloads the tiles those views see and
//! holds their bytes for the job, so disk eviction or a lost connection cannot affect a running
//! export. It then tessellates each tile once to size the GPU pools for the job's largest view. Any
//! failure here fails the job before FFmpeg starts.
//!
//! A renderer owns one device and draws one frame at a time, so frame workers send their requests
//! to one thread and collect the answers when they reach the map. A request whose tiles cannot all be
//! resident fails the job rather than drawing a blank patch.

use crate::{
    debug::RenderProfiler,
    encode::progress::RenderController,
    error::{CoreError, CoreResult},
    normalize::ValidatedRenderConfig,
};
use ovrley_map::gpu::labels::{GlyphId, GlyphService};
use ovrley_map::gpu::{GpuMapConfig, GpuMapRenderer, GpuPoolSizes, MapTileSizer, MeasuredTile};
use ovrley_map::{MapDataSource, MapSource, MapStyleId, MapStyleInfo, TileId};
use ovrley_widgets::{
    activity::ActivitySampler,
    error::{WidgetError, WidgetResult},
    host::WidgetHost,
    model::scene::ValidatedScenePresentation,
    preview_widget::ValidatedWidget,
    widgets::map::{
        content::{ActivityMap, MapContent},
        preview::{MapFramePixels, MapFrameProvider, MapFrameReply, MapFrameRequest},
        validation::ValidatedMapWidget,
    },
};
use std::{
    collections::{hash_map::Entry, BTreeSet, HashMap},
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
const PREPARING_MESSAGE: &str = "Preparing video assets...";

/// What a job's maps are prepared from.
pub struct ExportMapJob<'a> {
    /// The installed styles and their bytes. The native tools have none, so they fail on any map.
    pub source: Option<&'a MapSource>,
    /// The dense frames the export renders.
    pub frame_indices: &'a [usize],
    pub controller: &'a RenderController,
}

struct ExportMap {
    content: MapContent,
    /// The installed style the widget's own resolved to.
    style: MapStyleInfo,
    frames: Arc<dyn MapFrameProvider>,
}

/// Downloads every tile `widgets` will show over the activity into `source`'s cache, so scrubbing
/// mostly hits the disk. `progress` hears how many of how many, first with none done. Stops at the
/// first tile that fails: the preview still fetches on demand, so nothing depends on this finishing.
pub fn download_preview_tiles(
    widgets: &[ValidatedMapWidget],
    scene: &ValidatedScenePresentation,
    host: &WidgetHost,
    activity: &ActivitySampler,
    source: &dyn MapDataSource,
    cancelled: &AtomicBool,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> CoreResult<()> {
    let mut views = Vec::new();
    for widget in widgets {
        if let MapContent::Activity(map) = MapContent::for_host(widget, scene, host, Some(activity))? {
            let seconds: Vec<f64> = map.activity_seconds().collect();
            views.extend(walk_views(&map, &seconds, cancelled)?);
        }
    }
    let tiles = distinct_tiles(&views);
    let total = tiles.len();
    progress(0, total);
    for_each_parallel(
        &tiles,
        FETCH_THREADS,
        cancelled,
        |finished| progress(finished, total),
        |tile| fetch_tile(source, *tile).map(drop),
    )?;
    Ok(())
}

/// Every tile of `views`, once each, in order.
fn distinct_tiles<'a>(views: impl IntoIterator<Item = &'a Vec<TileId>>) -> Vec<TileId> {
    views
        .into_iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
        let source = job.source.ok_or_else(|| {
            CoreError::Config(
                "plots: a map widget needs the installed map styles, which this tool does not have"
                    .into(),
            )
        })?;
        let styles: Vec<MapStyleInfo> = config
            .maps
            .iter()
            .map(|widget| source.styles.resolve(&widget.style).clone())
            .collect();
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
                    MapContent::Activity(map) => {
                        walk_views(map, seconds, &job.controller.cancel_flag())
                    }
                    MapContent::NoGps => Ok(Vec::new()),
                })
                .collect::<CoreResult<Vec<_>>>()
        })?;
        // Without GPS, or with no frame inside the activity, a map needs no tile or GPU.
        let mut slots = vec![None; views.len()];
        let mut thread = None;
        if views.iter().any(|views| !views.is_empty()) {
            let data = profiler.measure("prepare.maps.fetch", || {
                JobMapData::fetch(&*source.data, &styles, &views, job.controller)
            })?;
            let mut renderers = Vec::new();
            for (index, ((widget, content), views)) in
                config.maps.iter().zip(&contents).zip(&views).enumerate()
            {
                if let (MapContent::Activity(map), false) = (content, views.is_empty()) {
                    renderers.push(profiler.measure("prepare.maps.renderer", || {
                        ExportRenderer::new(widget, &styles[index], map, views, &data, job.controller)
                    })?);
                    slots[index] = Some(renderers.len() - 1);
                }
            }
            thread = Some(MapFrameThread::start(renderers, data)?);
        }
        Ok(Self {
            widgets: contents
                .into_iter()
                .zip(styles)
                .zip(slots)
                .map(|((content, style), slot)| ExportMap {
                    content,
                    style,
                    frames: match (slot, &thread) {
                        (Some(renderer), Some((sender, _))) => Arc::new(ExportMapFrames {
                            renderer,
                            sender: sender.clone(),
                        }),
                        _ => Arc::new(NoMapFrames),
                    },
                })
                .collect(),
            _thread: thread.map(|(_, thread)| thread),
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
            style: map.style.clone(),
            frames: map.frames.clone(),
            content: map.content.clone(),
        }
    }
}

/// A map that draws no frame in this export: no GPS, or no frame inside the activity.
struct NoMapFrames;

impl MapFrameProvider for NoMapFrames {
    fn frame(&self, _request: &MapFrameRequest, _spare: Option<Vec<u8>>) -> WidgetResult<MapFrameReply> {
        Err(WidgetError::Render(
            "A map asked for a frame that its export did not prepare".into(),
        ))
    }
}

/// Every view `map` shows over `seconds`, in order, with the views of its labels' placements.
fn walk_views(
    map: &ActivityMap,
    seconds: &[f64],
    cancelled: &AtomicBool,
) -> CoreResult<Vec<Vec<TileId>>> {
    let threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let chunks: Vec<&[f64]> = seconds.chunks(seconds.len().div_ceil(threads).max(1)).collect();
    let walked = for_each_parallel(&chunks, threads, cancelled, |_| {}, |chunk| {
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
    cancelled: &AtomicBool,
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
                    let result = if cancelled.load(Ordering::SeqCst) {
                        Err(CoreError::Cancelled)
                    } else {
                        work(item)
                    };
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
    styles: HashMap<MapStyleId, Vec<u8>>,
    tiles: HashMap<TileId, Vec<u8>>,
    glyphs: Arc<GlyphService>,
}

impl JobMapData {
    /// `styles` are the maps' resolved styles, one per entry of `views`.
    fn fetch(
        source: &dyn MapDataSource,
        styles: &[MapStyleInfo],
        views: &[Vec<Vec<TileId>>],
        controller: &RenderController,
    ) -> CoreResult<Self> {
        let mut documents = HashMap::new();
        for (style, _) in styles.iter().zip(views).filter(|(_, views)| !views.is_empty()) {
            if let Entry::Vacant(entry) = documents.entry(style.id.clone()) {
                let style = source
                    .style(entry.key())
                    .map_err(|error| CoreError::Render(format!("Map style unavailable: {error}")))?;
                entry.insert(style);
            }
        }
        let tiles = distinct_tiles(views.iter().flatten());
        let total = tiles.len();
        let fetched = for_each_parallel(
            &tiles,
            FETCH_THREADS,
            &controller.cancel_flag(),
            |finished| {
                controller.set_preparing_message(format!(
                    "{PREPARING_MESSAGE} map tiles {finished} of {total}"
                ));
            },
            |tile| fetch_tile(source, *tile).map(|bytes| (*tile, bytes)),
        )?;
        controller.set_preparing_message(PREPARING_MESSAGE.to_string());
        let tiles: HashMap<TileId, Vec<u8>> = fetched.into_iter().collect();
        log::info!(
            "Map export holds {} tiles, {} bytes",
            tiles.len(),
            tiles.values().map(Vec::len).sum::<usize>()
        );
        Ok(Self {
            styles: documents,
            tiles,
            glyphs: source.glyphs(),
        })
    }
}

fn fetch_tile(source: &dyn MapDataSource, tile: TileId) -> CoreResult<Vec<u8>> {
    source
        .tile(tile.z, tile.x as u32, tile.y as u32)
        .map_err(|error| {
            CoreError::Render(format!(
                "Map tile {}/{}/{} is unavailable: {error}",
                tile.z, tile.x, tile.y
            ))
        })
}

impl MapDataSource for JobMapData {
    fn style(&self, style: &MapStyleId) -> Result<Vec<u8>, String> {
        self.styles
            .get(style)
            .cloned()
            .ok_or_else(|| format!("Map style {style} was not prepared"))
    }

    fn tile(&self, z: u8, x: u32, y: u32) -> Result<Vec<u8>, String> {
        self.tiles
            .get(&TileId::new(z, x as i32, y as i32))
            .cloned()
            .ok_or_else(|| format!("Map tile {z}/{x}/{y} was not prepared"))
    }

    fn glyphs(&self) -> Arc<GlyphService> {
        self.glyphs.clone()
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
    /// `style` is the installed style the widget's own resolved to.
    fn new(
        widget: &ValidatedMapWidget,
        style: &MapStyleInfo,
        map: &ActivityMap,
        views: &[Vec<TileId>],
        data: &JobMapData,
        controller: &RenderController,
    ) -> CoreResult<Self> {
        let config = GpuMapConfig {
            spec: widget.spec(style),
            viewport: map.viewport(),
            sample_count: widget.msaa_samples,
            style_zoom: map.zoom(),
            buildings: style.buildings.clone(),
        };
        let sizer = MapTileSizer::new(&config, data).map_err(CoreError::Render)?;
        let tiles = distinct_tiles(views);
        let threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
        let measured: Vec<(TileId, MeasuredTile)> =
            for_each_parallel(&tiles, threads, &controller.cancel_flag(), |_| {}, |tile| {
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
            })?;
        // Measuring laid out every tile's labels, so the glyph service has made every glyph the job uses.
        let glyphs: BTreeSet<GlyphId> = measured
            .iter()
            .flat_map(|(_, measured)| measured.glyphs.iter().copied())
            .collect();
        let sizes: HashMap<TileId, GpuPoolSizes> = measured
            .into_iter()
            .map(|(tile, measured)| (tile, measured.pools))
            .collect();
        let pools = GpuPoolSizes::for_views(&sizes, views);
        let mut renderer = GpuMapRenderer::new(config, pools, data).map_err(CoreError::Render)?;
        renderer
            .preload_glyphs(&glyphs, data)
            .map_err(CoreError::Render)?;
        log::info!(
            "Map export renders {} tiles in {} views on {}, pools {} bytes: {pools:?}, {} label glyphs",
            tiles.len(),
            views.len(),
            renderer.adapter_name(),
            renderer.pool_bytes(),
            glyphs.len()
        );
        Ok(Self {
            renderer,
            frames: 0,
            load_ms: 0.0,
            render_ms: 0.0,
        })
    }

    fn frame(
        &mut self,
        request: &MapFrameRequest,
        spare: Option<Vec<u8>>,
        data: &JobMapData,
    ) -> Result<MapFramePixels, String> {
        let tiles = request.tiles();
        let started = Instant::now();
        self.renderer.load_view(&tiles, data)?;
        self.load_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let mut pixels = spare.unwrap_or_default();
        self.renderer.render(
            &request.camera,
            request.labels.as_ref(),
            request.route.as_ref(),
            request.corner_radius_px,
            &mut pixels,
        )?;
        let frame = MapFramePixels::new(
            request.viewport.width_px(),
            request.viewport.height_px(),
            pixels,
        )
        .map_err(|error| error.to_string())?;
        self.render_ms += started.elapsed().as_secs_f64() * 1000.0;
        self.frames += 1;
        Ok(frame)
    }
}

struct FrameJob {
    renderer: usize,
    request: MapFrameRequest,
    spare: Option<Vec<u8>>,
    reply: mpsc::Sender<Result<MapFramePixels, String>>,
}

/// A worker's view of the frame thread. It never returns `Pending`. Performance: `frame` only queues
/// the job, and the worker collects the reply when it reaches the map. Do not wait for it here.
struct ExportMapFrames {
    renderer: usize,
    sender: mpsc::Sender<FrameJob>,
}

impl MapFrameProvider for ExportMapFrames {
    fn frame(&self, request: &MapFrameRequest, spare: Option<Vec<u8>>) -> WidgetResult<MapFrameReply> {
        let (reply, answer) = mpsc::channel();
        self.sender
            .send(FrameJob {
                renderer: self.renderer,
                request: request.clone(),
                spare,
                reply,
            })
            .map_err(|_| WidgetError::Render("The map frame thread has stopped".into()))?;
        Ok(answer)
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
                let rasterized = data.glyphs.rasterized();
                for job in receiver {
                    let result = renderers[job.renderer].frame(&job.request, job.spare, &data);
                    // A worker that stopped waiting has already failed the job.
                    let _ = job.reply.send(result);
                }
                log::info!(
                    "Map export rasterized {} glyphs while drawing frames",
                    data.glyphs.rasterized() - rasterized
                );
                for renderer in &renderers {
                    log::info!(
                        "Map export drew {} frames: load {:.0} ms, render {:.0} ms, {} layers evicted, {} pool clears, {} label atlas clears",
                        renderer.frames,
                        renderer.load_ms,
                        renderer.render_ms,
                        renderer.renderer.evicted_layers(),
                        renderer.renderer.pool_clears(),
                        renderer.renderer.atlas_clears()
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
