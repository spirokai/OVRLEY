//! Video playback (Phase 6B spike, branch spike/mdk-macos, never merged): MDK renders the video
//! into a wgpu texture on the shared device and queue (Metal on macOS, DX12 on Windows), shown as
//! the canvas's `video-frame`.
//!
//! `OVRLEY_PLAYBACK`:
//! - `1`: MDK is the video clock inside clips and follows the store elsewhere (steps 2 and 4; D10
//!   revised: the timeline clock plays only outside clips, as in React).
//! - `loop`: plays the first imported video (or `OVRLEY_PLAYBACK_FILE`) on a loop, ignoring the
//!   store (step 3).
//!
//! `OVRLEY_PLAYBACK_SIZE=canvas` sizes the texture to the canvas in physical pixels (step 6);
//! `OVRLEY_PLAYBACK_SIZE=<w>x<h>` fixes it at that size (e.g. 16x16 makes MDK's draw nearly free,
//! to measure its GPU cost); otherwise it is a fixed 1920×1080. `MDK_DECODERS` overrides the decoders (step 5).
//!
//! Windows: `OVRLEY_PLAYBACK_TARGET=d3d11` renders through D3D11 instead: MDK
//! draws into a texture of our own D3D11 device on the same adapter, which the DX12 device opens
//! through a shared handle, with a shared fence between the two (see [`d3d11`]). For D3D11
//! decoders, which otherwise cross into MDK's D3D12 renderer.
//!
//! Everything here runs on the UI thread, which owns the GPU queue (D7): MDK's render callback
//! only posts a render to it.

use crate::platform::gpu::GpuDevice;
use crate::{AppWindow, OverlayEditor};
use ovrley_app::features::player::playback;
use ovrley_app::lib::video_program::program::{resolve_preview_target, PlaybackSource};
use ovrley_app::store::slices::editor::PreviewPlaybackState;
use ovrley_app::store::{DispatchHandle, Store};
use ovrley_playback::player::{Event, State};
use slint::wgpu_30::wgpu;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FIXED_SIZE: (u32, u32) = (1920, 1080);
/// A canvas size counts as settled after this long without a change (step 6).
const RESIZE_SETTLE: Duration = Duration::from_millis(150);

thread_local! {
    static VIDEO_PLAYBACK: RefCell<Option<VideoPlayback>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Follow,
    Loop,
}

struct Target {
    texture: wgpu::Texture,
    size: (u32, u32),
    /// The D3D11 texture MDK draws into, when the target is D3D11 (it backs `texture`).
    #[cfg(windows)]
    _d3d11_texture: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
}

/// What the follower last told MDK.
#[derive(Default)]
struct Follow {
    playing: bool,
    /// The piece whose video MDK plays as the clock, and its last frame's source second.
    piece: Option<String>,
    piece_last_frame_second: Option<f64>,
    /// The last new frame while MDK is the clock, and when it was drawn.
    last_frame: Option<(f64, Instant)>,
    last_seek_ms: Option<i64>,
    last_state: Option<PreviewPlaybackState>,
    muted: Option<bool>,
}

/// Per-second render and drift statistics (steps 3 and 4).
struct Stats {
    fps: f64,
    frames: u32,
    /// `renderVideo()` on the UI thread: total and worst this second.
    render_time: Duration,
    render_worst: Duration,
    renders: u32,
    empty: u32,
    dropped: u32,
    max_gap: f64,
    last_ts: Option<f64>,
    drift_samples: u32,
    drift_last: f64,
    drift_max: f64,
    /// Since playback started, for the drift over the clip's length.
    drift_total_max: f64,
    /// MDK's delivered-frame count at the last flush.
    delivered_seen: u64,
}

impl Stats {
    fn new(fps: f64) -> Self {
        Self {
            fps,
            frames: 0,
            render_time: Duration::ZERO,
            render_worst: Duration::ZERO,
            renders: 0,
            empty: 0,
            dropped: 0,
            max_gap: 0.0,
            last_ts: None,
            drift_samples: 0,
            drift_last: 0.0,
            drift_max: 0.0,
            drift_total_max: 0.0,
            delivered_seen: 0,
        }
    }

