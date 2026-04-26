# Parallel Skia GPU (Vulkan) Renderer Implementation Plan

This document outlines the strategy for migrating to a highly optimized, Zero-Copy GPU-accelerated Vulkan renderer. It integrates with the Vulkan-accelerated ProRes encoding pipeline for maximum performance.

## 1. Goal Description

The objective is to eliminate the current bottleneck (CPU rendering and memory transfer) for hardware-accelerated encode jobs by moving the entire pipeline to the GPU. 

By utilizing Skia's Vulkan backend and integrating FFmpeg at the memory level (Zero-Copy), the video frame data will never leave the GPU (VRAM) between the rendering step and the final encoding step. This is the industry-standard approach for maximum throughput and minimal latency.

> [!IMPORTANT]
> **Parallel Pathway:** This new GPU renderer must be implemented as a parallel, optional pathway. The existing CPU-based Skia rendering pipeline and FFmpeg CLI subprocess architecture must remain fully intact and available. If the user opts to use a CPU-based encoder (e.g., the regular `prores_ks`), the application should gracefully fall back to the current CPU renderer.

## 2. Architecture Decisions & Constraints

Based on the requirement for maximum performance and a true "Zero-Copy" architecture, we have solidified the following technical decisions:

1. **Vulkan Initialization (`ash`)**: We will use the `ash` crate. It is the de-facto industry standard for Vulkan in Rust. It provides the low-level control required to manually create the Vulkan `Instance`, `Device`, and `Queue`, which we need to share between Skia and the video encoder.
2. **Separation of Concerns (New Files Only)**: To ensure the existing CPU pipeline remains completely untouched and stable, all new code implemented for this plan must be placed in entirely new files with a `_gpu.rs` suffix (e.g., `video_gpu.rs`, `surface_gpu.rs`). We will avoid modifying the existing core render files wherever possible.
3. **Zero-Copy Pixel Delivery (In-Process FFmpeg)**: 
   - *The Challenge*: You cannot pass raw Vulkan GPU texture handles to a separate FFmpeg CLI process via `stdin`.
   - *The Solution*: To achieve true zero-copy for the GPU pathway, we will use an in-process FFmpeg integration. Since you have the **shared developer libraries** (`.lib`, `.dll`, `.h`) for your custom FFmpeg 8.1 build, we can link Rust directly to them using `ffmpeg-next` (or raw bindings). 
   - *The Flow*: We will initialize a Vulkan `AVHWDeviceContext` in `libavutil`. We pass this Vulkan device to Skia. Skia renders directly to a Vulkan texture. We then wrap this texture in an `AVFrame` (with `AV_PIX_FMT_VULKAN`) and pass it directly to the `prores_ks_vulkan` encoder. **The pixels never touch the CPU or system RAM.**
3. **Thread Context Management**: We will use a **Single Vulkan Device with Synchronized Queue Submissions**. Creating multiple Vulkan Devices can cause severe VRAM overhead and context-switching penalties. Instead, we will initialize one Vulkan Physical/Logical Device. Each parallel rendering thread will have its own Skia `DirectContext`, but they will share the Vulkan Device and synchronize their command buffer submissions to the GPU's hardware queue using a lightweight Mutex.

## 3. Proposed Changes (Assuming Option B via FFI)

### Phase 1: Dependency & FFmpeg Shared Build Setup

Before modifying Cargo dependencies, we must set up the FFmpeg 8.1 shared libraries so the Rust compiler can link them.

**1. File Extraction:** You will need to extract the following specific files from your shared FFmpeg 8.1 build into a dedicated folder (e.g., `src-tauri/ffmpeg-dev`):
- **C Headers (`.h` files):** The entire `include` folder containing `libavcodec/`, `libavformat/`, `libavutil/`, `libswscale/`, and `libhwcontext/` directories.
- **Link Libraries (`.lib` or `.dll.a` files):**
  - `avcodec.lib` (Video encoding logic)
  - `avformat.lib` (Multiplexing the `.mov` container)
  - `avutil.lib` (Hardware device contexts, `AVFrame` memory management, Vulkan context)
  - `swscale.lib` (Color space and pixel format utilities)
- **Runtime Libraries (`.dll` files):**
  - `avcodec-*.dll`, `avformat-*.dll`, `avutil-*.dll`, and `swscale-*.dll`. 
  - *Note: These DLLs must be placed in your `target/debug` and `target/release` folders (or alongside the final application `.exe`) so Windows can load them when the app runs.*

**2. Cargo Configuration:**
**`src-tauri/cyclemetry_core/Cargo.toml`**
- Add `ash` for Vulkan bindings.
- Add `ffmpeg-next` (configured via environment variables to point to the `ffmpeg-dev` folder created above).
- Enable `vulkan` features for `skia-safe`.

### Phase 2: Shared Vulkan Context Initialization
**`src-tauri/cyclemetry_core/src/render/vulkan_gpu.rs` (NEW)**
- Initialize the Vulkan `Instance`, `PhysicalDevice`, and `Device` via `ash`.
- Initialize an FFmpeg `AVHWDeviceContext` from the `ash` Vulkan handles.
- Provide a thread-safe wrapper so multiple Skia instances can access the device.

### Phase 3: Skia GPU Surface & Rendering
**`src-tauri/cyclemetry_core/src/render/surface_gpu.rs` (NEW)**
- Implement `create_gpu_surface` (mirroring `create_surface` from the CPU side).
- Allocate a Vulkan texture via FFmpeg's `av_hwframe_get_buffer`, import the texture into Skia via `skia_safe::gpu::BackendRenderTarget`, and wrap it in a Skia `Surface`.

### Phase 4: Zero-Copy Encoding Pipeline
**`src-tauri/cyclemetry_core/src/encode/video_gpu.rs` (NEW)**
- **Implement** a new direct `libavcodec` encoding loop for the GPU pathway (mirroring `video.rs` but without subprocesses):
  1. Skia renders to the `AVFrame` Vulkan texture.
  2. `avcodec_send_frame()` sends the Vulkan `AVFrame` to the `prores_ks_vulkan` encoder.
  3. `avcodec_receive_packet()` retrieves the encoded ProRes packets.
  4. Write packets directly to the output `.mov` file using `libavformat`.

## 4. Verification Plan

### Automated / Diagnostic Tests
- Monitor VRAM usage during `cargo run --bin parallel_render` to ensure no memory leaks occur with the shared Vulkan context.
- Verify `timing_summary.json` shows rendering times in the low milliseconds (under 5ms per frame) and zero CPU-to-GPU transfer times (`queue_wait` and `ffmpeg.write` will be replaced by API calls).
- Verify that standard CPU renders still complete successfully without triggering the Vulkan pipeline.

### Visual Verification
- Produce a sample test render to verify Skia's GPU text rendering and path rasterization look visually identical to the CPU implementation.
- Check alpha channel blending to ensure no unpremultiplied alpha regressions occurred.
