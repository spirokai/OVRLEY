# Implementation Plan: Rust + Vello/WGPU Backend Migration

## Background

The current backend is a Python Flask sidecar (5,541 LOC) bundled via PyInstaller, communicating with the Tauri host over TCP/Unix socket. This plan migrates it to a native Rust binary embedded directly in the Tauri host process, using Vello/WGPU for GPU-accelerated frame rendering and piping raw RGBA bytes to FFmpeg for ProRes 4444 encoding.

**Platforms:** Windows 10+ (DirectX 12 / Vulkan), macOS 12+ (Metal)  
**Codec:** ProRes 4444 (hard requirement, software `prores_ks` initially, `prores_videotoolbox` where available)  
**Schema:** Existing JSON config schema preserved. No deviations without explicit approval.  
**Cutover:** Hard cutover once feature parity is reached. Python sidecar remains unchanged and runnable throughout migration.  
**Regression baseline:** Existing `debug_render/` timing JSON and sample frames serve as the benchmark for each phase.

---

## Architecture Overview

```
Tauri Host (Rust)
├── IPC Commands (existing lib.rs, unchanged)
│   ├── backend_render
│   ├── backend_demo
│   ├── backend_progress
│   └── ... (all existing commands)
└── [NEW] cyclemetry_core (Rust library crate)
    ├── gpx/          — GPX parsing, interpolation, smoothing
    ├── config/       — JSON config deserialization (serde)
    ├── render/       — Vello scene builder, frame compositor
    │   ├── text/     — Parley text layout module
    │   ├── route/    — Map/course path renderer
    │   └── elevation/— Elevation graph renderer
    ├── encode/       — FFmpeg pipe manager
    └── debug/        — Timing profiler (mirrors render_debug.py)
```

The Python sidecar is deleted only at the end of Phase 5, after full validation.

---

## Phase 1 — Rust Crate Scaffold & IPC Rewire

### Goal

Create the `cyclemetry_core` Rust library crate, wire it into Tauri's `src-tauri`, and replace the Flask HTTP/socket layer with a direct Tauri command dispatcher. The sidecar process is no longer spawned; Tauri commands call Rust functions in-process.

### Deliverables

- `src-tauri/cyclemetry_core/` crate with `Cargo.toml`
- `config/` module: `serde` structs mirroring the full JSON config schema (see Schema Reference below)
- `encode/ffmpeg.rs`: FFmpeg binary resolver (mirrors `scene.py::resolve_ffmpeg_binary`)
- `debug/profiler.rs`: timing profiler mirroring `RenderProfiler` / `TimingBucket` from `render_debug.py`
- Modified `src-tauri/src/lib.rs`: Tauri commands call `cyclemetry_core` functions directly instead of making HTTP requests
- Stub implementations returning `{"status": "not_implemented"}` for all endpoints

### Config Schema Reference (serde structs to implement)

Derived directly from `store-utils.js::DEFAULT_CONFIG`, `config-utils.js`, `template-snapshot.js`, and `constant.py`:

```
SceneConfig       { width, height, fps, start, end, font, color, font_size,
                    overlay_filename, scale?, decimal_rounding?, render_debug? }
LabelConfig       { text, x, y, font?, font_size?, color?, opacity?,
                    shadow_color?, shadow_strength?, shadow_distance?,
                    border_color?, border_thickness?, border_strength?, border_distance? }
ValueConfig       { value: AttributeKind, x, y, font?, font_size?, color?, opacity?,
                    suffix?, unit?, hours_offset?, time_format?, decimal_rounding?,
                    icon_color?, shadow_*, border_* }
PlotConfig        { value: AttributeKind, x, y, width, height, color?, opacity?,
                    rotation?, line { width, color }?, points[]?, fill?,
                    marker?, label?, shadow_*, render_mode? }
RootConfig        { scene: SceneConfig, labels: [], values: [], plots: [] }
AttributeKind     enum { Course, Elevation, Speed, Heartrate, Cadence,
                          Power, Temperature, Time, Gradient }
```

