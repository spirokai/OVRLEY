//! A video player: MDK decodes the media (with audio) and renders each frame into a texture the
//! caller owns: Metal on macOS, D3D12 on Windows, an OpenGL framebuffer on Linux.
//!
//! Adapted from Gyroflow's `qml-video-rs` (MIT, Copyright (c) 2021 AdrianEddy), whose MDK
//! integration drove this work: the decoder list, the player setup and above all the teardown
//! order, which is where MDK integrations crash.
//!
//! Threads: MDK calls [`Callbacks`] from its own threads. Everything else, [`Player::render`]
//! and dropping the player above all, belongs on the thread that owns the GPU queue (the UI
//! thread).


use cpp::cpp;
use std::ffi::{c_char, c_void, CStr, CString};

cpp! {{
    #ifdef _WIN32
    #include <d3d11.h> // before RenderAPI.h, which declares D3D11RenderAPI and D3D12RenderAPI
    #include <d3d12.h> // only after these
    #endif
    #include <mdk/Player.h>
    #include <mdk/RenderAPI.h>
    #include <mdk/VideoFrame.h>
    #include <atomic>
    #include <chrono>
    #include <memory>
    #include <mutex>
    #include <sstream>
    #include <string>
    #include <thread>
    #include <vector>

    extern "C" void ovrley_playback_on_render(void* rust);
    extern "C" void ovrley_playback_on_event(void* rust, int64_t error, const char* category, const char* detail);
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
        std::atomic<int> rotation{-1}; // clockwise degrees of the first frame, -1 before it
        std::atomic<bool> loaded{false};
        std::atomic<uint64_t> delivered{0}; // decoded frames MDK handed toward rendering
    };

    struct OvrleyMediaSummary {
        int64_t duration_ms;
        int32_t width;
        int32_t height;
        float fps;
        int32_t stream_rotation;
        int32_t has_video;
    };
}}

/// What MDK reports from its threads.
#[derive(Debug, Clone)]
pub enum Event {
    /// An MDK media event. `"size"` (category `"video"`) marks the first decoded frame.
    Media {
        error: i64,
        category: String,
        detail: String,
    },
    /// A media status change (`mdk::MediaStatus` bits).
    Status { old: u32, new: u32 },
    /// A seek finished: the position in ms, or a negative `mdk::SeekError`.
    Seek { result: i64 },
}

/// Called from MDK's threads. `render` must not call [`Player::render`] (MDK deadlocks); it
/// schedules it on the render thread.
pub struct Callbacks {
    pub render: Box<dyn Fn() + Send + Sync>,
    pub event: Box<dyn Fn(Event) + Send + Sync>,
}

/// `mdk::MediaStatus` bits the spike reads.
pub mod status {
    pub const LOADED: u32 = 1 << 2;
    pub const PREPARED: u32 = 1 << 8;
    pub const END: u32 = 1 << 6;
    pub const SEEKING: u32 = 1 << 7;
    pub const INVALID: u32 = 1 << 31;
}

/// `mdk::State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

/// The first video stream, once the media is loaded.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct MediaSummary {
    pub duration_ms: i64,
    pub width: i32,
    pub height: i32,
    pub fps: f32,
    /// Clockwise degrees from the container (`VideoStreamInfo::rotation`).
    pub stream_rotation: i32,
    has_video: i32,
}

pub struct Player {
    raw: *mut c_void,
    callbacks: *mut Callbacks,
}

