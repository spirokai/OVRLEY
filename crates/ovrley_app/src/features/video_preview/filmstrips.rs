//! Port of `utils/filmstrip.js`, `hooks/useVideoFilmstrips.js` (`useVideoFilmstripQueue`),
//! `features/player/utils/filmstripTiles.js` and `useWantedFilmstripWindows` (plan D12).
//!
//! Every found video gets a base strip of up to 24 frames; zoomed in, the timeline asks for zoom
//! windows once the view stood still for 200 ms. Cache reads run at once; decodes wait for one of
//! two slots, base strips first. Past 96 loaded windows, the ones the timeline no longer shows are
//! dropped, and a removed video's strips go with it. A failed strip is not retried while its video
//! stays; its tiles fall back to a coarser strip, or the clip shows plain.
//!
//! The grid itself (spacings, windows, sampling) is `ovrley_services::video`'s.

use crate::lib::video_program::Clip;
use ovrley_core::media::video_edits::VideoResolution;
use crate::store::services::{FilmstripHwaccel, FilmstripRequest, FilmstripWindow, Services};
use crate::store::slices::video_filmstrip::{ReadyStrip, StripState};
use crate::store::Store;
use ovrley_core::encode::ffmpeg::detect::AvailableCodecs;
use ovrley_services::video::{base_spacing_seconds, zoom_spacing_seconds, Filmstrip, FINEST_SPACING_SECONDS, FRAMES_PER_STRIP};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `FILMSTRIP_FRAME_HEIGHT_PX`: twice the clip height, so frames stay sharp on dense displays.
pub const FILMSTRIP_FRAME_HEIGHT_PX: u32 = 96;
/// `FILMSTRIP_CONCURRENCY`.
pub const FILMSTRIP_CONCURRENCY: usize = 2;
/// `FILMSTRIP_WINDOWS_KEPT`.
pub const FILMSTRIP_WINDOWS_KEPT: usize = 96;
/// `FILMSTRIP_WINDOW_SETTLE_MS`.
pub const FILMSTRIP_WINDOW_SETTLE: Duration = Duration::from_millis(200);

/// `filmstripFrameSize`: 96 px tall at the video's display aspect ratio.
pub fn filmstrip_frame_size(resolution: VideoResolution) -> (u32, u32) {
    let width = crate::js::round(f64::from(FILMSTRIP_FRAME_HEIGHT_PX) * f64::from(resolution.width) / f64::from(resolution.height));
    (width as u32, FILMSTRIP_FRAME_HEIGHT_PX)
}

/// `filmstripZoomLevel`: the coarsest level that still gives every tile its own frame, never finer
/// than level 0; `None` when the base strip already does.
pub fn filmstrip_zoom_level(seconds_per_tile: f64, duration_seconds: f64) -> Option<u32> {
    let base = base_spacing_seconds(duration_seconds);
    if seconds_per_tile >= base {
        return None;
    }
    let mut level = (seconds_per_tile / FINEST_SPACING_SECONDS).log2().floor().max(0.0) as u32;
    while level > 0 && zoom_spacing_seconds(level) >= base {
        level -= 1;
    }
    (zoom_spacing_seconds(level) < base).then_some(level)
}

/// `filmstripZoomWindows`: the windows of a level over a stretch of the video.
pub fn filmstrip_zoom_windows(level: u32, from_second: f64, to_second: f64, duration_seconds: f64) -> Vec<FilmstripWindow> {
    let window_seconds = f64::from(FRAMES_PER_STRIP) * zoom_spacing_seconds(level);
    let from = from_second.max(0.0);
    let to = to_second.min(duration_seconds);
    if to <= from {
        return Vec::new();
    }
    let first = (from / window_seconds).floor() as u32;
    // A stretch ending exactly on a window edge does not reach into the next window.
    let last = first.max(((to / window_seconds).ceil() as u32).saturating_sub(1));
    (first..=last).map(|window| FilmstripWindow::Zoom { level, window }).collect()
}

