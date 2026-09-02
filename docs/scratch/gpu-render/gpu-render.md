# Zero-Copy GPU Render and Encode Research Plan

This document audits the current GPU-rendering proposal and replaces it with a research-first plan for a **zero CPU-to-GPU copy** parallel rendering and encoding pipeline, starting with Vulkan on Windows/Linux.

The core requirement is strict:

> The production GPU path must not render a frame into CPU pixels and then upload that frame to the GPU. Rendering must happen directly into GPU-owned or GPU-importable images, and encoding must consume those images without a CPU staging step.

Debug readback is allowed only behind explicit diagnostics flags for image comparison and failure analysis.

## 1. Audit of the Existing Plan

The previous plan had the right instincts:

- It recognized that the current `&mut [u8]` render target cannot support true zero-copy GPU handoff.
- It separated Skia Vulkan rendering, FFmpeg Vulkan encoding, and shared-image interop into different proof points.
- It treated CPU fallback and capability gating as first-class requirements.
- It called out premultiplied alpha, straight alpha, RGB/YUV conversion, synchronization, and layout transitions.
- It avoided exposing the GPU codec option optimistically before capability probing succeeds.

However, it was still too implementation-shaped too early.

### 1.1 The Central Risk Is Interop, Not Refactoring

The plan started with shared abstraction work before proving that Skia and FFmpeg can actually share Vulkan images in this repo.

That order is risky. A clean abstraction is useful only after the image ownership model, synchronization model, and encoder frame format are known. Otherwise the abstraction will encode guesses.

The revised plan puts hard research gates first:

1. Can this repo build Skia with Vulkan enabled?
2. Can Skia render representative overlay content into a Vulkan image?
3. Can a selected hardware encoder accept a GPU-resident frame without CPU staging?
4. Can the same Vulkan image or imported image cross from Skia to FFmpeg without CPU staging?
5. Can synchronization, ownership, and cleanup be represented correctly without CPU pixel staging?

Only after those pass should production abstractions be introduced.

### 1.2 The Current Render Path Is CPU-Memory Centered

Relevant current files:

- `src-tauri/ovrley_core/src/render/surface.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/static_layer.rs`

Current behavior:

- `RenderTarget<'a>` is a mutable CPU byte slice.
- `render_frame_rgba` renders into caller-owned CPU memory.
- `wrap_native_surface` wraps CPU pixels with `skia_safe::surfaces::wrap_pixels`.
- `prepared_assets.base_rgba` restores a static CPU base layer via `copy_from_slice`.
- Preview rendering uses raster Skia surfaces and PNG output.

This means a zero-copy GPU path cannot reuse `render_frame_rgba` as-is. It needs a GPU surface path where static layers, labels, route geometry, elevation widgets, and metric text draw into a GPU-backed Skia surface.

The good news: most widget drawing code is already expressed in terms of Skia `Canvas`, `Paint`, `Path`, `Image`, and text helpers. If Skia Vulkan works, the widget drawing layer should not need a wholesale rewrite. The primary render rewrite is target/surface/cache ownership.

### 1.3 The Current Encode Path Is Subprocess and CPU-Buffer Centered

Relevant current files:

- `src-tauri/ovrley_core/src/encode/video_pipeline.rs`
- `src-tauri/ovrley_core/src/encode/pipeline_shared.rs`
- `src-tauri/ovrley_core/src/encode/ffmpeg_transparent_profiles.rs`
- `src-tauri/ovrley_core/src/encode/codec_detect.rs`
- `src-tauri/ovrley_core/src/encode/progress.rs`

Current behavior:

- The transparent video pipeline allocates a pool of `Vec<u8>` RGBA buffers.
- Render fills those buffers on CPU.
- A writer thread sends each buffer to FFmpeg stdin.
- A monitor thread parses FFmpeg stderr for `frame=N`.
- `prores_ks_vulkan` currently still receives raw CPU RGBA over stdin, then runs `format=yuva444p10le,hwupload` inside FFmpeg.

That existing Vulkan profile is hardware encode, but it is not zero CPU-to-GPU copy. It still crosses the CPU/GPU boundary after Rust renders the frame.

For true zero-copy, the encode side needs the larger rewrite:

- in-process FFmpeg/libavcodec, or another API that can accept imported GPU frames;
- explicit hardware frame context management;
- explicit frame lifetime and synchronization;
- progress based on submitted/received packets rather than stderr text;
- cancellation and drain semantics without stdin EOF as the control mechanism.

### 1.4 Build and Packaging Are Not Administrative Details

The current dependency is:

```toml
skia-safe = { version = "0.75", features = ["binary-cache"] }
```

`skia-safe 0.75.0` does expose Vulkan-related APIs locally, including `gpu::vk` and Vulkan direct-context helpers, but enabling and packaging a Vulkan-capable Skia build still has to be proven in this repository and CI environment.

Likewise, the current FFmpeg installer stages runtime binaries from packaged builds. In-process FFmpeg integration requires compatible headers, import libraries, and runtime DLLs/shared libraries. That is a separate supply-chain and packaging problem from running the `ffmpeg` executable.

### 1.5 macOS Compatibility Requires Backend Neutrality

macOS does not support native Vulkan. MoltenVK exists, but it is not the right foundation for a native zero-copy encode path because Apple media APIs are built around Metal, CoreVideo, IOSurface, and VideoToolbox.