impl Player {
    /// A player with `decoders` (MDK's names with options, e.g. `VT:duration=0`), set up for a
    /// local file.
    pub fn new(decoders: &[String], callbacks: Callbacks) -> Self {
        let callbacks = Box::into_raw(Box::new(callbacks));
        let rust = callbacks as *mut c_void;
        let joined = CString::new(decoders.join(",")).expect("decoder names have no NUL");
        let decoders = joined.as_ptr();
        let raw = unsafe {
            cpp!([rust as "void*", decoders as "const char*"] -> *mut c_void as "void*" {
                auto p = new OvrleyPlayer();
                p->player = new mdk::Player();
                p->rust = rust;
                p->guard = std::make_shared<OvrleyListenerGuard>();

                std::vector<std::string> names;
                std::stringstream list(decoders);
                std::string name;
                while (std::getline(list, name, ',')) if (!name.empty()) names.push_back(name);
                p->player->setDecoders(mdk::MediaType::Video, names);

                p->player->setProperty("continue_at_end", "1");
                p->player->setBufferRange(0);

                // Called with MDK's video mutex held: it reads the rotation of the first frame only.
                // VideoFrame::rotation() is counter-clockwise; stored clockwise.
                p->player->onFrame<mdk::VideoFrame>([p](mdk::VideoFrame& frame, int) -> int {
                    if (frame && frame.timestamp() != mdk::TimestampEOS) p->delivered.fetch_add(1, std::memory_order_relaxed);
                    if (p->rotation.load(std::memory_order_relaxed) >= 0) return 0;
                    if (frame && frame.timestamp() != mdk::TimestampEOS) {
                        int ccw = frame.rotation() % 360;
                        if (ccw < 0) ccw += 360;
                        p->rotation = (360 - ccw) % 360;
                    }
                    return 0;
                });
                p->player->setRenderCallback([rust](void*) { ovrley_playback_on_render(rust); });
                const auto guard = p->guard;
                p->player->onEvent([p, guard](const mdk::MediaEvent& event) -> bool {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (!guard->alive) return false;
                    ovrley_playback_on_event(p->rust, event.error, event.category.c_str(), event.detail.c_str());
                    return true;
                });
                p->player->onMediaStatus([p, guard](mdk::MediaStatus old_status, mdk::MediaStatus new_status) -> bool {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (!guard->alive) return false; // unloads the media at its next status change
                    if ((new_status & mdk::MediaStatus::Loaded) && (new_status & mdk::MediaStatus::Prepared)) p->loaded = true;
                    ovrley_playback_on_status(p->rust, uint32_t(old_status), uint32_t(new_status));
                    return true;
                });
                return p;
            })
        };
        Self { raw, callbacks }
    }

