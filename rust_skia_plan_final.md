# Rust + Skia Rewrite Plan (CPU-First, Implementation-Ready)

## Summary

Implement the rewrite as a **desktop-only, CPU-rendered Rust backend** embedded in Tauri. Do **not** include GPU rendering in the initial rewrite. The rewrite keeps `ProRes 4444` output via FFmpeg, replaces the Python sidecar completely only after parity is proven, and adopts the **frontend-parsed `parsedActivity` payload as the sole activity input contract**.

The original benchmarks baseline was performed with this command (seconds 600-630 of the activity were rendered):

```powershell
uv run main.py render -gpx "uploads\2025-04-21_2180810019_Velkonocne blbnutie z Zugu.gpx" -template ..\templates\safa_brian_a_4k_gradient.json
```

Use the same GPX/template pair for all end-to-end timing comparisons unless a reduced compatibility template is explicitly needed for an earlier phase.

## Canonical Interfaces

### 1. Activity ownership and contract

Use `parsedActivity` from the frontend as the only backend input. The Rust backend must accept the current frontend payload shape from [activityParserUtils.js](/h:/tools/cyclemetry/app/src/api/activityParserUtils.js:569), including:

- core arrays used today: `sample_elapsed_seconds`, `sample_distance_progress`, `sample_course_points`, `sample_elevations`, `course`, `elevation`, `speed`, `heartrate`, `cadence`, `power`, `temperature`, `gradient`, `time`
- metadata and trim fields: `metadata`, `trim_start_seconds`, `trim_end_seconds`, `source_start_time`
- passthrough tolerance for extended metrics currently emitted by the frontend but not rendered by the backend

Decision:

- The frontend remains the parser of GPX/FIT.
- Rust becomes the canonical implementation for **trim and frame-rate interpolation**.
- Rust must **not** apply a second smoothing pass in the first rewrite.
- Validation compares Rust dense arrays against the current Python backend behavior for the same exported `parsedActivity` fixture, not against raw file parsing.

### 2. Template/config schema

Implement a serde schema that matches the **actual current template/config surface**, not the reduced schema in the existing plan. The Rust config layer must support:

- `scene`: `width`, `height`, `fps`, `start`, `end`, `font`, `font_size`, `color`, `decimal_rounding`, `overlay_filename`, `ffmpeg`, optional `opacity`, optional `scale`
- `labels`: `text`, `x`, `y`, `font`, `font_family`, `font_size`, `color`, `opacity`, `shadow_color`, `shadow_strength`, `shadow_distance`, `border_color`, `border_thickness`, `border_strength`, `border_distance`
- `values`: all fields used by the current frontend/template system, including `value`, `x`, `y`, `font`, `font_family`, `font_size`, `color`, `opacity`, `suffix`, `unit`, `hours_offset`, `time_format`, `format`, `decimal_rounding`, `decimals`, `show_icon`, `icon_color`, `icon_size`, `icon_offset_x`, `icon_offset_y`, `show_units`, `speed_unit`, `temperature_unit`, `value_offset`, `triangle_positive_color`, `triangle_negative_color`, `show_sign`, `show_triangle`, `triangle_width`
- `plots.course`: `x`, `y`, `width`, `height`, `rotation`, `opacity`, `color`, `completed_line_width`, `completed_line_color`, `completed_line_opacity`, `remaining_line_width`, `remaining_line_color`, `remaining_line_opacity`, `marker_size`, `marker_color`, `marker_opacity`
- `plots.elevation`: the course fields plus `area_completed_color`, `area_completed_opacity`, `area_remaining_color`, `area_remaining_opacity`, `show_elevation_metric`, `show_elevation_imperial`, `y_scale`, `metric_label_offset_x`, `metric_label_offset_y`, `imperial_label_offset_x`, `imperial_label_offset_y`

Decision:

- Unknown fields are preserved on read/write boundaries only where needed for template round-tripping.
- Rust rendering ignores unsupported fields only if they are provably unused by current frontend templates and backend output.
- Legacy template fields like nested `line`, `fill`, `points`, and ad hoc keys from existing bundled templates remain accepted in deserialization and normalized into canonical internal structs.