The first production GPU backend can be Vulkan, but shared interfaces must not expose Vulkan types directly. The backend model should allow:

- CPU Skia fallback on every platform;
- Vulkan GPU path on Windows/Linux;
- future Metal GPU path on macOS;
- platform-specific encode backends where required.

## 2. Research Goal

Build enough proof to decide whether OVRLEY can support a true zero-copy GPU render and encode pipeline.

The desired first end state is:

```text
Skia Vulkan surface
  -> GPU-only conversion pass if needed
  -> hardware encoder frame accepted by the selected backend
  -> encoded packet/mux
```

The initial renderer target is Vulkan. The initial encoder target is intentionally undecided. Phase 0 should compare feasible hardware encode paths and pick the shortest architecture proof for this project, not assume FFmpeg Vulkan or ProRes is superior.

Candidate backend families include:

- FFmpeg Vulkan, including `prores_ks_vulkan`.
- QSV, usually through D3D11VA on Windows or VAAPI on Linux.
- CUDA/NVENC, if Vulkan external memory/semaphore import into CUDA is viable.
- AMF, usually through D3D11/DX interop on Windows.
- D3D11-oriented pipelines on Windows if they are a better bridge to QSV/AMF/NVENC.
- VAAPI-oriented pipelines on Linux if they are a better bridge to iGPU encode.
- Future Metal/VideoToolbox on macOS.

These are not interchangeable. The proof must state which project use case it supports:

- transparent overlay export;
- composited final-video export;
- both.

The proof should not privilege ProRes 4444, FFmpeg Vulkan, or any other codec/backend unless the project chooses that target explicitly.

The forbidden production path is:

```text
Skia/raster CPU pixels
  -> CPU buffer
  -> stdin or staging upload
  -> FFmpeg hwupload
```

Debug-only readback remains allowed:

```text
GPU frame
  -> explicit diagnostic readback
  -> PNG/image diff
```

## 3. Success Criteria

Research succeeds only if all of these are true for the Vulkan path:

- A representative overlay frame is rendered by Skia into a Vulkan-backed surface.
- A frame is encoded by a selected hardware backend from GPU-resident frames.
- The render output reaches the encoder without CPU pixel staging or stdin rawvideo.
- The proof clearly identifies whether it supports transparent overlay export, composited final-video export, or both.
- Hidden CPU copies are ruled out as far as practical with available tooling, API inspection, memory-flow tracing, and explicit debug instrumentation.
- Alpha semantics are correct: transparent edges, shadows, overlays, and text match the CPU baseline closely.
- Cancellation and drain can be implemented without deadlocks or leaked GPU/FFmpeg resources.
- Unsupported systems can reliably fall back to the current CPU path.

The research fails, or must change direction, if:

- Skia Vulkan cannot be built and packaged reproducibly.
- no candidate hardware encode path can accept externally rendered GPU images without CPU staging.
- Image sharing requires format/layout/synchronization behavior that FFmpeg cannot express safely.
- The only feasible path is `hwupload` from CPU memory.

## 4. Non-Goals

- Replacing the CPU renderer during research.
- Replacing the CPU preview path during the first Vulkan milestone.
- Solving macOS GPU acceleration in the first Vulkan implementation.
- Supporting every hardware codec.
- Optimizing performance before proving correctness and zero-copy semantics.
- Building a new renderer in Vello/wgpu unless Skia Vulkan interop fails and a separate decision is made.

## 5. Architecture Principles

### 5.1 Prove Before Abstracting

Do not refactor the production render and encode paths until the Vulkan research gates pass.

The first code should live in isolated proof binaries or examples, not in the hot production pipeline. This protects the current CPU renderer and prevents speculative abstractions from leaking into the app.

Suggested location:

```text
src-tauri/ovrley_core/examples/gpu_research/
```

or:

```text
src-tauri/ovrley_core/src/bin/gpu_research_*.rs
```

### 5.2 One GPU Owner, Borrowed Handles

The preferred Vulkan model is:

- Rust owns Vulkan instance/device/queues through `ash`.
- Skia borrows Vulkan handles to create its `DirectContext`.
- FFmpeg borrows or imports compatible Vulkan handles/images through its hardware device/frame APIs.

This avoids accidentally creating separate Vulkan devices that cannot share images.

If FFmpeg requires owning the hardware device context internally, the research must determine whether Skia can safely render into FFmpeg-owned images, or whether FFmpeg can import images created from the Rust-owned device.

### 5.3 Backend-Neutral Production Interfaces

After research succeeds, production interfaces should describe capabilities and lifetimes, not concrete Vulkan types.

At the shared layer, prefer concepts like:

```text
RenderBackend
RenderFrame
GpuFrameHandle
EncodeBackend
SubmittedFrame
BackendCapabilityReport
```

Vulkan-specific objects should stay inside Vulkan modules:

```text
render/vulkan_runtime.rs
render/surface_vulkan.rs
encode/video_vulkan.rs
```

Future macOS work can then add:

```text
render/surface_metal.rs
encode/video_videotoolbox.rs
```

### 5.4 No CPU Upload in Production

The production Vulkan path must not contain:

- `Vec<u8>` frame buffers for rendered frames;
- `wrap_pixels` for video frames;
- FFmpeg rawvideo stdin;
- FFmpeg `hwupload` fed from CPU-rendered RGBA;
- per-frame CPU base-layer restore with `copy_from_slice`;
- readback used as an encode input.

