//! The shared GPU device (plan D11; `concat-gpu-device.md`).
//!
//! One wgpu device is opened before the backend is selected, on the backend Skia is built for:
//! Metal on Apple, DX12 on Windows (with FXC, which ships with Windows), Vulkan elsewhere. Slint
//! renders the window on it, and MDK draws the video on the same device.
//! - A software adapter (WARP, llvmpipe, lavapipe) is not shared: the window uses Skia's CPU
//!   rasteriser, which has no device to lose.
//! - With no adapter, the platform API is required by name, so a silent fallback to the CPU
//!   rasteriser becomes a visible startup failure.

use slint::wgpu_30::wgpu;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The device Slint renders on and MDK draws on.
#[derive(Clone)]
pub struct GpuDevice {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// Set by the device-lost callback.
    pub lost: Arc<AtomicBool>,
}

/// What the backend selection does with the GPU.
pub enum GpuChoice {
    /// A hardware adapter: Slint renders on this device.
    Shared(GpuDevice),
    /// A CPU rasteriser adapter: not shared; the window uses Skia's software renderer.
    Software { adapter: String },
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
pub fn describe(info: &wgpu::AdapterInfo) -> String {
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
        return GpuChoice::Software {
            adapter: describe(&info),
        };
    }

    // The adapter's own limits: wgpu's defaults are WebGPU's baseline, which caps 2D textures at
    // 8192 and which some adapters cannot open.
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("ovrley"),
        required_limits: adapter.limits(),
        ..Default::default()
    };
    let (device, queue) = match request_device(&adapter, &descriptor) {
        Ok(pair) => pair,
        Err(error) => {
            log::warn!("could not open a device on {}: {error}", describe(&info));
            return GpuChoice::None;
        }
    };

    let lost = Arc::new(AtomicBool::new(false));
    let lost_flag = lost.clone();
    device.set_device_lost_callback(move |reason, message| {
        lost_flag.store(true, Ordering::SeqCst);
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
        lost,
    })
}

#[cfg(not(target_os = "linux"))]
fn request_device(
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor,
) -> Result<(wgpu::Device, wgpu::Queue), String> {
    pollster::block_on(adapter.request_device(descriptor)).map_err(|error| error.to_string())
}

/// Video playback's Vulkan–OpenGL bridge (`ovrley_playback::target`) shares
/// semaphores with OpenGL through `VK_KHR_external_semaphore_fd`, which wgpu does not enable
/// itself (it enables `VK_KHR_external_memory_fd` when the driver has it). Without the extension
/// the device opens as usual and playback reports it at start.
#[cfg(target_os = "linux")]
fn request_device(
    adapter: &wgpu::Adapter,
    descriptor: &wgpu::DeviceDescriptor,
) -> Result<(wgpu::Device, wgpu::Queue), String> {
    type Vulkan = wgpu::wgc::api::Vulkan;
    let semaphore_fd = ash::khr::external_semaphore_fd::NAME;
    let opened = {
        let hal_adapter =
            unsafe { adapter.as_hal::<Vulkan>() }.expect("the Linux adapter is Vulkan (backend())");
        if !hal_adapter.physical_device_capabilities().supports_extension(semaphore_fd) {
            None
        } else {
            let callback: Box<wgpu::hal::vulkan::CreateDeviceCallback> =
                Box::new(move |args| args.extensions.push(semaphore_fd));
            // SAFETY: the callback only adds an extension the adapter supports.
            Some(unsafe {
                hal_adapter.open_with_callback(
                    descriptor.required_features,
                    &descriptor.required_limits,
                    &descriptor.memory_hints,
                    Some(callback),
                )
            })
        }
    };
    match opened {
        None => pollster::block_on(adapter.request_device(descriptor)).map_err(|error| error.to_string()),
        Some(opened) => {
            let opened = opened.map_err(|error| error.to_string())?;
            // SAFETY: opened from this adapter with the descriptor's features and limits.
            unsafe { adapter.create_device_from_hal(opened, descriptor) }.map_err(|error| error.to_string())
        }
    }
}