### 3. Tauri command API

Replace socket/HTTP backend calls with direct Rust commands in `src-tauri/src/lib.rs` while preserving the command names already used by the frontend:

- `backend_health() -> { status, message, ready }`
- `backend_demo(config_json, parsed_activity_json, second) -> { filename }`
- `backend_render(config_json, parsed_activity_json) -> { filename? | started }`
- `backend_progress() -> RenderProgress`
- `backend_cancel()`
- `backend_list_templates()`, `backend_get_template()`, `backend_save_template()`
- `backend_open_downloads()`, `backend_open_video(filename)`
- `backend_socket_ready() -> true`

Decision:

- `backend_load_gpx` and `backend_upload` become compatibility no-ops or thin file-copy helpers only if the frontend still calls them.
- The frontend command payload must be updated to pass `parsedActivity` to demo/render explicitly.

## Implementation Plan

### Phase 1: Foundation and seam replacement

- Create `src-tauri/cyclemetry_core` with modules: `activity`, `config`, `render`, `encode`, `debug`, `commands`.
- Move FFmpeg resolution logic from Python into `encode/ffmpeg.rs`, preserving search order from [scene.py](/h:/tools/cyclemetry/backend/scene.py:20).
- Rewire `src-tauri/src/lib.rs` so all backend commands call Rust directly and no sidecar is spawned.
- Keep all render commands stubbed except `backend_health`.

Acceptance:

- App starts without sidecar.
- No port/socket probing remains.
- Frontend command names remain unchanged.

Test procedure:

1. Run:
   ```powershell
   cargo tauri dev
   ```
2. Verify:
   - app starts
   - no `cyclemetry-server` child process appears
   - `backend_health` returns JSON from Rust
3. Run:
   ```powershell
   netstat -an | findstr 31337
   ```
   Expected: no listener on `31337`.
4. Record no image/video output for this phase. The output artifact is only the health response and absence of sidecar/socket behavior.

### Phase 2: Activity processing and parity harness

- Implement `activity/schema.rs` for the actual frontend `parsedActivity`.
- Implement `activity/trim.rs` and `activity/interpolate.rs` to match Python behavior in [backend/activity.py](/h:/tools/cyclemetry/backend/activity.py:312) and [backend/activity.py](/h:/tools/cyclemetry/backend/activity.py:415):
  - trim by elapsed seconds
  - insert interpolated boundary samples at trim start/end
  - build dense frame arrays using `fps`, `scene.start`, `scene.end`
  - interpolate numeric series linearly
  - interpolate `course` as separate lat/lon arrays
  - derive dense timestamps from `source_start_time`
- Frontend currently contains basic on-demand interpolation around the currently played second; not a global interpolation.
- Do not re-smooth elevation/gradient in Rust in this phase.
- Add a `validate_activity` CLI that accepts a frontend debug payload and writes dense Rust arrays for diffing.
- Add fixture-based numeric parity tests against known debug payloads exported from the current app.

Acceptance:

- Dense arrays match Python output within explicit per-series tolerances.
- Frame counts match Python for the same trim/fps.
- `backend_demo` can consume config + parsedActivity without touching GPX/FIT files.

Test fixtures to generate before comparison:

1. In the app, load at least two activities:
   - the main benchmark GPX
   - one shorter or flatter activity to catch edge cases
2. Ensure the frontend writes parse debug payloads via `writeParseDebugFile`; these land under `app/debug/`.
3. Keep at least these fixture classes:
   - full activity, no trim
   - trimmed activity with non-integer start/end
   - activity with sparse/null metric coverage if available

Python baseline generation:

1. Add a temporary non-mutating helper script or CLI invocation in the Python backend that:
   - reads a frontend `parsedActivity` debug payload
   - runs the current Python trim/interpolation logic
   - writes `python_dense_activity.json`
