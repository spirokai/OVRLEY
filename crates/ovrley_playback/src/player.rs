//! MDK's player behind a safe Rust type: it decodes one media (with audio) and draws its frames
//! into the render target [`crate::Surface`] gives it.
//!
//! The binding's player setup and teardown order are adapted from Gyroflow's `qml-video-rs` (MIT,
//! Copyright (c) 2021 AdrianEddy). Teardown is where MDK integrations crash: see [`Player`]'s
//! `Drop`.
//!
//! Threads: MDK calls [`Callbacks`] from its own threads. Everything else, drawing and dropping
//! the player above all, belongs on the thread that owns the GPU queue.

use cpp::cpp;
use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt;
use std::sync::Once;

cpp! {{
    #ifdef _WIN32
    #include <d3d11.h> // before RenderAPI.h, which declares D3D11RenderAPI and D3D12RenderAPI
    #include <d3d12.h> // only after these
    #endif
    #include <mdk/Player.h>
    #include <mdk/RenderAPI.h>
    #include <atomic>
    #include <chrono>
    #include <memory>
    #include <mutex>
    #include <thread>

    extern "C" void ovrley_playback_on_render(void* rust);
    extern "C" void ovrley_playback_on_event(int64_t error, const char* category, const char* detail);
    extern "C" void ovrley_playback_on_status(void* rust, uint32_t old_status, uint32_t new_status);
    extern "C" void ovrley_playback_on_seek(void* rust, int64_t result);
    extern "C" void ovrley_playback_on_log(int level, const char* message);

    // onEvent() and onMediaStatus() listeners are never removed (removing frees a std::function MDK
    // may still call); teardown marks them dead under this mutex instead.
    struct OvrleyListenerGuard {
        std::recursive_mutex mutex; // recursive: MDK reports some statuses on the calling thread
        bool alive = true;
    };

    struct OvrleyPlayer {
        mdk::Player* player = nullptr;
        std::shared_ptr<OvrleyListenerGuard> guard;
        void* rust = nullptr;
        std::atomic<int64_t> media_start{-1}; // the container's start in ms, -1 until it is loaded
    };
}}

/// What a player reports from MDK's threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The media cannot be played: MDK found it invalid, or its path cannot be handed to MDK.
    Failed,
    /// A [`Player::seek`] finished, at its target or with an error (which the crate logs).
    SeekFinished,
}

/// What MDK calls from its own threads. Neither callback may block or panic: a panic aborts the
/// process.
pub struct Callbacks {
    /// The player has something to draw. Schedules [`crate::Surface::render`] on the render
    /// thread; calling it from here deadlocks MDK.
    pub render: Box<dyn Fn() + Send + Sync>,
    pub event: Box<dyn Fn(Event) + Send + Sync>,
}

impl fmt::Debug for Callbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Callbacks").finish_non_exhaustive()
    }
}

/// `mdk::State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

/// `mdk::MediaStatus::Invalid`: the media failed to load.
const STATUS_INVALID: u32 = 1 << 31;

/// One MDK player, which plays one media. Not `Send`: it lives on the render thread.
pub struct Player {
    /// `OvrleyPlayer*`, owned; deleted in `Drop`.
    raw: *mut c_void,
    /// Owned; MDK's listeners reach it through `OvrleyPlayer::rust` until `Drop` silences them.
    callbacks: *mut Callbacks,
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Player").finish_non_exhaustive()
    }
}

