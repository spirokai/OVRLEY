# Implementation Plan: Rust + Skia Backend Migration (CPU-first)

## Background

This is an alternative to `rust_vello_plan.md`. It uses `skia-safe` (the official Rust binding for Google's Skia graphics library) instead of Vello/WGPU. The key difference is the rendering strategy: **Skia's CPU rasterizer is the primary backend**, which is production-grade, heavily optimised via SIMD, and requires zero GPU driver setup. GPU acceleration is added in the final phase as an optional upgrade — not a requirement.

**Platforms:** Windows 10+ (DirectX 12 / Vulkan), macOS 12+ (Metal)  
**Codec:** ProRes 4444 (hard requirement, software `prores_ks` initially, `prores_videotoolbox` where available)  
**Schema:** Existing JSON config schema preserved. No deviations without explicit approval.  
**Cutover:** Hard cutover once feature parity is reached. Python sidecar remains unchanged throughout migration.  
**Regression baseline:** Existing `debug_render/` timing JSON and sample frames serve as the benchmark for each phase.

---

## Why Skia over Vello for a CPU-first strategy

| Concern | Skia (CPU) | Vello (GPU) |
|---|---|---|
| Text rendering | Native (`Font`, `TextBlob`, `Paint`) | Requires Parley integration |
| Drop shadows | Native `ImageFilter::drop_shadow` | Manual two-pass workaround |
| Path clipping (route reveal) | Native `canvas.clip_path()` | Manual mask texture |
| GPU requirement | None for CPU path | Always required |
| CPU fallback quality | Excellent (SIMD-optimised) | Slow (compute shaders on CPU) |
| Cross-platform build | Heavy C++ compile, well-established | Pure Rust, simpler build |
| GPU upgrade path | Additive (same Canvas API) | Native, but requires full rewire |

---

## Architecture Overview

```
Tauri Host (Rust)
├── IPC Commands (existing lib.rs, unchanged until Phase 5)
└── [NEW] cyclemetry_core (Rust library crate)
    ├── gpx/          — GPX parsing, interpolation, smoothing
    ├── config/       — JSON config deserialization (serde)
    ├── render/       — Skia surface, frame compositor
    │   ├── text/     — Skia text, font cache, shadow, border
    │   ├── route/    — Map/course path renderer
    │   └── elevation/— Elevation graph renderer
    ├── encode/       — FFmpeg pipe manager
    └── debug/        — Timing profiler (mirrors render_debug.py)
```

Python sidecar is deleted only at the end of Phase 5, after full validation.  
GPU acceleration is added non-destructively in Phase 6 — the same `Canvas` API is used throughout.

---

## Phase 1 — Rust Crate Scaffold & IPC Rewire

### Goal
Create the `cyclemetry_core` Rust library crate, wire it into `src-tauri`, and replace the Flask HTTP/socket layer with direct Tauri command dispatch. The sidecar is no longer spawned. All commands return stubs.

### Deliverables
- `src-tauri/cyclemetry_core/` crate with `Cargo.toml`
- `config/` module: `serde` structs for the full JSON config schema:

```
SceneConfig    { width, height, fps, start, end, font, color, font_size,
                 overlay_filename, scale?, decimal_rounding?, render_debug? }
LabelConfig    { text, x, y, font?, font_size?, color?, opacity?,
                 shadow_color?, shadow_strength?, shadow_distance?,
                 border_color?, border_thickness?, border_strength?, border_distance? }
ValueConfig    { value: AttributeKind, x, y, font?, font_size?, color?, opacity?,
                 suffix?, unit?, hours_offset?, time_format?, decimal_rounding?,
                 icon_color?, shadow_*, border_* }
PlotConfig     { value: AttributeKind, x, y, width, height, color?, opacity?,
                 rotation?, line { width, color }?, points[]?, fill?,
                 marker?, label?, shadow_*, render_mode? }
RootConfig     { scene: SceneConfig, labels: [], values: [], plots: [] }
AttributeKind  enum { Course, Elevation, Speed, Heartrate, Cadence,
                       Power, Temperature, Time, Gradient }
```

- `encode/ffmpeg.rs`: FFmpeg binary resolver — mirrors `scene.py::resolve_ffmpeg_binary`. Checks env override, bundled binary next to executable, then `PATH`.
- `debug/profiler.rs`: `RenderProfiler` and `TimingBucket` structs with `record()` and `measure()` — output format identical to Python `timing_summary.json`.
- Modified `src-tauri/src/lib.rs`: all Tauri commands call `cyclemetry_core` functions directly. HTTP/socket proxy code removed. Sidecar spawn removed from `setup()`.
- All endpoint stubs return `{"status": "not_implemented"}`.

### Manual Test
1. `cargo tauri dev` — app opens without spawning `cyclemetry-server`.
2. `backend_health` returns `{"status":"ok","ready":false}` from Rust.
3. `backend_render` returns `{"status":"not_implemented"}` without panic.
4. Windows: `netstat -an | findstr 31337` returns nothing. macOS: `lsof -i :31337` returns nothing.

---

## Phase 2 — Activity Data Processing (Smoothing & Interpolation)

### Goal
GPX and FIT file parsing is **fully handled in the frontend** (`app/src/api/activityParserUtils.js`, `gpxUtils.jsx`, `fitParserUtils.js`). The frontend produces a `parsedActivity` JSON object and passes it directly to the Rust backend via Tauri IPC on every `backend_demo` and `backend_render` call. This phase implements the Rust side: deserialise the `parsedActivity` payload, apply smoothing, interpolation, trim, and gradient derivation to produce dense per-frame arrays ready for rendering.

### Input: `parsedActivity` JSON Schema
Passed from the frontend as a serialised JSON string. Key fields:

```
parsedActivity {
  metadata            { duration_seconds, start_time, end_time,
                        total_distance_m, sample_count }
  sample_elapsed_seconds:      [f64]          — sparse, one per source sample
  sample_distance_progress:    [f64]          — 0.0–1.0 per sample
  sample_course_points:        [[f64,f64]]    — [lat, lon] per sample
  sample_elevations:           [f64|null]     — pre-smoothed by frontend
  valid_attributes:            [string]       — which metrics are populated
  trim_start_seconds:          f64
  trim_end_seconds:            f64
  // Per-attribute sparse arrays (null where unavailable):
  course:       [[f64,f64]|null]
  elevation:    [f64|null]
  speed:        [f64|null]         — m/s
  heartrate:    [f64|null]         — bpm
  cadence:      [f64|null]         — rpm
  power:        [f64|null]         — watts
  temperature:  [f64|null]         — celsius
  gradient:     [f64|null]         — percent
  time:         [ISO8601|null]
}
```

> **Note:** The frontend already applies Savitzky-Golay smoothing to elevation and gradient. The Rust backend applies its own smoothing pass to maintain parity with the Python backend during transition and to serve as the single canonical smoothing path once the sidecar is deleted.

### Deliverables
- `activity/schema.rs`: `serde` structs deserialising the full `parsedActivity` JSON shape above.
- `activity/smooth.rs`: Savitzky-Golay filter. Coefficients hard-coded to match frontend's `applyFixedSavitzkyGolay`: `[-36,9,44,69,84,89,84,69,44,9,-36]` for elevation, `[-2,3,6,7,6,3,-2]` for gradient.
- `activity/interpolate.rs`: 1D linear interpolation of all sparse sample arrays onto a dense frame-rate array. Given `fps` and `trim_start_seconds`/`trim_end_seconds` from the render config, produces arrays of length `round((trim_end - trim_start) * fps)`. `null` gaps filled by nearest-valid-neighbour before interpolation.
- `activity/trim.rs`: Bisect-based trim — clamp all arrays to `[trim_start_seconds, trim_end_seconds]` with boundary values interpolated at the exact trim point.
- `activity/processor.rs`: `ActivityProcessor` orchestrator — receives `parsedActivity`, runs trim → smooth → interpolate → returns `DenseActivity` struct with all per-frame `Vec<f64>` arrays.
- `backend_demo` Tauri command: now accepts `parsedActivity` JSON alongside `config` and `second`. Runs `ActivityProcessor`, stubs the render (blank PNG), returns `{ filename }`.
- `backend_render` Tauri command: accepts `parsedActivity` JSON alongside `config`. Runs `ActivityProcessor`, returns `{"status":"not_implemented"}`.
- CLI binary `validate_activity`: accepts a `parsedActivity` JSON file (generated by the frontend's debug export via `writeParseDebugFile`), runs the Rust processor, emits all dense frame arrays as JSON for diff against Python output. Tolerance: < 0.01% relative error per value.

### Manual Test
1. Load `demo.gpxinit` in the frontend — the parsed activity is passed automatically on the next `backend_demo` call.
2. Click "Preview Frame" — must return a valid transparent PNG without error.
3. Run `validate_activity <debug_payload.json>` and compare numeric arrays against Python output. All values within tolerance.
4. `debug_render/phase_2/timing_summary.json` contains `activity.smooth`, `activity.interpolate`, `activity.trim` timing buckets.
5. Verify that changing `trim_start_seconds` / `trim_end_seconds` in the UI correctly shortens the dense arrays.

---

## Phase 3 — Skia CPU Rendering Engine (Text & Static Composition)

### Goal
Wire `skia-safe` (CPU raster surface) into the pipeline. Render all `labels` (static text) and `values` (dynamic text) to an RGBA buffer. Save single preview frames as PNG. No charting yet.

### Key Skia API used in this phase
- `surfaces::raster_n32_premul((width, height))` — creates a CPU-backed RGBA surface
- `canvas.draw_str()` / `TextBlob` — text rendering
- `Font::new(Typeface, size)` — font loading
- `Paint` with `Color4f` and `set_alpha_f()` — fill, opacity
- `ImageFilter::drop_shadow()` — native soft/hard shadows (no manual two-pass workaround needed)
- `Paint::set_style(Stroke)` + `paint.set_stroke_width()` — text outline/border
- `surface.peek_pixels()` → `&[u8]` — zero-copy RGBA byte extraction

### Deliverables
- `render/surface.rs`: CPU surface factory. `fn new_cpu_surface(width: i32, height: i32) -> Surface`. Returns a premultiplied RGBA `skia_safe::Surface`.
- `render/text/layout.rs`: text rendering module:
  - `FontCache`: `HashMap<(String, i32), Font>` — loaded once, reused across frames.
  - `fn draw_label(canvas, text, x, y, config: &LabelConfig, font_cache)`:
    - Resolves font from `fonts/` dir, falls back to system Arial.
    - Applies `opacity` via `Paint::set_alpha_f`.
    - Shadow: `Paint::set_image_filter(ImageFilter::drop_shadow(...))` using `shadow_color`, `shadow_strength` as sigma, `shadow_distance` as offset. Native Gaussian blur — no workaround.
    - Border/outline: second `draw_str` call with `Paint::set_style(Stroke)`, `border_thickness` as width, `border_color`.
  - `fn draw_value(canvas, value_str, x, y, config: &ValueConfig, font_cache)`: same as `draw_label` but with dynamic formatted string.
  - Unit conversion and `decimal_rounding` applied before formatting (mirrors `frame.py::convert_value`).
- `render/compositor.rs`: `FrameCompositor`:
  - `build_base_layer(config, width, height, font_cache) -> Surface`: renders all `labels` once to an off-screen CPU surface. Returns the surface (not pixels — it is copied at frame time).
  - `render_frame(frame_index, activity, config, base_layer, font_cache) -> Vec<u8>`:
    - Copies base layer pixels into a fresh surface using `canvas.draw_image()`.
    - Draws all `values` at `frame_index` on top.
    - Extracts RGBA bytes via `surface.peek_pixels()`.
- `render/frame_buffer_pool.rs`: pool of pre-allocated `Vec<u8>` buffers (4 × `width * height * 4` bytes). Thread-safe via `Arc<Mutex<VecDeque<Vec<u8>>>>`.
- `backend_demo` command: fully implemented — calls `FrameCompositor`, saves output PNG to `PUBLIC_DIR`, returns `{ filename }`.
- `debug/profiler.rs` extended: `render.base_layer`, `render.frame`, `render.pixel_extract` timing buckets.

### Manual Test
1. Configure a template with `labels` and `values`, click "Preview Frame".
2. Visually compare generated PNG to Python preview for the same second — font, size, color, position must match within 2px.
3. Verify soft drop shadow renders correctly (Skia `drop_shadow` filter, not a hard offset).
4. `debug_render/phase_3/timing_summary.json` — `render.base_layer` fires once, `render.frame` fires once per preview.
5. Deliberately remove GPU from the test environment (or test in a VM) — must not crash, CPU path always available.

---

## Phase 4 — Charting Engine (Route & Elevation Widgets)

### Goal
Implement the two `plots` widget types (`course` and `elevation`) using native Skia `Path` geometry. Replaces `matplotlib` entirely.

### Key Skia API used in this phase
- `Path::new()` + `path.move_to()` / `path.line_to()` — polyline construction
- `Paint::set_style(Fill)` / `(Stroke)` — filled areas and lines
- `canvas.clip_path()` — native path-based clipping for route reveal
- `canvas.clip_rect()` — horizontal clip for elevation reveal
- `canvas.save()` / `canvas.restore()` — clip scope management
- `Paint::set_alpha_f()` — background layer opacity
- `Image::from_encoded()` — loading PNG marker sprites

### Deliverables

#### Route (Course) Widget
- `render/route/geometry.rs`:
  - `project_coords(points: &[(f64,f64)], bbox_px: Rect) -> Vec<Point>`: equirectangular projection, scales lat/lon bounding box to pixel widget bounds. Preserves aspect ratio with centering.
  - Ramer-Douglas-Peucker simplification with configurable pixel tolerance (mirrors Python logic).
  - `build_route_cache(config, activity, width, height) -> RouteWidgetCache`:
    - Pre-renders the full background path to an off-screen CPU `Surface` at `background_opacity * 0.75`.
    - Pre-computes `display_points` (simplified pixel coords).
    - Pre-computes per-frame `RouteFrameState { marker_x, marker_y, segment_index, progress01 }` for every frame index.
    - Pre-computes N bucket clip `Path` objects for fast reveal (mirrors Python `bucket_masks`).
- `render/route/renderer.rs`:
  - `fn composite_route(canvas, cache, frame_index)`:
    - Draws background surface via `canvas.draw_image()`.
    - Applies `canvas.save()`, `canvas.clip_path(bucket_path)`, draws completed-route `Path` in full opacity, `canvas.restore()`.
    - Draws marker: `Image::from_encoded()` sprite or default filled circle via `canvas.draw_circle()`.
    - Applies `rotation_deg` via `canvas.rotate()` around widget center.

#### Elevation Widget
- `render/elevation/geometry.rs`:
  - `project_elevation(distance_progress: &[f64], elevation_m: &[f64], bbox_px: Rect) -> Vec<Point>`: maps to pixel coords with Y-axis flipped.
  - `build_elevation_cache(config, activity, width, height) -> ElevationWidgetCache`:
    - Pre-renders background filled area to off-screen `Surface`: `Path` from points + closed bottom edge, drawn with `Paint::set_style(Fill)` at low opacity.
    - Pre-renders completed-line overlay `Surface` (full opacity stroke).
    - Pre-computes per-frame `ElevationFrameState { progress01, marker_x, marker_y, label_text }`.
    - Applies unit conversions (metric/imperial) and `decimal_rounding` for `label_text`.
- `render/elevation/renderer.rs`:
  - `fn composite_elevation(canvas, cache, frame_index, font_cache)`:
    - Draws background surface.
    - `canvas.save()`, `canvas.clip_rect(Rect::from_xywh(0, 0, progress01 * width, height))`, draws completed overlay, `canvas.restore()`.
    - Draws marker circle or sprite.
    - Draws dynamic elevation label using `render/text/layout.rs::draw_label`.
    - Applies `rotation_deg`.

#### Integration
- `render/compositor.rs` updated: `build_base_layer()` now calls `composite_route` and `composite_elevation` for background layers. `render_frame()` calls them again for the dynamic reveal/marker pass.
- `backend_render` command: stub — accepts request, returns `{"status":"not_implemented"}`.

### Manual Test
1. Configure template with `course` and `elevation` plots, scrub preview — route reveal and elevation marker animate correctly.
2. Compare Rust frames against Python frames at 0%, 25%, 50%, 75%, 100% of activity — route shape and elevation profile shape within visual tolerance.
3. `rotation_deg: 90` on both widgets renders correctly.
4. `debug_render/phase_4/timing_summary.json` contains `composite.route` and `composite.elevation` — both faster than Python baseline.
5. Elevation label text renders with correct unit, rounding, and font styling.

---

## Phase 5 — FFmpeg Encoding, Progress & Full Render Pipeline

### Goal
Wire the complete render pipeline into a full video encode. Implement all remaining commands. Validate against Python baseline. Delete Python sidecar.

### Deliverables
- `encode/pipeline.rs`: render + encode loop:
  - Spawns FFmpeg with `stdin=Pipe`, `stderr=Pipe`.
  - Input args: `-f rawvideo -pix_fmt rgba -s {w}x{h} -r {fps} -i -`
  - Output: configured via `scene.ffmpeg` block (mirrors `scene.py::build_ffmpeg_settings`). Default: `prores_ks`, `yuva444p10le`, profile `4444`.
  - Bounded frame queue (`maxsize=4`) with dedicated encoder thread writing to FFmpeg stdin.
  - FFmpeg stderr monitor thread parsing `frame=N` for encode progress.
  - Atomic `cancel_flag` polled between frames. On cancel: close stdin, terminate FFmpeg, delete partial output file.
- `encode/progress.rs`: `Arc<Mutex<RenderProgress>>` with `current`, `total`, `encoded`, `status`, `message`, `frame_times` (rolling window of 20 for ETA).
- All Tauri commands fully implemented:
  - `backend_render` → spawns pipeline on a Tokio task, returns immediately
  - `backend_progress` → returns current `RenderProgress` as JSON
  - `backend_cancel` → sets cancel flag
  - `backend_demo` → single frame preview (Phase 3)
  - `backend_open_downloads` / `backend_open_video` → `open::that()` (cross-platform)
  - `backend_list_templates` / `backend_get_template` / `backend_save_template` → filesystem
  - `backend_socket_ready` → always `true` (direct IPC, no socket)
  - `backend_upload` / `backend_load_gpx` → Phase 2
- `debug/profiler.rs` extended: `encoder.queue_wait`, `ffmpeg.write`, `frame.total`, `frame.draw`, `render.pixel_extract` — format identical to Python `timing_summary.json`.
- **Python sidecar deletion**: remove `backend/`, `compile_sidecar.sh`, sidecar entry in `src-tauri/tauri.conf.json`, sidecar spawn in `lib.rs`.
- Update `backend_migration_assessment.md` with benchmark comparison (Python phase_1 vs Rust phase_5).

### Manual Test
1. Full render with labels, values, course, elevation — `.mov` saved to `~/Downloads/Cyclemetry/`.
2. Open in video player — alpha channel present, all widgets animate correctly, values update per frame.
3. Compare `debug_render/phase_5/timing_summary.json` vs `phase_1`:
   - `frame.draw` avg < 20ms (Python baseline ~35ms)
   - `encoder.serialize` bucket absent (eliminated — Skia writes directly to `Vec<u8>`)
   - `render.pixel_extract` avg < 2ms (Skia `peek_pixels` is zero-copy)
4. Cancel mid-render — stops cleanly, no partial `.mov` on disk.
5. Test on Windows 10 and macOS — identical video output.
6. `validate_gpx demo.gpxinit` numeric parity still within tolerance.

---

## Phase 6 — GPU Acceleration (Metal + Vulkan/DirectX 12)

### Goal
Add an optional GPU-accelerated rendering path using Skia's `gpu::DirectContext`. The CPU path from Phases 1–5 remains fully intact as a fallback. The same `Canvas` API is used — no drawing logic changes.

### Key Skia API used in this phase
- `gpu::DirectContext::new_metal()` (macOS) / `gpu::DirectContext::new_vulkan()` (Windows)
- `gpu::surfaces::render_target()` — off-screen GPU surface, same `Canvas` as CPU
- `surface.flush_and_submit()` — flush GPU commands
- `surface.read_pixels()` — GPU → CPU readback for FFmpeg pipe

### Deliverables
- `render/surface.rs` refactored into `SurfaceBackend` enum:
  ```rust
  enum SurfaceBackend {
      Cpu(skia_safe::Surface),
      Metal(gpu::DirectContext, skia_safe::Surface),   // macOS
      Vulkan(gpu::DirectContext, skia_safe::Surface),  // Windows
  }
  ```
- `render/gpu/metal.rs` (macOS, `#[cfg(target_os = "macos")]`): initialise Metal device and queue, create Skia `gpu::DirectContext::new_metal()`.
- `render/gpu/vulkan.rs` (Windows, `#[cfg(target_os = "windows")]`): initialise Vulkan instance, create `gpu::DirectContext::new_vulkan()`.
- `render/surface.rs::try_gpu_surface(width, height) -> Option<SurfaceBackend>`: attempts GPU init. On any failure (missing drivers, headless VM, integrated GPU without Vulkan support), logs a warning and returns `None`. Caller falls back to `SurfaceBackend::Cpu`.
- `render/compositor.rs` updated: `FrameCompositor` accepts `SurfaceBackend`. All draw calls go to the same `canvas` regardless of backend. `render_frame()` calls `flush_and_submit()` before `read_pixels()` on GPU path.
- `debug/profiler.rs` extended: `render.gpu_flush`, `render.gpu_readback` buckets added.
- Config flag `scene.gpu: bool` (optional, default `true`) — allows users to opt out of GPU via template JSON.
- App startup log: `"Rendering backend: GPU (Metal)"` / `"GPU (Vulkan)"` / `"CPU (fallback)"`.

### Manual Test
1. Run on macOS with Metal — startup log confirms `GPU (Metal)`.
2. Run on Windows 10 with dedicated GPU — startup log confirms `GPU (Vulkan)`.
3. Run in a VM or on a machine with no Vulkan support — startup log confirms `CPU (fallback)`, render completes successfully.
4. Set `"gpu": false` in template JSON — forces CPU path even on GPU machine.
5. Compare `debug_render/phase_6/timing_summary.json` vs `phase_5`:
   - `frame.draw` avg: expect 1–3ms GPU vs 10–15ms CPU
   - `render.gpu_readback` avg: expect 2–5ms for 4K frame
   - `frame.total` still dominated by FFmpeg (`prores_ks` software encoder)
6. Full render on both backends — video files must be binary-identical (same pixel values).

---

## Crate Dependencies

```toml
# cyclemetry_core/Cargo.toml
[dependencies]
skia-safe = { version = "0.75", features = ["gl", "vulkan", "metal"] }
roxmltree   = "0.19"       # GPX + Garmin extension parsing
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
tokio       = { version = "1", features = ["sync", "rt-multi-thread"] }
open        = "5"          # Cross-platform file/folder opener
log         = "0.4"

# GPU feature-gated
[target.'cfg(target_os = "macos")'.dependencies]
metal = "0.28"             # Apple Metal bindings (for Skia GPU context)

[target.'cfg(target_os = "windows")'.dependencies]
ash = "0.38"               # Vulkan bindings (for Skia GPU context)
```

> **Build note:** `skia-safe` compiles Skia from C++ source via `cc`. First build takes 5–10 minutes. Subsequent incremental builds are fast. Prebuilt binaries via `skia-binaries` are available to skip the C++ compile in CI.

---

## Timing Profiler Output Format

Rust `debug/profiler.rs` must emit identical JSON to Python `render_debug.py`:

```json
{
  "phase": "phase_5",
  "timestamp": "...",
  "fps": 30, "width": 3840, "height": 2160,
  "total_frames": 900, "rendered_frames": 900,
  "total_time_taken": 45.2,
  "timings": {
    "frame.draw":           { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "composite.route":      { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "composite.elevation":  { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "encoder.queue_wait":   { "count": 901, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "ffmpeg.write":         { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "frame.total":          { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "render.pixel_extract": { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "render.gpu_readback":  { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... }
  }
}
```

---

## Comparison to Vello Plan

| Dimension | Rust + Skia (this plan) | Rust + Vello |
|---|---|---|
| Phases | 6 (GPU is additive) | 5 (GPU required from Phase 3) |
| Text rendering | Native Skia API | Requires Parley integration |
| Shadow rendering | Native `ImageFilter::drop_shadow` | Manual two-pass workaround |
| Route reveal | Native `canvas.clip_path()` | Manual mask texture |
| CPU fallback quality | Excellent | Poor (compute shaders on CPU) |
| iGPU support | Phase 6 (optional) | Phase 3 (always required) |
| Build complexity | Heavy C++ compile (first build) | Pure Rust, fast build |
| Web App potential | Limited (CanvasKit/WASM possible) | Excellent (Vello targets WebGPU) |
| Est. total LOC | 7,000–8,500 | 7,000–9,000 |

**Recommendation:** Choose Skia if CPU rendering performance is sufficient (~10–15ms/frame at 4K) and you want a simpler, more battle-tested rendering API with no hard GPU dependency. Choose Vello if you anticipate the Web App stretch goal becoming a priority — Vello's WASM/WebGPU story is significantly stronger.

---

## Open Questions / Schema Deviations Requiring Approval

None. The following are additions filling gaps in the Python backend — not deviations:
- `border_strength` and `border_distance` (in `config-utils.js`, not rendered in Python `frame.py`)
- `icon_color` on `ValueConfig` (in store schema, not rendered in Python)

Both will be implemented from Phase 3 onwards in the Rust backend.