/// `filmstripWindowKey`: `base`, or `z<level>-<window>`.
pub fn window_key(window: FilmstripWindow) -> String {
    match window {
        FilmstripWindow::Base => "base".into(),
        FilmstripWindow::Zoom { level, window } => format!("z{level}-{window}"),
    }
}

/// The window key of a request, for the fake video tools.
pub fn request_key(request: &FilmstripRequest) -> String {
    window_key(request.window())
}

/// `filmstripHwaccel`: the GPU an export can use, through FFmpeg's own decoders.
pub fn filmstrip_hwaccel(codecs: Option<&AvailableCodecs>) -> Option<FilmstripHwaccel> {
    let codecs = codecs?;
    if codecs.nvdec {
        Some(FilmstripHwaccel::Cuda)
    } else if codecs.vaapi {
        Some(FilmstripHwaccel::Vaapi)
    } else if codecs.qsv || codecs.h264_amf || codecs.hevc_amf {
        Some(FilmstripHwaccel::D3d11va)
    } else if codecs.videotoolbox {
        Some(FilmstripHwaccel::Videotoolbox)
    } else {
        None
    }
}

/// `filmstripTileWidthPx`: the clip height at the video's aspect ratio, in whole pixels.
pub fn tile_width_px(resolution: VideoResolution, tile_height_px: f64) -> f64 {
    crate::js::round(tile_height_px * f64::from(resolution.width) / f64::from(resolution.height))
}

/// One filmstrip tile, relative to its segment's left edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// The tile's index in the segment.
    pub index: usize,
    pub x: f64,
    pub width: f64,
    pub strip: Arc<ReadyStrip>,
    pub sample: u32,
}

/// The finest strip whose stretch holds `second`, or the coarsest when none does.
fn strip_at(strips: &[Arc<ReadyStrip>], second: f64) -> &Arc<ReadyStrip> {
    strips
        .iter()
        .filter(|strip| second >= strip.start_seconds && second < strip.end_seconds())
        .min_by(|left, right| left.spacing_seconds.total_cmp(&right.spacing_seconds))
        .unwrap_or_else(|| strips.iter().max_by(|left, right| left.spacing_seconds.total_cmp(&right.spacing_seconds)).expect("a clip with tiles has its base strip"))
}

fn sample_at(strip: &ReadyStrip, second: f64) -> u32 {
    ((second - strip.start_seconds) / strip.spacing_seconds).floor().clamp(0.0, f64::from(strip.sample_count - 1)) as u32
}

/// `listFilmstripTiles`: the tiles of one segment inside the viewport. Tiles run from the
/// segment's left edge at a fixed width; each shows the frame of the finest ready strip holding
/// the source time at its left edge.
pub fn list_filmstrip_tiles(strips: &[Arc<ReadyStrip>], source_start: f64, source_end: f64, left_px: f64, px_per_second: f64, tile_width: f64, width_px: f64) -> Vec<Tile> {
    if strips.is_empty() || tile_width <= 0.0 {
        return Vec::new();
    }
    let segment_width = (source_end - source_start) * px_per_second;
    let first = (-left_px / tile_width).floor().max(0.0) as usize;
    let end = (segment_width / tile_width).ceil().min(((width_px - left_px) / tile_width).ceil()).max(0.0) as usize;
    (first..end)
        .map(|index| {
            let second = source_start + index as f64 * tile_width / px_per_second;
            let strip = strip_at(strips, second);
            Tile { index, x: index as f64 * tile_width, width: tile_width, sample: sample_at(strip, second), strip: Arc::clone(strip) }
        })
        .collect()
}