impl Player {
    /// A player with `decoders` (MDK's names with options, comma-separated, e.g.
    /// `VT:duration=0,FFmpeg`), set up for a local file.
    pub(crate) fn new(decoders: &str, callbacks: Callbacks) -> Self {
        let decoders = CString::new(decoders).expect("decoder names come from the environment, which has no NUL");
        let decoders = decoders.as_ptr();
        let callbacks = Box::into_raw(Box::new(callbacks));
        let rust = callbacks.cast::<c_void>();
        // SAFETY: `decoders` is a C string alive for the call. `rust` stays valid until `Drop`,
        // which silences every listener before it frees it.
        let raw = unsafe {
            cpp!([rust as "void*", decoders as "const char*"] -> *mut c_void as "void*" {
                auto p = new OvrleyPlayer();
                p->player = new mdk::Player();
                p->rust = rust;
                p->guard = std::make_shared<OvrleyListenerGuard>();

                p->player->setProperty("video.decoders", decoders);
                p->player->setProperty("continue_at_end", "1");
                p->player->setBufferRange(0);
                p->player->setAspectRatio(mdk::KeepAspectRatioCrop);

                p->player->setRenderCallback([rust](void*) { ovrley_playback_on_render(rust); });
                const auto guard = p->guard;
                p->player->onEvent([guard](const mdk::MediaEvent& event) -> bool {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (!guard->alive) return false;
                    ovrley_playback_on_event(event.error, event.category.c_str(), event.detail.c_str());
                    return true;
                });
                p->player->onMediaStatus([p, guard](mdk::MediaStatus old_status, mdk::MediaStatus new_status) -> bool {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (!guard->alive) return false; // unloads the media at its next status change
                    if ((new_status & mdk::MediaStatus::Loaded) && (new_status & mdk::MediaStatus::Prepared) && p->media_start < 0) {
                        const auto start = p->player->mediaInfo().start_time;
                        p->media_start = start > 0 ? start : 0;
                    }
                    ovrley_playback_on_status(p->rust, uint32_t(old_status), uint32_t(new_status));
                    return true;
                });
                return p;
            })
        };
        Self { raw, callbacks }
    }

