//! `video`: a video playing into a `surface` element, the way the app plays it.
//!
//! The device is opened as `ovrley_ui::platform::gpu::open` opens it (Metal, DX12 or Vulkan, through
//! `ovrley_playback::request_device`, which adds what MDK's interop needs) and zgui adopts it, as
//! Slint is handed it in the app. MDK draws each frame into one 1920×1080 texture of that device on
//! the UI thread, and the texture is presented to the element without a copy; zgui damages only the
//! element's box. `PROBE_VIDEO_HIDDEN=1` draws the video into its texture and never presents it:
//! MDK's cost alone.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ovrley_playback::{Callbacks, Event, Renderer, State, Surface};
use zgui::platform::PlatformError;
use zgui::prelude::*;
use zgui::render::RenderTarget;
use zgui::runtime::{AppError, RendererFactory};
use zgui::surface::{SurfaceConfig, SurfaceHandle, SurfaceIntrinsic};
use zgui_render_wgpu::renderer::PrePresent;
use zgui_render_wgpu::{wgpu, Gpu, PresentPacing, SharedGraphics};

/// The texture MDK draws into; the element scales it to its box.
const FRAME: (u32, u32) = (1920, 1080);

thread_local! {
    /// The device and the playback on it. UI thread only, as `ovrley_playback` requires.
    static PLAYBACK: RefCell<Option<Playback>> = const { RefCell::new(None) };
    /// Video frames MDK drew since the last [`take_drawn`].
    static DRAWN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

struct Playback {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<Surface>,
}

/// Opens the device the way the app does, and keeps it for the playback. Every mode draws on it.
pub fn open_device() -> Arc<Gpu> {
    let backends = if cfg!(target_vendor = "apple") {
        wgpu::Backends::METAL
    } else if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::VULKAN
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = backends;
    if std::env::var_os("WGPU_DX12_COMPILER").is_none() {
        descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("a GPU adapter");
    // The app's descriptor, plus dual-source blending where the adapter has it, which zgui's text
    // asks for.
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("ovrley"),
        // Timestamp queries too when zgui's pass timing is asked for, as its own device would.
        required_features: adapter.features()
            & (wgpu::Features::DUAL_SOURCE_BLENDING
                | if std::env::var_os("ZGUI_GPU_TIMING").is_some() {
                    wgpu::Features::TIMESTAMP_QUERY
                        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                } else {
                    wgpu::Features::empty()
                }),
        required_limits: adapter.limits(),
        ..Default::default()
    };
    let (device, queue) =
        ovrley_playback::request_device(&adapter, &descriptor).expect("the playback device");
    println!("probe: device on {:?}", adapter.get_info().backend);
    PLAYBACK.with_borrow_mut(|playback| {
        *playback = Some(Playback {
            device: device.clone(),
            queue: queue.clone(),
            surface: None,
        });
    });
    Gpu::from_existing(instance, adapter, device, queue)
}

/// zgui's own renderer factory (`zgui::app::graphics`), on the adopted device.
pub fn factory(gpu: Arc<Gpu>) -> RendererFactory {
    let graphics = SharedGraphics::with_gpu(gpu);
    Box::new(move |surface, target| {
        let handles = Arc::clone(surface).gpu_shared().ok_or_else(|| {
            AppError::Platform(PlatformError::Backend("no GPU handles".to_owned()))
        })?;
        let pacing = match surface.present_pacing() {
            zgui::platform::PresentPacing::Platform => PresentPacing::Platform,
            _ => PresentPacing::Display,
        };
        let pre_present = (pacing == PresentPacing::Platform).then(|| {
            let notified = Arc::clone(surface);
            Box::new(move |target: RenderTarget| {
                notified.pre_present_frame(target.size, f64::from(target.scale.get()));
            }) as PrePresent
        });
        let mut renderer = graphics.renderer_for_surface(target, handles, pacing, pre_present)?;
        if pacing == PresentPacing::Platform {
            let gate = Arc::clone(surface);
            renderer.set_present_gate(Box::new(move || gate.present_now()));
        }
        renderer.set_crops_buffers(surface.crops_buffers());
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        Ok(Box::new(renderer))
    })
}

/// The `surface` element, with `path` loaded paused; [`play`] starts it.
pub fn element(path: &str) -> impl IntoView {
    let handle = SurfaceHandle::new(SurfaceConfig {
        premultiplied: true,
        intrinsic: SurfaceIntrinsic::ratio(16.0 / 9.0),
    });
    let ui = ui();
    let pending = Arc::new(AtomicBool::new(false));
    let hidden = std::env::var_os("PROBE_VIDEO_HIDDEN").is_some();
    let presented = handle.clone();
    let callbacks = Callbacks {
        // As the app: posts coalesce while one is pending, and the frame is drawn on the UI thread.
        render: Box::new(move || {
            if pending.swap(true, Ordering::AcqRel) {
                return;
            }
            let pending = Arc::clone(&pending);
            let presented = presented.clone();
            ui.post(move || {
                pending.store(false, Ordering::Release);
                // `try_with`: a post can still run while the thread's locals are torn down.
                let _ = PLAYBACK.try_with(|playback| {
                    let playback = playback.borrow();
                    let Some(surface) = playback.as_ref().and_then(|p| p.surface.as_ref()) else {
                        return;
                    };
                    if surface.render().is_some() {
                        DRAWN.set(DRAWN.get() + 1);
                        if !hidden {
                            presented.present(Arc::new(surface.texture().clone()));
                        }
                    }
                });
            });
        }),
        event: Box::new(|event| {
            if event == Event::Failed {
                eprintln!("probe: the video cannot be played");
            }
        }),
    };
    PLAYBACK.with_borrow_mut(|playback| {
        let playback = playback.as_mut().expect("open_device ran");
        let renderer =
            Renderer::new(&playback.device, &playback.queue).expect("MDK on this device");
        let mut surface = Surface::new(&renderer, FRAME).expect("the video texture");
        surface.load(path, 0.0, callbacks);
        playback.surface = Some(surface);
    });
    zgui::elements::surface()
        .class("video")
        .source(&handle)
        .into_view()
}

/// Starts the loaded video, muted.
pub fn play() {
    PLAYBACK.with_borrow(|playback| {
        if let Some(player) = playback
            .as_ref()
            .and_then(|p| p.surface.as_ref())
            .and_then(Surface::player)
        {
            player.set_muted(true);
            player.set_state(State::Playing);
        }
    });
}

/// Releases the player before the application quits, while the device is still there.
pub fn stop() {
    PLAYBACK.with_borrow_mut(|playback| {
        if let Some(playback) = playback.as_mut() {
            playback.surface = None;
        }
    });
}

/// Video frames MDK drew since the last call.
pub fn take_drawn() -> u32 {
    DRAWN.replace(0)
}
