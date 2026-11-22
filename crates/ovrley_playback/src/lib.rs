//! Video playback: MDK decodes and draws into textures of the host's wgpu device (Metal on macOS,
//! D3D11 on Windows, OpenGL into Vulkan memory on Linux), without a copy to system memory.
//! The crate owns MDK, its FFI and the GPU interop; it knows neither Slint nor the store.

// The `cpp!` block in `player` expands deeply.
#![recursion_limit = "1024"]

pub mod player;
pub mod target;