2. For each fixture/config pair, run:
   ```powershell
   uv run python backend\tools\validate_activity_baseline.py --payload app\debug\<fixture>.json --config templates\safa_brian_a_4k_gradient.json --out tmp\python_dense_<fixture>.json
   ```
3. Expected output file shape:
   ```json
   {
     "frame_count": 900,
     "frame_elapsed_seconds": [...],
     "frame_distance_progress": [...],
     "series": {
       "speed": [...],
       "elevation": [...],
       "gradient": [...],
       "heartrate": [...],
       "cadence": [...],
       "power": [...],
       "temperature": [...],
       "course_lat": [...],
       "course_lon": [...],
       "time": [...]
     }
   }
   ```

Rust output generation:

1. Run:
   ```powershell
   cargo run --bin validate_activity -- --payload app\debug\<fixture>.json --config templates\safa_brian_a_4k_gradient.json --out tmp\rust_dense_<fixture>.json
   ```
2. Expected output shape must match the Python baseline shape exactly enough for structural diffing.

Comparison command:

1. Run:
   ```powershell
   uv run python backend\tools\compare_dense_activity.py --left tmp\python_dense_<fixture>.json --right tmp\rust_dense_<fixture>.json --report tmp\dense_diff_<fixture>.json
   ```
2. Expected report shape:
   ```json
   {
     "frame_count_equal": true,
     "series": {
       "speed": { "max_abs_error": 0.0001, "max_rel_error": 0.00001, "mismatch_count": 0 },
       "course_lat": { "max_abs_error": 0.000001, "mismatch_count": 0 }
     },
     "pass": true
   }
   ```

Required comparison rules:

- `frame_count` must match exactly.
- `frame_elapsed_seconds` and `frame_distance_progress`:
  - exact length match
  - max absolute error <= `1e-6`
- `speed`, `elevation`, `heartrate`, `cadence`, `power`, `temperature`, `gradient`:
  - max relative error <= `1e-4`
  - max absolute error <= `1e-3`
- `course_lat`, `course_lon`:
  - max absolute error <= `1e-6`
- `time`:
  - ISO timestamps must match exactly after truncating to milliseconds, or epoch-second delta must be <= `1e-3`

Extra acceptance cases:

- Run the same payload with at least:
  - `fps=24`
  - `fps=30`
  - `fps=60`
- Run one non-integer trim case:
  - `scene.start=600.25`
  - `scene.end=629.75`
- Compare resulting `frame_count`, first frame values, last frame values, and 5 evenly sampled indices.

Recorded artifacts for Phase 2:

- `tmp/python_dense_<fixture>.json`
- `tmp/rust_dense_<fixture>.json`
- `tmp/dense_diff_<fixture>.json`

### Phase 3: Skia text/value renderer

- Add `render/surface.rs` using CPU raster `skia-safe` surfaces only.
- Implement `render/text.rs` for:
  - font resolution from bundled/user fonts with stable fallback
  - fill color and opacity
  - value formatting compatible with current Python and frontend expectations
  - border stroke and shadow rendering
- Implement `render/format.rs` to centralize value formatting:
  - speed unit conversion from m/s to `kmh` or `mph`
  - temperature unit conversion
  - time formatting from timestamps
  - gradient decimals and sign handling
  - scene/global decimal rounding fallback behavior
- Implement base-layer caching for static labels.
- Implement `backend_demo` to output a PNG preview using Rust.

Decision:

- Text parity target is visual parity, not byte-identical glyph rasterization.
- Missing exact Pillow metrics are handled by matching baseline screenshots within a small positional tolerance.

Acceptance:

- Preview frames for labels/values visually match current output.
- Fonts, color, shadow, border, units, and time formatting are correct on bundled templates.

Test procedure:

1. Generate Python baseline preview frames at fixed seconds:
   ```powershell
   uv run main.py demo -gpx "uploads\2025-04-21_2180810019_Velkonocne blbnutie z Zugu.gpx" -template ..\templates\safa_brian_a_4k_gradient.json -second 600
   ```
   Repeat for `607`, `615`, `622`, `629`.