It may contain:

- CPU-side scene preparation;
- CPU-side path/text/layout preparation;
- GPU texture uploads for static assets during initialization;
- explicit debug readback for sampled frames only.

### 5.5 Feasibility Before Performance

Research Phases 0-6 are architecture feasibility phases, not performance phases.

During those phases, do not reject or accept an approach based on throughput, FPS, wall-clock export time, queue depth, or whether render and encode overlap efficiently. The only performance-adjacent measurements allowed are diagnostic checks that help prove whether an unwanted CPU copy, readback, upload, or blocking lifecycle dependency exists.

The architecture must be capable of becoming a parallel pipeline later:

```text
Frame N+2: CPU scene/frame preparation
Frame N+1: Skia records/submits GPU render commands
Frame N:   GPU conversion/synchronization
Frame N-1: FFmpeg encodes/drains packets
```

But Phases 0-6 only need to prove that the frame ownership, synchronization, encode submission, drain, and cleanup model can exist. Performance proof starts after the app-specific baseline and production integration plan.

## 6. Research Phase 0: Disconnected Primitive Feasibility Spike

Purpose: prove the core GPU-render-to-GPU-encode idea in the smallest possible throwaway program before spending time on OVRLEY pipeline baselines, abstractions, or production refactors.

This phase should be completely disconnected from the production render and encode folders except for living in the repository and using the same toolchain. It should not call `render_frame_rgba`, should not use `video_pipeline.rs`, should not parse OVRLEY scene config, and should not try to preserve UI progress/cancellation behavior.

The only question is:

> Can this repo produce one required output file from a GPU-rendered image without a CPU pixel upload/readback in the encode path?

Phase 0 does **not** require a dedicated GPU. It should run against any adapter that exposes the required Vulkan render features and a compatible zero-copy encode path, including an Intel iGPU if those capabilities are present. The research binary should print the selected adapter name, vendor/device IDs, Vulkan driver/API versions, and the hardware device/encoder path it actually used.

Intel QSV is a separate hardware encode API, not FFmpeg Vulkan. QSV may count as a successful Phase 0 backend if the exact GPU-memory interop path is documented and still avoids CPU pixel staging. It should be evaluated on its own merits, not treated as secondary to FFmpeg Vulkan.

### Work Items

#### 0A. Create a Throwaway Research Binary

Create an isolated binary/example such as:

```text
src-tauri/ovrley_core/src/bin/gpu_poc_zero_copy_vulkan.rs
```

or:

```text
src-tauri/ovrley_core/examples/gpu_poc_zero_copy_vulkan.rs
```

It may use ugly, duplicated, unsafe, research-only code. Clean production architecture is explicitly not a goal here.

#### 0B. Minimum Dependency Proof

Prove the minimum dependency set needed for the primitive binary:

- `ash` can create a Vulkan instance/device/queue.
- the selected physical device can be either integrated or dedicated, but it must expose the image usage, memory, synchronization, and interop features required by the chosen proof path.
- `skia-safe` can be built with Vulkan support, or a lower-level Skia binding path is identified.
- FFmpeg development headers/import libraries can be linked by Rust.
- The FFmpeg runtime used by the binary matches the linked development artifacts.

This phase can use local paths, environment variables, and rough scripts. Reproducible packaging comes later.

#### 0C. Primitive GPU Render

Render something visibly identifiable into a Vulkan image:

- clear transparent background;
- draw a few colored translucent rectangles/lines;
- draw at least one anti-aliased vector shape;
- if Skia Vulkan is available quickly, draw via Skia;
- if Skia Vulkan setup blocks the first proof, temporarily render via raw Vulkan commands or a compute/fragment shader, then return to Skia in Phase 2.

The point is to prove a GPU-produced image can become encoder input. It does not need OVRLEY widgets yet.

#### 0D. Primitive Hardware Encode

Encode the GPU-produced image with a hardware path that matches the selected target:

- use direct FFmpeg/libavcodec APIs, not the `ffmpeg` subprocess;
- create or import the required hardware device/frame context;
- document whether the backend is Vulkan, QSV, CUDA/NVENC, AMF, D3D11VA, VAAPI, or another hardware path;
- document which output use case the backend proves: transparent overlay export, composited final-video export, or both;
- write/mux a one-frame or few-frame output file;
- avoid rawvideo stdin;
- avoid CPU pixel upload as the frame source;
- allow debug readback only as a side artifact.

#### 0E. Primitive Interop Decision

Try the simplest viable interop model first:

```text
GPU render target
  -> optional GPU-only layout/format conversion
  -> selected hardware encoder frame
  -> encoded packet
```

Document which of these worked:

- FFmpeg-owned image rendered by Skia/Vulkan;
- Rust/Skia-owned image imported by FFmpeg;
- separate render image plus GPU-only copy/conversion into FFmpeg-owned frame.
- Vulkan external memory imported into another hardware API such as CUDA, D3D11/QSV, or AMF.

Do not try to make all three elegant. One ugly working route is enough to justify deeper research.

### Deliverables

- `gpu_poc_zero_copy_vulkan` binary/example.
- One encoded output produced from a GPU-rendered frame.
- The output use case clearly labeled: transparent overlay export, composited final-video export, or both.
- Optional debug PNG readback of the same frame.
- `docs/gpu-research/primitive-zero-copy-feasibility.md`.