    /// Sets the media and prepares it, paused, at `start_seconds`. A path MDK cannot take (one with
    /// a NUL) is reported as [`Event::Failed`], as an unplayable file is.
    pub(crate) fn open(&self, path: &str, start_seconds: f64) {
        let Ok(url) = CString::new(path) else {
            log::error!("playback: {path:?} holds a NUL, which MDK cannot open");
            // SAFETY: `callbacks` is owned by `self` and freed only in `Drop`.
            (unsafe { &*self.callbacks }.event)(Event::Failed);
            return;
        };
        let url = url.as_ptr();
        let raw = self.raw;
        let start_ms = milliseconds(start_seconds);
        // SAFETY: `raw` is this player's; `url` is a C string alive for the call.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", url as "const char*", start_ms as "int64_t"] {
                raw->player->setMedia(url);
                raw->player->prepare(start_ms);
            })
        }
    }

    /// Renders into `texture` on `device` and `queue` (`id<MTLDevice>`, `id<MTLCommandQueue>`,
    /// `id<MTLTexture>`). MDK begins its own render pass on that queue.
    ///
    /// # Safety
    /// The three must be valid Metal objects, alive until the target is replaced or the player is
    /// dropped. Call on the render thread.
    #[cfg(target_os = "macos")]
    pub(crate) unsafe fn set_metal_target(
        &self,
        device: *const c_void,
        queue: *const c_void,
        texture: *const c_void,
    ) {
        let raw = self.raw;
        // SAFETY: the caller upholds the objects' validity; `raw` is this player's.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", device as "const void*", queue as "const void*", texture as "const void*"] {
                #ifdef __APPLE__
                mdk::MetalRenderAPI ra{};
                ra.device = device;
                ra.cmdQueue = queue;
                ra.texture = texture;
                raw->player->setRenderAPI(&ra);
                #else
                (void)raw; (void)device; (void)queue; (void)texture;
                #endif
            })
        }
    }

    /// Renders into `texture` (`ID3D12Resource*`, a 2D render target) through `queue`
    /// (`ID3D12CommandQueue*`, a direct queue of the texture's device). MDK records its own
    /// command list and submits it on that queue, so the caller's later submissions see the frame.
    ///
    /// # Safety
    /// Both must be valid D3D12 objects, alive until the target is replaced or the player is
    /// dropped. Call on the render thread.
    #[cfg(windows)]
    pub(crate) unsafe fn set_d3d12_target(&self, queue: *const c_void, texture: *const c_void) {
        let raw = self.raw;
        // SAFETY: the caller upholds the objects' validity; `raw` is this player's.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", queue as "const void*", texture as "const void*"] {
                #ifdef _WIN32
                mdk::D3D12RenderAPI ra{};
                ra.cmdQueue = static_cast<ID3D12CommandQueue*>(const_cast<void*>(queue));
                ra.rt = static_cast<ID3D12Resource*>(const_cast<void*>(texture));
                raw->player->setRenderAPI(&ra);
                #else
                (void)raw; (void)queue; (void)texture;
                #endif
            })
        }
    }

    /// Renders into `texture` (`ID3D11Texture2D*`, a 2D render target) on its device's immediate
    /// context, which MDK flushes after each `renderVideo()`.
    ///
    /// # Safety
    /// The texture must be a valid D3D11 texture, alive until the target is replaced or the player is
    /// dropped. Call on the render thread.
    #[cfg(windows)]
    pub(crate) unsafe fn set_d3d11_target(&self, texture: *const c_void) {
        let raw = self.raw;
        // SAFETY: the caller upholds the texture's validity; `raw` is this player's.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", texture as "const void*"] {
                #ifdef _WIN32
                mdk::D3D11RenderAPI ra{};
                ra.rtv = static_cast<ID3D11DeviceChild*>(const_cast<void*>(texture));
                raw->player->setRenderAPI(&ra);
                #else
                (void)raw; (void)texture;
                #endif
            })
        }
    }

    /// Renders into framebuffer `fbo` of the OpenGL context current on the render thread. GL's
    /// framebuffer origin is bottom-left; the frame is flipped so that row 0 is the top, as the
    /// caller's Vulkan sampler reads it.
    ///
    /// # Safety
    /// The context must be current on the render thread whenever the player draws or is dropped
    /// (MDK releases its GL resources in `Drop`), and the framebuffer alive until the target is
    /// replaced or the player is dropped.
    #[cfg(target_os = "linux")]
    pub(crate) unsafe fn set_gl_target(&self, fbo: u32) {
        let raw = self.raw;
        let fbo = i32::try_from(fbo).expect("OpenGL framebuffer names are small");
        // SAFETY: the caller upholds the framebuffer's validity; `raw` is this player's.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", fbo as "int"] {
                mdk::GLRenderAPI ra{};
                ra.fbo = fbo;
                raw->player->setRenderAPI(&ra);
                raw->player->scale(1.0f, -1.0f);
            })
        }
    }

    /// The render target's size. MDK calls [`Callbacks::render`] from within.
    pub(crate) fn set_surface_size(&self, width: i32, height: i32) {
        let raw = self.raw;
        // SAFETY: `raw` is this player's.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", width as "int", height as "int"] {
                raw->player->setVideoSurfaceSize(width, height);
            })
        }
    }

    /// Draws the next or current frame into the target: its timestamp in seconds, or < 0 when no
    /// frame was drawn. Render thread only, with the target's context ready.
    pub(crate) fn render(&self) -> f64 {
        let raw = self.raw;
        // SAFETY: `raw` is this player's.
        unsafe { cpp!([raw as "OvrleyPlayer*"] -> f64 as "double" { return raw->player->renderVideo(); }) }
    }

    pub fn set_state(&self, state: State) {
        let raw = self.raw;
        let value: i32 = match state {
            State::Stopped => 0,
            State::Playing => 1,
            State::Paused => 2,
        };
        // SAFETY: `raw` is this player's; `value` is an `mdk::State`.
        unsafe { cpp!([raw as "OvrleyPlayer*", value as "int"] { raw->player->set(mdk::State(value)); }) }
    }

    #[must_use]
    pub fn state(&self) -> State {
        let raw = self.raw;
        // SAFETY: `raw` is this player's.
        let value = unsafe { cpp!([raw as "OvrleyPlayer*"] -> i32 as "int" { return int(raw->player->state()); }) };
        match value {
            1 => State::Playing,
            2 => State::Paused,
            _ => State::Stopped,
        }
    }

    /// Seeks to the frame nearest `seconds` from the media's start (`FromStart | InCache`). Its end
    /// arrives as [`Event::SeekFinished`]; past the last frame it ends with `OutOfRange`, with the
    /// last frame drawn all the same. `false` when MDK refused, and then no event comes.
    #[must_use]
    pub fn seek(&self, seconds: f64) -> bool {
        let raw = self.raw;
        let ms = milliseconds(seconds);
        // SAFETY: `raw` is this player's. The callback reaches Rust only while the listeners live.
        unsafe {
            cpp!([raw as "OvrleyPlayer*", ms as "int64_t"] -> bool as "bool" {
                const auto guard = raw->guard;
                void* rust = raw->rust;
                return raw->player->seek(ms, mdk::SeekFlag::FromStart | mdk::SeekFlag::InCache, [guard, rust](int64_t result) {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (guard->alive) ovrley_playback_on_seek(rust, result);
                });
            })
        }
    }

    pub fn set_muted(&self, muted: bool) {
        let raw = self.raw;
        // SAFETY: `raw` is this player's.
        unsafe { cpp!([raw as "OvrleyPlayer*", muted as "bool"] { raw->player->setMute(muted); }) }
    }

    /// Where the media's timestamps start, once it is loaded.
    pub(crate) fn media_start_seconds(&self) -> Option<f64> {
        let raw = self.raw;
        // SAFETY: `raw` is this player's.
        let ms = unsafe { cpp!([raw as "OvrleyPlayer*"] -> i64 as "int64_t" { return raw->media_start.load(); }) };
        // Container start times are far below 2^52 ms, where f64 stops being exact.
        #[expect(clippy::cast_precision_loss, reason = "a start time in ms is exact in f64")]
        (ms >= 0).then(|| ms as f64 / 1000.0)
    }
}