/// `listWantedFilmstripWindows`: for each found clip zoomed in past its base strip's spacing, the
/// windows of the matching level under its visible part; a video on several clips asks once.
pub fn list_wanted_windows(clips: &[Clip], view_start: f64, view_end: f64, width_px: f64, tile_height_px: f64) -> BTreeMap<String, Vec<FilmstripWindow>> {
    let mut wanted: BTreeMap<String, Vec<FilmstripWindow>> = BTreeMap::new();
    if width_px <= 0.0 || view_end <= view_start {
        return wanted;
    }
    let px_per_second = width_px / (view_end - view_start);
    for clip in clips.iter().filter(|clip| !clip.video.missing) {
        let seconds_per_tile = tile_width_px(clip.video.resolution, tile_height_px) / px_per_second;
        let Some(level) = filmstrip_zoom_level(seconds_per_tile, clip.duration_seconds()) else { continue };
        let windows = filmstrip_zoom_windows(level, view_start - clip.offset_seconds(), view_end - clip.offset_seconds(), clip.duration_seconds());
        if windows.is_empty() {
            continue;
        }
        let entry = wanted.entry(clip.video.path.clone()).or_default();
        for window in windows {
            if !entry.contains(&window) {
                entry.push(window);
            }
        }
    }
    wanted
}

/// Codec detection (`fetchAvailableCodecs`), once per session.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CodecDetection {
    #[default]
    NotStarted,
    Detecting,
    /// `None` when detection failed: filmstrips decode in software.
    Done(Option<AvailableCodecs>),
}

/// A strip waiting for a decode slot.
#[derive(Clone, Debug)]
struct Waiting {
    priority: u8,
    order: u64,
    path: String,
    key: String,
    job: u64,
    request: FilmstripRequest,
}

/// `useVideoFilmstripQueue`'s module state, and the wanted windows waiting to settle.
#[derive(Debug, Default)]
pub struct FilmstripQueue {
    last_job: u64,
    busy: usize,
    waiting: Vec<Waiting>,
    /// The wanted windows of the current view, and when they last changed.
    pending_wanted: Option<(BTreeMap<String, Vec<FilmstripWindow>>, Instant)>,
}

impl FilmstripQueue {
    /// When the pending wanted windows settle, so `ovrley_ui` can come back then.
    pub fn wanted_deadline(&self) -> Option<Instant> {
        self.pending_wanted.as_ref().map(|(_, changed)| *changed + FILMSTRIP_WINDOW_SETTLE)
    }
}

fn is_current_job(store: &Store, path: &str, key: &str, job: u64) -> bool {
    store.video_filmstrip.get(path, key) == Some(&StripState::Generating { job })
}

/// Decodes a strip's JPEG to RGBA, off the UI thread.
fn decode_sprite(strip: Filmstrip) -> Result<ReadyStrip, String> {
    let image = image::load_from_memory_with_format(&strip.sprite_jpeg, image::ImageFormat::Jpeg).map_err(|error| error.to_string())?.to_rgba8();
    Ok(ReadyStrip {
        width: image.width(),
        height: image.height(),
        pixels: Arc::new(image.into_raw()),
        start_seconds: strip.start_seconds,
        spacing_seconds: strip.spacing_seconds,
        sample_count: strip.sample_count,
    })
}

fn settled(result: Result<Option<Filmstrip>, String>) -> StripState {
    match result.and_then(|strip| strip.map(decode_sprite).transpose()) {
        Ok(Some(strip)) => StripState::Ready(Arc::new(strip)),
        Ok(None) => StripState::Failed,
        Err(error) => {
            log::error!("filmstrip failed: {error}");
            StripState::Failed
        }
    }
}

/// `startStrip`: from the disk cache when it is there, otherwise queued for a decode slot.
fn start_strip(store: &mut Store, services: &mut Services, clip: &Clip, window: FilmstripWindow) {
    let key = window_key(window);
    let path = clip.video.path.clone();
    let (frame_width, frame_height) = filmstrip_frame_size(clip.video.resolution);
    store.filmstrip_queue.last_job += 1;
    let job = store.filmstrip_queue.last_job;
    let request = match FilmstripRequest::new(PathBuf::from(&path), clip.duration_seconds(), window, frame_width, frame_height) {
        Ok(request) => request,
        Err(error) => {
            log::error!("filmstrip of {path} cannot be planned: {error}");
            store.video_filmstrip.set(&path, &key, StripState::Failed);
            return;
        }
    };
    store.video_filmstrip.set(&path, &key, StripState::Generating { job });
    let tools = Arc::clone(&services.video_tools);
    let priority = u8::from(window != FilmstripWindow::Base);
    services.workers.spawn("filmstrip-cache", move || {
        let cached = tools.cached_filmstrip(&request);
        let cached = match cached {
            Ok(Some(strip)) => Some(settled(Ok(Some(strip)))),
            Ok(None) => None,
            Err(error) => Some(settled(Err(error))),
        };
        Box::new(move |store: &mut Store, services: &mut Services| {
            if !is_current_job(store, &path, &key, job) {
                return;
            }
            match cached {
                Some(state) => store.video_filmstrip.set(&path, &key, state),
                None => {
                    let queue = &mut store.filmstrip_queue;
                    queue.last_job += 1;
                    let order = queue.last_job;
                    queue.waiting.push(Waiting { priority, order, path, key, job, request });
                    grant_decode_slots(store, services);
                }
            }
        })
    });
}