### Gate

Proceed only if the primitive binary proves a GPU-rendered image can reach a selected hardware encoder without CPU pixel staging, and the result clearly states which project use case it enables.

If this fails, stop the zero-copy production plan. Do not baseline or abstract the OVRLEY pipeline for a backend that cannot exist.

## 7. Research Phase 1: Build and Dependency Reproducibility

Purpose: turn the primitive spike's dependency discoveries into a repeatable local build path before adding representative Skia rendering or app-shaped code.

### Work Items

1. Create a branch-only or feature-gated dependency experiment.
2. Test `skia-safe` with Vulkan enabled:
   - remove reliance on CPU-only binary-cache assumptions;
   - confirm whether binary cache works for the desired Vulkan feature set;
   - if source build is required, document GN/Ninja/depot_tools requirements and build duration.
3. Keep `ash` behind a research feature until production work starts.
4. Stabilize FFmpeg development artifact discovery:
   - headers;
   - import libraries on Windows;
   - shared libraries/runtime DLLs;
   - version compatibility with bundled FFmpeg;
   - whether `ffmpeg-sys-next` is sufficient or raw bindgen/build scripting is needed.
5. Verify packaging implications only enough to avoid obvious dead ends:
   - Tauri dev build;
   - Tauri production bundle;
   - portable package scripts;
   - runtime DLL/shared-library discovery.
6. Document CI/cache implications for Skia and FFmpeg developer artifacts.

### Deliverables

- `docs/gpu-research/build-feasibility.md`
- A feature-gated `cargo check` path for Vulkan/FFmpeg research.
- A list of required local tools and environment variables.

### Gate

Proceed only if a clean checkout can reproduce the research build steps on the target Windows development machine. If this cannot be made reproducible, stop before touching production code.

## 8. Research Phase 2: Skia Vulkan Rendering Proof

Purpose: prove Skia can render OVRLEY-like content into a Vulkan image.

### Work Items

1. Create Vulkan instance/device/queue through `ash`.
2. Create a Skia Vulkan `DirectContext` from the Rust-owned Vulkan handles.
3. Render a representative frame using GPU-backed Skia surfaces:
   - transparent background;
   - metric text;
   - font lookup;
   - shadows;
   - route polyline;
   - elevation widget;
   - semi-transparent overlays;
   - parsed SVG icon/path content.
4. Read back one debug frame to PNG for visual inspection.
5. Compare that PNG to the CPU Skia baseline.
6. Identify which existing render functions can be reused unchanged because they draw against `Canvas`.
7. Identify CPU-only pieces that must be replaced:
   - `wrap_native_surface`;
   - `base_rgba`;
   - CPU label image cache;
   - CPU sample frame writing.

### Deliverables

- `gpu_research_skia_vulkan` binary/example.
- `docs/gpu-research/skia-vulkan.md`
- One debug PNG output.
- Notes on required render-layer changes.

### Gate

Proceed only if Skia Vulkan can render representative content with acceptable visual parity and no unexplained Vulkan validation errors.

## 9. Research Phase 3: Hardware Encode Backend Proof

Purpose: prove at least one relevant hardware encoder can consume GPU-resident frames in-process.

The backend target should be selected from the project priorities and the shortest credible interop path. FFmpeg Vulkan, QSV, CUDA/NVENC, AMF, D3D11VA, and VAAPI are all candidates. This phase should not assume one is superior before the proof.

### Work Items

1. Use direct FFmpeg APIs, not the `ffmpeg` subprocess.
2. Create or import the selected hardware device context.
3. Allocate or import a hardware frame acceptable to the selected encoder.
4. Fill a synthetic frame without CPU rawvideo stdin:
   - first with a GPU clear or compute pass if practical;
   - debug CPU initialization is allowed only for this isolated proof, not as the target design.
5. Encode a short `.mov` file.
6. Document exact requirements:
   - `AVPixelFormat`;
   - hardware frame format;
   - software format;
   - codec/profile;
   - alpha bits, if the target is transparent export;
   - pixel format expectations;
   - frame pool requirements;
   - packet drain sequence.
7. Verify the encoded output satisfies the selected use case.
8. Label the proof clearly as transparent-overlay, composited-final-video, or both.

### Deliverables

- `gpu_research_hardware_encode` binary/example.
- `docs/gpu-research/hardware-encode-backend.md`
- Encoded synthetic output.
- Minimal hardware-encoder FFI/wrapper notes.

### Gate

Proceed only if at least one selected hardware backend can encode from GPU-resident frames in-process without CPU rawvideo staging, and the result is classified by the project use case it enables.

## 10. Research Phase 4: Shared Vulkan Image Interop Proof

Purpose: prove the central zero-copy requirement.

This is the most important phase. It decides whether the project can have the desired pipeline at all.

### Candidate Ownership Models

#### Model A: FFmpeg-Owned Images, Skia Renders Into Them

```text
FFmpeg allocates AVHWFramesContext images
  -> Rust extracts/wraps VkImage metadata
  -> Skia wraps as backend render target/texture
  -> Skia renders
  -> FFmpeg encodes same frame
```

Questions:

- Can the underlying `VkImage` be accessed with enough metadata for Skia wrapping?
- Does FFmpeg expose layout, format, allocation, queue-family, and memory ownership information safely?
- Can Skia render into the image layout/usage flags FFmpeg requires?
- Can synchronization be expressed without undefined behavior?