    fn record_render(&mut self, ts: f64) {
        if ts < 0.0 {
            self.empty += 1;
            return;
        }
        if let Some(last) = self.last_ts {
            if ts == last {
                self.empty += 1;
                return;
            }
            let gap = ts - last;
            // A backwards step is a seek or a loop, not a gap.
            if gap > 0.0 {
                self.max_gap = self.max_gap.max(gap);
                let missed = (gap * self.fps).round() as i64 - 1;
                if missed > 0 && gap < 1.0 {
                    self.dropped += missed as u32;
                }
            }
        }
        self.frames += 1;
        self.last_ts = Some(ts);
    }

    fn record_drift(&mut self, drift_ms: f64) {
        self.drift_samples += 1;
        self.drift_last = drift_ms;
        if drift_ms.abs() > self.drift_max.abs() {
            self.drift_max = drift_ms;
        }
        if drift_ms.abs() > self.drift_total_max.abs() {
            self.drift_total_max = drift_ms;
        }
    }

    fn flush(&mut self, position_ms: i64, delivered: u64) {
        log::info!(
            "playback: MDK delivered {} decoded frames this second",
            delivered - self.delivered_seen
        );
        self.delivered_seen = delivered;
        if self.renders > 0 {
            log::info!(
                "playback: renderVideo {} calls, mean {:.2} ms, worst {:.2} ms",
                self.renders,
                self.render_time.as_secs_f64() * 1000.0 / f64::from(self.renders),
                self.render_worst.as_secs_f64() * 1000.0
            );
        }
        if self.frames > 0 || self.dropped > 0 {
            log::info!(
                "playback: {} frames/s, largest gap {:.1} ms, dropped {}, empty renders {}, position {} ms",
                self.frames,
                self.max_gap * 1000.0,
                self.dropped,
                self.empty,
                position_ms
            );
        }
        if self.drift_samples > 0 {
            log::info!(
                "playback: render lag (MDK position − drawn frame) last {:+.1} ms, worst this second {:+.1} ms, worst since play {:+.1} ms",
                self.drift_last,
                self.drift_max,
                self.drift_total_max
            );
        }
        self.frames = 0;
        self.render_time = Duration::ZERO;
        self.render_worst = Duration::ZERO;
        self.renders = 0;
        self.empty = 0;
        self.dropped = 0;
        self.max_gap = 0.0;
        self.drift_samples = 0;
        self.drift_max = 0.0;
    }
}

struct VideoPlayback {
    window: slint::Weak<AppWindow>,
    dispatch: DispatchHandle,
    gpu: GpuDevice,
    mode: Mode,
    canvas_sized: bool,
    // Dropped before `target` and `d3d11`: the player releases its GPU resources first.
    player: ovrley_playback::player::Player,
    target: Option<Target>,
    /// `OVRLEY_PLAYBACK_TARGET=d3d11`: the D3D11 device MDK draws on.
    #[cfg(windows)]
    d3d11: Option<d3d11::Device>,
    pending: Arc<AtomicBool>,
    seek_started: Arc<Mutex<std::collections::VecDeque<(Instant, i64, bool)>>>,
    /// While dragging, one seek at a time; the latest target waits here (latest wins).
    scrub_waiting: Option<i64>,
    /// `OVRLEY_PLAYBACK_SCRUB=keyframe` scrubs on key frames; otherwise exactly.
    scrub_exact: bool,
    media: Option<String>,
    follow: Follow,
    stats: Stats,
    /// The canvas size last seen, and since when (step 6).
    seen_size: Option<((u32, u32), Instant)>,
    _timers: [slint::Timer; 2],
}