    /// Sets the media and prepares it at `start_ms`.
    pub fn open(&self, path: &str, start_ms: i64) {
        let raw = self.raw;
        let url = CString::new(path).expect("a path has no NUL");
        let url = url.as_ptr();
        unsafe {
            cpp!([raw as "OvrleyPlayer*", url as "const char*", start_ms as "int64_t"] {
                raw->loaded = false;
                raw->rotation = -1;
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
    pub unsafe fn set_metal_target(
        &self,
        device: *const c_void,
        queue: *const c_void,
        texture: *const c_void,
    ) {
        let raw = self.raw;
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

    /// Renders into `texture` (`ID3D12Resource*`, a 2D render target) through `queue`
    /// (`ID3D12CommandQueue*`, a direct queue of the texture's device). MDK records its own
    /// command list and submits it on that queue, so the caller's later submissions see the frame.
    ///
    /// # Safety
    /// Both must be valid D3D12 objects, alive until the target is replaced or the player is
    /// dropped. Call on the render thread.
    #[cfg(windows)]
    pub unsafe fn set_d3d12_target(&self, queue: *const c_void, texture: *const c_void) {
        let raw = self.raw;
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

    /// Renders into `texture` (`ID3D11Texture2D*`, a 2D render target) on its device's immediate
    /// context, which MDK flushes after each `renderVideo()`.
    ///
    /// # Safety
    /// The texture must be a valid D3D11 texture, alive until the target is replaced or the player is
    /// dropped. Call on the render thread.
    #[cfg(windows)]
    pub unsafe fn set_d3d11_target(&self, texture: *const c_void) {
        let raw = self.raw;
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

    /// Renders into framebuffer `fbo` of the OpenGL context current on the render thread. GL's
    /// framebuffer origin is bottom-left; the frame is flipped so that row 0 is the top, as the
    /// caller's Vulkan sampler reads it.
    ///
    /// # Safety
    /// The context must stay current on the render thread, and the framebuffer alive, until the
    /// target is replaced or the player is dropped (MDK releases its GL resources in `Drop`).
    #[cfg(target_os = "linux")]
    pub unsafe fn set_gl_target(&self, fbo: u32) {
        let raw = self.raw;
        let fbo = fbo as i32;
        cpp!([raw as "OvrleyPlayer*", fbo as "int"] {
            mdk::GLRenderAPI ra{};
            ra.fbo = fbo;
            raw->player->setRenderAPI(&ra);
            raw->player->scale(1.0f, -1.0f);
        })
    }

    pub fn set_surface_size(&self, width: i32, height: i32) {
        let raw = self.raw;
        unsafe {
            cpp!([raw as "OvrleyPlayer*", width as "int", height as "int"] { raw->player->setVideoSurfaceSize(width, height); })
        }
    }

    /// Draws the next or current frame into the target: its timestamp in seconds, or < 0 when
    /// no frame was rendered. Render thread only, never from [`Callbacks::render`].
    pub fn render(&self) -> f64 {
        let raw = self.raw;
        unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> f64 as "double" { return raw->player->renderVideo(); })
        }
    }

    pub fn set_state(&self, state: State) {
        let raw = self.raw;
        let value: i32 = match state {
            State::Stopped => 0,
            State::Playing => 1,
            State::Paused => 2,
        };
        unsafe {
            cpp!([raw as "OvrleyPlayer*", value as "int"] { raw->player->set(mdk::State(value)); })
        }
    }

    pub fn state(&self) -> State {
        let raw = self.raw;
        match unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> i32 as "int" { return int(raw->player->state()); })
        } {
            1 => State::Playing,
            2 => State::Paused,
            _ => State::Stopped,
        }
    }

    /// `exact`: `FromStart | InCache` (a cached frame at the target is the exact frame). Otherwise `FromStart | KeyFrame | Backward`: the key frame at or before `ms`.
    /// Not `KeyFrame | InCache`: with a forward key-frame seek and seeks landing on cached frames, a
    /// forward scrub jumps back and forth. The result arrives as [`Event::Seek`].
    pub fn seek(&self, ms: i64, exact: bool) -> bool {
        let raw = self.raw;
        unsafe {
            cpp!([raw as "OvrleyPlayer*", ms as "int64_t", exact as "bool"] -> bool as "bool" {
                auto flags = exact ? (mdk::SeekFlag::FromStart | mdk::SeekFlag::InCache)
                                   : (mdk::SeekFlag::FromStart | mdk::SeekFlag::KeyFrame | mdk::SeekFlag::Backward);
                const auto guard = raw->guard;
                void* rust = raw->rust;
                return raw->player->seek(ms, flags, [guard, rust](int64_t result) {
                    const std::lock_guard<std::recursive_mutex> lock(guard->mutex);
                    if (guard->alive) ovrley_playback_on_seek(rust, result);
                });
            })
        }
    }

    /// The presented video frame's time, in ms.
    pub fn position(&self) -> i64 {
        let raw = self.raw;
        unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> i64 as "int64_t" { return raw->player->position(); })
        }
    }

    pub fn set_mute(&self, mute: bool) {
        let raw = self.raw;
        unsafe { cpp!([raw as "OvrleyPlayer*", mute as "bool"] { raw->player->setMute(mute); }) }
    }

    /// `count` < 0 loops forever.
    pub fn set_loop(&self, count: i32) {
        let raw = self.raw;
        unsafe { cpp!([raw as "OvrleyPlayer*", count as "int"] { raw->player->setLoop(count); }) }
    }

    /// How many decoded frames MDK has delivered toward rendering (`onFrame`), since the player was made.
    pub fn delivered_frames(&self) -> u64 {
        let raw = self.raw;
        unsafe { cpp!([raw as "OvrleyPlayer*"] -> u64 as "uint64_t" { return raw->delivered.load(); }) }
    }

    /// The first frame's clockwise rotation, once it was decoded.
    pub fn frame_rotation(&self) -> Option<i32> {
        let raw = self.raw;
        let value = unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> i32 as "int" { return raw->rotation.load(); })
        };
        (value >= 0).then_some(value)
    }

    /// The first video stream, once the media is loaded.
    pub fn media_summary(&self) -> Option<MediaSummary> {
        let raw = self.raw;
        let mut summary = MediaSummary::default();
        let out = &mut summary as *mut MediaSummary as *mut c_void;
        unsafe {
            cpp!([raw as "OvrleyPlayer*", out as "void*"] {
                auto s = static_cast<OvrleyMediaSummary*>(out);
                const auto& info = raw->player->mediaInfo();
                s->duration_ms = info.duration;
                if (info.video.empty()) return;
                const auto& v = info.video[0];
                s->has_video = 1;
                s->width = v.codec.width;
                s->height = v.codec.height;
                s->fps = v.codec.frame_rate;
                s->stream_rotation = v.rotation;
            })
        };
        (summary.has_video != 0).then_some(summary)
    }

    /// An MDK player property, e.g. `"video.decoder"`.
    pub fn property(&self, key: &str) -> String {
        let raw = self.raw;
        let key = CString::new(key).expect("a key has no NUL");
        let key = key.as_ptr();
        let mut out = String::new();
        let out_ptr = &mut out as *mut String as *mut c_void;
        unsafe {
            cpp!([raw as "OvrleyPlayer*", key as "const char*", out_ptr as "void*"] {
                const auto property = raw->player->property(key);
                const char* value = property.c_str();
                rust!(OvrleyMdkSetString [out_ptr: *mut c_void as "void*", value: *const c_char as "const char*"] {
                    *(out_ptr as *mut String) = CStr::from_ptr(value).to_string_lossy().into_owned();
                });
            })
        };
        out
    }
}