2. Save or copy the resulting preview PNGs into:
   - `tmp/python_preview_600.png`
   - `tmp/python_preview_607.png`
   - etc.
3. Generate matching Rust preview frames through `backend_demo` or a dedicated CLI:
   ```powershell
   cargo run --bin render_preview -- --config templates\safa_brian_a_4k_gradient.json --payload app\debug\<fixture>.json --second 600 --out tmp\rust_preview_600.png
   ```
4. Produce an image diff report:
   ```powershell
   uv run python backend\tools\compare_images.py --left tmp\python_preview_600.png --right tmp\rust_preview_600.png --report tmp\preview_diff_600.json
   ```
5. Expected numeric report:
   ```json
   {
     "dimensions_equal": true,
     "mean_abs_channel_error": 1.8,
     "max_abs_channel_error": 24,
     "changed_pixel_ratio": 0.012,
     "pass": true
   }
   ```

Required automated thresholds for Phase 3:

- identical image dimensions
- changed pixel ratio <= `2%`
- mean absolute channel error <= `3`
- no large bounding-box drift on text blocks if a bbox-based comparator is available

Visual review remains mandatory for:

- font substitution
- shadow softness
- border thickness
- time/unit formatting

Recorded artifacts for Phase 3:

- paired PNGs for all 5 sample seconds
- JSON diff reports per second

### Phase 4: Route and elevation widgets

- Port route/elevation cache-building from Python `Scene.prepare_render_assets()` and related geometry code in [scene.py](/h:/tools/cyclemetry/backend/scene.py:1872).
- Keep the same architecture the Python backend already uses:
  - precompute route/elevation geometry once
  - pre-render static backgrounds once
  - precompute per-frame marker/progress state
  - redraw only dynamic reveal + marker + elevation labels each frame
- Route implementation:
  - preserve aspect ratio
  - support `rotation`
  - render completed and remaining paths as separate styles
  - render marker circle first; marker image support may remain optional until a template requires it
- Elevation implementation:
  - support completed and remaining fill areas
  - support completed and remaining lines
  - support marker
  - support `y_scale`
  - support metric and imperial marker labels with offsets
- Use the same dirty-region philosophy as the current Python renderer only if profiling shows it still matters in Rust CPU Skia.

Decision:

- Bucket-mask optimization from Python is optional in the first Rust implementation.
- Elevation label values come from the same interpolated elevation series used to plot the line.

Acceptance:

- Route/elevation frames at 0/25/50/75/100% match current output visually.
- Rotation and y-scale behave correctly.
- No field used by current course/elevation widget editors is silently ignored.

Test procedure:

1. Generate Python baseline sample frames for frame indices already used by the current debug path:
   - `0%`
   - `25%`
   - `50%`
   - `75%`
   - `100%-1 frame`
2. Enable current Python debug output so `sample_frame_indices` and sample images are saved under `backend/debug_render/...` using existing `RenderDebugOptions`.
3. Generate equivalent Rust sample frames to:
   - `tmp/route_elev_python_000.png`
   - `tmp/route_elev_rust_000.png`
   - etc.
4. Run:
   ```powershell
   uv run python backend\tools\compare_images.py --left tmp\route_elev_python_000.png --right tmp\route_elev_rust_000.png --report tmp\route_elev_diff_000.json
   ```
5. Generate geometry comparison outputs if available:
   - Rust writes `route_geometry_data.json` and `elevation_geometry_data.json`
   - compare extents, point counts, and marker positions against Python debug artifacts
6. Generate Rust preview frames at fixed seconds 600, 607, 615, 622, 629 using parsed activity "velkonocne blbnutie z Zugu" in app/debug and:

```powershell
cargo run --bin render_preview -- --config templates\safa_brian_a_4k_gradient.json --payload app\debug\<fixture>.json --second 600 --out tmp\rust_preview_600.png
```

7. Generat Python preview frames at fixed seconds 600, 607, 615, 622, 629 using:
   Ensure this generate the timing_summary.json in backend/debug_render which will be used as a baseline.