/// Starts playback when `OVRLEY_PLAYBACK` is set. Missing setup fails loudly in the log.
pub fn start_if_requested(window: &AppWindow, gpu: Option<&GpuDevice>, dispatch: &DispatchHandle) {
    let Ok(mode) = std::env::var("OVRLEY_PLAYBACK") else {
        return;
    };
    let mode = match mode.as_str() {
        "1" => Mode::Follow,
        "loop" => Mode::Loop,
        other => panic!("OVRLEY_PLAYBACK={other}: expected 1 or loop"),
    };
    let Some(gpu) = gpu else {
        log::error!("playback: no shared GPU device; playback needs Metal or DX12");
        return;
    };
    ovrley_playback::player::install_log_handler();
    let decoders = ovrley_playback::player::decoders_from_env();
    let (canvas_sized, initial_size) = match std::env::var("OVRLEY_PLAYBACK_SIZE") {
        Err(_) => (false, FIXED_SIZE),
        Ok(value) if value == "canvas" => (true, FIXED_SIZE),
        Ok(value) => {
            let parsed = value
                .split_once('x')
                .and_then(|(width, height)| {
                    Some((width.parse::<u32>().ok()?, height.parse::<u32>().ok()?))
                })
                .filter(|&(width, height)| width > 0 && height > 0);
            (
                false,
                parsed.unwrap_or_else(|| {
                    panic!("OVRLEY_PLAYBACK_SIZE={value}: expected canvas or <width>x<height>")
                }),
            )
        }
    };
    #[cfg(windows)]
    let d3d11 = match std::env::var("OVRLEY_PLAYBACK_TARGET").as_deref() {
        Err(_) | Ok("d3d12") => None,
        Ok("d3d11") => Some(
            d3d11::Device::new(gpu).expect("a D3D11 device sharing textures with the DX12 device"),
        ),
        Ok(other) => panic!("OVRLEY_PLAYBACK_TARGET={other}: expected d3d11 or d3d12"),
    };
    log::info!(
        "playback: MDK {}, mode {mode:?}, decoders {decoders:?}, canvas-sized {canvas_sized}",
        ovrley_playback::player::version()
    );

    let pending = Arc::new(AtomicBool::new(false));
    let seek_started: Arc<Mutex<std::collections::VecDeque<(Instant, i64, bool)>>> = Arc::default();
    let render_pending = Arc::clone(&pending);
    let seeks = Arc::clone(&seek_started);
    let player = ovrley_playback::player::Player::new(
        &decoders,
        ovrley_playback::player::Callbacks {
            // D7: posts coalesce while one is pending.
            render: Box::new(move || {
                if !render_pending.swap(true, Ordering::AcqRel) {
                    if let Err(error) = slint::invoke_from_event_loop(render_posted) {
                        log::error!("playback: could not post a render: {error}");
                    }
                }
            }),
            event: Box::new(move |event| on_event(event, &seeks)),
        },
    );

    let stats_timer = slint::Timer::default();
    stats_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), || {
        with_video_playback(|state| {
            let position = state.player.position();
            let delivered = state.player.delivered_frames();
            state.stats.flush(position, delivered);
        })
    });
    let resize_timer = slint::Timer::default();
    resize_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(50),
        || {
            with_video_playback(VideoPlayback::check_size);
            check_end();
        },
    );

    let state = VideoPlayback {
        window: window.as_weak(),
        dispatch: dispatch.clone(),
        gpu: gpu.clone(),
        mode,
        canvas_sized,
        player,
        target: None,
        #[cfg(windows)]
        d3d11,
        pending,
        seek_started,
        scrub_waiting: None,
        scrub_exact: std::env::var("OVRLEY_PLAYBACK_SCRUB").as_deref() != Ok("keyframe"),
        media: None,
        follow: Follow::default(),
        stats: Stats::new(30.0),
        seen_size: None,
        _timers: [stats_timer, resize_timer],
    };
    VIDEO_PLAYBACK.with(|cell| *cell.borrow_mut() = Some(state));
    with_video_playback(|state| state.set_target(initial_size));
    if mode == Mode::Follow {
        // The video clock (Phase 6B): MDK owns playback inside clips, as the <video> element does in
        // React; the timeline clock plays outside them.
        dispatch.dispatch(|store, _| store.player.video_clock_available = true);
    } else {
        dispatch.with_store(project);
    }
}

