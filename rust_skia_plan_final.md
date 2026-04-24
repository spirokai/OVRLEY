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
- Implement dropdown menu in global settings of frontend to allow selection of different codecs with alpha channel. Primary one will be prores_ks (Prores4444; see the current Python implementation for invocation), the other options must include HEVC(H.265 with alpha), prores_videotoolbox (if device is macOS), and prores_vulkan (ensure FFMPEG8.1 is used, and see the source code to derive the invocation: https://github.com/FFmpeg/FFmpeg/blob/master/libavcodec/vulkan_prores.c)
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

### Phase 6: Performance optimization and parity recovery

This phase is dedicated to eliminating the current Phase 5 regression versus Python and driving the Rust renderer to clear, repeatable wins on both 1080p and 4K templates. The focus is not feature growth. The focus is removing unnecessary full-frame memory work, aligning the runtime lifecycle more closely with the Python architecture where it helps, and proving each optimization with timing artifacts.

Measured motivation from the current Rust implementation:

- 1080p `new_template.json`:
  - `frame.draw` is about `7.5 ms/frame`
  - `surface.clear` is about `3.4 ms/frame`
  - `surface.readback_rgba` is about `4.2 ms/frame`
  - total is about `15.6 ms/frame`
- 4K `safa_brian_a_4k_gradient.json`:
  - `frame.draw` is about `15.9 ms/frame`
  - `surface.clear` is about `14.8 ms/frame`
  - `surface.readback_rgba` is about `16.5 ms/frame`
  - `ffmpeg.write` is about `4.5 ms/frame`
  - total is about `48.9 ms/frame`
- The writer thread spends most of its time in `encoder.queue_wait`, which means the encoder is usually waiting on frame production rather than blocking the renderer.

Decision:

- Treat `surface.clear` and `surface.readback_rgba` as the primary Phase 6 bottlenecks.
- Optimize in descending order of measured impact, not architectural elegance.
- Preserve Phase 5 timing buckets and add new production buckets only when they explain performance without breaking Python comparability.
- Keep preview-only work and debug image writing outside baseline comparisons.
- Do not introduce GPU Skia in this phase.

Implementation:

- Step 1. Establish a locked performance baseline before optimization.
  - Freeze one benchmark matrix:
    - `templates/new_template.json` with `Test_FIT-parse-debug.json`
    - `templates/safa_brian_a_4k_gradient.json` with the same payload
  - Record at least 3 repeated runs per case and save:
    - total wall time
    - `prepare_render_assets_timing.json`
    - `timing_summary.json`
    - ffprobe output
  - Use the median run as the comparison baseline for each optimization step.

- Step 2. Eliminate per-frame full-surface clear where possible.
  - Replace the current `surface.clear` + redraw approach with a reusable base image restore path that mirrors Python `base.restore`.
  - Maintain a render-ready static layer for:
    - transparent background
    - cached labels
    - any other fully static content that does not depend on frame index
  - Evaluate two native strategies:
    - draw a cached base `Image` onto the frame surface every frame
    - render into a reusable caller-owned buffer and restore from a copied pristine base buffer
  - Keep the faster of the two.
  - Requirement:
    - `surface.clear` must disappear from the hot path or fall below `10%` of `frame.total`.

- Step 3. Remove or drastically reduce Skia readback cost.
  - Stop treating the rendered frame as something that must always be copied out with `read_pixels()`.
  - Prototype and compare these approaches:
    - raster surface backed by caller-owned RGBA memory reused across frames
    - bitmap/pixmap-backed drawing where FFmpeg-ready bytes already exist in writable memory
    - direct access to raster pixels via Skia peek/pixmap APIs when safe and stable
  - The target architecture is:
    - render directly into a reusable RGBA frame buffer
    - hand that buffer to the encoder queue without a second full-frame copy
  - Requirement:
    - `surface.readback_rgba` must either disappear or fall below `2 ms/frame` at 1080p and below `6 ms/frame` at 4K.

- Step 4. Introduce explicit reusable frame-buffer pools.
  - Replace per-frame transient pixel ownership with a bounded pool sized to the encoder queue plus one render slot.
  - Reuse:
    - Skia surfaces or bitmaps
    - backing RGBA buffers
    - any scratch memory needed by route/elevation compositing
  - The render thread should acquire a free frame buffer, render into it, enqueue it, and reclaim it after the writer finishes.
  - Requirement:
    - no per-frame heap allocation of full-frame RGBA buffers on steady-state hot path.

- Step 5. Minimize format conversion and alpha conversion work.
  - Audit whether the current raster surface format forces conversion when producing FFmpeg input.
  - Ensure the steady-state render target is as close as possible to the FFmpeg input format:
    - packed `rgba`
    - expected alpha handling
    - no avoidable premultiply/unpremultiply conversions
  - If Skia must use premultiplied alpha internally, measure whether a custom fast conversion on caller-owned buffers is cheaper than `read_pixels()`.
  - Requirement:
    - no hidden full-frame format conversion should remain unmeasured.

- Step 6. Refine base-layer restore architecture rather than redrawing static content.
  - Expand the current static label cache into a full-frame reusable render state:
    - static label layer
    - optional static widget background layers if they are truly frame-invariant
    - static route/elevation layers where only reveals and markers are dynamic
  - Ensure the per-frame draw path does only:
    - restore base
    - dynamic values
    - dynamic route reveal and marker
    - dynamic elevation reveal, marker, and labels
  - Requirement:
    - `frame.draw` at 4K for `safa_brian_a_4k_gradient.json` should stay stable or improve; Phase 6 must not shift work back into draw just to hide it elsewhere.

- Step 7. Keep debug/sample work off the benchmark hot path.
  - Make `debug.sample_frame_write` fully optional and disabled in benchmark mode.
  - Any benchmark CLI or test harness used for Rust-vs-Python timing comparisons must:
    - disable sample PNG writing
    - disable preview-only instrumentation
    - still emit machine-readable timing JSON
  - Requirement:
    - final Phase 6 benchmark numbers must exclude debug frame emission from the critical path.

- Step 8. Improve measurement granularity without breaking comparability.
  - Keep Phase 5 baseline buckets:
    - `frame.draw`
    - `base.restore`
    - `text.dynamic`
    - `composite.route`
    - `composite.elevation`
    - `text.elevation_label`
    - `queue.put_wait`
    - `encoder.queue_wait`
    - `ffmpeg.write`
  - Keep additional Rust-only analysis buckets in a clearly non-baseline namespace or clearly documented production namespace:
    - `surface.create`
    - `surface.restore`
    - `surface.readback_rgba`
    - `buffer.acquire_wait`
    - `buffer.release_wait`
    - `debug.sample_frame_write`
  - Update comparison tooling so Python-vs-Rust timing diffs compare only the shared bucket set by default and report Rust-only buckets separately.

- Step 10. Tune FFmpeg only after frame-production costs are reduced.
  - Continue using `prores_ks` with default Rust fallback:
    - `qscale=4`
    - `threads=0`
  - Only once clear/readback costs have been reduced, run a narrow encoder tuning sweep for:
    - `qscale`
    - `threads`
    - `pix_fmt`
    - `prores_profile`
  - The goal is to confirm that frame production, not encoder tuning, is the main unlock.

- Step 9. Hardening and cutover cleanup.
  - Fix the current output-copy error handling so successful renders are not reported as failed when copying to Downloads is blocked.
  - Keep final output success independent from secondary copy-to-downloads best effort.
  - Only after Phase 6 acceptance passes:
    - remove remaining Python renderer dependencies from the main desktop path
    - keep Python only as a benchmark/parity harness until explicitly retired

Optimization order and expected impact:

1. render into reusable caller-owned RGBA buffers
2. replace `surface.clear` with base restore or base buffer copy
3. eliminate full-frame readback copies
4. reuse frame buffers across bounded queue slots
5. audit and minimize format conversion
6. disable debug work in benchmark path
7. encoder tuning last

Acceptance:

- Rust beats the current Python baseline on the same machine for both:
  - `templates/new_template.json`
  - `templates/safa_brian_a_4k_gradient.json`
- 1080p target:
  - median `total_time_taken` improves materially versus current Phase 5
  - `surface.readback_rgba` is no longer a top-two cost bucket
- 4K target:
  - end-to-end wall clock is no worse than Python and the target is to beat it
  - combined `surface.clear` + `surface.readback_rgba` is reduced by at least `40%` from the current Phase 5 median
- `queue.put_wait` remains low, indicating the renderer is not stalled on the encoder queue.
- `encoder.queue_wait` remaining high is acceptable if end-to-end total improves, because that means the encoder is no longer the bottleneck.
- Render output remains visually within the existing Phase 3/4 parity thresholds.
- Progress reporting and cancellation semantics remain unchanged from the user’s perspective.

Test procedure:

1. We have already established Phase 5 baselines for:
   - `new_template.json`
   - `safa_brian_a_4k_gradient.json`
2. For each optimization branch or milestone:
   ```powershell
   cargo run --bin render_video -- --config templates\new_template.json --payload app\debug\Test_FIT-parse-debug.json
   cargo run --bin render_video -- --config templates\safa_brian_a_4k_gradient.json --payload app\debug\Test_FIT-parse-debug.json
   ```
3. Save into tmp\Phase6:
   - final `.mov`
   - `prepare_render_assets_timing.json`
   - `timing_summary.json`
   - ffprobe JSON
4. Compare against the locked baseline medians.
5. For each milestone, record:
   - delta in `total_time_taken`
   - delta in `frame.total`
   - delta in `frame.draw`
   - delta in `surface.clear`
   - delta in `surface.readback_rgba`
   - delta in `ffmpeg.write`
6. Run visual regression checks on the same sampled frames already used in earlier phases.
7. Run a cancel test after the major buffer/surface architecture changes to ensure resource reuse does not leak or deadlock.

Recorded artifacts for Phase 6:

- median baseline reports for 1080p and 4K
- per-milestone `timing_summary.json`
- per-milestone `prepare_render_assets_timing.json`
- ffprobe JSONs
- final optimized `.mov` outputs
- optimization notes documenting which surface/buffer strategy won and why

### Phase 7: Route/elevation cached composition and draw-path collapse

This phase is dedicated to removing the remaining per-frame vector redraw cost of the route and elevation widgets. Phase 6 proved that the old full-frame clear and readback costs can be reduced substantially, but also showed that the Rust renderer still spends too much time rebuilding and redrawing route/elevation geometry every frame, especially for the elevation widget at 4K. The focus here is to move the widget architecture closer to the proven legacy Python approach: pre-render static widget imagery once, precompute reveal/composition state once, and keep the frame-time path limited to lightweight compositing plus marker and label updates where unavoidable.

Measured motivation from the current Phase 6 implementation:

- 1080p `new_template.json` after the first Phase 6 pass:
  - `composite.elevation` is about `5.7 ms/frame`
  - `composite.route` is about `0.48 ms/frame`
  - `frame.draw` is about `10.2 ms/frame`
- 4K `safa_brian_a_4k_gradient.json` after the first Phase 6 pass:
  - `composite.elevation` is about `19.6 ms/frame`
  - `composite.route` is about `1.55 ms/frame`
  - `frame.draw` is about `22.4 ms/frame`
- A 50% reduction in elevation point budget helped, but only partially:
  - 1080p `composite.elevation` improved to about `4.4 ms/frame`
  - 4K `composite.elevation` improved to about `17.9 ms/frame`
- The legacy Python renderer is faster in this area not because Pillow is inherently faster than Skia, but because the Python path pre-renders:
  - route background and completed layers
  - elevation background and completed layers
  - rotated variants of those layers
  - marker sprites
  - route reveal masks / reveal buckets
  - elevation label text
  - dirty-region restore patches

Decision:

- Treat route/elevation per-frame vector redraw as the primary post-Phase-6 bottleneck.
- Preserve the current Phase 6 buffer-pool / no-readback architecture unless a later measurement proves it incompatible with fast compositing.
- Prefer cached image composition over repeated path reconstruction for both widgets.
- Match the old Python bucket names where possible so Phase 7 results remain comparable to both Python and Phase 6 Rust reports.
- Keep preview rendering functional, but optimize the production video path first.

Implementation:

- Step 1. Add explicit cached widget layer types to Rust render assets.
  - Expand `PreparedRenderAssets` and widget cache structs so route/elevation caches can hold:
    - background layer
    - completed layer
    - rotated background layer
    - rotated completed layer
    - marker sprite
    - marker anchor
    - route reveal masks or route reveal overlays
    - optional representative debug images for validation only
  - Route and elevation caches must separate:
    - geometry state used for prepare-time construction
    - frame state used for marker/reveal lookup
    - image layers used in steady-state render

- Step 2. Build pre-rendered route layers during prepare.
  - During route cache preparation:
    - render remaining/background route line once into a widget-local layer
    - render completed route line once into a widget-local layer
    - pre-rotate both layers if rotation is non-zero
    - pre-render marker sprite once
  - Preserve current route geometry simplification, but use it only to generate the cached layers and frame states, not to redraw the route every frame.
  - Requirement:
    - `draw_route_widget()` in steady-state must stop rebuilding and stroking route paths each frame.

- Step 3. Build pre-rendered elevation layers during prepare.
  - During elevation cache preparation:
    - render the remaining elevation area + line once into a widget-local background layer
    - render the completed elevation area + line once into a widget-local completed layer
    - pre-rotate both layers if rotation is non-zero
    - pre-render marker sprite once
  - Preserve the existing elevation geometry generation only as prepare-time input.
  - Requirement:
    - `draw_elevation_widget()` in steady-state must stop redrawing both fills and both lines every frame.

- Step 4. Precompute reveal strategy for route.
  - Port the old Python reveal strategy into Rust and evaluate two options:
    - bucketed reveal masks
    - bucketed reveal overlays
  - Route reveal should be selected by precomputed bucket or precomputed frame index mapping, not by rebuilding prefix points and re-stroking the line every frame.
  - Bucket count should be configurable from scene/render settings if needed, with a sensible default tied to widget size and frame count.
  - Requirement:
    - route reveal selection in the hot path must be O(1) lookup plus image composition.

- Step 5. Precompute reveal strategy for elevation.
  - Evaluate the two practical strategies already proven conceptually in the Python path:
    - reveal width crop from a pre-rendered completed elevation layer
    - bucketed reveal overlays if crop/composite is visually insufficient
  - The first implementation should prefer crop-based reveal for simplicity if it preserves parity.
  - If the crop-based approach is not visually correct for all templates, introduce bucketed completed overlays analogous to route.
  - Requirement:
    - per-frame elevation rendering must reduce to:
      - composite background layer
      - composite completed reveal layer or crop
      - draw/paste marker
      - draw/paste labels if enabled

- Step 6. Precompute marker sprites and avoid procedural marker redraw in hot path.
  - Route and elevation currently rebuild marker circles procedurally through Skia every frame.
  - Replace this with pre-rendered widget-local marker sprites, including layered/ring styles matching current templates.
  - Marker placement should become:
    - integer or subpixel position lookup from frame state
    - image draw/paste at that position
  - Requirement:
    - marker rendering should no longer contribute materially to `composite.route` or `composite.elevation`.

- Step 7. Precompute elevation label strings and measure text placement separately.
  - Expand `ElevationFrameState` to include optional preformatted label text, matching the legacy Python path.
  - If label drawing remains expensive after composition caching, evaluate:
    - cached glyph/text blobs for unique label strings
    - pre-rendered text sprites for metric/imperial label variants
  - Keep the timing bucket name `text.elevation_label` for the actual draw/paste work.
  - Requirement:
    - no per-frame string formatting should remain on the elevation hot path.

- Step 8. Add widget-local compositing helpers rather than full vector redraw helpers.
  - Introduce render helpers dedicated to:
    - compositing widget layers onto the main frame surface
    - cropping reveal regions
    - applying rotation-aware widget-local offsets
    - drawing/pasting pre-rendered marker sprites
  - Keep geometry/path-building helpers only in prepare-time code.
  - Requirement:
    - frame-time widget functions should operate on cached images and frame indices, not rebuild `SkPath`s from long point arrays.

- Step 9. Reintroduce dirty-region-aware base restore where it wins.
  - Phase 6 replaced the costly `surface.clear` and full readback path, but still restores the full base frame every time.
  - Revisit the Python dirty-region philosophy specifically for dynamic widget/value regions:
    - restore only route widget region
    - restore only elevation widget region
    - restore only dynamic text regions
  - Evaluate two implementations:
    - CPU copy of rectangular RGBA regions from the pristine base buffer
    - widget-local cached layers composited over a transparent frame without full-frame restore
  - Keep whichever is faster and simpler to reason about.
  - Requirement:
    - `base.restore` or `surface.restore` should not grow as widget caching is introduced.

- Step 10. Preserve benchmark hygiene and timing comparability.
  - Keep shared buckets:
    - `base.restore`
    - `text.dynamic`
    - `composite.route`
    - `composite.elevation`
    - `text.elevation_label`
    - `queue.put_wait`
    - `encoder.queue_wait`
    - `ffmpeg.write`
  - Rust-only buckets may include:
    - `prepare.route.layers`
    - `prepare.route.reveal_masks`
    - `prepare.elevation.layers`
    - `prepare.elevation.reveal_overlays`
    - `marker.sprite.prepare`
    - `widget.composite.route`
    - `widget.composite.elevation`
  - Keep debug image generation optional and disabled for benchmark runs.

- Step 11. Validate parity while optimizing.
  - Reuse the existing sampled-frame parity checks from Phases 4-6.
  - Specifically validate:
    - route reveal correctness across 0/25/50/75/100%
    - elevation reveal correctness across the same checkpoints
    - rotated route layers
    - marker positioning
    - elevation labels
  - If bucketing introduces visible reveal stepping, increase bucket count or switch reveal strategy rather than accepting parity loss.

Optimization order and expected impact:

1. pre-render elevation background/completed layers
2. switch elevation reveal to cached crop/composition
3. pre-render route background/completed layers
4. switch route reveal to cached masks/overlays
5. pre-render marker sprites
6. precompute elevation label strings
7. re-evaluate dirty-region restore versus full-frame restore

Acceptance:

- Rust steady-state widget rendering is no longer dominated by repeated route/elevation path redraw.
- 1080p target:
  - `composite.elevation` should drop materially from current Phase 6 numbers, with a target below `3 ms/frame`
  - `frame.draw` should improve correspondingly
- 4K target:
  - `composite.elevation` should drop materially from current Phase 6 numbers, with a target below `10 ms/frame`
  - `composite.route` should remain low, ideally below `1 ms/frame`
  - `frame.draw` should improve materially versus the current Phase 6 median
- Route/elevation output remains within existing visual parity thresholds from Phases 4-6.
- Encoder-side buckets may remain unchanged; the success criterion here is reducing render-side widget cost.

Test procedure:

1. Run the same benchmark matrix used in Phase 6:
   ```powershell
   cargo run --bin render_video -- --config templates\new_template.json --payload app\debug\Test_FIT-parse-debug.json
   cargo run --bin render_video -- --config templates\safa_brian_a_4k_gradient.json --payload app\debug\Test_FIT-parse-debug.json
   ```
2. Save into `tmp\Phase7`:
   - final `.mov`
   - `prepare_render_assets_timing.json`
   - `timing_summary.json`
   - any widget-debug images or JSONs used for validation
3. Compare against:
   - Phase 6 median baselines
   - the most recent reduced-point experiments if relevant
4. For each milestone, record:
   - delta in `frame.draw`
   - delta in `composite.route`
   - delta in `composite.elevation`
   - delta in `text.elevation_label`
   - delta in `base.restore`
   - delta in total wall clock
5. Run sampled-frame visual diffs for:
   - route widget representative frames
   - elevation widget representative frames
   - full preview frames at fixed benchmark seconds
6. Re-run a cancel test after cached layer composition is introduced to ensure no deadlocks or resource leaks in the pooled-buffer path.

Recorded artifacts for Phase 7:

- per-template `timing_summary.json`
- per-template `prepare_render_assets_timing.json`
- final `.mov` outputs
- route/elevation sampled PNGs
- optional widget-debug layers and reveal-mask previews
- optimization notes documenting which cached composition strategy won and why

### Deferred optional phase: Browser-hosted build from the same repo

This phase is optional and does not change the success criteria for Phases 1-5. Its goal is to support two build targets from the same repository. Some parts of the Rust rendering-encoding architecture might be outdated in this plan. Please verify everything.

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
