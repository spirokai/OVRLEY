//! The shared GPU device.
//!
//! One wgpu device is opened before the backend is selected, on the backend Skia is built for:
//! Metal on Apple, DX12 on Windows (with FXC, which ships with Windows), Vulkan elsewhere. Slint
//! renders the window on it, and MDK draws the video on the same device.
//! - A software adapter (WARP, llvmpipe, lavapipe) is not shared: the window uses Skia's CPU
//!   rasteriser, which has no device to lose.
//! - With no adapter, the platform API is required by name, so a silent fallback to the CPU
//!   rasteriser becomes a visible startup failure.

use slint::wgpu_30::wgpu;
use std::sync::Arc;

/// The device Slint renders on and MDK draws on.
#[derive(Clone)]
pub struct GpuDevice {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// What the backend selection does with the GPU.
pub enum GpuChoice {
    /// A hardware adapter: Slint renders on this device.
    Shared(GpuDevice),
    /// A CPU rasteriser adapter: not shared; the window uses Skia's software renderer.
    Software,
    /// No adapter opened: the platform API is required by name.
    None,
}

fn backend() -> wgpu::Backends {
    if cfg!(target_vendor = "apple") {
        wgpu::Backends::METAL
    } else if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::VULKAN
    }
}

/// A one-line description of an adapter, for the log.
fn describe(info: &wgpu::AdapterInfo) -> String {
    let software = if info.device_type == wgpu::DeviceType::Cpu {
        ", software"
    } else {
        ""
    };
    format!("{} ({:?}{software})", info.name, info.backend)
}

/// Opens the shared device. A failure is logged, and the window renders without one.
pub fn open() -> GpuChoice {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = backend();
    // wgpu's automatic choice can load a stray `dxcompiler.dll` from PATH and crash at the first
    // shader; FXC ships with Windows. `WGPU_DX12_COMPILER` still overrides it for testing.
    if std::env::var_os("WGPU_DX12_COMPILER").is_none() {
        descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
    }
    let instance = wgpu::Instance::new(descriptor);

    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            log::warn!("no GPU adapter for {:?}: {error}", backend());
            return GpuChoice::None;
        }
    };
    let info = adapter.get_info();
    if info.device_type == wgpu::DeviceType::Cpu {
        log::info!(
            "GPU adapter {} is a software rasteriser; the window renders on the CPU",
            describe(&info)
        );
        return GpuChoice::Software;
    }

    // The adapter's own limits: wgpu's defaults are WebGPU's baseline, which caps 2D textures at
    // 8192 and which some adapters cannot open.
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("ovrley"),
        required_limits: adapter.limits(),
        ..Default::default()
    };
    let (device, queue) = match ovrley_playback::request_device(&adapter, &descriptor) {
        Ok(pair) => pair,
        Err(error) => {
            log::warn!("could not open a device on {}: {error}", describe(&info));
            return GpuChoice::None;
        }
    };

    device.set_device_lost_callback(|reason, message| {
        log::error!("GPU device lost ({reason:?}): {message}");
    });
    // wgpu's default handler panics, which would take the UI thread down.
    device.on_uncaptured_error(Arc::new(|error: wgpu::Error| {
        log::error!("uncaptured GPU error: {error}")
    }));

    log::info!("shared GPU device on {}", describe(&info));
    GpuChoice::Shared(GpuDevice {
        instance,
        adapter,
        device,
        queue,
    })
}