impl Drop for Player {
    /// Teardown, in this order: stop (waiting up to 3 s, on this thread); mark the listeners dead;
    /// replace the render callback with a no-op (null leaves MDK a C callback that dereferences a
    /// null pointer); release the GPU resources on this (the render) thread; delete the player about
    /// a second later, on a thread of its own.
    fn drop(&mut self) {
        let raw = self.raw;
        // SAFETY: `raw` is this player's and is not used after this block, which deletes it.
        let stopped = unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> bool as "bool" {
                bool stopped = true;
                if (raw->media_start >= 0) {
                    raw->player->set(mdk::State::Stopped);
                    stopped = raw->player->waitFor(mdk::State::Stopped, 3000);
                }
                {
                    const std::lock_guard<std::recursive_mutex> lock(raw->guard->mutex);
                    raw->guard->alive = false;
                }
                raw->player->setRenderCallback([](void*) {});
                raw->player->setVideoSurfaceSize(-1, -1);
                auto player = raw->player;
                std::thread([player] {
                    std::this_thread::sleep_for(std::chrono::seconds(1));
                    delete player;
                }).detach();
                delete raw;
                return stopped;
            })
        };
        if !stopped {
            log::error!("playback: the player did not stop within 3 s; deleting it anyway");
        }
        // SAFETY: from `Box::into_raw` in `new`. No callback reaches it any more: the listeners are
        // dead and the render callback was replaced under the mutex MDK holds while it runs.
        drop(unsafe { Box::from_raw(self.callbacks) });
    }
}

/// Routes MDK's errors into the `log` crate, once per process. Its other levels log every seek and
/// stream.
pub(crate) fn install_log_handler() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        // SAFETY: sets MDK's global handler, which only calls `ovrley_playback_on_log`.
        unsafe {
            cpp!([] {
                mdk::setLogHandler([](mdk::LogLevel level, const char* message) { ovrley_playback_on_log(int(level), message); });
                mdk::SetGlobalOption("log", "Error");
            })
        }
    });
}