/// Runs after each dispatch (from the projections).
pub fn project(store: &Store) {
    with_video_playback(|state| match state.mode {
        Mode::Follow => state.follow(store),
        Mode::Loop => state.start_loop(store),
    });
}

/// Drops the player (on the UI thread, before the device) when the window closed.
pub fn shutdown() {
    let state = VIDEO_PLAYBACK.with(|cell| cell.borrow_mut().take());
    if let Some(state) = state {
        let started = Instant::now();
        drop(state);
        log::info!("playback: torn down in {:?}", started.elapsed());
    }
}

fn with_video_playback(run: impl FnOnce(&mut VideoPlayback)) {
    VIDEO_PLAYBACK.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            run(state);
        }
    });
}

fn render_posted() {
    let frame = VIDEO_PLAYBACK.with(|cell| {
        let mut cell = cell.borrow_mut();
        let state = cell.as_mut()?;
        state.pending.store(false, Ordering::Release);
        state.render()
    });
    // Outside the borrow: the dispatch projects, which comes back into the video playback state.
    if let Some((dispatch, piece, second)) = frame {
        dispatch.dispatch(move |store, _| {
            playback::video_clock_frame(store, &piece, second, false, Instant::now())
        });
    }
}

/// With `continue_at_end` MDK reports no end status: the video clock has ended when it plays,
/// its last frame is within two frames of the piece's last, and no frame came for 100 ms.
fn check_end() {
    let ended = VIDEO_PLAYBACK.with(|cell| {
        let mut cell = cell.borrow_mut();
        let state = cell.as_mut()?;
        let piece = state.follow.piece.clone()?;
        let (last_frame, at) = state.follow.last_frame?;
        let piece_last = state.follow.piece_last_frame_second?;
        if at.elapsed() < Duration::from_millis(100) || last_frame < piece_last - 2.0 / state.stats.fps {
            return None;
        }
        log::info!("playback: the video clock ended at {last_frame:.3} s (piece's last frame {piece_last:.3} s)");
        state.follow.last_frame = None;
        Some((state.dispatch.clone(), piece, last_frame))
    });
    if let Some((dispatch, piece, second)) = ended {
        dispatch.dispatch(move |store, _| {
            playback::video_clock_frame(store, &piece, second, true, Instant::now())
        });
    }
}

/// A seek finished: the waiting scrub target, if any, goes next.
fn seek_finished() {
    with_video_playback(|state| {
        if let Some(ms) = state.scrub_waiting.take() {
            state.seek(ms, state.scrub_exact);
        }
    });
}

fn on_event(event: Event, seeks: &Mutex<std::collections::VecDeque<(Instant, i64, bool)>>) {
    match event {
        Event::Seek { result } => {
            if let Err(error) = slint::invoke_from_event_loop(seek_finished) {
                log::error!("playback: could not post a seek result: {error}");
            }
            let started = seeks.lock().expect("seek mutex").pop_front();
            match started {
                Some((at, target, exact)) => {
                    log::info!(
                        "playback: seek to {target} ms ({}) → {result} in {:?}",
                        if exact { "exact" } else { "key frame" },
                        at.elapsed()
                    )
                }
                None => log::info!("playback: seek → {result}"),
            }
        }
        Event::Status { old, new } => {
            log::debug!("playback: status {old:#x} → {new:#x}");
            if new & ovrley_playback::player::status::INVALID != 0 {
                log::error!("playback: MDK could not open the media");
            }
        }
        Event::Media {
            error,
            category,
            detail,
        } => {
            if error < 0 {
                log::error!("playback: event {category} {detail} ({error})");
            } else {
                log::info!("playback: event {category} {detail} ({error})");
            }
        }
    }
}