#### Model B: Skia/Rust-Owned Images, FFmpeg Imports Them

```text
Rust creates VkImage with required usage/export flags
  -> Skia renders into VkImage
  -> Rust transitions layout and signals sync
  -> FFmpeg imports/wraps VkImage as hardware frame
  -> FFmpeg encodes
```

Questions:

- Can FFmpeg import externally created Vulkan images for encoding?
- Which external memory/semaphore extensions are required on Windows and Linux?
- Does the selected encoder accept the image format/layout directly?
- Are hidden copies introduced during import or format negotiation?

#### Model C: Shared Device, FFmpeg Frame Pool, GPU Conversion Target

```text
Skia renders into one Vulkan image
  -> GPU conversion pass writes into FFmpeg-compatible hardware frame
  -> FFmpeg encodes converted frame
```

This still satisfies zero CPU-to-GPU copy if the transfer/conversion is GPU-only.

Questions:

- Is an extra GPU-to-GPU copy/conversion acceptable?
- Can it solve premul-to-straight alpha and format conversion in one pass?
- Does it simplify FFmpeg frame compatibility enough to be worth the copy?

### Work Items

1. Implement one-frame tests for all viable ownership models.
2. For each model, document:
   - image format;
   - usage flags;
   - memory flags;
   - layout transitions;
   - queue-family transfers;
   - semaphore/fence handoff;
   - FFmpeg frame fields;
   - Skia wrapping API;
   - cleanup order.
3. Validate with Vulkan validation layers.
4. Confirm whether any CPU readback/upload occurs.
5. Use API inspection, memory-flow tracing, validation output, and explicit debug labels to identify hidden copies. Timing markers may be added only as supporting diagnostics, not as success criteria.
6. Encode one visually identifiable frame generated by Skia.
7. Read back only a diagnostic copy for visual comparison.

### Deliverables

- `gpu_research_vulkan_interop` binary/example.
- `docs/gpu-research/vulkan-interop-decision.md`
- A decision selecting Model A, B, or C.
- A list of unsupported GPUs/drivers/extensions discovered during testing.

### Gate

Proceed to production design only if one model successfully encodes a Skia-rendered frame without CPU pixel staging.

If all models fail, the plan should stop and pivot to a less ambitious path, such as GPU-assisted conversion with one CPU-to-GPU upload, or a platform-specific native encoder path.

## 11. Research Phase 5: GPU Format and Alpha Correctness

Purpose: prove the output is visually correct, not merely encodable.

### Work Items

1. Identify Skia Vulkan surface format and alpha type.
2. Identify selected hardware frame format and encoder expectations.
3. If the selected use case includes transparency, verify alpha semantics:
   - premultiplied vs straight alpha;
   - 8-bit vs 10-bit alpha;
   - RGB/YUV conversion;
   - channel order;
   - color range and matrix.
4. Implement a GPU-only conversion pass if required:
   - premul RGBA to straight alpha;
   - RGBA/BGRA to encoder format;
   - RGB to YUV if FFmpeg requires preformatted input.
5. Test edge cases:
   - semi-transparent text;
   - blurred shadows;
   - anti-aliased route lines;
   - fully transparent background;
   - near-zero alpha pixels;
   - saturated colors.

### Deliverables

- `docs/gpu-research/format-alpha.md`
- Shader/conversion proof if required.
- CPU vs GPU image-diff report.

### Gate

Proceed only if sampled frames preserve transparency and visual parity closely enough for production overlay exports.

## 12. Research Phase 6: Multi-Frame Lifecycle Prototype

Purpose: prove the GPU frame lifecycle works across multiple frames. This is still a feasibility phase, not a performance phase.

### Target Shape

```text
GpuFramePool
  acquire writable frame
  render with Skia Vulkan
  optional GPU conversion
  submit to FFmpeg
  receive encoded packets
  recycle frame after encoder release
```

### Work Items

1. Build a small multi-frame prototype outside production code.
2. Use a fixed-size GPU frame pool.
3. Track each frame state:
   - free;
   - rendering;
   - render submitted;
   - conversion submitted;
   - submitted to encoder;
   - encoder released;
   - recyclable.
4. Implement cancellation:
   - stop acquiring new frames;
   - flush/abort encoder according to FFmpeg API rules;
   - release all GPU frames;
   - destroy Skia and FFmpeg contexts in safe order.
5. Implement drain:
   - submit final frames;
   - send encoder EOF;
   - receive all packets;
   - mux trailer.
6. Prove lifecycle correctness:
   - no frame is reused while Skia, conversion work, or FFmpeg may still reference it;
   - synchronization objects have a clear owner and cleanup path;
   - encoder drain releases or retires every submitted frame;
   - cancellation releases all GPU and FFmpeg resources;
   - repeated short runs do not leak handles or leave the encoder/runtime poisoned.
7. Record only diagnostic events needed to understand lifecycle state:
   - frame acquired;
   - render submitted;
   - conversion submitted, if present;
   - encoder submitted;
   - packet received;
   - frame released;
   - frame recycled;
   - drain complete.

### Deliverables

- `gpu_research_multiframe_vulkan_lifecycle` binary/example.
- `docs/gpu-research/multiframe-lifecycle.md`
- A frame-state trace for a short multi-frame output.

### Gate