### Manual Test

1. Build the Tauri app (`cargo tauri dev`).
2. Open the app — it must load without spawning `cyclemetry-server`.
3. Call `backend_health` from the frontend — it must return `{"status":"ok","ready":false}` from Rust.
4. Call `backend_render` — it must return `{"status":"not_implemented"}` without crashing.
5. Confirm no TCP port 31337 is bound (`netstat -an | findstr 31337` on Windows, `lsof -i :31337` on macOS — both must return nothing).

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

## Phase 3 — Vello/WGPU Rendering Engine (Text & Compositing)

### Goal

Implement the full Vello-based frame compositor. By the end of this phase, the renderer can draw all `labels` (static text) and `values` (dynamic text) to an RGBA surface and save single frames as PNG. No charting yet.

### Deliverables

- `render/surface.rs`: WGPU device initialization with automatic adapter selection (prefer high-performance GPU on both Metal and DirectX 12). CPU software fallback via WGPU's `Gl` backend if no hardware GPU is detected.
- `render/text/layout.rs`: Parley-based text layout module wrapping the Vello `SceneBuilder`. Must support:
  - Custom `.ttf`/`.otf` fonts loaded from the `fonts/` directory
  - Font size, fill color (hex RGBA), opacity
  - Hard/offset shadows (two-pass draw: shadow color at offset, then fill color)
  - Stroke outlines (border_thickness / border_color / border_distance)
  - A `FontCache` keyed on `(path, size)` — initialized once before the render loop
- `render/compositor.rs`: `FrameCompositor` struct implementing the layered composition strategy:
  - `build_base_layer()`: renders all `labels` once to an off-screen Vello surface → `wgpu::Texture`
  - `render_frame(frame_index)`: copies base layer → draws `values` on top → returns raw RGBA `Vec<u8>`
- `render/frame_buffer_pool.rs`: thread-safe pool of pre-allocated `Vec<u8>` buffers (mirrors Python `FrameBufferPool`)
- `backend_demo` command: fully implemented with Vello — renders a real frame with labels and values, saves as PNG.
- `debug/profiler.rs`: extended with `render.base_layer`, `render.frame`, `render.gpu_readback` timing buckets.

### Shadow & Border Implementation Notes

Vello does not have a native CSS-style shadow. Implement as follows:

- **Hard shadow:** draw the text path twice — first at `(x + shadow_distance, y + shadow_distance)` in `shadow_color` with `shadow_strength` as opacity, then at `(x, y)` in the fill color.
- **Soft shadow:** if `shadow_strength > 0` and a blur radius is implied, use a pre-blurred off-screen pass. For Phase 3, hard shadow is acceptable. Soft blur is a Phase 3 stretch goal.
- **Border/outline:** use Vello's stroke path on the glyph outlines with `border_thickness` as stroke width and `border_color` as color.

### Manual Test

1. Load `demo.gpxinit`, configure a template with several `labels` and `values`, click "Preview Frame".
2. Visually compare the generated PNG to the Python-generated preview frame for the same second.
3. Verify font, size, color, and position of all text elements match within 1–2px.
4. Check `debug_render/phase_3/timing_summary.json` — confirm `render.base_layer` fires once and `render.frame` fires once per preview.
5. Test on a machine without a dedicated GPU (integrated Intel graphics) — app must not crash, must fall back gracefully.

---

## Phase 4 — Charting Engine (Route & Elevation Widgets)

### Goal

Implement the two `plots` widget types (`course` and `elevation`) using native Vello path geometry. This fully replaces `matplotlib` and the Python compositing layer in `scene.py` / `frame.py`.

### Deliverables

#### Route (Course) Widget