/// `grantDecodeSlots`: starts waiting decodes, by priority then order, while slots are free and the
/// codecs are known. A zoom window the timeline stopped wanting meanwhile is dropped unstarted.
fn grant_decode_slots(store: &mut Store, services: &mut Services) {
    if store.filmstrip_queue.waiting.is_empty() {
        return;
    }
    let codecs = match &store.video_import.available_codecs {
        CodecDetection::Done(codecs) => codecs.clone(),
        CodecDetection::Detecting => return,
        CodecDetection::NotStarted => {
            fetch_available_codecs(store, services);
            return;
        }
    };
    store.filmstrip_queue.waiting.sort_by_key(|waiting| (waiting.priority, waiting.order));
    while store.filmstrip_queue.busy < FILMSTRIP_CONCURRENCY && !store.filmstrip_queue.waiting.is_empty() {
        let next = store.filmstrip_queue.waiting.remove(0);
        if !is_current_job(store, &next.path, &next.key, next.job) {
            continue;
        }
        let window = next.request.window();
        if window != FilmstripWindow::Base && !store.video_filmstrip.wanted.get(&next.path).is_some_and(|wanted| wanted.contains(&window)) {
            store.video_filmstrip.remove(&next.path, &next.key);
            continue;
        }
        store.filmstrip_queue.busy += 1;
        let tools = Arc::clone(&services.video_tools);
        let hwaccel = filmstrip_hwaccel(codecs.as_ref());
        let Waiting { path, key, job, request, .. } = next;
        services.workers.spawn("filmstrip-decode", move || {
            let state = settled(tools.filmstrip(&request, hwaccel));
            Box::new(move |store: &mut Store, services: &mut Services| {
                store.filmstrip_queue.busy -= 1;
                if is_current_job(store, &path, &key, job) {
                    store.video_filmstrip.set(&path, &key, state);
                }
                grant_decode_slots(store, services);
            })
        });
    }
}

/// `fetchAvailableCodecs`: detects the codecs once, on a worker; the render dialog reuses them in
/// Phase 7.
pub fn fetch_available_codecs(store: &mut Store, services: &mut Services) {
    if store.video_import.available_codecs != CodecDetection::NotStarted {
        return;
    }
    store.video_import.available_codecs = CodecDetection::Detecting;
    let tools = Arc::clone(&services.video_tools);
    services.workers.spawn("detect-codecs", move || {
        let codecs = tools.detect_codecs();
        Box::new(move |store: &mut Store, services: &mut Services| {
            let codecs = codecs.map_err(|error| log::error!("Failed to detect ffmpeg codecs: {error}")).ok();
            store.video_import.available_codecs = CodecDetection::Done(codecs);
            grant_decode_slots(store, services);
        })
    });
}

/// `dropUnshownWindows`: past 96 loaded zoom windows, drops the ones the timeline does not show.
fn drop_unshown_windows(store: &mut Store) {
    let wanted = &store.video_filmstrip.wanted;
    let mut loaded: usize = 0;
    let mut unshown = Vec::new();
    for (path, strips) in &store.video_filmstrip.strips {
        let shown: Vec<String> = wanted.get(path).map(|windows| windows.iter().map(|window| window_key(*window)).collect()).unwrap_or_default();
        for (key, strip) in strips {
            if key == "base" || !matches!(strip, StripState::Ready(_)) {
                continue;
            }
            loaded += 1;
            if !shown.contains(key) {
                unshown.push((path.clone(), key.clone()));
            }
        }
    }
    for (path, key) in unshown.into_iter().take(loaded.saturating_sub(FILMSTRIP_WINDOWS_KEPT)) {
        store.video_filmstrip.remove(&path, &key);
    }
}

