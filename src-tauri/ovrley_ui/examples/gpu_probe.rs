//! GPU cost probe (measurement aid, not product code): what one presented frame of a maximized
//! window costs, with and without Slint.
//!
//! `cargo run -p ovrley_ui --example gpu_probe -- <mode> <fps> [busy]`
//! - `raw` (Windows): winit + wgpu on the shared DX12 device: a full-window clear and present per
//!   frame.
//! - `d3d-full` (Windows): winit + an own FLIP_SEQUENTIAL DXGI swapchain on the shared DX12 queue:
//!   a full-window clear, a 2 px playhead, and `Present1` without dirty rectangles.
//! - `d3d-dirty` (Windows): the same swapchain, but once each buffer has been drawn whole, only the
//!   strip the playhead crossed since that buffer was last drawn is cleared and presented as dirty.
//! - `slint`: the app's backend selection (`platform::select_backend`) and a window whose only
//!   change per frame is a 2 px playhead moving 7 px. `fps` 0 never changes it.
//! - `slint-sw`: the same on Skia's software renderer (partial rendering).
//! - `busy` adds ~900 text labels and shadowed rounded rectangles that never change.
//!
//! `PROBE_SECONDS` (default 14) closes the window after that long. `PROBE_SIZE=<w>x<h>` makes a
//! window of that size instead of maximizing. `PROBE_IDLE=<s>` keeps the window still for that
//! long before the frames start (to measure DWM against its own baseline).
//!
//! The probe prints frames and CPU time; GPU time comes from outside. On Windows, read the
//! process's `\GPU Engine(pid_<pid>_*engtype_3D)\Running Time` counter (100 ns units) before and
//! after a span, run with `WGPU_VALIDATION=0 WGPU_DEBUG=0` (debug builds otherwise turn on the
//! D3D12 debug layer), and measure `dwm.exe` the same way, as an idle→active delta.

use slint::winit_030::{winit, WinitWindowAccessor};
use slint::ComponentHandle;
use std::time::Duration;

slint::slint! {
    #[edition = "1.19"]
    export component Probe inherits Window {
        in property <length> playhead-x: 0px;
        in property <bool> busy: false;
        out property <length> view-width: root.width;
        background: #1b1b1f;
        if busy: Rectangle {
            for i in 600: Text {
                x: Math.mod(i, 30) * 112px + 8px;
                y: Math.floor(i / 30) * 30px + 40px;
                text: "Label " + i;
                color: #c8c8d0;
                font-size: 13px;
            }
            for i in 300: Rectangle {
                x: Math.mod(i, 30) * 112px + 8px;
                y: Math.floor(i / 30) * 64px + 680px;
                width: 100px;
                height: 48px;
                border-radius: 6px;
                background: #2a2a33;
                border-width: 1px;
                border-color: #44444f;
                drop-shadow-blur: 6px;
                drop-shadow-color: #00000080;
            }
        }
        Rectangle {
            x: root.playhead-x;
            y: 0;
            width: 2px;
            height: root.height;
            background: #e5484d;
        }
    }
}

fn seconds() -> Duration {
    Duration::from_secs(
        std::env::var("PROBE_SECONDS")
            .ok()
            .map_or(14, |value| value.parse().expect("PROBE_SECONDS")),
    )
}

fn idle() -> Duration {
    Duration::from_secs(
        std::env::var("PROBE_IDLE")
            .ok()
            .map_or(0, |value| value.parse().expect("PROBE_IDLE")),
    )
}

fn fixed_size() -> Option<(u32, u32)> {
    let value = std::env::var("PROBE_SIZE").ok()?;
    let (width, height) = value.split_once('x').expect("PROBE_SIZE=<w>x<h>");
    Some((
        width.parse().expect("width"),
        height.parse().expect("height"),
    ))
}

fn attributes(attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let attributes = attributes
        .with_title("gpu probe")
        .with_decorations(false)
        .with_active(true);
    #[cfg(windows)]
    let attributes = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes.with_undecorated_shadow(true)
    };
    match fixed_size() {
        Some((width, height)) => {
            attributes.with_inner_size(winit::dpi::PhysicalSize::new(width, height))
        }
        None => attributes.with_maximized(true),
    }
}

fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("slint");
    let fps: u32 = args.get(1).map_or(30, |value| value.parse().expect("fps"));
    let busy = args.iter().any(|arg| arg == "busy");
    println!(
        "probe: pid {} mode {mode} fps {fps} busy {busy}",
        std::process::id()
    );
    match mode {
        #[cfg(windows)]
        "raw" => raw::run(raw::Kind::Wgpu, fps),
        #[cfg(windows)]
        "d3d-full" => raw::run(raw::Kind::D3dFull, fps),
        #[cfg(windows)]
        "d3d-dirty" => raw::run(raw::Kind::D3dDirty, fps),
        "slint" | "slint-sw" => run_slint(mode == "slint-sw", fps, busy),
        other => panic!("unknown mode {other}"),
    }
}

fn run_slint(software: bool, fps: u32, busy: bool) {
    let _gpu = if software {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("skia-software".into())
            .with_winit_window_attributes_hook(attributes)
            .select()
            .expect("software backend");
        None
    } else {
        ovrley_ui::platform::select_backend().expect("the app's backend")
    };
    let window = Probe::new().expect("window");
    window.set_busy(busy);
    if let Some((width, height)) = fixed_size() {
        window
            .window()
            .set_size(slint::PhysicalSize::new(width, height));
    }
    let resized = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let counted = resized.clone();
    window.window().on_winit_window_event(move |_, event| {
        if let winit::event::WindowEvent::Resized(size) = event {
            counted.set(counted.get() + 1);
            println!(
                "probe: resized to {}x{} (#{})",
                size.width,
                size.height,
                counted.get()
            );
        }
        slint::winit_030::EventResult::Propagate
    });
    let ticker = std::rc::Rc::new(slint::Timer::default());
    if fps > 0 {
        let weak = window.as_weak();
        let started = ticker.clone();
        slint::Timer::single_shot(idle(), move || {
            println!("probe: frames start");
            started.start(
                slint::TimerMode::Repeated,
                Duration::from_secs_f64(1.0 / f64::from(fps)),
                move || {
                    let Some(window) = weak.upgrade() else { return };
                    let width = window.get_view_width();
                    let next = window.get_playhead_x() + 7.0;
                    window.set_playhead_x(if next > width { 0.0 } else { next });
                },
            );
        });
    }
    slint::Timer::single_shot(seconds(), || slint::quit_event_loop().expect("quit"));
    window.run().expect("run");
}

/// The Windows-only modes: presentation without Slint, through wgpu or an own DXGI swapchain.
#[cfg(windows)]
mod raw {
    use super::{attributes, idle, seconds, winit};
    use slint::wgpu_30::wgpu;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Kind {
        Wgpu,
        D3dFull,
        D3dDirty,
    }

    const PLAYHEAD_WIDTH: i32 = 2;
    const PLAYHEAD_STEP: i32 = 7;

    enum Presenter {
        Wgpu(wgpu::Surface<'static>, wgpu::SurfaceConfiguration),
        D3d(d3d::Chain),
    }

    struct Raw {
        kind: Kind,
        gpu: ovrley_ui::platform::gpu::GpuDevice,
        interval: Option<Duration>,
        window: Option<Arc<winit::window::Window>>,
        presenter: Option<Presenter>,
        playhead: i32,
        next_frame: Instant,
        started: Instant,
        frames: u32,
        cpu: Duration,
        report_at: Instant,
        announced: bool,
    }

    pub fn run(kind: Kind, fps: u32) {
        let ovrley_ui::platform::gpu::GpuChoice::Shared(gpu) = ovrley_ui::platform::gpu::open()
        else {
            panic!("no hardware adapter");
        };
        let event_loop = EventLoop::new().expect("event loop");
        let now = Instant::now();
        let mut app = Raw {
            kind,
            gpu,
            interval: (fps > 0).then(|| Duration::from_secs_f64(1.0 / f64::from(fps))),
            window: None,
            presenter: None,
            playhead: 0,
            next_frame: now + idle(),
            started: now,
            frames: 0,
            cpu: Duration::ZERO,
            report_at: now + Duration::from_secs(1),
            announced: false,
        };
        event_loop.run_app(&mut app).expect("run");
    }