/// Hands MDK the shared Metal device and queue and the texture (D5).
///
/// # Safety
/// The device, queue and texture must outlive the player or its next target.
#[cfg(target_os = "macos")]
unsafe fn set_render_target(
    gpu: &GpuDevice,
    player: &ovrley_playback::player::Player,
    texture: &wgpu::Texture,
) {
    type Metal = wgpu::wgc::api::Metal;
    let device =
        unsafe { gpu.device.as_hal::<Metal>() }.expect("the shared device is Metal (gpu.rs)");
    let queue = unsafe { gpu.queue.as_hal::<Metal>() }.expect("the shared queue is Metal");
    let raw_texture = unsafe { texture.as_hal::<Metal>() }.expect("a Metal texture");
    unsafe {
        player.set_metal_target(
            &**device.raw_device() as *const _ as *const c_void,
            queue.as_raw() as *const _ as *const c_void,
            raw_texture.raw_handle() as *const _ as *const c_void,
        )
    };
}

/// Hands MDK the shared DX12 queue and the texture's resource (D5). MDK submits its draw on that
/// queue, ahead of Slint's next frame, which samples the texture.
///
/// # Safety
/// The queue and texture must outlive the player or its next target.
#[cfg(windows)]
unsafe fn set_render_target(
    gpu: &GpuDevice,
    player: &ovrley_playback::player::Player,
    texture: &wgpu::Texture,
) {
    use windows_core::Interface;
    type Dx12 = wgpu::wgc::api::Dx12;
    let queue = unsafe { gpu.queue.as_hal::<Dx12>() }.expect("the shared queue is DX12 (gpu.rs)");
    let resource = unsafe { texture.as_hal::<Dx12>() }.expect("a DX12 texture");
    unsafe {
        player.set_d3d12_target(
            Interface::as_raw(queue.as_raw()) as *const c_void,
            Interface::as_raw(resource.raw_resource()) as *const c_void,
        )
    };
}

impl VideoPlayback {
    /// Renders into the target. While MDK is the clock, a new frame is returned for the store.
    fn render(&mut self) -> Option<(DispatchHandle, String, f64)> {
        self.target.as_ref()?;
        let started = Instant::now();
        #[cfg(windows)]
        if let Some(d3d11) = self.d3d11.as_mut() {
            d3d11.before_render();
        }
        let ts = self.player.render();
        #[cfg(windows)]
        if let Some(d3d11) = self.d3d11.as_mut() {
            d3d11.after_render();
        }
        let took = started.elapsed();
        self.stats.render_time += took;
        self.stats.render_worst = self.stats.render_worst.max(took);
        self.stats.renders += 1;
        let new_frame = ts >= 0.0 && self.stats.last_ts != Some(ts);
        self.stats.record_render(ts);
        if new_frame && self.follow.playing {
            // How far MDK's clock (audio-led) is ahead of the frame just drawn.
            self.stats
                .record_drift(self.player.position() as f64 - ts * 1000.0);
        }
        if let Some(window) = self.window.upgrade() {
            window.window().request_redraw();
        }
        let piece = self.follow.piece.clone().filter(|_| new_frame)?;
        self.follow.last_frame = Some((ts, Instant::now()));
        Some((self.dispatch.clone(), piece, ts))
    }

    /// Creates the texture MDK renders into and hands it to MDK and Slint (D5, D6).
    fn set_target(&mut self, size: (u32, u32)) {
        #[cfg(windows)]
        let mut d3d11_texture = None;
        #[cfg(windows)]
        let texture = match &self.d3d11 {
            Some(d3d11) => {
                let (shared, texture) = d3d11
                    .create_target(&self.gpu, size)
                    .expect("a D3D11 texture shared with the DX12 device");
                // SAFETY: the texture lives in `self.target` until the next target is set.
                unsafe {
                    self.player
                        .set_d3d11_target(windows_core::Interface::as_raw(&shared) as *const c_void)
                };
                d3d11_texture = Some(shared);
                texture
            }
            None => self.create_shared_device_target(size),
        };
        #[cfg(not(windows))]
        let texture = self.create_shared_device_target(size);
        self.player.set_surface_size(size.0 as i32, size.1 as i32);
        let image = slint::Image::try_from(texture.clone())
            .expect("an Rgba8Unorm texture with TEXTURE_BINDING | RENDER_ATTACHMENT");
        if let Some(window) = self.window.upgrade() {
            window.global::<OverlayEditor>().set_video_frame(image);
        }
        log::info!("playback: render target {}×{}", size.0, size.1);
        self.target = Some(Target {
            texture,
            size,
            #[cfg(windows)]
            _d3d11_texture: d3d11_texture,
        });
        // Paused, nothing else redraws the current frame into the new texture.
        let _ = self.render();
    }