/// The timeline's tile height: the video clip height (3rem).
fn tile_height_px(store: &Store) -> f64 {
    crate::features::player::lanes::VIDEO_CLIP_HEIGHT_REM * store.player.viewport.metrics.rem
}

/// `useWantedFilmstripWindows`: the view's wanted windows become pending, and publish once they
/// stood still for 200 ms (`publish_wanted_windows`).
fn track_wanted_windows(store: &mut Store, now: Instant) {
    let view = store.editor.timeline_viewport;
    let displayed = &store.player.timeline.timeline().displayed;
    let wanted = list_wanted_windows(&displayed.clips, view.view_start, view.view_end, store.player.viewport.metrics.width_px, tile_height_px(store));
    let queue = &mut store.filmstrip_queue;
    match &queue.pending_wanted {
        Some((pending, _)) if *pending == wanted => {}
        _ if queue.pending_wanted.is_none() && wanted == store.video_filmstrip.wanted => {}
        _ => queue.pending_wanted = Some((wanted, now)),
    }
}

/// Publishes the pending wanted windows once they settled; `ovrley_ui` calls it at the deadline.
pub fn publish_wanted_windows(store: &mut Store, now: Instant) {
    let Some(deadline) = store.filmstrip_queue.wanted_deadline() else { return };
    if now < deadline {
        return;
    }
    if let Some((wanted, _)) = store.filmstrip_queue.pending_wanted.take() {
        store.video_filmstrip.wanted = wanted;
    }
}