```powershell
   uv run main.py demo -gpx "uploads\2025-04-21_2180810019_Velkonocne blbnutie z Zugu.gpx" -template ..\templates\safa_brian_a_4k_gradient.json -second 600
```

Required non-visual checks for Phase 4:

- route marker position delta <= `3 px` on all sampled frames
- elevation marker position delta <= `3 px`
- route/elevation widget output dimensions exactly match template
- rotation angle used in render metadata matches template
- geometry bounding boxes match within `2 px`

Recorded artifacts for Phase 4:

- sampled Python/Rust PNGs
- image diff JSONs
- route/elevation geometry JSONs
- marker position comparison JSON

### Phase 5: Full encode pipeline and cutover

- Implement render loop plus FFmpeg pipe in Rust, preserving the proven Python queueing model from [scene.py](/h:/tools/cyclemetry/backend/scene.py:204):
  - bounded frame queue
  - dedicated writer thread
  - stderr progress monitor
  - cancel flag
- Static label cache must store a render-ready in-memory representation, not PNG-encoded bytes
- Preview PNG rendering remains decoupled from production encode path
- Write raw `rgba` frames to FFmpeg stdin exactly as today.
- Preserve template-configurable FFmpeg options under `scene.ffmpeg`.
- Implement dropdown menu in global settings of frontend to allow selection of different codecs with alpha channel. Primary one will be prores_ks (Prores4444), the other options must include HEVC(H.265), prores_videotoolbox (if device is macOS), and prores_vulkan (ensure FFMPEG8.1 is used, and see the source code to derive the invocation: https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/vulkan_prores.c)
- Runtime timings must mirror the Python baseline lifecycle rather than arbitrary Rust internals:
  - one-time preparation goes to `prepare_render_assets_timing.json`, including `create_base_image`, route/elevation cache setup, static label cache population, and `prepare_render_assets.total`
  - per-frame steady-state work goes to `timing_summary.json`, with top-level `frame.draw` wrapping only work that Python measured inside `Frame.draw(...)`
  - restoring the pre-rendered static layer must map to Python `base.restore`/`base.copy`, not to `text.static.cache`
  - dynamic text drawing must map to Python `text.dynamic`
  - route/elevation compositing and elevation point labels must keep the Python bucket names `composite.route`, `composite.elevation`, and `text.elevation_label`
  - queue backpressure and encoder blocking must keep the Python bucket names `queue.put_wait`, `encoder.queue_wait`, and `ffmpeg.write`
  - any preview-only work such as PNG encoding, preview surface allocation, or debug image writing must be reported separately under a clearly non-baseline namespace and excluded from Rust-vs-Python timing comparisons
- Implement progress reporting and cancellation semantics compatible with the current frontend. Make progress reporting more intelligent so that it does not include the initialization/cold start and perhaps include some sort of moving average/exponential weighing/whatever industry standard for estimate completion time would be.
- Keep output locations and open-folder/open-video behavior unchanged.

Decision:

- Success criterion is improved end-to-end wall-clock time on the same machine and template.
- Python sidecar is deleted only after the parity suite passes and at least one full 4K template render passes on Windows and macOS.

Acceptance:

- Full render succeeds for the reference 4K template.
- Alpha channel survives in output.
- Progress and cancellation work from the UI.
- End-to-end time is materially better than the current Python baseline or at minimum no worse while the sidecar is removed.

Test procedure:

1. Python baseline render:
   ```powershell
   uv run main.py render -gpx "uploads\2025-04-21_2180810019_Velkonocne blbnutie z Zugu.gpx" -template ..\templates\safa_brian_a_4k_gradient.json
   ```
2. Collect artifacts from the latest Python debug directory:
   - `timing_summary.json`
   - `prepare_render_assets_timing.json`
   - sample frames
3. Rust full render:
   ```powershell
   cargo run --bin render_video -- --config templates\safa_brian_a_4k_gradient.json --payload app\debug\<fixture>.json --debug-phase phase_5 --out tmp\rust_overlay.mov
   ```
4. Collect Rust artifacts:
   - `timing_summary.json`
   - sample frames
   - final `.mov`

