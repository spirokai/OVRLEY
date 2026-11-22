//! Video playback: MDK decodes and draws into textures of the host's wgpu device (Metal on macOS,
//! D3D11 on Windows, OpenGL into Vulkan memory on Linux), without a copy to system memory.
//!
//! - [`request_device`] opens the host's device with what playback needs.
//! - A [`Renderer`] holds the interop with that device; each [`Surface`] is a texture and the
//!   [`Player`] that draws into it.
//!
//! The crate owns MDK, its FFI and the GPU interop; it knows neither the UI toolkit nor the app's
//! state. Everything but [`Callbacks`] runs on the thread that owns the GPU queue.

// The `cpp!` blocks in `player` expand deeply.
#![recursion_limit = "1024"]

mod player;
mod surface;

#[cfg(windows)]
mod d3d11;
#[cfg(target_os = "linux")]
mod opengl;

pub use player::{Callbacks, Event, Player, State};
pub use surface::{request_device, Renderer, Surface};

/// Why playback cannot run on a device, or a texture cannot be made for it.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(#[from] Kind);

#[derive(Debug, thiserror::Error)]
pub(crate) enum Kind {
    #[error("the device is not a {0} device")]
    Backend(&'static str),
    #[error("{width}×{height} is not a texture size this device supports (1 to {max} pixels a side)")]
    Size { width: u32, height: u32, max: u32 },
    #[error("could not open the device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[cfg(target_os = "linux")]
    #[error("could not open the device: {0}")]
    OpenDevice(#[from] wgpu::hal::DeviceError),
    #[cfg(target_os = "linux")]
    #[error("the Vulkan device has no {0}")]
    VulkanExtension(String),
    #[cfg(target_os = "linux")]
    #[error("{operation}: {source}")]
    Vulkan { operation: &'static str, source: ash::vk::Result },
    #[cfg(target_os = "linux")]
    #[error("no device-local memory type for the frame")]
    MemoryType,
    #[cfg(target_os = "linux")]
    #[error("could not load EGL 1.5: {0}")]
    LoadEgl(String),
    #[cfg(target_os = "linux")]
    #[error("{operation}: {source}")]
    Egl { operation: &'static str, source: khronos_egl::Error },
    #[cfg(target_os = "linux")]
    #[error("no EGL display shares memory with the Vulkan device: {}", .0.join("; "))]
    NoDisplay(Vec<String>),
    #[cfg(target_os = "linux")]
    #[error("OpenGL has no {0}")]
    GlFunction(&'static str),
    #[cfg(target_os = "linux")]
    #[error("OpenGL error {code:#x} {operation}")]
    Gl { operation: &'static str, code: u32 },
    #[cfg(target_os = "linux")]
    #[error("the frame's framebuffer is incomplete ({0:#x})")]
    Framebuffer(u32),
    #[cfg(windows)]
    #[error("{operation}: {source}")]
    Direct3d { operation: &'static str, source: windows::core::Error },
}
