# Parallel Skia GPU (Vulkan) Renderer Implementation Plan

This document outlines the strategy for migrating to a highly optimized, Zero-Copy GPU-accelerated Vulkan renderer. It integrates with the Vulkan-accelerated ProRes encoding pipeline for maximum performance.

## 1. Goal Description

The objective is to eliminate the current bottleneck (CPU rendering and memory transfer) for hardware-accelerated encode jobs by moving the entire pipeline to the GPU.

By utilizing Skia's Vulkan backend and integrating FFmpeg at the memory level (Zero-Copy), the video frame data will never leave the GPU (VRAM) between the rendering step and the final encoding step.

> [!IMPORTANT]
> **Parallel Pathway & Fallback:** This GPU renderer is an optional pathway. We will build a **clean backend abstraction** to switch between CPU and GPU pipelines. The application must gracefully fall back to the existing CPU renderer if Vulkan init fails, encoder compatibility fails, memory allocation fails, TDR (device loss) occurs, or if a CPU encoder is explicitly chosen by the user.

## 2. Architecture Decisions & Constraints

Based on a rigorous technical assessment of Skia and FFmpeg interoperability, we have solidified the following technical constraints:

1. **Vulkan Device Ownership:** Rust/`ash` will own the Vulkan device creation. Both Skia (`GrDirectContext`) and FFmpeg (`AVHWDeviceContext`) will _borrow_ this device by importing the exact same handles.
2. **Image Ownership & `AVHWFramesContext` Configuration**: We must explicitly define who creates the `VkImage`.
   - **Hypothesis**: FFmpeg allocates the image via `av_hwframe_get_buffer`. We configure `AVHWFramesContext` with `format=AV_PIX_FMT_VULKAN`, `sw_format` (e.g., `AV_PIX_FMT_YUVA444P10LE` or `AV_PIX_FMT_RGBA`), optimal tiling, and necessary usage flags (`COLOR_ATTACHMENT`, `TRANSFER_SRC`).
   - **Constraint**: We must validate that FFmpeg-allocated images can actually be wrapped as Skia render targets. If Skia rejects the layout constraints or usage flags, we will pivot to **Skia-owned images** (Skia allocates `VkImage` with `BackendTexture` and we import it into FFmpeg via custom `AVVkFrame` wrapping). This decision will be definitively settled in Phase 1.
3. **Synchronization & Layout Transitions**: We must explicitly manage GPU serialization without stalling the CPU.
   - **Transitions**: Skia will likely leave the image in `COLOR_ATTACHMENT_OPTIMAL` or `GENERAL`. We must inject explicit Vulkan pipeline barriers to transition to the layout required by FFmpeg/`AVVkFrame`.
   - **Primitives**: We will extract Skia's internal flush semaphores (if exposed) or manually insert Vulkan binary/timeline semaphores and pass them into the `AVVkFrame` struct so FFmpeg's hardware context waits on the GPU, not the CPU. Queue family ownership transfers will be handled if Skia and FFmpeg end up on different hardware queues.
4. **Format Pipeline & Alpha Handling**: ProRes 4444 expects **straight alpha** (e.g., `yuva444p10le`), while Skia renders **premultiplied** RGBA8888 (or F16). We will not rely on implicit FFmpeg format conversions.
   - **Strategy**: Default to an **Explicit GPU shader pass** to convert Skia's premultiplied RGBA output into straight alpha (and potentially do the RGB -> YUV conversion if `prores_ks_vulkan` requires it) inside Vulkan before handing the frame to FFmpeg.
5. **Separation of Concerns (New Files Only)**: To ensure the existing CPU pipeline remains completely untouched and stable, all new code implemented for this plan must be placed in entirely new files with a `-gpu.rs` suffix (e.g., `video-gpu.rs`, `surface-gpu.rs`). The logic in these files should follow how the existing `video.rs` and `create_surface` work, providing an identical API surface but powered by the GPU. It can deviate if you can provide a clear reasoning why it is necessary.

## 3. Implementation Phases

### Phase 1: Minimal Proof-of-Concept (PoC) Technical Spike

Before modifying the main pipeline, we must prove the central claim ("Pixels never touch CPU/RAM") with a standalone feasibility spike.

1. Create a Vulkan device with `ash`.
2. Create an FFmpeg Vulkan HW device from the same `ash` handles.
3. Test **Image Ownership**: Try allocating an `AVVkFrame` via FFmpeg and wrapping it in Skia. If Skia rejects the usage flags/tiling, immediately pivot to Skia-allocation -> FFmpeg-wrapping.
4. Test **Synchronization**: Flush Skia, extract/create a Vulkan Semaphore, transition the `VkImage` layout to `GENERAL` or `SHADER_READ`, and pass the semaphore to FFmpeg.
5. Test **Format/Alpha**: Run an explicit Skia shader pass to convert premultiplied RGBA to straight alpha YUVA.
6. Encode that one frame with `prores_ks_vulkan`.
7. Verify alpha blending, colors, and confirm via profiling that zero hidden CPU copies occurred.

**Deliverables:**
- A standalone Rust binary (e.g., `poc_vulkan_interop.rs`) that allocates a frame, renders a test pattern to it, and encodes it into a `.mov`.

**Manual Testing Protocol:**
- Run the PoC binary directly from the terminal.
- Check the console output for any Vulkan validation layer errors or FFmpeg memory warnings.
- Play the output `.mov` file in a media player to visually confirm that the text/graphics rendered successfully without corruption or layout artifacts.