Non-visual comparisons:

1. Compare timing JSONs:
   ```powershell
   uv run python backend\tools\compare_timing_summary.py --left backend\debug_render\phase_1\<python_run>\timing_summary.json --right backend\debug_render\phase_5\<rust_run>\timing_summary.json --report tmp\timing_diff.json
   ```
2. Verify:
   - `total_frames` equal
   - `rendered_frames` equal
   - `overlay_filename` present
   - Rust has no `encoder.serialize` bucket
3. Compare output container properties:
   ```powershell
   ffprobe -v error -show_streams -show_format -of json tmp\rust_overlay.mov > tmp\rust_ffprobe.json
   ffprobe -v error -show_streams -show_format -of json <python_output.mov> > tmp\python_ffprobe.json
   ```
4. Compare:
   - codec name
   - width/height
   - frame rate
   - duration
   - pixel format
   - presence of alpha-compatible output format

Cancel test:

1. Start a Rust render.
2. Trigger `backend_cancel` after at least 10% progress.
3. Verify:
   - status becomes `cancelled`
   - no partial output file remains
   - ffmpeg subprocess exits

Recorded artifacts for Phase 5:

- Python/Rust `timing_summary.json`
- `tmp/timing_diff.json`
- Python/Rust `ffprobe` JSON
- final Rust `.mov`

### Optional Phase 6: Browser-hosted build from the same repo

This phase is optional and does not change the success criteria for Phases 1-5. Its goal is to support two build targets from the same repository:

- desktop app: current Tauri shell with native Rust binary
- hosted web app: browser UI with shared Rust data/geometry core plus a browser-specific CanvasKit renderer

Decision:

- Reuse the Rust core for non-rendering logic and rendering preparation, but do not assume native `rust-skia` is the browser rendering path.
- Use CanvasKit as the browser renderer because Skia's official web deployment path is CanvasKit rather than native `rust-skia`.
- Code splitting in the frontend is recommended for startup and bundle size, but the required architectural split is primarily:
  - shared Rust core for activity/config/render preparation
  - native `rust-skia` renderer for Tauri/desktop
  - browser-specific CanvasKit renderer for hosted web
- Browser export must not depend on desktop-only process spawning, filesystem assumptions, or FFmpeg stdin piping.
- Browser encode/output format may differ from desktop if required by browser APIs. Desktop ProRes 4444 remains the native target; web export uses the best browser-supported alpha-capable path available at implementation time.

Implementation:

- Refactor `cyclemetry_core` into target-aware layers:
  - `core`: activity processing, config normalization, value formatting, route/elevation geometry preparation, frame orchestration
  - `render_model`: target-neutral render instructions and prepared geometry with no dependency on Tauri, Skia native surfaces, or browser APIs
  - `host_native`: FFmpeg process, native file IO, OS integration
  - `host_web`: `wasm-bindgen` entrypoints, browser memory transfer, worker messaging, browser download/export helpers
- Keep desktop-specific commands in `src-tauri/src/lib.rs`; add a separate wasm crate or wasm target entrypoint that exposes:
  - `init_renderer(config_json, parsed_activity_json)`
  - `build_preview_scene(second) -> render instruction payload`
  - `build_frame_batch(start_frame, count) -> transferable render instruction payload`
  - `cancel_render()`
- Move long-running browser render work off the main thread:
  - run wasm preparation logic inside a Web Worker
  - transfer render-model payloads or compact geometry/series buffers via `postMessage` with transferable `ArrayBuffer`s
  - keep UI preview/progress in the existing app shell
- Make the browser renderer consume the shared render model using CanvasKit:
  - map text, paths, fills, markers, opacity, transforms, and cached static layers onto CanvasKit APIs
  - load and register the same fonts in the browser where licensing and packaging allow
  - keep visual parity targets at the level of rendered output, not identical renderer internals