    /// A texture on the shared device, handed to MDK as its Metal or D3D12 target.
    fn create_shared_device_target(&self, size: (u32, u32)) -> wgpu::Texture {
        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video playback frame"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // SAFETY: the device and queue outlive the player (`self.gpu`), and the texture lives in
        // `self.target` until the next target is set.
        unsafe { set_render_target(&self.gpu, &self.player, &texture) };
        texture
    }

    /// Step 6: follows the canvas's physical size once it settled.
    fn check_size(&mut self) {
        if !self.canvas_sized {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let editor = window.global::<OverlayEditor>();
        let scale = window.window().scale_factor();
        let size = (
            (editor.get_scene_width() * scale).round().max(2.0) as u32,
            (editor.get_scene_height() * scale).round().max(2.0) as u32,
        );
        let now = Instant::now();
        match self.seen_size {
            Some((seen, _)) if seen != size => self.seen_size = Some((size, now)),
            None => self.seen_size = Some((size, now)),
            Some((seen, since)) => {
                let current = self.target.as_ref().map(|target| target.size);
                if current != Some(seen) && now.duration_since(since) >= RESIZE_SETTLE {
                    self.set_target(seen);
                    let _ = &self.target.as_ref().map(|target| &target.texture);
                }
            }
        }
    }

    fn seek(&mut self, ms: i64, exact: bool) {
        // A release or pause target replaces a waiting scrub target; the scrub paths take theirs first.
        if exact {
            self.scrub_waiting = None;
        }
        self.seek_started
            .lock()
            .expect("seek mutex")
            .push_back((Instant::now(), ms, exact));
        if !self.player.seek(ms, exact) {
            log::warn!("playback: MDK refused a seek to {ms} ms");
            // No result will come for it.
            self.seek_started.lock().expect("seek mutex").pop_back();
        }
        self.follow.last_seek_ms = Some(ms);
    }

    fn open(&mut self, path: &str, start_ms: i64, fps: f64) {
        log::info!("playback: opening {path} at {start_ms} ms");
        self.player.open(path, start_ms);
        self.media = Some(path.to_string());
        self.follow = Follow {
            last_seek_ms: Some(start_ms),
            ..Follow::default()
        };
        self.stats = Stats::new(fps);
        self.stats.delivered_seen = self.player.delivered_frames();
    }

    /// Step 3: the first imported video (or `OVRLEY_PLAYBACK_FILE`) on a loop.
    fn start_loop(&mut self, store: &Store) {
        if self.media.is_some() {
            return;
        }
        let (path, fps) = match std::env::var("OVRLEY_PLAYBACK_FILE") {
            Ok(path) => (path, 30.0),
            Err(_) => match store.video_import.imported_videos.first() {
                Some(video) => (video.path.clone(), video.fps.as_f64()),
                None => return,
            },
        };
        self.open(&path, 0, fps);
        self.player.set_loop(-1);
        self.player.set_state(State::Playing);
    }

    /// Step 4: inside a clip MDK is the video clock and the store follows its frames
    /// (`video_clock_frame`); paused, scrubbing or on the timeline clock, MDK follows the store.
    fn follow(&mut self, store: &Store) {
        let program = &store.player.timeline.timeline().committed.program;
        let second = store.editor.selected_second;
        let Some(target) = resolve_preview_target(program, second) else {
            return;
        };
        let video = store
            .video_import
            .imported_videos
            .iter()
            .find(|video| video.id == target.video_id)
            .expect("a preview target's video is imported");
        if video.missing {
            return;
        }
        let source_ms = (target.source_second * 1000.0).round() as i64;
        let state = store.editor.preview_playback_state;
        let video_clock = state == PreviewPlaybackState::Playing
            && store.editor.preview_playback_source == PlaybackSource::Video
            && target.is_inside;

        if self.media.as_deref() != Some(video.path.as_str()) {
            self.open(&video.path.clone(), source_ms, video.fps.as_f64());
            self.player.set_state(State::Paused);
        }
        let muted = store.editor.is_video_muted;
        if self.follow.muted != Some(muted) {
            self.player.set_mute(muted);
            self.follow.muted = Some(muted);
        }

        if video_clock {
            if self.follow.piece.as_deref() != Some(target.piece.id.as_str()) {
                // A new piece: from its frame at the playhead (a cut's next piece starts elsewhere
                // in the file).
                if self.follow.last_seek_ms != Some(source_ms) {
                    self.seek(source_ms, true);
                }
                self.follow.piece = Some(target.piece.id.clone());
                self.follow.piece_last_frame_second = Some(
                    target
                        .piece
                        .fps
                        .frames_to_seconds(target.piece.source_end_frame - 1),
                );
                self.follow.last_frame = None;
                if !self.follow.playing {
                    self.player.set_state(State::Playing);
                    self.follow.playing = true;
                    self.stats.drift_total_max = 0.0;
                }
                log::info!(
                    "playback: video clock plays piece {} from {source_ms} ms",
                    target.piece.id
                );
            }
        } else {
            if self.follow.playing {
                self.player.set_state(State::Paused);
                self.follow.playing = false;
                self.follow.piece = None;
                self.follow.last_frame = None;
                log::info!(
                    "playback: paused at {source_ms} ms (MDK at {} ms), {state:?}",
                    self.player.position()
                );
                // The store decides where the pause lands.
                self.follow.last_seek_ms = None;
            }
            match state {
                PreviewPlaybackState::Scrubbing => {
                    let in_flight = !self.seek_started.lock().expect("seek mutex").is_empty();
                    if in_flight {
                        self.scrub_waiting = Some(source_ms);
                    } else if self.follow.last_seek_ms != Some(source_ms) {
                        self.seek(source_ms, self.scrub_exact);
                    }
                }
                _ => {
                    let committed_scrub =
                        self.follow.last_state == Some(PreviewPlaybackState::Scrubbing);
                    if committed_scrub || self.follow.last_seek_ms != Some(source_ms) {
                        self.seek(source_ms, true);
                    }
                }
            }
        }
        self.follow.last_state = Some(state);
    }
}

/// The D3D11 target (`OVRLEY_PLAYBACK_TARGET=d3d11`) moves D3D11 frames into wgpu a D3D11 device on the shared device's adapter, whose
/// textures the DX12 device opens through NT handles. One fence, shared by both, orders the two
/// queues on the GPU around each `renderVideo()`; the UI thread never waits.
#[cfg(windows)]
mod d3d11 {
    use super::GpuDevice;
    use slint::wgpu_30::wgpu;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE, HMODULE};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_1};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Device5, ID3D11DeviceContext, ID3D11DeviceContext4,
        ID3D11Fence, ID3D11Multithread, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_FENCE_FLAG_SHARED, D3D11_RESOURCE_MISC_SHARED,
        D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12CommandQueue, ID3D12Device, ID3D12Fence, ID3D12Resource,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory4, IDXGIResource1, DXGI_SHARED_RESOURCE_READ,
        DXGI_SHARED_RESOURCE_WRITE,
    };

    type Dx12 = wgpu::wgc::api::Dx12;

    pub struct Device {
        device: ID3D11Device,
        context: ID3D11DeviceContext4,
        device12: ID3D12Device,
        queue: ID3D12CommandQueue,
        fence11: ID3D11Fence,
        fence12: ID3D12Fence,
        value: u64,
    }

    /// Opens an NT handle on the DX12 device and closes it.
    fn open_shared<T: Interface>(
        device12: &ID3D12Device,
        handle: HANDLE,
    ) -> windows::core::Result<T> {
        let mut opened: Option<T> = None;
        let result = unsafe { device12.OpenSharedHandle(handle, &mut opened) };
        unsafe { CloseHandle(handle) }?;
        result?;
        Ok(opened.expect("OpenSharedHandle succeeded"))
    }

    impl Device {
        pub fn new(gpu: &GpuDevice) -> windows::core::Result<Self> {
            let device12 = unsafe { gpu.device.as_hal::<Dx12>() }
                .expect("the shared device is DX12 (gpu.rs)")
                .raw_device()
                .clone();
            let queue = unsafe { gpu.queue.as_hal::<Dx12>() }
                .expect("the shared queue is DX12")
                .as_raw()
                .clone();
            let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1() }?;
            let adapter: IDXGIAdapter =
                unsafe { factory.EnumAdapterByLuid(device12.GetAdapterLuid()) }?;

            let mut device = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            unsafe {
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            }?;
            let device: ID3D11Device = device.expect("D3D11CreateDevice succeeded");
            let context: ID3D11DeviceContext4 =
                context.expect("D3D11CreateDevice succeeded").cast()?;
            // MDK's decoder threads share the immediate context with renderVideo() on the UI thread.
            let _ = unsafe {
                device
                    .cast::<ID3D11Multithread>()?
                    .SetMultithreadProtected(true)
            };

            let mut fence11: Option<ID3D11Fence> = None;
            unsafe {
                device.cast::<ID3D11Device5>()?.CreateFence(
                    0,
                    D3D11_FENCE_FLAG_SHARED,
                    &mut fence11,
                )
            }?;
            let fence11 = fence11.expect("CreateFence succeeded");
            let handle =
                unsafe { fence11.CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null()) }?;
            let fence12: ID3D12Fence = open_shared(&device12, handle)?;
            log::info!(
                "playback: D3D11 target device on the shared adapter, fence shared with DX12"
            );
            Ok(Self {
                device,
                context,
                device12,
                queue,
                fence11,
                fence12,
                value: 0,
            })
        }

        /// A D3D11 render target MDK draws into, and the same memory as a wgpu texture Slint samples.
        pub fn create_target(
            &self,
            gpu: &GpuDevice,
            size: (u32, u32),
        ) -> windows::core::Result<(ID3D11Texture2D, wgpu::Texture)> {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size.0,
                Height: size.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0)
                    as u32,
            };
            let mut texture = None;
            unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }?;
            let texture: ID3D11Texture2D = texture.expect("CreateTexture2D succeeded");
            let handle = unsafe {
                texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                    None,
                    (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                    PCWSTR::null(),
                )
            }?;
            let resource: ID3D12Resource = open_shared(&self.device12, handle)?;

            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let hal = unsafe {
                wgpu::hal::dx12::Device::texture_from_raw(
                    resource,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::TextureDimension::D2,
                    extent,
                    1,
                    1,
                )
            };
            // Slint only samples it. A shared texture starts in COMMON, which D3D12 promotes to a
            // shader-read state implicitly and decays back after each submission, so wgpu's tracked
            // state (sampled) never needs a barrier that would hold it against D3D11.
            let texture12 = unsafe {
                gpu.device.create_texture_from_hal::<Dx12>(
                    hal,
                    &wgpu::TextureDescriptor {
                        label: Some("video playback frame (D3D11)"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    },
                    wgpu::TextureUses::RESOURCE,
                )
            };
            Ok((texture, texture12))
        }

        /// Before MDK draws: D3D11 waits on the GPU for what DX12 already submitted (Slint's last
        /// frame, which samples the texture MDK is about to overwrite).
        pub fn before_render(&mut self) {
            self.value += 1;
            unsafe {
                self.queue
                    .Signal(&self.fence12, self.value)
                    .expect("signal the shared fence on DX12");
                self.context
                    .Wait(&self.fence11, self.value)
                    .expect("wait on the shared fence on D3D11");
            }
        }

        /// After MDK drew: DX12 waits on the GPU for D3D11's draw before Slint's next frame samples it.
        pub fn after_render(&mut self) {
            self.value += 1;
            unsafe {
                self.context
                    .Signal(&self.fence11, self.value)
                    .expect("signal the shared fence on D3D11");
                self.context.Flush();
                self.queue
                    .Wait(&self.fence12, self.value)
                    .expect("wait on the shared fence on DX12");
            }
        }
    }
}