Proceed only if the prototype can encode a short multi-frame output, drain cleanly, cancel cleanly, recycle frames safely, and shut down all Vulkan/Skia/FFmpeg resources without leaks or invalid lifetime assumptions.

## 13. Research Phase 7: App Baseline Evidence and Abstraction Map

Purpose: after the disconnected GPU pipeline proves technical feasibility, return to the actual OVRLEY pipeline and document exactly where production integration must happen.

This phase still should not refactor production code. It creates the evidence and map needed for the first production implementation phase.

### Work Items

#### 7A. Baseline Current App Behavior

1. Capture current timings for representative export targets with:
   - current transparent-overlay codecs;
   - current composite hardware-accelerated codecs;
   - representative short and long activities;
   - sample frames with text, shadows, route, elevation, and transparent overlays.
2. Record current timing buckets:
   - frame render time;
   - `base.restore`;
   - `surface.create`;
   - text/widget draw buckets;
   - queue wait;
   - `ffmpeg.write`;
   - encode/finalization wait.
3. Capture FFmpeg command lines generated by the current profiles.
4. Save baseline output files and sampled PNGs for later comparison.
5. Confirm the exact bottleneck symptoms around the reported 33 ms/frame ceiling.

#### 7B. Identify the Concrete Abstraction Seams

Create an abstraction map that names the current CPU-specific assumption, the future backend-neutral concept, and the likely production file boundary.

The required seams are:

| Current assumption | Current code | Future abstraction | Why it matters |
| --- | --- | --- | --- |
| A frame is `Vec<u8>` / `&mut [u8]` RGBA CPU memory | `RenderTarget<'a>`, `FrameBuffer`, `render_frame_rgba`, `pipeline_shared.rs` | `RenderFrame` with `CpuFrame` and later `GpuFrame` variants | GPU frames cannot be represented as byte slices without destroying zero-copy. |
| Rendering means wrapping CPU pixels as a Skia raster surface | `wrap_native_surface`, `surface.rs` | `RenderSurface` / backend-owned frame target | CPU uses `wrap_pixels`; Vulkan uses GPU-backed Skia surfaces or wrapped `VkImage`s. |
| Static layer restore is a CPU byte copy | `prepared_assets.base_rgba`, `prepare_base_rgba`, `copy_from_slice` in `render_frame_rgba` | `StaticLayerCache` with CPU and GPU implementations | GPU path needs pre-rendered GPU images/textures, not per-frame CPU restore. |
| Label/icon cache is a CPU Skia image/cache | `cached_labels_image`, `PreparedPreviewAssets.labels_image` | `StaticOverlayCache` / `PreparedBackendAssets` | GPU path may need texture-backed label/icon images and different lifetime rules. |
| Encoder input is written to FFmpeg stdin | `writer_worker`, `spawn_ffmpeg_process`, `video_pipeline.rs` | `EncodeBackend::submit_frame` | GPU encoder receives hardware frames, not bytes over stdin. |
| Progress comes from rendered count plus stderr `frame=N` parsing | `monitor_ffmpeg`, `parse_ffmpeg_frame`, `RenderController::set_frame_progress` | `PipelineProgress` counters: rendered/submitted/encoded/drained | In-process encode reports packets/drain, not stderr lines. |
| Cancellation is closing the sender/stdin and waiting/killing the child | `video_pipeline.rs`, `WriterCancellation` | `PipelineCancellation` with backend-specific drain/abort | GPU encode needs explicit flush/abort and GPU resource release. |
| Capability means FFmpeg binary/profile availability | `codec_detect.rs`, codec catalog/profile checks | `BackendCapabilityReport` | Vulkan also depends on device extensions, Skia context creation, FFmpeg hwcontext, and interop model. |
| Timing buckets assume CPU render plus pipe write | `RenderProfiler`, `merge_timing_maps`, `video_debug.rs` | backend-neutral timing buckets plus GPU-specific buckets | GPU work needs submit/wait/packet timings instead of only write/render timings. |

#### 7C. Draw the Intended Production Boundaries

Write a short design note with these proposed modules. This is still a design artifact, not implementation.

```text
render/
  mod.rs                 shared render entrypoints and CPU compatibility
  surface.rs             CPU/raster surface helpers
  surface_vulkan.rs      future Vulkan Skia surface/frame target
  cache_vulkan.rs        future GPU static label/base cache

encode/
  video_pipeline.rs      existing CPU/subprocess transparent pipeline
  pipeline_shared.rs     CPU queue/writer helpers only, or renamed to cpu_pipeline_shared
  video_vulkan.rs        future in-process Vulkan encode pipeline
  ffmpeg_ffi.rs          future direct FFmpeg API wrapper
  ffmpeg_hw_vulkan.rs    future Vulkan hwdevice/hwframe wrapper

gpu/
  capabilities.rs        backend capability/fallback reporting
  vulkan_runtime.rs      Vulkan device, queues, Skia context, FFmpeg hwcontext
```

#### 7D. Decide What Must Not Be Abstracted Yet

Do not abstract these until the interop proof identifies the real shape:

- concrete Vulkan frame handle fields;
- image ownership model;
- synchronization object type;
- FFmpeg hardware frame wrapper;
- conversion shader inputs/outputs;
- frame pool recycling rules.

Prematurely abstracting those would bake in guesses. The completed interop prototype should decide them.

### Deliverables