- Isolate rendering backend assumptions so browser builds can swap native-only pieces cleanly:
  - avoid direct use of `std::fs`, subprocess APIs, and blocking thread models inside shared render code
  - gate target-specific modules with Cargo features and `cfg(target_arch = "wasm32")`
  - keep image/frame intent as an explicit intermediate representation so desktop `rust-skia` and browser CanvasKit can consume the same prepared scene
- Add a web export path that consumes rendered frames in-browser:
  - prefer browser-native encoding APIs such as WebCodecs where supported
  - provide a fallback path such as PNG frame sequence or canvas-recorded video if required
  - keep export capability detection explicit in the UI so unsupported browser/codec combinations fail predictably
- Update the frontend build so web-only heavy modules load lazily:
  - lazy-load CanvasKit and wasm preparation modules only when preview or export is requested
  - keep editor-only routes/components separate from render/export worker code and CanvasKit bootstrap code
  - load desktop-only API wrappers only in the Tauri build

Acceptance:

- The repo can produce both:
  - a Tauri desktop build using the native Rust path
  - a hosted web build using shared Rust wasm preparation plus CanvasKit rendering
- Shared fixtures produce matching dense activity outputs between native and wasm builds within existing Phase 2 tolerances.
- Shared preview frames from desktop `rust-skia` and browser CanvasKit are visually equivalent within Phase 3/4 image-diff thresholds, allowing small documented text/raster differences.
- Browser preview remains responsive while rendering is active.
- Browser export completes without server-side rendering.
- Desktop-only integrations are not bundled into the hosted web build.

Test procedure:

1. Build desktop target:
   ```powershell
   cargo tauri build
   ```
2. Build web target from the same repo:
   ```powershell
   cd app
   pnpm build
   ```
   and ensure the wasm artifact is included only in the hosted web output path.
3. For one shared fixture/config pair, generate dense outputs from:
   - native Rust
   - wasm Rust in a browser test harness
4. Compare numeric outputs with the existing Phase 2 comparison tooling and tolerances.
5. Generate browser and desktop preview frames for the same sample seconds:
   - `600`
   - `607`
   - `615`
   - `622`
   - `629`
6. Compare browser/native preview frames with the same image diff tooling used earlier.
7. Run a manual browser export check in at least:
   - Chromium-based browser
   - Safari if macOS support is required
8. Verify:
   - render work happens in a worker, not the UI thread
   - cancellation stops further frame production
   - produced artifact downloads successfully
   - unsupported encode paths surface a clear UI message

Recorded artifacts for Optional Phase 6:

- wasm/native dense JSON comparisons
- desktop `rust-skia` vs browser CanvasKit preview PNGs or equivalent captured frames
- bundle analysis showing deferred wasm/render chunks
- browser export capability matrix by browser
- manual test notes for cancellation, responsiveness, and artifact download

### Deferred phase: GPU Skia

Explicitly defer GPU Skia. Do not include it in the initial rewrite plan, code layout, or acceptance criteria beyond leaving room for a future `SurfaceBackend` abstraction.

## Test Plan

- Numeric parity tests:
  - trim boundary insertion
  - frame count for multiple fps/start/end combinations
  - interpolation of `speed`, `course`, `elevation`, `time`, `gradient`
- Render regression tests:
  - fixed preview frames for at least one bundled template and one widget-heavy editor-generated template
  - 0/25/50/75/100% snapshots for route/elevation
- Manual desktop checks:
  - Windows 10/11 and macOS
  - no sidecar process
  - no socket/port dependency
  - output opens in an editor/player with alpha preserved
- Performance checks:
  - compare Rust and Python on the same `safa_brian_a_4k_gradient.json`
  - capture total render time, frame draw average, FFmpeg write average

## Assumptions and defaults

- The rewrite target is **desktop export parity**, not browser reuse.
- `parsedActivity` from the frontend is trusted input and the only activity source for rendering.
- Rust CPU Skia is the only renderer in v1 of the rewrite.
- `prores_ks` remains the default encoder
- Template compatibility includes current bundled templates and current frontend-generated templates, even when they use both legacy and newer field shapes.
- Unknown or weakly specified visual differences are resolved in favor of matching the current shipped Python output, not the editor preview.