/// MDK's version, `major.minor.patch`.
pub(crate) fn version() -> String {
    // SAFETY: a plain query.
    let v = unsafe { cpp!([] -> i32 as "int" { return mdk::version(); }) };
    format!("{}.{}.{}", (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
}

/// The decoders, comma-separated: in debug builds `MDK_DECODERS` when set, else the platform's
/// hardware decoder that renders without a copy, then FFmpeg's software decoder.
/// - Windows: the active target's (`WindowsTarget::decoders`).
/// - Linux: `CUDA` (NVIDIA, CUDA–GL interop) and `VAAPI` (Mesa's Intel and AMD drivers, dma-buf
///   into EGL) render zero-copy into a GL target. `CUDA` goes first because it fails at once
///   without NVIDIA's driver, while VAAPI also opens on NVIDIA through nvidia-vaapi-driver.
/// - macOS: VideoToolbox.
pub(crate) fn decoders() -> String {
    #[cfg(windows)]
    const PLATFORM: &str = crate::surface::WindowsTarget::ACTIVE.decoders();
    #[cfg(target_os = "macos")]
    const PLATFORM: &str = "VT:duration=0,FFmpeg";
    #[cfg(target_os = "linux")]
    const PLATFORM: &str = "CUDA,VAAPI,FFmpeg";
    match std::env::var("MDK_DECODERS") {
        Ok(list) if cfg!(debug_assertions) && !list.trim().is_empty() => {
            list.split(',').map(str::trim).collect::<Vec<_>>().join(",")
        }
        _ => PLATFORM.to_string(),
    }
}

#[expect(clippy::cast_possible_truncation, reason = "media times in ms fit i64; NaN becomes 0")]
fn milliseconds(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

/// # Safety
/// `text` is a valid C string.
unsafe fn string(text: *const c_char) -> String {
    // SAFETY: the caller's contract.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

/// # Safety
/// `rust` is a player's `callbacks`, alive (the listener guard ensures it for MDK's calls).
unsafe fn callbacks<'a>(rust: *mut c_void) -> &'a Callbacks {
    // SAFETY: the caller's contract.
    unsafe { &*rust.cast::<Callbacks>() }
}

#[no_mangle]
unsafe extern "C" fn ovrley_playback_on_render(rust: *mut c_void) {
    // SAFETY: MDK passes back the `rust` the player registered.
    (unsafe { callbacks(rust) }.render)();
}

#[no_mangle]
unsafe extern "C" fn ovrley_playback_on_event(error: i64, category: *const c_char, detail: *const c_char) {
    // SAFETY: both are `std::string::c_str()` of the event, alive for the call.
    let (category, detail) = unsafe { (string(category), string(detail)) };
    if error < 0 {
        log::warn!("playback: event {category} {detail} ({error})");
    } else {
        log::debug!("playback: event {category} {detail} ({error})");
    }
}

#[no_mangle]
unsafe extern "C" fn ovrley_playback_on_status(rust: *mut c_void, old: u32, new: u32) {
    log::trace!("playback: status {old:#x} → {new:#x}");
    if new & STATUS_INVALID != 0 {
        // SAFETY: MDK passes back the `rust` the player registered.
        (unsafe { callbacks(rust) }.event)(Event::Failed);
    }
}

#[no_mangle]
unsafe extern "C" fn ovrley_playback_on_seek(rust: *mut c_void, result: i64) {
    if result < 0 {
        log::debug!("playback: a seek ended with {result}");
    }
    // SAFETY: MDK passes back the `rust` the player registered.
    (unsafe { callbacks(rust) }.event)(Event::SeekFinished);
}

#[no_mangle]
unsafe extern "C" fn ovrley_playback_on_log(level: i32, message: *const c_char) {
    // SAFETY: MDK passes a C string alive for the call.
    let message = unsafe { string(message) };
    let message = message.trim_end();
    match level {
        1 => log::error!(target: "mdk", "{message}"),
        2 => log::warn!(target: "mdk", "{message}"),
        3 => log::info!(target: "mdk", "{message}"),
        _ => log::debug!(target: "mdk", "{message}"),
    }
}
