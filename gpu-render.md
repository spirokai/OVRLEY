# Parallel Skia GPU (Vulkan) Renderer Implementation Plan v2

This document defines a safer implementation path for a GPU-accelerated Skia renderer with a true zero-copy Vulkan encode path where feasible. The goal remains the same as v1, but the plan is now organized to reduce integration risk, make failures diagnosable, and fit the current OVRLEY codebase more cleanly.

## 1. Goal

The objective is to eliminate the current CPU render plus CPU memory transfer bottleneck for hardware-accelerated encode jobs by moving rendering and hardware frame handoff onto the GPU.

The desired end state is:

- Skia renders frames through its Vulkan backend.
- FFmpeg encodes from Vulkan hardware frames without an intermediate CPU readback.
- The application can select between CPU and GPU render/encode backends at runtime.
- The application falls back cleanly to the current CPU path whenever GPU capability, initialization, or runtime stability is insufficient.

> [!IMPORTANT]
> **Fallback and safety are first-class requirements.** The GPU pipeline is optional. The CPU renderer remains the default safe path until Vulkan capability probing, renderer initialization, and encoder compatibility all pass.

## 2. Current-State Reality

The current pipeline is built around CPU memory:

- Rendering targets are `&mut [u8]` RGBA buffers.
- Static label/base layers are cached as CPU pixel buffers.
- Video encoding streams raw RGBA frames to an FFmpeg subprocess over stdin.
- Encode progress is inferred from FFmpeg stderr output.

This means a true zero-copy GPU path cannot be achieved by only adding new GPU files that mimic the current API exactly. We must allow limited, deliberate changes to shared abstractions.

## 3. Design Principles

1. **Shared abstractions first**
   - Introduce backend-neutral render and encode abstractions before implementing the GPU path.
   - GPU-specific code lives in new files, but small changes to shared files are explicitly allowed when needed to support backend selection.

2. **Rust module naming**
   - Use Rust-friendly filenames such as `video_gpu.rs`, `surface_gpu.rs`, and `vulkan_gpu.rs`.
   - Avoid hyphenated filenames like `video-gpu.rs`, which do not fit standard Rust module conventions.

3. **Do not assume image ownership up front**
   - Whether FFmpeg owns the `VkImage` or Skia owns the `VkImage` remains an implementation decision to be settled by spike work.
   - The plan must succeed with either ownership model.

4. **Capability-gated rollout**
   - The UI must not expose "ProRes Vulkan (GPU)" unless startup probing confirms that the Vulkan renderer and the Vulkan encoder path are usable on the current machine.

5. **Incremental proof, not one giant spike**
   - Build validation, Skia Vulkan rendering, FFmpeg Vulkan encoding, and shared-image interop are separate checkpoints.
   - Each checkpoint must have a pass/fail gate before moving on.

6. **Parity before performance claims**
   - Color, alpha, and visual parity with the CPU renderer must be proven before optimizing for throughput.

## 4. Architecture Decisions and Constraints

1. **Vulkan device ownership**
   - Rust via `ash` owns Vulkan instance/device/queue creation.
   - Skia and FFmpeg borrow the same underlying Vulkan handles.

2. **Backend abstraction**
   - Introduce backend-neutral interfaces for:
     - render target acquisition
     - per-frame submission
     - encoder frame submission
     - progress reporting
     - cancellation and drain
   - The CPU path should be adapted to these abstractions first so the GPU path plugs into a proven structure.

3. **Image ownership**
   - Two valid strategies are supported:
     - FFmpeg-owned hardware images wrapped for Skia rendering
     - Skia-owned Vulkan images wrapped/imported for FFmpeg encoding
   - Phase 2 spikes will choose the better path based on actual interoperability.

4. **Synchronization**
   - GPU/encoder synchronization must use Vulkan synchronization primitives, not CPU blocking waits, in the steady state.
   - Layout transitions and queue-family ownership transfers must be explicit and measured.

5. **Format and alpha handling**
   - Skia renders premultiplied RGBA.
   - ProRes 4444 expects straight alpha semantics.
   - Do not rely on implicit format conversion for correctness.
   - Plan for an explicit GPU conversion pass for premul-to-straight alpha, and for RGB-to-YUV conversion if required by the selected FFmpeg Vulkan path.

6. **Debuggability**
   - Every GPU phase must preserve an optional debug path that can read back a frame for PNG comparison, even though the production path is zero-copy.

## 5. Non-Goals for Initial Delivery

- Cross-platform GPU parity on day one.
- Replacing the CPU preview pipeline immediately.
- Supporting every FFmpeg hardware codec at launch.
- Eliminating all disk I/O time from total export time.

