//! Video playback (Phase 6B spike, branch spike/mdk-macos, never merged): MDK renders the video
//! into a wgpu texture on the shared device and queue (Metal on macOS, DX12 on Windows, Vulkan on
//! Linux through OpenGL), shown as the canvas's `video-frame`.
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
//! Linux: MDK renders with OpenGL into Vulkan memory the shared device exports, with shared
//! semaphores between the two (see [`vulkan_gl`]).
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
#[cfg(any(target_os = "macos", windows))]
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
    /// The canvas's `video-frame`: the texture as a Slint image.
    image: slint::Image,
    size: (u32, u32),
    /// The D3D11 texture MDK draws into, when the target is D3D11 (it backs `texture`).
    #[cfg(windows)]
    _d3d11_texture: Option<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    /// MDK's framebuffer over the texture's memory.
    #[cfg(target_os = "linux")]
    gl: vulkan_gl::Target,
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
    /// Linux: the bridge holds the device and queue it needs.
    #[cfg(any(target_os = "macos", windows))]
    gpu: GpuDevice,
    mode: Mode,
    canvas_sized: bool,
    // Dropped before `target`, `d3d11` and `bridge`: the player releases its GPU resources first.
    player: ovrley_playback::player::Player,
    target: Option<Target>,
    /// `OVRLEY_PLAYBACK_TARGET=d3d11`: the D3D11 device MDK draws on.
    #[cfg(windows)]
    d3d11: Option<d3d11::Device>,
    /// The OpenGL context MDK draws in, current on this thread until it is dropped.
    #[cfg(target_os = "linux")]
    bridge: vulkan_gl::Bridge,
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
        log::error!("playback: no shared GPU device; playback needs Metal, DX12 or Vulkan");
        return;
    };
    #[cfg(target_os = "linux")]
    let bridge = match vulkan_gl::Bridge::new(gpu) {
        Ok(bridge) => bridge,
        Err(error) => {
            log::error!("playback: no Vulkan–OpenGL bridge on this GPU: {error}");
            return;
        }
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
        #[cfg(any(target_os = "macos", windows))]
        gpu: gpu.clone(),
        mode,
        canvas_sized,
        player,
        target: None,
        #[cfg(windows)]
        d3d11,
        #[cfg(target_os = "linux")]
        bridge,
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
        #[cfg(target_os = "linux")]
        if let Some(target) = self.target.as_ref() {
            self.bridge.before_render(&target.gl);
        }
        let ts = self.player.render();
        #[cfg(windows)]
        if let Some(d3d11) = self.d3d11.as_mut() {
            d3d11.after_render();
        }
        #[cfg(target_os = "linux")]
        if let Some(target) = self.target.as_mut() {
            self.bridge.after_render(&mut target.gl);
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
        // MDK drew into the same texture. Setting its image again marks the canvas's video item
        // dirty (Slint never counts two wgpu-texture images as equal), so partial rendering
        // repaints the video's area; a redraw request alone repaints nothing.
        if let (Some(window), Some(target)) = (self.window.upgrade(), self.target.as_ref()) {
            window.global::<OverlayEditor>().set_video_frame(target.image.clone());
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
        #[cfg(target_os = "linux")]
        let (gl, texture) = {
            let (gl, texture) = self
                .bridge
                .create_target(size)
                .expect("a Vulkan texture shared with the OpenGL context");
            // SAFETY: the context stays current on this thread, and the framebuffer lives in
            // `self.target` until the next target is set.
            unsafe { self.player.set_gl_target(gl.fbo()) };
            (gl, texture)
        };
        #[cfg(target_os = "macos")]
        let texture = self.create_shared_device_target(size);
        self.player.set_surface_size(size.0 as i32, size.1 as i32);
        let image = slint::Image::try_from(texture.clone())
            .expect("an Rgba8Unorm texture with TEXTURE_BINDING | RENDER_ATTACHMENT");
        if let Some(window) = self.window.upgrade() {
            window.global::<OverlayEditor>().set_video_frame(image.clone());
        }
        log::info!("playback: render target {}×{}", size.0, size.1);
        self.target = Some(Target {
            texture,
            image,
            size,
            #[cfg(windows)]
            _d3d11_texture: d3d11_texture,
            #[cfg(target_os = "linux")]
            gl,
        });
        // Paused, nothing else redraws the current frame into the new texture.
        let _ = self.render();
    }

    /// A texture on the shared device, handed to MDK as its Metal or D3D12 target.
    #[cfg(any(target_os = "macos", windows))]
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

/// The Linux bridge: MDK renders with OpenGL, the shared device is Vulkan (gpu.rs). The frame
/// stays on the GPU:
/// - Vulkan allocates the frame texture with exportable memory (`VK_KHR_external_memory_fd`);
///   an OpenGL context on the same GPU imports it (`GL_EXT_memory_object_fd`) as the colour
///   attachment of the framebuffer MDK draws into, and wgpu wraps the same image for Slint.
/// - Two binary semaphores shared the same way (`VK_KHR_external_semaphore_fd`,
///   `GL_EXT_semaphore_fd`) order the two queues on the GPU around each `renderVideo()`, as the
///   Windows D3D11 target's shared fence does; the UI thread never waits.
///
/// The context is an EGL context without a surface, on the EGL device whose OpenGL device UUID
/// is the Vulkan device's (`EGL_EXT_platform_device`), so it works with any vendor's driver and
/// picks the right GPU on hybrid systems. It stays current on the UI thread for the bridge's
/// life: MDK's `renderVideo()` and its teardown in `Player::drop` run there and draw in whatever
/// context is current, and nothing else on the UI thread uses OpenGL.
///
/// The image never changes queue family ownership to `VK_QUEUE_FAMILY_EXTERNAL`: wgpu cannot
/// record that barrier. Drivers that share memory between their own GL and Vulkan (Mesa, NVIDIA)
/// do not need it for an exclusive image used by one queue on each side.
#[cfg(target_os = "linux")]
mod vulkan_gl {
    use super::GpuDevice;
    use ash::vk;
    use khronos_egl as egl;
    use slint::wgpu_30::wgpu;
    use std::ffi::{c_void, CStr};
    use std::rc::Rc;

    type Vulkan = wgpu::wgc::api::Vulkan;
    type Egl = egl::DynamicInstance<egl::EGL1_5>;

    const GL_TEXTURE_2D: u32 = 0x0DE1;
    const GL_RENDERER: u32 = 0x1F01;
    const GL_EXTENSIONS: u32 = 0x1F03;
    const GL_RGBA8: u32 = 0x8058;
    const GL_NUM_EXTENSIONS: u32 = 0x821D;
    const GL_FRAMEBUFFER: u32 = 0x8D40;
    const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
    const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
    const GL_TEXTURE_TILING_EXT: u32 = 0x9580;
    const GL_DEDICATED_MEMORY_OBJECT_EXT: u32 = 0x9581;
    const GL_OPTIMAL_TILING_EXT: u32 = 0x9584;
    const GL_HANDLE_TYPE_OPAQUE_FD_EXT: u32 = 0x9586;
    const GL_LAYOUT_SHADER_READ_ONLY_EXT: u32 = 0x9591;
    const GL_NUM_DEVICE_UUIDS_EXT: u32 = 0x9596;
    const GL_DEVICE_UUID_EXT: u32 = 0x9597;
    /// `GL_NONE` as a layout: the contents are undefined.
    const GL_LAYOUT_UNDEFINED: u32 = 0;

    const EGL_PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;

    const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
    const HANDLE: vk::ExternalMemoryHandleTypeFlags = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;

    /// The OpenGL entry points the bridge calls, from `eglGetProcAddress`.
    struct Gl {
        get_error: unsafe extern "system" fn() -> u32,
        get_integerv: unsafe extern "system" fn(u32, *mut i32),
        get_string: unsafe extern "system" fn(u32) -> *const u8,
        get_stringi: unsafe extern "system" fn(u32, u32) -> *const u8,
        get_unsigned_bytei_v: unsafe extern "system" fn(u32, u32, *mut u8),
        flush: unsafe extern "system" fn(),
        finish: unsafe extern "system" fn(),
        gen_textures: unsafe extern "system" fn(i32, *mut u32),
        delete_textures: unsafe extern "system" fn(i32, *const u32),
        bind_texture: unsafe extern "system" fn(u32, u32),
        tex_parameteri: unsafe extern "system" fn(u32, u32, i32),
        gen_framebuffers: unsafe extern "system" fn(i32, *mut u32),
        delete_framebuffers: unsafe extern "system" fn(i32, *const u32),
        bind_framebuffer: unsafe extern "system" fn(u32, u32),
        framebuffer_texture_2d: unsafe extern "system" fn(u32, u32, u32, u32, i32),
        check_framebuffer_status: unsafe extern "system" fn(u32) -> u32,
        create_memory_objects: unsafe extern "system" fn(i32, *mut u32),
        delete_memory_objects: unsafe extern "system" fn(i32, *const u32),
        memory_object_parameteriv: unsafe extern "system" fn(u32, u32, *const i32),
        import_memory_fd: unsafe extern "system" fn(u32, u64, u32, i32),
        tex_storage_mem_2d: unsafe extern "system" fn(u32, i32, u32, i32, i32, u32, u64),
        gen_semaphores: unsafe extern "system" fn(i32, *mut u32),
        delete_semaphores: unsafe extern "system" fn(i32, *const u32),
        import_semaphore_fd: unsafe extern "system" fn(u32, u32, i32),
        wait_semaphore: unsafe extern "system" fn(u32, u32, *const u32, u32, *const u32, *const u32),
        signal_semaphore: unsafe extern "system" fn(u32, u32, *const u32, u32, *const u32, *const u32),
    }

    impl Gl {
        /// Loads the entry points. `eglGetProcAddress` may hand out stubs for extensions the context
        /// lacks, so the caller checks the extensions before calling any of theirs.
        #[allow(clippy::missing_transmute_annotations)] // the struct field names the type
        fn load(egl: &Egl) -> Result<Self, String> {
            macro_rules! load {
                ($name:literal) => {{
                    let function = egl
                        .get_proc_address($name)
                        .ok_or(concat!("OpenGL has no ", $name))?;
                    // SAFETY: the field's type is the entry point's signature in the GL registry.
                    unsafe { std::mem::transmute::<extern "system" fn(), _>(function) }
                }};
            }
            Ok(Self {
                get_error: load!("glGetError"),
                get_integerv: load!("glGetIntegerv"),
                get_string: load!("glGetString"),
                get_stringi: load!("glGetStringi"),
                get_unsigned_bytei_v: load!("glGetUnsignedBytei_vEXT"),
                flush: load!("glFlush"),
                finish: load!("glFinish"),
                gen_textures: load!("glGenTextures"),
                delete_textures: load!("glDeleteTextures"),
                bind_texture: load!("glBindTexture"),
                tex_parameteri: load!("glTexParameteri"),
                gen_framebuffers: load!("glGenFramebuffers"),
                delete_framebuffers: load!("glDeleteFramebuffers"),
                bind_framebuffer: load!("glBindFramebuffer"),
                framebuffer_texture_2d: load!("glFramebufferTexture2D"),
                check_framebuffer_status: load!("glCheckFramebufferStatus"),
                create_memory_objects: load!("glCreateMemoryObjectsEXT"),
                delete_memory_objects: load!("glDeleteMemoryObjectsEXT"),
                memory_object_parameteriv: load!("glMemoryObjectParameterivEXT"),
                import_memory_fd: load!("glImportMemoryFdEXT"),
                tex_storage_mem_2d: load!("glTexStorageMem2DEXT"),
                gen_semaphores: load!("glGenSemaphoresEXT"),
                delete_semaphores: load!("glDeleteSemaphoresEXT"),
                import_semaphore_fd: load!("glImportSemaphoreFdEXT"),
                wait_semaphore: load!("glWaitSemaphoreEXT"),
                signal_semaphore: load!("glSignalSemaphoreEXT"),
            })
        }

        fn string(&self, name: u32) -> String {
            let value = unsafe { (self.get_string)(name) };
            if value.is_null() {
                return String::new();
            }
            unsafe { CStr::from_ptr(value.cast()) }
                .to_string_lossy()
                .into_owned()
        }

        fn has_extension(&self, name: &str) -> bool {
            let mut count = 0;
            unsafe { (self.get_integerv)(GL_NUM_EXTENSIONS, &mut count) };
            (0..count.max(0) as u32).any(|index| {
                let value = unsafe { (self.get_stringi)(GL_EXTENSIONS, index) };
                !value.is_null()
                    && unsafe { CStr::from_ptr(value.cast()) }.to_bytes() == name.as_bytes()
            })
        }

        /// The UUIDs of the devices behind the current context.
        fn device_uuids(&self) -> Vec<[u8; vk::UUID_SIZE]> {
            let mut count = 0;
            unsafe { (self.get_integerv)(GL_NUM_DEVICE_UUIDS_EXT, &mut count) };
            (0..count.max(0) as u32)
                .map(|index| {
                    let mut uuid = [0; vk::UUID_SIZE];
                    unsafe {
                        (self.get_unsigned_bytei_v)(GL_DEVICE_UUID_EXT, index, uuid.as_mut_ptr())
                    };
                    uuid
                })
                .collect()
        }

        fn check(&self, what: &str) -> Result<(), String> {
            match unsafe { (self.get_error)() } {
                0 => Ok(()),
                error => Err(format!("OpenGL error {error:#x} {what}")),
            }
        }
    }

    /// An EGL context on the Vulkan device's GPU, current on this thread.
    struct Context {
        egl: Egl,
        display: egl::Display,
        context: egl::Context,
        gl: Gl,
    }

    impl Context {
        /// The first EGL device (then the default display) whose OpenGL context runs on the device
        /// with `uuid` and can import Vulkan memory and semaphores.
        fn on_device(uuid: [u8; vk::UUID_SIZE]) -> Result<Self, String> {
            // SAFETY: libEGL.so.1 is the EGL ABI (glvnd's dispatcher or a vendor's library).
            let egl = unsafe { Egl::load_required() }
                .map_err(|error| format!("could not load EGL 1.5: {error}"))?;
            let client = egl
                .query_string(None, egl::EXTENSIONS)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut displays = Vec::new();
            if client.contains("EGL_EXT_device_enumeration")
                && client.contains("EGL_EXT_platform_device")
            {
                for device in query_devices(&egl) {
                    // SAFETY: `device` is an EGLDeviceEXT from eglQueryDevicesEXT.
                    match unsafe {
                        egl.get_platform_display(EGL_PLATFORM_DEVICE_EXT, device, &[egl::ATTRIB_NONE])
                    } {
                        Ok(display) => displays.push(display),
                        Err(error) => log::debug!("playback: EGL device display: {error}"),
                    }
                }
            }
            // SAFETY: EGL_DEFAULT_DISPLAY needs no native display.
            if let Some(display) = unsafe { egl.get_display(egl::DEFAULT_DISPLAY) } {
                displays.push(display);
            }

            let mut rejected = Vec::new();
            for display in displays {
                match Self::try_display(&egl, display, uuid) {
                    Ok((context, gl)) => {
                        return Ok(Self {
                            egl,
                            display,
                            context,
                            gl,
                        })
                    }
                    Err(reason) => rejected.push(reason),
                }
            }
            Err(format!(
                "no EGL display shares memory with the Vulkan device: {}",
                rejected.join("; ")
            ))
        }

        fn try_display(
            egl: &Egl,
            display: egl::Display,
            uuid: [u8; vk::UUID_SIZE],
        ) -> Result<(egl::Context, Gl), String> {
            egl.initialize(display)
                .map_err(|error| format!("eglInitialize: {error}"))?;
            let reject = |reason: String| {
                let _ = egl.terminate(display);
                reason
            };
            let extensions = egl
                .query_string(Some(display), egl::EXTENSIONS)
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            for needed in ["EGL_KHR_surfaceless_context", "EGL_KHR_no_config_context"] {
                if !extensions.contains(needed) {
                    return Err(reject(format!("a display without {needed}")));
                }
            }
            egl.bind_api(egl::OPENGL_API)
                .map_err(|error| reject(format!("eglBindAPI(OpenGL): {error}")))?;
            // SAFETY: EGL_NO_CONFIG_KHR, which EGL_KHR_no_config_context allows.
            let no_config = unsafe { egl::Config::from_ptr(std::ptr::null_mut()) };
            let context = egl
                .create_context(display, no_config, None, &[egl::NONE])
                .map_err(|error| reject(format!("eglCreateContext: {error}")))?;
            let reject_context = |reason: String| {
                let _ = egl.make_current(display, None, None, None);
                let _ = egl.destroy_context(display, context);
                reject(reason)
            };
            egl.make_current(display, None, None, Some(context))
                .map_err(|error| reject_context(format!("eglMakeCurrent: {error}")))?;
            let gl = Gl::load(egl).map_err(&reject_context)?;
            let renderer = gl.string(GL_RENDERER);
            for needed in ["GL_EXT_memory_object_fd", "GL_EXT_semaphore_fd"] {
                if !gl.has_extension(needed) {
                    return Err(reject_context(format!("{renderer} lacks {needed}")));
                }
            }
            if !gl.device_uuids().contains(&uuid) {
                return Err(reject_context(format!("{renderer} is another GPU")));
            }
            log::info!("playback: OpenGL bridge context on {renderer}");
            Ok((context, gl))
        }

        fn make_current(&self) {
            self.egl
                .make_current(self.display, None, None, Some(self.context))
                .expect("the bridge's EGL context stays valid");
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            let _ = self.egl.make_current(self.display, None, None, None);
            let _ = self.egl.destroy_context(self.display, self.context);
            let _ = self.egl.terminate(self.display);
        }
    }

    /// The EGL devices (`eglQueryDevicesEXT`).
    fn query_devices(egl: &Egl) -> Vec<*mut c_void> {
        type QueryDevices = unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32;
        let Some(function) = egl.get_proc_address("eglQueryDevicesEXT") else {
            return Vec::new();
        };
        // SAFETY: eglQueryDevicesEXT's signature (EGL_EXT_device_enumeration).
        let query: QueryDevices =
            unsafe { std::mem::transmute::<extern "system" fn(), QueryDevices>(function) };
        let mut count = 0;
        if unsafe { query(0, std::ptr::null_mut(), &mut count) } == 0 || count <= 0 {
            return Vec::new();
        }
        let mut devices = vec![std::ptr::null_mut(); count as usize];
        if unsafe { query(count, devices.as_mut_ptr(), &mut count) } == 0 {
            return Vec::new();
        }
        devices.truncate(count.max(0) as usize);
        devices
    }

    /// A binary semaphore created on Vulkan and imported into OpenGL.
    struct SharedSemaphore {
        vulkan: vk::Semaphore,
        gl: u32,
    }

    pub struct Bridge {
        device: wgpu::Device,
        queue: wgpu::Queue,
        raw_device: ash::Device,
        memory_types: vk::PhysicalDeviceMemoryProperties,
        memory_fd: ash::khr::external_memory_fd::Device,
        /// Vulkan signals it after its last use of the frame (Skia's last draw sampling it); OpenGL
        /// waits on it before MDK draws.
        to_gl: SharedSemaphore,
        /// OpenGL signals it after MDK drew; Vulkan waits on it before Skia's next draw samples.
        to_vulkan: SharedSemaphore,
        gl: Rc<Context>,
    }

    /// The frame texture's OpenGL side: MDK's framebuffer, over the memory Vulkan exported.
    pub struct Target {
        context: Rc<Context>,
        texture: u32,
        memory: u32,
        fbo: u32,
        /// OpenGL wrote the image at least once: its layout is shader-read-only (what wgpu and Skia
        /// assume) instead of undefined.
        written: bool,
    }

    impl Target {
        pub fn fbo(&self) -> u32 {
            self.fbo
        }
    }

    impl Drop for Target {
        /// OpenGL defers the deletes until its queued work is done; Vulkan's side is wgpu's texture.
        fn drop(&mut self) {
            let gl = &self.context.gl;
            self.context.make_current();
            unsafe {
                (gl.delete_framebuffers)(1, &self.fbo);
                (gl.delete_textures)(1, &self.texture);
                (gl.delete_memory_objects)(1, &self.memory);
            }
        }
    }

    impl Bridge {
        pub fn new(gpu: &GpuDevice) -> Result<Self, String> {
            let hal_device = unsafe { gpu.device.as_hal::<Vulkan>() }
                .expect("the shared device is Vulkan on Linux (gpu.rs)");
            for needed in [
                ash::khr::external_memory_fd::NAME,
                ash::khr::external_semaphore_fd::NAME,
            ] {
                if !hal_device.enabled_device_extensions().contains(&needed) {
                    return Err(format!(
                        "the Vulkan device has no {}",
                        needed.to_string_lossy()
                    ));
                }
            }
            let instance = hal_device.shared_instance().raw_instance();
            let physical = hal_device.raw_physical_device();
            let raw_device = hal_device.raw_device().clone();
            let mut id = vk::PhysicalDeviceIDProperties::default();
            let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut id);
            unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
            let uuid = id.device_uuid;
            let memory_types = unsafe { instance.get_physical_device_memory_properties(physical) };
            let memory_fd = ash::khr::external_memory_fd::Device::new(instance, &raw_device);
            let semaphore_fd = ash::khr::external_semaphore_fd::Device::new(instance, &raw_device);
            drop(hal_device);

            let gl = Rc::new(Context::on_device(uuid)?);
            let share = |label: &str| -> Result<SharedSemaphore, String> {
                let mut export = vk::ExportSemaphoreCreateInfo::default()
                    .handle_types(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
                let vulkan = unsafe {
                    raw_device.create_semaphore(
                        &vk::SemaphoreCreateInfo::default().push_next(&mut export),
                        None,
                    )
                }
                .map_err(|error| format!("vkCreateSemaphore ({label}): {error}"))?;
                let fd = unsafe {
                    semaphore_fd.get_semaphore_fd(
                        &vk::SemaphoreGetFdInfoKHR::default()
                            .semaphore(vulkan)
                            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD),
                    )
                }
                .map_err(|error| {
                    unsafe { raw_device.destroy_semaphore(vulkan, None) };
                    format!("vkGetSemaphoreFdKHR ({label}): {error}")
                })?;
                let mut imported = 0;
                unsafe {
                    (gl.gl.gen_semaphores)(1, &mut imported);
                    // OpenGL owns the descriptor from here.
                    (gl.gl.import_semaphore_fd)(imported, GL_HANDLE_TYPE_OPAQUE_FD_EXT, fd);
                }
                gl.gl.check(&format!("importing the {label} semaphore"))?;
                Ok(SharedSemaphore {
                    vulkan,
                    gl: imported,
                })
            };
            let to_gl = share("Vulkan → OpenGL")?;
            let to_vulkan = share("OpenGL → Vulkan")?;
            log::info!("playback: Vulkan–OpenGL bridge with shared memory and semaphores");
            Ok(Self {
                device: gpu.device.clone(),
                queue: gpu.queue.clone(),
                raw_device,
                memory_types,
                memory_fd,
                to_gl,
                to_vulkan,
                gl,
            })
        }

        /// A Vulkan image with exportable memory as a wgpu texture Slint samples, and its memory
        /// imported into OpenGL as MDK's framebuffer.
        pub fn create_target(&self, size: (u32, u32)) -> Result<(Target, wgpu::Texture), String> {
            let device = &self.raw_device;
            let mut external = vk::ExternalMemoryImageCreateInfo::default().handle_types(HANDLE);
            let info = vk::ImageCreateInfo::default()
                .push_next(&mut external)
                .image_type(vk::ImageType::TYPE_2D)
                .format(FORMAT)
                .extent(vk::Extent3D {
                    width: size.0,
                    height: size.1,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = unsafe { device.create_image(&info, None) }
                .map_err(|error| format!("vkCreateImage: {error}"))?;
            let requirements = unsafe { device.get_image_memory_requirements(image) };
            let memory_type = self
                .memory_types
                .memory_types_as_slice()
                .iter()
                .enumerate()
                .position(|(index, kind)| {
                    requirements.memory_type_bits & (1 << index) != 0
                        && kind
                            .property_flags
                            .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                });
            let Some(memory_type) = memory_type else {
                unsafe { device.destroy_image(image, None) };
                return Err("no device-local memory type for the frame".into());
            };
            // Dedicated: OpenGL imports it as a dedicated memory object, which must match.
            let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
            let mut export = vk::ExportMemoryAllocateInfo::default().handle_types(HANDLE);
            let allocate = vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type as u32)
                .push_next(&mut dedicated)
                .push_next(&mut export);
            let memory = match unsafe { device.allocate_memory(&allocate, None) } {
                Ok(memory) => memory,
                Err(error) => {
                    unsafe { device.destroy_image(image, None) };
                    return Err(format!("vkAllocateMemory: {error}"));
                }
            };
            let fd = unsafe { device.bind_image_memory(image, memory, 0) }.and_then(|()| unsafe {
                self.memory_fd.get_memory_fd(
                    &vk::MemoryGetFdInfoKHR::default()
                        .memory(memory)
                        .handle_type(HANDLE),
                )
            });
            let fd = match fd {
                Ok(fd) => fd,
                Err(error) => {
                    unsafe {
                        device.destroy_image(image, None);
                        device.free_memory(memory, None);
                    }
                    return Err(format!("exporting the frame's memory: {error}"));
                }
            };

            // wgpu owns the image and its memory from here, and frees them once the GPU is done.
            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let hal_texture = {
                let hal_device =
                    unsafe { self.device.as_hal::<Vulkan>() }.expect("the shared device is Vulkan");
                unsafe {
                    hal_device.texture_from_raw(
                        image,
                        &wgpu::hal::TextureDescriptor {
                            label: Some("video playback frame (OpenGL)"),
                            size: extent,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COLOR_TARGET,
                            memory_flags: wgpu::hal::MemoryFlags::empty(),
                            view_formats: Vec::new(),
                        },
                        None,
                        wgpu::hal::vulkan::TextureMemory::Dedicated(memory),
                    )
                }
            };
            // Slint only samples it, in the layout OpenGL leaves it in after each draw.
            let texture = unsafe {
                self.device.create_texture_from_hal::<Vulkan>(
                    hal_texture,
                    &wgpu::TextureDescriptor {
                        label: Some("video playback frame (OpenGL)"),
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

            let context = Rc::clone(&self.gl);
            context.make_current();
            let gl = &context.gl;
            let mut target = Target {
                context: Rc::clone(&context),
                texture: 0,
                memory: 0,
                fbo: 0,
                written: false,
            };
            unsafe {
                (gl.create_memory_objects)(1, &mut target.memory);
                (gl.memory_object_parameteriv)(target.memory, GL_DEDICATED_MEMORY_OBJECT_EXT, &1);
                // OpenGL owns the descriptor from here.
                (gl.import_memory_fd)(
                    target.memory,
                    requirements.size,
                    GL_HANDLE_TYPE_OPAQUE_FD_EXT,
                    fd,
                );
                (gl.gen_textures)(1, &mut target.texture);
                (gl.bind_texture)(GL_TEXTURE_2D, target.texture);
                (gl.tex_parameteri)(
                    GL_TEXTURE_2D,
                    GL_TEXTURE_TILING_EXT,
                    GL_OPTIMAL_TILING_EXT as i32,
                );
                (gl.tex_storage_mem_2d)(
                    GL_TEXTURE_2D,
                    1,
                    GL_RGBA8,
                    size.0 as i32,
                    size.1 as i32,
                    target.memory,
                    0,
                );
                (gl.bind_texture)(GL_TEXTURE_2D, 0);
                (gl.gen_framebuffers)(1, &mut target.fbo);
                (gl.bind_framebuffer)(GL_FRAMEBUFFER, target.fbo);
                (gl.framebuffer_texture_2d)(
                    GL_FRAMEBUFFER,
                    GL_COLOR_ATTACHMENT0,
                    GL_TEXTURE_2D,
                    target.texture,
                    0,
                );
            }
            let status = unsafe { (gl.check_framebuffer_status)(GL_FRAMEBUFFER) };
            unsafe { (gl.bind_framebuffer)(GL_FRAMEBUFFER, 0) };
            gl.check("importing the frame's memory")?;
            if status != GL_FRAMEBUFFER_COMPLETE {
                return Err(format!(
                    "the frame's framebuffer is incomplete ({status:#x})"
                ));
            }
            Ok((target, texture))
        }

        /// Before MDK draws: OpenGL waits on the GPU for what Vulkan already submitted (Slint's last
        /// frame, which samples the texture MDK is about to overwrite). Skia submits on the same
        /// VkQueue as wgpu, so the signal, in a submission of its own, follows Skia's work too.
        pub fn before_render(&self, target: &Target) {
            unsafe { self.queue.as_hal::<Vulkan>() }
                .expect("the shared queue is Vulkan")
                .add_signal_semaphore(self.to_gl.vulkan, None);
            self.queue.submit([]);
            self.gl.make_current();
            let layout = if target.written {
                GL_LAYOUT_SHADER_READ_ONLY_EXT
            } else {
                GL_LAYOUT_UNDEFINED
            };
            unsafe {
                (self.gl.gl.wait_semaphore)(
                    self.to_gl.gl,
                    0,
                    std::ptr::null(),
                    1,
                    &target.texture,
                    &layout,
                )
            };
        }

        /// After MDK drew: Vulkan waits on the GPU for OpenGL's draw before Slint's next frame samples
        /// it. The wait goes in a submission of its own: a semaphore wait also holds back everything
        /// submitted after it on the queue, Skia's draws included.
        pub fn after_render(&self, target: &mut Target) {
            let layout = GL_LAYOUT_SHADER_READ_ONLY_EXT;
            unsafe {
                (self.gl.gl.signal_semaphore)(
                    self.to_vulkan.gl,
                    0,
                    std::ptr::null(),
                    1,
                    &target.texture,
                    &layout,
                );
                // Submits the signal, which Vulkan's wait below must follow.
                (self.gl.gl.flush)();
            }
            target.written = true;
            unsafe { self.queue.as_hal::<Vulkan>() }
                .expect("the shared queue is Vulkan")
                .add_wait_semaphore(
                    self.to_vulkan.vulkan,
                    None,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                );
            self.queue.submit([]);
        }
    }

    impl Drop for Bridge {
        /// Neither queue may still use the semaphores.
        fn drop(&mut self) {
            self.gl.make_current();
            unsafe {
                (self.gl.gl.finish)();
                (self.gl.gl.delete_semaphores)(1, &self.to_gl.gl);
                (self.gl.gl.delete_semaphores)(1, &self.to_vulkan.gl);
            }
            if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
                log::error!("playback: waiting for the GPU before the bridge's teardown: {error}");
            }
            unsafe {
                self.raw_device.destroy_semaphore(self.to_gl.vulkan, None);
                self.raw_device
                    .destroy_semaphore(self.to_vulkan.vulkan, None);
            }
        }
    }
}