impl Drop for Player {
    /// Teardown, in this order: stop; mark the listeners dead; replace the other callbacks
    /// with no-ops (null leaves MDK a C callback that dereferences a null pointer); release the
    /// GPU resources on this (the render) thread; delete the player about a second later.
    fn drop(&mut self) {
        let raw = self.raw;
        let stopped = unsafe {
            cpp!([raw as "OvrleyPlayer*"] -> bool as "bool" {
                bool stopped = true;
                if (raw->loaded) {
                    raw->player->set(mdk::State::Stopped);
                    stopped = raw->player->waitFor(mdk::State::Stopped, 3000);
                }
                {
                    const std::lock_guard<std::recursive_mutex> lock(raw->guard->mutex);
                    raw->guard->alive = false;
                }
                raw->player->setRenderCallback([](void*) {});
                raw->player->onStateChanged([](mdk::State) {});
                raw->player->onFrame<mdk::VideoFrame>([](mdk::VideoFrame&, int) -> int { return 0; });
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
            log::error!("mdk: the player did not stop within 3 s; deleting it anyway");
        }
        // No callback reaches Rust after the block above: the listeners are dead and the render
        // callback was replaced under the mutex MDK holds while it runs.
        drop(unsafe { Box::from_raw(self.callbacks) });
    }
}

/// Routes MDK's log into the `log` crate at Debug and below (step 1).
pub fn install_log_handler() {
    unsafe {
        cpp!([] {
            mdk::setLogHandler([](mdk::LogLevel level, const char* message) { ovrley_playback_on_log(int(level), message); });
            mdk::SetGlobalOption("log", "Debug");
        })
    }
}

/// MDK's version, `major.minor.patch`.
pub fn version() -> String {
    let v = unsafe { cpp!([] -> i32 as "int" { return mdk::version(); }) };
    format!("{}.{}.{}", (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
}

/// The decoders: `MDK_DECODERS` (comma-separated) when set, else the spike's default (D9): the
/// platform's hardware decoder that renders without a copy, then FFmpeg's software decoder.
/// Windows: `D3D12` (FFmpeg's d3d12va) and `MFT:d3d=12` are the decoders MDK renders zero-copy
/// into a D3D12 target. Linux: `CUDA` (NVIDIA, CUDA–GL interop) and `VAAPI` (Mesa's Intel and AMD
/// drivers, dma-buf into EGL) render zero-copy into a GL target; `CUDA` goes first because it
/// fails at once without NVIDIA's driver, while VAAPI also opens on NVIDIA through
/// nvidia-vaapi-driver.
pub fn decoders_from_env() -> Vec<String> {
    match std::env::var("MDK_DECODERS") {
        Ok(list) if !list.trim().is_empty() => list
            .split(',')
            .map(|name| name.trim().to_string())
            .collect(),
        _ if cfg!(windows) => vec!["D3D12".into(), "MFT:d3d=12".into(), "FFmpeg".into()],
        _ if cfg!(target_os = "linux") => vec!["CUDA".into(), "VAAPI".into(), "FFmpeg".into()],
        _ => vec!["VT:duration=0".into(), "FFmpeg".into()],
    }
}

fn text(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[no_mangle]
extern "C" fn ovrley_playback_on_render(rust: *mut c_void) {
    let callbacks = unsafe { &*(rust as *const Callbacks) };
    (callbacks.render)();
}

#[no_mangle]
extern "C" fn ovrley_playback_on_event(
    rust: *mut c_void,
    error: i64,
    category: *const c_char,
    detail: *const c_char,
) {
    let callbacks = unsafe { &*(rust as *const Callbacks) };
    (callbacks.event)(Event::Media {
        error,
        category: text(category),
        detail: text(detail),
    });
}

#[no_mangle]
extern "C" fn ovrley_playback_on_status(rust: *mut c_void, old: u32, new: u32) {
    let callbacks = unsafe { &*(rust as *const Callbacks) };
    (callbacks.event)(Event::Status { old, new });
}

#[no_mangle]
extern "C" fn ovrley_playback_on_seek(rust: *mut c_void, result: i64) {
    let callbacks = unsafe { &*(rust as *const Callbacks) };
    (callbacks.event)(Event::Seek { result });
}

#[no_mangle]
extern "C" fn ovrley_playback_on_log(level: i32, message: *const c_char) {
    let message = text(message);
    let message = message.trim_end();
    match level {
        1 => log::error!(target: "mdk", "{message}"),
        2 => log::warn!(target: "mdk", "{message}"),
        3 => log::info!(target: "mdk", "{message}"),
        _ => log::debug!(target: "mdk", "{message}"),
    }
}