- `docs/gpu-research/baseline.md`
- `docs/gpu-research/abstraction-map.md`
- Baseline timing JSON files.
- Baseline sample outputs.

### Gate

Proceed to production integration only when:

- the disconnected GPU prototype has already proven technical feasibility;
- the baseline describes whether the current app limit appears in CPU render, pipe write, FFmpeg upload/conversion, encode, mux/write, or queue backpressure;
- `abstraction-map.md` names the exact current symbols/modules that need to change;
- the map clearly separates early safe abstractions from Vulkan details already decided by interop research.

## 14. Production Phase 1: Backend-Neutral Frame Abstractions

Start production changes only after the research gates pass.

### Work Items

1. Introduce backend-neutral frame concepts without breaking the CPU path.

The first production abstraction layer should include only these stable concepts:

```text
RenderBackend
  prepare()
  acquire_frame()
  render_frame()
  finish_frame()

EncodeBackend
  start()
  submit_frame()
  drain()
  cancel()

RenderFrame
  Cpu(CpuFrame)
  Gpu(opaque backend-owned handle)

StaticLayerCache
  CpuStaticLayer
  GpuStaticLayer

PipelineProgress
  planned_frames
  rendered_frames
  submitted_frames
  encoded_frames
  drained

BackendCapabilityReport
  supported
  backend_name
  fallback_reason
  diagnostics
```

2. Move CPU behavior behind those concepts without changing behavior:
   - CPU `RenderFrame` still owns or borrows `Vec<u8>` / `&mut [u8]`.
   - CPU render still uses `wrap_native_surface`.
   - CPU encode still uses FFmpeg subprocess stdin.
   - CPU progress can still parse stderr for encoded frames.
3. Split CPU-only helpers from backend-neutral helpers:
   - keep `writer_worker` and `FrameBuffer` as CPU/subprocess-specific;
   - do not make GPU code depend on `pipeline_shared.rs` if it remains byte-buffer oriented.
4. Replace direct calls to `render_frame_rgba` from the high-level export path with a backend-selected render call.
5. Replace direct calls to `spawn_ffmpeg_process` from the high-level export path with a backend-selected encode call.
6. Preserve existing progress and cancellation semantics for CPU exports.
7. Avoid exposing Vulkan types in shared app-facing APIs.
8. Add capability reports that can explain why a GPU backend is unavailable.

### Deferred Until Vulkan Backend Implementation

Do not add generic versions of these until the selected interop model is known:

- concrete `GpuFrame` fields;
- Vulkan image layout state;
- semaphore/fence ownership;
- FFmpeg `AVFrame` wrapper shape;
- GPU frame pool recycling policy;
- GPU conversion pass API.

### Likely Code Areas

- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/surface.rs`
- `src-tauri/ovrley_core/src/encode/video.rs`
- `src-tauri/ovrley_core/src/encode/progress.rs`
- `src-tauri/ovrley_core/src/encode/pipeline_shared.rs`

### Gate

CPU exports must produce identical output and preserve current cancellation/progress behavior.

## 15. Production Phase 2: Vulkan Runtime Module

### New Code Areas

```text
src-tauri/ovrley_core/src/gpu/
src-tauri/ovrley_core/src/gpu/vulkan_runtime.rs
src-tauri/ovrley_core/src/gpu/capabilities.rs
```

### Work Items

1. Own Vulkan instance/device/queues.
2. Enable required validation in debug/research builds.
3. Check required extensions:
   - external memory where needed;
   - external semaphores where needed;
   - timeline semaphores if used;
   - storage image or transfer usage for conversion;
   - queue families.
4. Create Skia Vulkan context.
5. Create selected hardware encoder context or compatible import wrapper.
6. Expose structured fallback reasons.
7. Handle device loss/TDR as a design concern, not only a final test:
   - detect device-lost errors;
   - abandon Skia context;
   - close encoder safely;
   - return a clear fallback/error.

### Gate

Forced initialization failures must not crash the app and must leave CPU rendering available.

## 16. Production Phase 3: Vulkan Render Backend

### New Code Areas

```text
src-tauri/ovrley_core/src/render/surface_vulkan.rs
src-tauri/ovrley_core/src/render/cache_vulkan.rs
```

### Work Items

1. Render frames into GPU-backed Skia surfaces.
2. Reuse existing widget draw code where possible by passing a Skia `Canvas`.
3. Replace CPU `base_rgba` with a GPU static-layer strategy:
   - pre-render static layer to GPU image;
   - draw/copy static GPU image each frame;
   - avoid per-frame CPU restore.
4. Replace CPU label image cache with GPU image cache where beneficial.
5. Emit synchronization artifacts required by the encode backend.
6. Keep debug readback for sampled PNG comparisons only.

### Gate

GPU-rendered sampled frames must match CPU baseline closely, especially around transparency and antialiasing.

## 17. Production Phase 4: Vulkan Encode Backend

### New Code Areas

```text
src-tauri/ovrley_core/src/encode/video_vulkan.rs
src-tauri/ovrley_core/src/encode/ffmpeg_ffi.rs
src-tauri/ovrley_core/src/encode/ffmpeg_hw_vulkan.rs
```

### Work Items

1. Implement direct FFmpeg encode orchestration.
2. Replace stdin writer semantics with frame submission semantics.
3. Replace stderr progress parsing with internal counters:
   - rendered frames;
   - submitted frames;
   - packets received;
   - frames released/drained.
4. Implement hardware frame pool ownership.
5. Implement synchronization handoff from renderer to encoder.
6. Implement muxing and trailer finalization.
7. Implement cancellation and drain without relying on closing stdin.
8. Ensure partial output cleanup matches current behavior.

### Gate

The backend must encode a full output file, cancel reliably, and recover resources after repeated exports.

## 18. Production Phase 5: Runtime Selection and UX Gating

### Work Items

1. Keep CPU as the default safe backend.
2. Probe GPU capability at startup or export time.
3. Show GPU export options only when:
   - Vulkan runtime initializes;
   - Skia Vulkan context initializes;
   - selected hardware encoder is available;
   - selected interop model is supported;
   - required formats and alpha path are supported.
4. Report fallback reasons clearly:
   - missing Vulkan;
   - unsupported GPU extension;
   - FFmpeg build lacks encoder;
   - FFmpeg dev/runtime mismatch;
   - image import unsupported;
   - validation/device error.
5. Keep macOS on CPU/VideoToolbox-compatible paths until a Metal research plan exists.

### Gate

Unsupported systems must not offer the Vulkan option. Supported systems must export successfully or fall back with a specific reason.

## 19. Verification and Regression Plan

### Required Automated Tests

- CPU pipeline regression tests continue to pass.
- FFmpeg settings tests continue to verify current profiles.
- GPU capability parser/probe tests where practical.
- Image-diff tests for sampled GPU frames once the backend is stable.
- Cancellation tests for short GPU exports.

### Required Manual Tests

- representative transparent-overlay export, if still supported by the chosen roadmap.
- representative composited final-video hardware export.
- Repeated exports in one app session.
- Cancellation during render, encode, and drain.
- Unsupported GPU/driver fallback.
- Driver reset/device-loss scenario where practical.
- Output import into target video editing software.
- Transparent overlay compositing over real video.

### Required Timing Buckets

- `gpu.cpu_prepare`
- `gpu.frame_pool.acquire_wait`
- `gpu.skia.record`
- `gpu.skia.submit`
- `gpu.render.wait`
- `gpu.convert.submit`
- `gpu.convert.wait`
- `gpu.encode.submit`
- `gpu.encode.receive_packet`
- `gpu.encode.drain`
- `gpu.mux.write`
- `gpu.frame_pool.release_wait`

## 20. macOS Compatibility Strategy

The Vulkan implementation must not make the codebase Vulkan-only.

Short term:

- macOS keeps the current CPU Skia render path.
- macOS continues using existing supported encode options, including VideoToolbox where available.
- GPU Vulkan UI is hidden on macOS.

Long term:

- Research a separate Metal path:

```text
Skia Metal surface
  -> IOSurface/CVPixelBuffer-compatible texture
  -> VideoToolbox or FFmpeg VideoToolbox encode
```

Shared abstractions created after Vulkan research must be capable of representing `MetalFrame` later without changing frontend-facing concepts.

## 21. Decision Points

### Decision A: Is True Zero CPU-to-GPU Copy Feasible?

Answer after Phase 4.

If no, do not proceed with the production Vulkan rewrite. Consider a smaller GPU-assisted path with one upload only if profiling shows it is still valuable.

### Decision B: Which Component Owns Images?

Answer after Phase 4.

Choices:

- FFmpeg-owned frame pool;
- Rust/Skia-owned frame pool;
- separate render image plus GPU-only conversion into FFmpeg frame pool.

### Decision C: Is In-Process FFmpeg Worth the Maintenance Cost?

Answer after Phases 3-6.

If FFmpeg import is possible only in-process, the encode rewrite is justified. If a subprocess can import external GPU images portably and safely, that path can be revisited, but it should not be assumed.

### Decision D: Does Skia Vulkan Preserve Render Quality?

Answer after Phase 2 and Phase 5.

If render quality diverges materially from CPU Skia, either fix the GPU path or keep CPU fallback as the default for quality-sensitive output.

## 22. Expected Rewrite Scope

If research succeeds:

- `render/` requires a medium rewrite:
  - new GPU surface/frame path;
  - GPU static-layer cache;
  - debug readback;
  - most widget draw functions should remain reusable if Skia `Canvas` remains the draw API.
- `encode/` requires a large rewrite:
  - new in-process FFmpeg backend;
  - hardware frame pool;
  - packet/mux/drain lifecycle;
  - synchronization and cancellation;
  - no stdin writer or stderr progress parser for GPU path.
- shared code requires careful abstraction:
  - progress;
  - cancellation;
  - frame lifecycle;
  - capability reporting;
  - backend selection.

The CPU path should remain intact throughout.

## 23. Recommended First Issue

Create a narrow research issue:

> Build a disconnected primitive GPU-render-to-hardware-encoder proof that either proves or disproves zero CPU-to-GPU copy feasibility for at least one project-relevant output path.

Scope:

- feature-gated dependencies only;
- no production pipeline refactor;
- no OVRLEY scene/config/widget integration;
- one GPU-rendered Vulkan frame, preferably through Skia if the build path is immediately available;
- one encoded output through a selected hardware backend;
- direct FFmpeg/libavcodec path, not FFmpeg stdin;
- explicit note proving whether CPU staging occurred;
- decision document naming the viable ownership model or explaining why none works.

This issue should be completed before app baselining, abstraction mapping, or broad render/encode refactoring starts.