### Phase 2: Dependency & FFmpeg Shared Build Setup

**1. File Extraction:** Extract the specific files from your shared FFmpeg 8.1 build into `src-tauri/ffmpeg-dev`:

- **C Headers:** The entire `include` folder (`libavcodec/`, `libavformat/`, etc.).
- **Link Libraries:** `avcodec.lib`, `avformat.lib`, `avutil.lib`, `swscale.lib`.
- **Runtime Libraries:** The corresponding `.dll` files.

**2. Cargo Configuration (`src-tauri/cyclemetry_core/Cargo.toml`)**

- Add `ash` for Vulkan bindings.
- Add `ffmpeg-sys-next` (configured via env vars to point to `ffmpeg-dev`).
- Enable `vulkan` features for `skia-safe`.

**3. Windows Packaging**

- Update Tauri build scripts to ensure the FFmpeg `.dll`s are copied into the final build output directory, matching the MSVC ABI and DLL search paths.

**Deliverables:**
- Extracted FFmpeg developer files residing in `src-tauri/ffmpeg-dev`.
- Updated `Cargo.toml` with `ash` and `ffmpeg-sys-next` resolving correctly.
- Updated Tauri build configuration (`tauri.conf.json` or `build.rs`) to package the DLLs.

**Manual Testing Protocol:**
- Run `cargo check` to ensure the new dependencies resolve and link correctly against the C headers in `ffmpeg-dev`.
- Run a standard `cargo build` and manually inspect the `target/debug` folder to verify that `avcodec-*.dll`, `avutil-*.dll`, etc., were successfully copied alongside the `.exe`.

### Phase 3: Shared Vulkan Context Initialization

**`src-tauri/cyclemetry_core/src/render/vulkan-gpu.rs` (NEW)**

- Initialize Vulkan `Instance`, `PhysicalDevice`, and `Device` via `ash`.
- Initialize `AVHWDeviceContext` from the `ash` handles.
- Implement the strict fallback triggers defined in Section 1.

**Deliverables:**
- A new `vulkan-gpu.rs` file exporting the shared Vulkan device and FFmpeg hardware context.
- Fallback logic integrated into the rendering pipeline initialization.

**Manual Testing Protocol:**
- Write a temporary test in `parallel_render.rs` to instantiate the Vulkan context.
- Force a Vulkan initialization failure (e.g., by deliberately requesting an unsupported Vulkan extension) and verify that the application correctly logs the error and gracefully switches back to the CPU rendering pathway without crashing.

### Phase 4: Skia GPU Surface & Rendering

**`src-tauri/cyclemetry_core/src/render/surface-gpu.rs` (NEW)**

- Implement `create_gpu_surface`, following the existing API and behavior of `create_surface` from `surface.rs`. It can deviate if you can provide a clear and unequivocal reasoning why it is necessary.
- Implement the allocation strategy determined in Phase 1 (FFmpeg-owned or Skia-owned).
- Execute the explicit GPU shader pass for premul -> straight alpha conversion.
- Inject Vulkan layout transition barriers and extract semaphores during `DirectContext::flush`.

**Deliverables:**
- A new `surface-gpu.rs` file containing `create_gpu_surface` and the explicit alpha conversion shader pass.

**Manual Testing Protocol:**
- Call `create_gpu_surface` in a test harness, render a frame containing semi-transparent text/widgets, and use `surface.read_pixels` to temporarily save the GPU frame as a PNG.
- Visually inspect the PNG side-by-side with a CPU-rendered equivalent to ensure the alpha blending matches perfectly, proving the explicit shader pass worked.

### Phase 5: Zero-Copy Encoding Pipeline

**`src-tauri/cyclemetry_core/src/encode/video-gpu.rs` (NEW)**

- **Implement** a new encoding loop that follows the existing orchestration, parallel thread spawning, and `RenderController` progress tracking of `video.rs`, but replaces the subprocess CLI logic with direct `libavcodec` encoding via `ffmpeg-sys-next`. It can deviate if you can provide a clear and unequivocal reasoning why it is necessary.
- Push the Vulkan semaphores and hardware frames into `avcodec_send_frame()`.
- Manage the hardware frame pool lifecycle based on the ownership strategy from Phase 1.

**Deliverables:**
- A new `video-gpu.rs` file containing the full FFI encoding loop.
- Full integration with the Cyclemetry UI so the user can select "ProRes Vulkan (GPU)".

**Manual Testing Protocol:**
- Launch the full application GUI.
- Select a complex activity template and choose the "ProRes Vulkan (GPU)" codec from the sidebar/modal.
- Trigger a render. Verify that the progress bar updates smoothly, matching the CPU pipeline's behavior.
- Play the final output video and verify that the visual quality, alpha channel, and framerate are flawless.
- Inspect the generated `timing_summary.json` to verify that `queue_wait` and `ffmpeg.write` times are eliminated, and that actual GPU submission times are recorded.

## 4. Verification & Profiling Plan

We cannot assume an "under 5ms" render target without proving it. The `timing_summary.json` must be heavily instrumented to measure:

- CPU Scene preparation time
- Skia recording time (CPU)
- GPU render submission time
- GPU completion/wait time (fences)
- Encoder queue wait
- Encode time (GPU)
- Mux write time (Disk IO)

**Visual Verification:**

- Cross-reference alpha channel edges (transparent boundaries) against CPU renders to guarantee no premultiplied/straight alpha regressions occurred during the explicit GPU shader pass.