Initial delivery should focus on Windows Vulkan + ProRes GPU export with clean fallback.

## 6. Implementation Phases

### Phase 0: Baseline and Shared Abstraction Prep

Before any Vulkan work, reshape the pipeline so CPU and GPU backends can coexist cleanly.

**Work items**

1. Add backend-neutral abstractions for render and encode.
2. Refactor the current CPU path to implement those abstractions without changing behavior.
3. Separate:
   - render progress
   - encode progress
   - frame ownership/lifecycle
   - cancellation/drain behavior
4. Make room for non-CPU frame handles so the render path is no longer hard-coded to `&mut [u8]`.

**Likely code areas**

- Shared updates in `src-tauri/ovrley_core/src/render/mod.rs`
- Shared updates in `src-tauri/ovrley_core/src/encode/video.rs`
- New backend-specific files added later as `*_gpu.rs`

**Deliverables**

- CPU pipeline still working through the new abstraction layer
- No functional regression in preview or export
- A documented interface for GPU frame submission

**Acceptance gate**

- CPU exports produce identical output to the current baseline
- Existing progress UI and cancellation behavior still work

### Phase 1: Build and Packaging Feasibility Spike

Prove that the project can build and package the required dependencies before writing renderer logic.

**Work items**

1. Add `ash`.
2. Add Vulkan-enabled `skia-safe` configuration.
3. Add `ffmpeg-sys-next` wired to a local FFmpeg developer bundle.
4. Stage FFmpeg headers/libs in a repo-local location such as `src-tauri/ffmpeg-dev`.
5. Update build logic to copy required FFmpeg runtime DLLs into the app output.
6. Validate that Tauri packaging still works.

**Deliverables**

- `cargo check` succeeds with the new dependency set
- `cargo build` succeeds on the target Windows environment
- Packaged output contains all required FFmpeg DLLs

**Acceptance gate**

- Build reproducibility is documented
- No unresolved linker/runtime DLL issues remain

### Phase 2: Narrow Technical Spikes

Do not combine all interop risks into one proof-of-concept. Complete these spikes independently.

#### Phase 2A: Skia Vulkan Rendering Spike

**Goal**

Prove that Skia can render the required frame content through Vulkan in this project.

**Work items**

1. Create a Vulkan device with `ash`.
2. Create a Skia `GrDirectContext` using Vulkan.
3. Render a representative test frame with:
   - text
   - semi-transparent overlays
   - route/elevation content if practical
4. Read back one debug frame to PNG for inspection.

**Deliverables**

- Standalone Rust test binary such as `poc_skia_vulkan.rs`

**Acceptance gate**

- Output is visually correct
- Vulkan validation errors are understood or eliminated

#### Phase 2B: FFmpeg Vulkan Encode Spike

**Goal**

Prove that FFmpeg Vulkan hardware encoding works in-process from Rust on the target environment.

**Work items**

1. Create a Vulkan hardware device context in FFmpeg.
2. Allocate or import a Vulkan hardware frame.
3. Encode a single synthetic frame to `.mov` using direct `libavcodec` calls.
4. Confirm the exact requirements for hardware frame format, alpha handling, and encoder options.

**Deliverables**

- Standalone Rust test binary such as `poc_ffmpeg_vulkan_encode.rs`

**Acceptance gate**

- A valid output file is produced
- Required FFmpeg frame setup is documented precisely

#### Phase 2C: Shared Image Interop Spike

**Goal**

Prove zero-copy interoperability between Skia-rendered Vulkan images and FFmpeg hardware encode.

**Work items**

1. Test FFmpeg-owned image -> Skia wrapping.
2. Test Skia-owned image -> FFmpeg wrapping/import.
3. Compare complexity, correctness, and synchronization behavior.
4. Prototype required layout transitions and semaphore handoff.
5. Choose one ownership model based on evidence, not preference.

**Deliverables**

- Standalone Rust binary such as `poc_vulkan_interop.rs`
- A short written decision note naming the chosen ownership strategy

**Acceptance gate**

- One frame reaches encode without CPU pixel readback in the production path
- Hidden CPU copies are ruled out as far as available tooling allows

### Phase 3: Shared Vulkan Runtime

After the spikes succeed, implement reusable runtime support in the app codebase.

**New file**

- `src-tauri/ovrley_core/src/render/vulkan_gpu.rs`

**Work items**

1. Initialize Vulkan instance, physical device, logical device, and queues.
2. Initialize the reusable Skia Vulkan context.
3. Initialize the reusable FFmpeg hardware device context.
4. Expose capability probing results:
   - Vulkan available
   - required extensions/features available
   - FFmpeg Vulkan encoder available
   - chosen ownership strategy supported