    impl Raw {
        fn resize(&mut self, width: u32, height: u32) {
            match self.presenter.as_mut() {
                Some(Presenter::Wgpu(surface, config)) => {
                    config.width = width.max(1);
                    config.height = height.max(1);
                    surface.configure(&self.gpu.device, config);
                }
                Some(Presenter::D3d(chain)) => chain.resize(width.max(1), height.max(1)),
                None => {}
            }
            println!("probe: configured {width}x{height}");
        }

        fn render(&mut self) {
            let started = Instant::now();
            let width = self
                .window
                .as_ref()
                .map_or(1, |window| window.inner_size().width as i32);
            let previous = self.playhead;
            self.playhead = if previous + PLAYHEAD_STEP > width - PLAYHEAD_WIDTH {
                0
            } else {
                previous + PLAYHEAD_STEP
            };
            match self.presenter.as_mut() {
                Some(Presenter::Wgpu(surface, _)) => {
                    let frame = match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(frame) => frame,
                        other => {
                            println!("probe: acquire returned {other:?}");
                            return;
                        }
                    };
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder =
                        self.gpu
                            .device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("probe clear"),
                            });
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("probe clear"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.1,
                                    b: 0.12,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    self.gpu.queue.submit(Some(encoder.finish()));
                    if let Some(window) = &self.window {
                        window.pre_present_notify();
                    }
                    self.gpu.queue.present(frame);
                }
                Some(Presenter::D3d(chain)) => {
                    if let Some(window) = &self.window {
                        window.pre_present_notify();
                    }
                    chain.frame(self.playhead, self.kind == Kind::D3dDirty);
                }
                None => return,
            }
            self.cpu += started.elapsed();
            self.frames += 1;
        }
    }

    impl ApplicationHandler for Raw {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let window = Arc::new(
                event_loop
                    .create_window(attributes(winit::window::Window::default_attributes()))
                    .expect("window"),
            );
            let size = window.inner_size();
            self.presenter = Some(match self.kind {
                Kind::Wgpu => {
                    let surface = self
                        .gpu
                        .instance
                        .create_surface(window.clone())
                        .expect("surface");
                    let mut config = surface
                        .get_default_config(
                            &self.gpu.adapter,
                            size.width.max(1),
                            size.height.max(1),
                        )
                        .expect("config");
                    let capabilities = surface.get_capabilities(&self.gpu.adapter);
                    config.format = capabilities
                        .formats
                        .iter()
                        .copied()
                        .find(|format| {
                            matches!(
                                format,
                                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                            )
                        })
                        .unwrap_or(capabilities.formats[0]);
                    config.present_mode = wgpu::PresentMode::AutoVsync;
                    surface.configure(&self.gpu.device, &config);
                    println!(
                        "probe: wgpu surface {}x{} {:?} {:?}",
                        config.width, config.height, config.format, config.alpha_mode
                    );
                    Presenter::Wgpu(surface, config)
                }
                Kind::D3dFull | Kind::D3dDirty => {
                    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    let RawWindowHandle::Win32(handle) =
                        window.window_handle().expect("handle").as_raw()
                    else {
                        panic!("not Win32")
                    };
                    let hwnd = windows::Win32::Foundation::HWND(
                        handle.hwnd.get() as *mut core::ffi::c_void
                    );
                    println!(
                        "probe: own FLIP_SEQUENTIAL swapchain {}x{}",
                        size.width, size.height
                    );
                    Presenter::D3d(d3d::Chain::new(
                        &self.gpu,
                        hwnd,
                        size.width.max(1),
                        size.height.max(1),
                    ))
                }
            });
            self.window = Some(window);
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    self.resize(size.width, size.height);
                    self.render();
                }
                WindowEvent::RedrawRequested => self.render(),
                _ => {}
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            let now = Instant::now();
            if now.duration_since(self.started) >= seconds() {
                event_loop.exit();
                return;
            }
            if now >= self.report_at {
                if self.frames > 0 {
                    println!(
                        "probe: {} frames, cpu {:.2} ms/frame",
                        self.frames,
                        self.cpu.as_secs_f64() * 1000.0 / f64::from(self.frames)
                    );
                }
                self.frames = 0;
                self.cpu = Duration::ZERO;
                self.report_at += Duration::from_secs(1);
            }
            let Some(interval) = self.interval else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.report_at));
                return;
            };
            if now >= self.next_frame {
                if !self.announced {
                    self.announced = true;
                    println!("probe: frames start");
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                self.next_frame += interval;
                if self.next_frame < now {
                    self.next_frame = now + interval;
                }
            }
            event_loop
                .set_control_flow(ControlFlow::WaitUntil(self.next_frame.min(self.report_at)));
        }
    }

    /// An own DXGI swapchain on the shared DX12 queue, so that the swap effect and `Present1`'s
    /// dirty rectangles (which wgpu does not expose) can be measured.
    mod d3d {
        use windows::core::Interface;
        use windows::Win32::Foundation::{HANDLE, HWND, RECT};
        use windows::Win32::Graphics::Direct3D12::*;
        use windows::Win32::Graphics::Dxgi::Common::*;
        use windows::Win32::Graphics::Dxgi::*;

        type Dx12 = slint::wgpu_30::wgpu::wgc::api::Dx12;
        const BUFFERS: u32 = 3;
        const BACKGROUND: [f32; 4] = [0.1, 0.1, 0.12, 1.0];
        const PLAYHEAD: [f32; 4] = [0.9, 0.28, 0.3, 1.0];

        pub struct Chain {
            device: ID3D12Device,
            queue: ID3D12CommandQueue,
            swap: IDXGISwapChain3,
            buffers: Vec<ID3D12Resource>,
            heap: ID3D12DescriptorHeap,
            rtv_size: usize,
            allocator: ID3D12CommandAllocator,
            list: ID3D12GraphicsCommandList,
            fence: ID3D12Fence,
            fence_value: u64,
            width: u32,
            height: u32,
            /// Per buffer: the playhead it last showed, once drawn whole since the last resize.
            drawn: Vec<Option<i32>>,
        }

        fn transition(
            resource: &ID3D12Resource,
            before: D3D12_RESOURCE_STATES,
            after: D3D12_RESOURCE_STATES,
        ) -> D3D12_RESOURCE_BARRIER {
            D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: core::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: unsafe { core::mem::transmute_copy(resource) },
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: before,
                        StateAfter: after,
                    }),
                },
            }
        }

        impl Chain {
            pub fn new(
                gpu: &ovrley_ui::platform::gpu::GpuDevice,
                hwnd: HWND,
                width: u32,
                height: u32,
            ) -> Self {
                unsafe {
                    let device = gpu
                        .device
                        .as_hal::<Dx12>()
                        .expect("DX12")
                        .raw_device()
                        .clone();
                    let queue = gpu.queue.as_hal::<Dx12>().expect("DX12").as_raw().clone();
                    let factory: IDXGIFactory2 =
                        CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).expect("factory");
                    let desc = DXGI_SWAP_CHAIN_DESC1 {
                        Width: width,
                        Height: height,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        Stereo: false.into(),
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                        BufferCount: BUFFERS,
                        Scaling: DXGI_SCALING_STRETCH,
                        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                        AlphaMode: DXGI_ALPHA_MODE_UNSPECIFIED,
                        Flags: 0,
                    };
                    let swap: IDXGISwapChain3 = factory
                        .CreateSwapChainForHwnd(&queue, hwnd, &desc, None, None)
                        .expect("swapchain")
                        .cast()
                        .expect("swapchain 3");
                    factory
                        .MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)
                        .expect("association");
                    let heap: ID3D12DescriptorHeap = device
                        .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                            NumDescriptors: BUFFERS,
                            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                            NodeMask: 0,
                        })
                        .expect("rtv heap");
                    let rtv_size = device
                        .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
                        as usize;
                    let allocator: ID3D12CommandAllocator = device
                        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                        .expect("allocator");
                    let list: ID3D12GraphicsCommandList = device
                        .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                        .expect("list");
                    list.Close().expect("close");
                    let fence: ID3D12Fence =
                        device.CreateFence(0, D3D12_FENCE_FLAG_NONE).expect("fence");
                    let mut chain = Self {
                        device,
                        queue,
                        swap,
                        buffers: Vec::new(),
                        heap,
                        rtv_size,
                        allocator,
                        list,
                        fence,
                        fence_value: 0,
                        width,
                        height,
                        drawn: vec![None; BUFFERS as usize],
                    };
                    chain.fetch_buffers();
                    chain
                }
            }

            fn rtv(&self, index: usize) -> D3D12_CPU_DESCRIPTOR_HANDLE {
                let start = unsafe { self.heap.GetCPUDescriptorHandleForHeapStart() };
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: start.ptr + index * self.rtv_size,
                }
            }

            fn fetch_buffers(&mut self) {
                self.buffers = (0..BUFFERS)
                    .map(|index| unsafe {
                        let buffer: ID3D12Resource = self.swap.GetBuffer(index).expect("buffer");
                        self.device
                            .CreateRenderTargetView(&buffer, None, self.rtv(index as usize));
                        buffer
                    })
                    .collect();
                self.drawn = vec![None; BUFFERS as usize];
            }

            /// Blocks until the GPU finished everything submitted here.
            fn wait(&self) {
                // A null event makes the call block until the fence reaches the value.
                unsafe {
                    self.fence
                        .SetEventOnCompletion(self.fence_value, HANDLE::default())
                }
                .expect("wait");
            }

            pub fn resize(&mut self, width: u32, height: u32) {
                if (width, height) == (self.width, self.height) {
                    return;
                }
                self.wait();
                self.buffers.clear();
                // The debug layer counts the last command list's use of a buffer as a reference.
                unsafe {
                    self.allocator.Reset().expect("reset allocator");
                    self.list.Reset(&self.allocator, None).expect("reset list");
                    self.list.Close().expect("close");
                    self.swap.ResizeBuffers(
                        0,
                        width,
                        height,
                        DXGI_FORMAT_UNKNOWN,
                        DXGI_SWAP_CHAIN_FLAG(0),
                    )
                }
                .expect("resize");
                self.width = width;
                self.height = height;
                self.fetch_buffers();
            }

            /// Draws the background and the playhead at `playhead`. With `dirty`, a buffer drawn
            /// whole before only gets the strip between the playhead it showed and the new one.
            pub fn frame(&mut self, playhead: i32, dirty: bool) {
                self.wait();
                let index = unsafe { self.swap.GetCurrentBackBufferIndex() } as usize;
                let height = self.height as i32;
                let full = RECT {
                    left: 0,
                    top: 0,
                    right: self.width as i32,
                    bottom: height,
                };
                let line = RECT {
                    left: playhead,
                    top: 0,
                    right: playhead + super::PLAYHEAD_WIDTH,
                    bottom: height,
                };
                let region = match (dirty, self.drawn[index]) {
                    (true, Some(shown)) if shown <= playhead => RECT {
                        left: shown,
                        top: 0,
                        right: playhead + super::PLAYHEAD_WIDTH,
                        bottom: height,
                    },
                    _ => full,
                };
                unsafe {
                    self.allocator.Reset().expect("reset allocator");
                    self.list.Reset(&self.allocator, None).expect("reset list");
                    let buffer = &self.buffers[index];
                    self.list.ResourceBarrier(&[transition(
                        buffer,
                        D3D12_RESOURCE_STATE_PRESENT,
                        D3D12_RESOURCE_STATE_RENDER_TARGET,
                    )]);
                    self.list
                        .ClearRenderTargetView(self.rtv(index), &BACKGROUND, Some(&[region]));
                    self.list
                        .ClearRenderTargetView(self.rtv(index), &PLAYHEAD, Some(&[line]));
                    self.list.ResourceBarrier(&[transition(
                        buffer,
                        D3D12_RESOURCE_STATE_RENDER_TARGET,
                        D3D12_RESOURCE_STATE_PRESENT,
                    )]);
                    self.list.Close().expect("close");
                    self.queue
                        .ExecuteCommandLists(&[Some(self.list.cast().expect("command list"))]);
                    self.fence_value += 1;
                    self.queue
                        .Signal(&self.fence, self.fence_value)
                        .expect("signal");
                    let mut rects = [region];
                    let parameters = DXGI_PRESENT_PARAMETERS {
                        DirtyRectsCount: if dirty { 1 } else { 0 },
                        pDirtyRects: if dirty {
                            rects.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        pScrollRect: core::ptr::null_mut(),
                        pScrollOffset: core::ptr::null_mut(),
                    };
                    self.swap
                        .Present1(1, DXGI_PRESENT(0), &parameters)
                        .ok()
                        .expect("present");
                }
                self.drawn[index] = Some(playhead);
            }
        }

        impl Drop for Chain {
            fn drop(&mut self) {
                self.wait();
            }
        }
    }
}