/// The filmstrip reconcile step (`useVideoFilmstripQueue`'s effect), after the player's.
pub fn reconcile_filmstrips(store: &mut Store, services: &mut Services) {
    track_wanted_windows(store, Instant::now());
    let clips: Vec<Clip> = store.player.timeline.timeline().committed.clips.iter().filter(|clip| !clip.video.missing).cloned().collect();
    let gone: Vec<String> = store.video_filmstrip.strips.keys().filter(|path| !clips.iter().any(|clip| &clip.video.path == *path)).cloned().collect();
    for path in gone {
        store.video_filmstrip.strips.remove(&path);
    }
    let mut seen = Vec::new();
    for clip in &clips {
        if seen.contains(&clip.video.path) {
            continue;
        }
        seen.push(clip.video.path.clone());
        if store.video_filmstrip.get(&clip.video.path, "base").is_none() {
            start_strip(store, services, clip, FilmstripWindow::Base);
        }
    }
    let wanted = store.video_filmstrip.wanted.clone();
    for (path, windows) in wanted {
        let Some(clip) = clips.iter().find(|clip| clip.video.path == path) else { continue };
        for window in windows {
            if store.video_filmstrip.get(&path, &window_key(window)).is_none() {
                start_strip(store, services, clip, window);
            }
        }
    }
    drop_unshown_windows(store);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lib::video_program::collection::join_video_edits;
    use crate::lib::video_program::fixtures::*;

    #[test]
    fn frames_are_96_px_tall_at_the_aspect_ratio() {
        assert_eq!(filmstrip_frame_size(VideoResolution { width: 3840, height: 2160 }), (171, 96));
        assert_eq!(filmstrip_frame_size(VideoResolution { width: 1080, height: 1920 }), (54, 96));
    }

    #[test]
    fn picks_the_coarsest_level_that_gives_every_tile_a_frame() {
        assert_eq!(base_spacing_seconds(7200.0), 300.0);
        assert_eq!(base_spacing_seconds(237.0), 10.0);
        assert_eq!(filmstrip_zoom_level(400.0, 7200.0), None);
        assert_eq!(filmstrip_zoom_level(300.0, 7200.0), None);
        assert_eq!(filmstrip_zoom_level(299.0, 7200.0), Some(4));
        assert_eq!(filmstrip_zoom_level(25.0, 7200.0), Some(1));
        assert_eq!(filmstrip_zoom_level(3.0, 7200.0), Some(0));
        assert_eq!(filmstrip_zoom_level(24.0, 600.0), Some(1));
        assert_eq!(filmstrip_zoom_level(1.0, 237.0), None);
    }

    #[test]
    fn lists_the_windows_over_a_stretch() {
        let windows = |level, from, to| filmstrip_zoom_windows(level, from, to, 7200.0).into_iter().map(window_key).collect::<Vec<_>>();
        assert_eq!(windows(0, 200.0, 500.0), ["z0-0", "z0-1", "z0-2"]);
        assert_eq!(windows(0, 240.0, 480.0), ["z0-1"]);
        assert_eq!(windows(0, 7100.0, 9000.0), ["z0-29"]);
        assert_eq!(windows(2, -50.0, 100.0), ["z2-0"]);
        assert!(windows(0, 8000.0, 9000.0).is_empty());
        assert_eq!(window_key(FilmstripWindow::Base), "base");
    }

    #[test]
    fn decodes_on_the_gpu_an_export_can_use() {
        let codecs = |change: fn(&mut AvailableCodecs)| {
            let mut codecs = AvailableCodecs::default();
            change(&mut codecs);
            filmstrip_hwaccel(Some(&codecs))
        };
        assert_eq!(codecs(|codecs| (codecs.nvdec, codecs.vaapi) = (true, true)), Some(FilmstripHwaccel::Cuda));
        assert_eq!(codecs(|codecs| codecs.vaapi = true), Some(FilmstripHwaccel::Vaapi));
        assert_eq!(codecs(|codecs| codecs.qsv = true), Some(FilmstripHwaccel::D3d11va));
        assert_eq!(codecs(|codecs| codecs.hevc_amf = true), Some(FilmstripHwaccel::D3d11va));
        assert_eq!(codecs(|codecs| codecs.videotoolbox = true), Some(FilmstripHwaccel::Videotoolbox));
        assert_eq!(codecs(|_| {}), None);
        assert_eq!(filmstrip_hwaccel(None), None);
    }

    fn strip(start_seconds: f64, spacing_seconds: f64, sample_count: u32) -> Arc<ReadyStrip> {
        Arc::new(ReadyStrip { pixels: Arc::new(Vec::new()), width: 0, height: 0, start_seconds, spacing_seconds, sample_count })
    }

    fn tiles(strips: &[Arc<ReadyStrip>], source: (f64, f64), left: f64, px_per_second: f64, width: f64) -> Vec<(f64, u32, f64)> {
        list_filmstrip_tiles(strips, source.0, source.1, left, px_per_second, 85.0, width).into_iter().map(|tile| (tile.x, tile.sample, tile.strip.spacing_seconds)).collect()
    }

    #[test]
    fn tiles_repeat_frames_zoomed_in_and_skip_them_zoomed_out() {
        let base = strip(0.0, 20.0, 4);
        let list = tiles(std::slice::from_ref(&base), (0.0, 60.0), 0.0, 17.0, 500.0);
        assert_eq!(list.iter().map(|tile| tile.0).collect::<Vec<_>>(), [0.0, 85.0, 170.0, 255.0, 340.0, 425.0]);
        assert_eq!(list.iter().map(|tile| tile.1).collect::<Vec<_>>(), [0, 0, 0, 0, 1, 1]);
        assert_eq!(tiles(std::slice::from_ref(&base), (0.0, 60.0), 0.0, 85.0 / 40.0, 500.0).iter().map(|tile| tile.1).collect::<Vec<_>>(), [0, 2]);
    }

    #[test]
    fn each_tile_takes_the_finest_strip_holding_it() {
        let list = tiles(&[strip(0.0, 20.0, 4), strip(20.0, 5.0, 4)], (0.0, 60.0), 0.0, 17.0, 500.0);
        assert_eq!(list.iter().map(|tile| (tile.2, tile.1)).collect::<Vec<_>>(), [(20.0, 0), (20.0, 0), (20.0, 0), (20.0, 0), (5.0, 0), (5.0, 1)]);
    }

    #[test]
    fn only_visible_tiles_are_listed_and_late_tiles_show_the_last_frame() {
        let base = strip(0.0, 20.0, 4);
        let visible = list_filmstrip_tiles(std::slice::from_ref(&base), 0.0, 60.0, -200.0, 17.0, 85.0, 100.0);
        assert_eq!(visible.iter().map(|tile| (tile.index, tile.x)).collect::<Vec<_>>(), [(2, 170.0), (3, 255.0)]);
        assert_eq!(tiles(&[base], (85.0, 90.0), 0.0, 17.0, 500.0).iter().map(|tile| tile.1).collect::<Vec<_>>(), [3]);
    }

    fn long_clip(offset: f64, path: &'static str, missing: bool) -> Clip {
        let mut video = video(TestVideo { id: "long", path, duration_seconds: 7200.0, ..TestVideo::default() });
        video.missing = missing;
        clip(video, ovrley_core::media::video_edits::VideoEdits { offset_seconds: offset, ..Default::default() })
    }

    #[test]
    fn the_timeline_wants_windows_only_zoomed_in() {
        assert_eq!(tile_width_px(VideoResolution { width: 1920, height: 1080 }, 48.0), 85.0);
        let clips = [long_clip(0.0, "/v/long.mp4", false)];
        assert!(list_wanted_windows(&clips, 0.0, 7200.0, 1000.0, 48.0).is_empty());
        let keys = |wanted: BTreeMap<String, Vec<FilmstripWindow>>| wanted.into_iter().map(|(path, windows)| (path, windows.into_iter().map(window_key).collect::<Vec<_>>())).collect::<Vec<_>>();
        assert_eq!(keys(list_wanted_windows(&clips, 1500.0, 2500.0, 1000.0, 48.0)), [("/v/long.mp4".to_string(), vec!["z3-0".to_string(), "z3-1".to_string()])]);
        let later = [long_clip(1000.0, "/v/long.mp4", false)];
        assert_eq!(keys(list_wanted_windows(&later, 1100.0, 1200.0, 1000.0, 48.0)), [("/v/long.mp4".to_string(), vec!["z0-0".to_string()])]);
        let several = [long_clip(0.0, "/v/long.mp4", false), long_clip(50.0, "/v/long.mp4", false), long_clip(0.0, "/v/missing.mp4", true)];
        assert_eq!(keys(list_wanted_windows(&several, 100.0, 200.0, 1000.0, 48.0)), [("/v/long.mp4".to_string(), vec!["z0-0".to_string()])]);
        assert!(list_wanted_windows(&clips, 100.0, 200.0, 0.0, 48.0).is_empty());
        let _ = join_video_edits;
    }

    mod queue {
        use super::*;
        use crate::store::services::fakes::{fake_services, Fakes};
        use crate::store::slices::video_import::tests::store_with;
        use crate::store::Dispatcher;

        fn jpeg_strip(spacing_seconds: f64, sample_count: u32) -> Filmstrip {
            let mut jpeg = Vec::new();
            image::codecs::jpeg::JpegEncoder::new(&mut jpeg).encode(&[255, 0, 0, 0, 255, 0], 2, 1, image::ExtendedColorType::Rgb8).unwrap();
            Filmstrip { sprite_jpeg: jpeg, start_seconds: 0.0, spacing_seconds, sample_count }
        }

        fn dispatcher(videos: Vec<crate::lib::video_program::ImportedVideo>) -> (Dispatcher, Fakes) {
            let (parts, fakes) = fake_services(serde_json::json!({}));
            (Dispatcher::new(store_with(videos, &[]), parts), fakes)
        }

        fn settle(dispatcher: &Dispatcher, fakes: &Fakes, done: impl Fn(&Store) -> bool) {
            for _ in 0..400 {
                fakes.ui_post.drain(dispatcher);
                if dispatcher.with_store(&done) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("the filmstrip queue did not settle");
        }

        fn state(dispatcher: &Dispatcher, key: &str) -> Option<StripState> {
            dispatcher.with_store(|store| store.video_filmstrip.get("/videos/ride.mp4", key).cloned())
        }

        #[test]
        fn a_cached_base_strip_loads_without_a_decode() {
            let (dispatcher, fakes) = dispatcher(vec![simple("ride", 600.0, 0)]);
            fakes.video_tools.cached.lock().unwrap().insert("base".into(), jpeg_strip(25.0, 24));
            dispatcher.dispatch(|_, _| {});
            settle(&dispatcher, &fakes, |store| matches!(store.video_filmstrip.get("/videos/ride.mp4", "base"), Some(StripState::Ready(_))));
            let Some(StripState::Ready(strip)) = state(&dispatcher, "base") else { unreachable!() };
            assert_eq!((strip.width, strip.height, strip.sample_count), (2, 1, 24));
            assert!(fakes.video_tools.decodes.lock().unwrap().is_empty());
        }

        #[test]
        fn a_missed_strip_decodes_once_the_codecs_are_known_and_a_removed_video_drops_its_strips() {
            let (dispatcher, fakes) = dispatcher(vec![simple("ride", 600.0, 0)]);
            fakes.video_tools.codecs.lock().unwrap().videotoolbox = true;
            fakes.video_tools.decoded.lock().unwrap().insert("base".into(), jpeg_strip(25.0, 24));
            dispatcher.dispatch(|_, _| {});
            settle(&dispatcher, &fakes, |store| matches!(store.video_filmstrip.get("/videos/ride.mp4", "base"), Some(StripState::Ready(_))));
            assert_eq!(*fakes.video_tools.decodes.lock().unwrap(), [("base".to_string(), Some(FilmstripHwaccel::Videotoolbox))]);
            dispatcher.dispatch(|store, _| crate::features::player::remove_video(store, "ride").unwrap());
            assert!(dispatcher.with_store(|store| store.video_filmstrip.strips.is_empty()));
        }

        #[test]
        fn a_strip_that_decodes_nothing_fails_and_is_not_retried() {
            let (dispatcher, fakes) = dispatcher(vec![simple("ride", 600.0, 0)]);
            dispatcher.dispatch(|_, _| {});
            settle(&dispatcher, &fakes, |store| store.video_filmstrip.get("/videos/ride.mp4", "base") == Some(&StripState::Failed));
            dispatcher.dispatch(|_, _| {});
            assert_eq!(fakes.video_tools.decodes.lock().unwrap().len(), 1);
        }

        #[test]
        fn wanted_windows_publish_after_the_view_settles() {
            let (dispatcher, fakes) = dispatcher(vec![simple("ride", 7200.0, 0)]);
            dispatcher.dispatch(|store, _| {
                store.player.viewport.metrics.width_px = 1000.0;
                store.player.viewport.metrics.rem = 16.0;
            });
            dispatcher.dispatch(|store, _| store.editor.set_timeline_viewport(crate::store::slices::editor::TimelineViewport { view_start: 1500.0, view_end: 2500.0 }).unwrap());
            assert!(dispatcher.with_store(|store| store.video_filmstrip.wanted.is_empty() && store.filmstrip_queue.wanted_deadline().is_some()));
            let deadline = dispatcher.with_store(|store| store.filmstrip_queue.wanted_deadline().unwrap());
            dispatcher.dispatch(move |store, _| publish_wanted_windows(store, deadline - Duration::from_millis(1)));
            assert!(dispatcher.with_store(|store| store.video_filmstrip.wanted.is_empty()), "not before the view stood still");
            dispatcher.dispatch(move |store, _| publish_wanted_windows(store, deadline));
            assert_eq!(dispatcher.with_store(|store| store.video_filmstrip.wanted["/videos/ride.mp4"].len()), 2);
            settle(&dispatcher, &fakes, |store| store.video_filmstrip.strips.get("/videos/ride.mp4").is_some_and(|strips| strips.values().all(|strip| *strip == StripState::Failed)));
        }
    }
}