5. Define fallback reasons and log them clearly.

**Deliverables**

- Shared Vulkan runtime module
- Structured capability report

**Acceptance gate**

- Forced init failure falls back to CPU cleanly
- No crash on unsupported systems

### Phase 4: GPU Render Backend

Implement the render side against the new backend abstractions.

**New file**

- `src-tauri/ovrley_core/src/render/surface_gpu.rs`

**Work items**

1. Implement GPU frame/surface acquisition.
2. Render the same scene content as the CPU path.
3. Revisit static-layer caching:
   - GPU image cache for labels/base layers where useful
   - avoid mandatory CPU `base_rgba` copies in the GPU path
4. Add the explicit GPU conversion pass for:
   - premul RGBA -> straight alpha
   - RGB -> YUV if required
5. Emit synchronization artifacts needed by the encoder path.
6. Preserve an optional debug readback path for PNG comparisons.

**Deliverables**

- GPU render backend producing correct frames
- Debug frame extraction for side-by-side comparison

**Acceptance gate**

- Semi-transparent text and overlays match CPU output closely
- Frame render path works without CPU pixel staging in production mode

### Phase 5: GPU Encode Backend

Implement the encode side with direct FFmpeg FFI, not a subprocess.

**New file**

- `src-tauri/ovrley_core/src/encode/video_gpu.rs`

**Work items**

1. Implement direct `libavcodec` encode orchestration.
2. Replace subprocess-specific assumptions:
   - no stdin writer thread
   - no stderr frame parsing
3. Define explicit progress semantics for:
   - rendered frames
   - submitted frames
   - encoded/drained frames
4. Handle cancellation and end-of-stream drain correctly.
5. Manage hardware frame pools and synchronization lifetimes.
6. Record GPU-aware timing buckets.

**Deliverables**

- Full in-process GPU encode loop
- Progress reporting compatible with the existing UI

**Acceptance gate**

- Export completes successfully through the FFI path
- Cancellation works without deadlocks or leaked resources

### Phase 6: App Integration and Capability-Gated UX

Only expose the GPU path when it is actually available.

**Work items**

1. Add backend selection logic in the render entrypoint.
2. Extend health/probe reporting to include GPU capability.
3. Gate the UI codec option based on backend capability.
4. Show clear fallback/error messages when GPU export is unavailable.
5. Keep the CPU backend as the safe fallback path.

**Deliverables**

- Runtime backend selection
- UI gating for "ProRes Vulkan (GPU)"

**Acceptance gate**

- Unsupported machines do not offer the GPU option
- Supported machines can select it and export successfully

### Phase 7: Verification, Regression, and Profiling

Performance work only matters after correctness is proven.

**Required measurements**

- CPU scene preparation time
- Skia command recording time
- GPU render submission time
- GPU completion/wait time
- Encoder submission wait
- Hardware encode time
- Drain time
- Mux/write time

**Required verification**

1. Visual parity checks against CPU output
   - especially transparent edges and overlay compositing
2. Automated image-diff regression tests for sampled frames
3. Stress tests:
   - repeated exports
   - high resolution
   - cancellation mid-render
   - forced fallback paths
4. Device-loss/TDR behavior validation

**Success criteria**

- No correctness regressions relative to CPU baseline
- Zero-copy confirmed for the production GPU path
- Timing output clearly shows where time is spent

## 7. Testing Strategy

### Manual testing

- Run each spike binary directly from the terminal.
- Check Vulkan validation output.
- Inspect encoded `.mov` outputs visually.
- Compare sampled PNGs from CPU and GPU paths side by side.
- Test both supported and unsupported machines.

### Automated testing

- Keep CPU export regression tests running throughout the refactor.
- Add sampled image-diff tests for the GPU path once stable.
- Add startup capability probe tests where practical.

## 8. Rollout Strategy

1. Land shared abstractions with CPU behavior preserved.
2. Land build/dependency support.
3. Land spikes and choose the ownership strategy.
4. Land hidden GPU backend behind an internal flag.
5. Enable UI exposure only after capability probing is implemented.
6. Keep CPU fallback as the default escape hatch until the GPU path proves stable.

## 9. Final Notes

This v2 plan is intentionally more conservative than v1. The main change is not the technical goal, but the order of operations:

- prove buildability before deep implementation
- prove rendering and encoding separately before interop
- reshape the shared abstractions before chasing zero-copy
- gate the feature by actual runtime capability instead of optimistic UI exposure

That sequence should materially increase the chance of success while keeping the current CPU renderer stable.