- `render/route/geometry.rs`:
  - Project `(Lat, Lon)` pairs to 2D pixel coordinates within the widget bounding box using equirectangular projection scaled to fit.
  - Ramer-Douglas-Peucker path simplification (port of current Python simplification logic) with configurable tolerance.
  - Pre-compute `RouteWidgetCache`: background path (full route, low opacity), completed path geometry, `display_points`, per-frame `RouteFrameState` (marker_x, marker_y, segment_index, progress01).
- `render/route/renderer.rs`:
  - Draw background layer (full route path) at `background_opacity * 0.75`.
  - Reveal completed route using a clipping mask up to `segment_index` + interpolated sub-segment point.
  - Draw marker sprite (PNG loaded from config `marker` path, or a default circle).
  - Support `rotation_deg`.
- Bucket mask optimization: pre-compute N bucket overlay textures to avoid per-frame path clipping (mirrors Python `bucket_masks`).

#### Elevation Widget

- `render/elevation/geometry.rs`:
  - Map `(distance_progress, elevation_m)` pairs to pixel coordinates within the widget bounding box.
  - Pre-compute `ElevationWidgetCache`: background filled area path, completed path (revealed left-to-right), per-frame `ElevationFrameState` (progress01, marker_x, marker_y, label_text).
  - Apply unit conversions (metric/imperial) and `decimal_rounding`.
- `render/elevation/renderer.rs`:
  - Draw filled background area (full elevation profile at low opacity).
  - Reveal completed portion using horizontal clip up to `progress01 * width`.
  - Draw moving marker.
  - Draw dynamic elevation label (uses `render/text/layout.rs`).
  - Support `rotation_deg`.

#### Integration

- `render/compositor.rs` updated: `build_base_layer()` now includes route and elevation background layers. Per-frame step composites the reveal masks and markers.
- `backend_render` command: stub — accepts the request and returns progress but does not yet encode video.

### Manual Test

1. Load a GPX with elevation data, configure a template with both a `course` and `elevation` plot.
2. Scrub through preview frames — route reveal and elevation marker must animate correctly.
3. Compare Rust preview frames against Python preview frames at 0%, 25%, 50%, 75%, 100% of activity duration. Route path shape and elevation profile shape must match within visual tolerance.
4. Test `rotation_deg: 90` on both widgets — must rotate correctly.
5. Check `debug_render/phase_4/timing_summary.json` — confirm `composite.route` and `composite.elevation` timing buckets are present and faster than Python equivalents.

---

## Phase 5 — FFmpeg Encoding, Progress & Full Render Pipeline

### Goal

Wire the complete render pipeline into a full video encode. Implement all remaining API endpoints. Validate against Python baseline. Delete Python sidecar.

### Deliverables

- `encode/pipeline.rs`: Full render + encode loop:
  - Spawns FFmpeg subprocess with `stdin = Pipe`, `stderr = Pipe`.
  - Input format: `-f rawvideo -pix_fmt rgba -s {width}x{height} -r {fps} -i -`
  - Output: configurable via `scene.ffmpeg` config block (mirrors `scene.py::build_ffmpeg_settings`). Defaults to `prores_ks`, `yuva444p10le`, `profile 4444`.
  - Bounded frame queue (maxsize=4, mirrors Python `frame_queue`) with a dedicated encoder thread consuming frames and writing to FFmpeg stdin.
  - FFmpeg stderr monitor thread parsing `frame=N` lines for encoding progress.
  - Cancel-check hook polled between frames.
- `encode/progress.rs`: Thread-safe `RenderProgress` struct (`current`, `total`, `encoded`, `status`, `message`, `frame_times` rolling window of 20 for ETA smoothing) — mirrors Python `video_render_progress` dict.
- All Tauri commands fully implemented:
  - `backend_render` → triggers full encode pipeline
  - `backend_progress` → returns `RenderProgress` as JSON
  - `backend_cancel` → sets atomic cancel flag
  - `backend_demo` → single frame preview (already done in Phase 3)
  - `backend_open_downloads`, `backend_open_video` → `open::that()` (cross-platform)
  - `backend_list_templates`, `backend_get_template`, `backend_save_template` → filesystem ops
  - `backend_socket_ready` → always returns `true` (no socket needed, direct IPC)
  - `backend_upload` / `backend_load_gpx` → already done in Phase 2
