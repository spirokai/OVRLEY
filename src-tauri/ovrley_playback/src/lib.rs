//! Video playback (Phase 6B spike, branch spike/mdk-macos, never merged): decoders that render
//! into a texture of the shared wgpu device: Metal on macOS, D3D12 on Windows, OpenGL on Linux
//! (into Vulkan memory the GL context imports), with MDK as the one engine.

// The `cpp!` block in `player` expands deeply.
#![recursion_limit = "1024"]

pub mod player;