- `debug/profiler.rs`: extended with `encoder.queue_wait`, `encoder.serialize`, `ffmpeg.write`, `frame.total`, `frame.draw` buckets — output format identical to Python `timing_summary.json` so existing tooling works.
- **Python sidecar deletion**: remove `backend/`, `compile_sidecar.sh`, sidecar reference from `src-tauri/tauri.conf.json`, sidecar spawn code from `lib.rs`.
- Update `backend_migration_assessment.md` and `rust_vello_plan.md` with final benchmark comparison.

### Manual Test

1. Load `demo.gpxinit`, apply a full template (labels, values, course, elevation plots), click "Render Video".
2. Confirm render progress bar updates correctly in the UI.
3. Confirm video file is saved to `~/Downloads/Cyclemetry/`.
4. Open the rendered `.mov` in a video player — verify alpha channel is present (transparent background), route and elevation widgets animate correctly, all value labels update per frame.
5. Compare `debug_render/phase_5/timing_summary.json` against `debug_render/phase_1/timing_summary.json`:
   - `frame.draw` avg must be < 20ms (vs Python ~35ms baseline)
   - `encoder.serialize` must be 0ms (eliminated — Vello writes directly to RGBA bytes)
   - `frame.total` avg must be dominated by FFmpeg, not rendering
6. Test render cancellation mid-way — must stop cleanly with no partial file left on disk.
7. Test on Windows 10 (DirectX 12) and macOS (Metal) — both must produce identical video output.
8. Run `validate_gpx demo.gpxinit` and confirm numeric parity with Python output is still within tolerance.

---

## Crate Dependencies

```toml
# cyclemetry_core/Cargo.toml
[dependencies]
vello = "0.3"          # GPU vector renderer
wgpu = "0.20"          # GPU backend abstraction
parley = "0.2"         # Text layout for Vello
swash = "0.1"          # Font shaping (used by Parley)
roxmltree = "0.19"     # Lightweight XML parsing (GPX + Garmin extensions)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync", "rt-multi-thread"] }
bytemuck = "1"         # Safe RGBA byte casting
open = "5"             # Cross-platform file/folder opener
log = "0.4"
```

> **Note:** The `gpx` crate on crates.io does not support Garmin `TrackPointExtension` fields (HR, cadence, power, temperature) natively. `roxmltree` is used for the full GPX parse to ensure extension data is accessible.

---

## Timing Profiler Compatibility

The Rust `debug/profiler.rs` must emit `timing_summary.json` in the identical format to the Python version, enabling direct diff comparisons between Python phase_1 and Rust phase_5 results:

```json
{
  "phase": "phase_5",
  "timestamp": "...",
  "fps": 30,
  "width": 3840,
  "height": 2160,
  "total_frames": 900,
  "rendered_frames": 900,
  "total_time_taken": 45.2,
  "timings": {
    "frame.draw":           { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "composite.route":      { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "composite.elevation":  { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "encoder.queue_wait":   { "count": 901, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "ffmpeg.write":         { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "frame.total":          { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... },
    "render.gpu_readback":  { "count": 900, "total_ms": ..., "avg_ms": ..., "max_ms": ... }
  }
}
```

---

## Open Questions / Schema Deviations Requiring Approval

None identified at this time. The following config fields are present in the frontend schema but not yet rendered by the Python backend — the Rust backend will implement them from the start:

- `border_strength` and `border_distance` (present in `config-utils.js`, not rendered in Python `frame.py`)
- `icon_color` on `ValueConfig` (present in store, not rendered in Python)

These are **additions filling existing gaps**, not schema deviations, and are flagged for awareness only.
