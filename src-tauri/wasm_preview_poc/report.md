# WASM Preview POC Report

## Purpose

The `wasm_preview_poc` crate is an isolated proof of concept for running a Skia-based preview renderer inside the Tauri webview through WebAssembly. It is intentionally separate from `ovrley_core` and from the existing React/SVG preview so the project can answer a narrow question first:

- Can a Rust + `skia-safe` renderer be built for the current Emscripten target?
- Can the frontend call it reliably from the Tauri webview?
- Can a widget surface be rendered from Rust/Wasm and presented in the current frontend host model?

This POC is not a production architecture commitment. It is a feasibility slice and a boundary-finding exercise.

The intended Wasm host is the Tauri frontend only. Native-browser support is out of scope for this document.

## Work Completed

### Build and Runtime Proof

- Set up a standalone `wasm_preview_poc` crate that builds with `skia-safe` for `wasm32-unknown-emscripten`.
- Verified local build output for:
  - `wasm_preview_poc.js`
  - `wasm_preview_poc.wasm`
- Added and validated the current Emscripten linker workaround and scoped target configuration.
- Verified native dependency resolution and local `cargo check` for the crate.

### Minimal Wasm Renderer ABI

Implemented a small C-style ABI exported through `src/main.rs` and backed by `src/lib.rs`:

- allocate / deallocate RGBA buffers
- query width / height / RGBA length
- render a static frame
- load font bytes from the frontend
- render a dynamic text widget with value + unit strings

This established the end-to-end host model:

1. Frontend loads the Wasm runtime.
2. Frontend loads bundled font bytes.
3. Frontend passes strings into Wasm.
4. Wasm renders into an RGBA buffer.
5. Frontend copies that buffer into canvas.

### Frontend Debug Mount and Benchmark Harness

A dedicated debug page was wired up in the frontend to exercise the POC renderer and collect timing data. The benchmark path now:

- loads the generated Wasm artifacts
- loads font bytes once
- reuses persistent Wasm buffers for RGBA and strings
- records phase timings for:
  - prepare
  - wasm draw
  - buffer copy
  - `putImageData`
  - total
- follows the same frame-index update model as live preview instead of naive elapsed-time gating

### Benchmark Alignment Improvements

The debug benchmark was adjusted to behave more like live preview code:

- benchmark scheduling now follows frame-index dedupe instead of interval gating
- target FPS derives from live scene FPS and update rate
- benchmark pass thresholds were made target-FPS aware

This removed a misleading self-cap that previously produced artificial `50 ms` frame intervals.

### Multi-Widget Stress Path

The benchmark was extended to simulate multiple widgets in a single visible canvas by:

- rendering multiple widget surfaces offscreen
- compositing those surfaces into one visible benchmark canvas

This provided a more realistic stress case than a single isolated widget while keeping the POC renderer simple.

### Text Quality Improvement

The text rendering path in `src/lib.rs` was updated to explicitly configure Skia font rendering:

- subpixel positioning enabled
- subpixel anti-aliased edging enabled
- full hinting enabled

This improves glyph rasterization in the current software-raster path, though final quality is still affected by later canvas scaling and compositing.

### POC Rendering Improvements

The first meaningful structural improvements were implemented in the POC itself.

#### 1. Widget-Local Surface Size

The renderer no longer draws the dynamic text widget into a full-scene `1920x1080` RGBA buffer. It now renders a widget-local `1280x480` surface.

This reduces work in three places at once:

- Rust/Skia draw work
- JS-side RGBA copy volume
- canvas `putImageData` upload volume

#### 2. Cached Static Base Layer in Wasm

The dynamic widget path no longer redraws all static chrome every frame. A cached base RGBA layer is built once and restored before drawing dynamic text.

This directly reduces the `wasm draw` bucket for the current widget shape.

#### 3. Smaller Offscreen Benchmark Canvases

The debug page no longer allocates full-size offscreen canvases and crops from them for each widget. It now composites widget-local canvases directly.

This removes the JS-side equivalent of full-scene waste.

## Current State

The POC currently demonstrates all of the following:

- Rust + Skia can be built and called from the Tauri webview through Wasm.
- The frontend can feed font bytes and dynamic strings into the module.
- The module can render a software-raster RGBA widget surface.
- The debug page can benchmark the renderer with live-style frame scheduling.
- The current path is functional enough to compare render, copy, and upload behavior at a narrow scope.

The POC is still intentionally narrow:

- It renders a single text-widget-style surface, not the real `ovrley_core` renderer.
- It uses software-raster RGBA presentation inside the Tauri webview, not a browser-agnostic GPU render path.
- It still copies pixels into `ImageData` and then uploads them to canvas.
- It is useful for feasibility and migration planning, not yet for app-wide replacement.

The runtime target here is specifically the Tauri shell. This report is not describing a native-browser preview strategy.

## Architectural Migration Steps Toward Full-App Implementation

The next stage should focus on migrating the current preview architecture into a Wasm-backed render path. The sequence below is the concrete architectural order that currently makes the most sense.

### 1. Carve a Host-Neutral Preview Render Core out of `ovrley_core`

The first change is inside the Rust render core.

Areas to modify first:

- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/text.rs`
- `src-tauri/ovrley_core/src/render/static_layer.rs`
- `src-tauri/ovrley_core/src/render/widgets/**`

Goal:

- keep the existing Skia drawing logic
- isolate the preview-facing render seam from desktop-only concerns
- make the reusable surface explicit: prepare assets, then render a frame into caller-owned RGBA

This step should produce a render API that does not assume preview PNG output and does not force direct dependence on desktop app wiring in the hot path.

Preview PNG generation should remain available only as a debug and verification side-path. It should not define the primary realtime preview boundary.

### 2. Replace Desktop Resource Assumptions with Injected Resources

The next change is to remove filesystem and app-path assumptions from the preview render path.

Areas most likely to change:

- `src-tauri/ovrley_core/src/render/text.rs`
- `src-tauri/ovrley_core/src/paths.rs`
- any widget preparation code that currently pulls assets from `AppPaths`

Goal:

- fonts are provided as bytes or through an explicit font registry
- preview render code consumes injected resources instead of reading from disk
- the Wasm path and the native path can share the same rendering logic with different hosts

This is the key boundary refactor. Without it, the render core stays tied to the native Tauri runtime.

### 3. Create the Real Wasm Adapter Layer

Once the render seam is clean enough, the next step is to build the actual Wasm-facing adapter crate or expand `wasm_preview_poc` into that role.

Primary area:

- `src-tauri/wasm_preview_poc/**`

Expected responsibilities:

- initialize and hold renderer-side state
- accept font/resource bytes from JS
- accept validated config and activity payloads or prepared equivalents
- expose explicit prepare / invalidate / render frame entrypoints
- expose buffer size and frame metadata needed by the frontend host

This is where the current toy ABI grows into the real preview ABI.

### 4. Migrate One Frontend Preview Surface to the Wasm Renderer

The first real frontend consumer should be narrow and reversible.

Likely areas:

- `app/src/features/video-preview/**`
- small supporting glue outside that feature if needed

Goal:

- keep the existing preview clock and frame scheduling in JS
- replace only the drawing backend for one preview surface
- route config, activity, and invalidation events into the Wasm adapter

This should not try to replace the full editor and all widget previews at once. The target consumer is the Tauri preview surface, not a generic browser runtime.

### 5. Replace Frontend Widget Rendering Logic with Wasm-Backed Presentation

After one preview surface is stable, the next step is to retire the current frontend-side widget drawing and layout paths.

Primary areas:

- `app/src/features/widget-preview/components/**`
- `app/src/features/widget-preview/hooks/**`
- any related preview-model utilities that exist only to support SVG rendering

Goal:

- keep frontend orchestration and selection/interaction logic where needed
- remove frontend-side drawing responsibilities where the Wasm renderer now owns them
- simplify route, elevation, metric, heading, and text preview paths so they become render-host plumbing rather than parallel rendering implementations

This is the step where the current React/SVG preview stops being the renderer and becomes mainly a host/controller.

### 6. Widen Coverage Across Widget Families

Only after the adapter and one preview surface are stable should the render coverage expand.

Suggested order:

1. text and metric widgets
2. heading / boxed presentations
3. route and elevation widgets
4. any remaining specialized preview surfaces

Relevant Rust areas:

- `src-tauri/ovrley_core/src/render/widgets/value/**`
- `src-tauri/ovrley_core/src/render/widgets/heading/**`
- `src-tauri/ovrley_core/src/render/widgets/route/**`
- `src-tauri/ovrley_core/src/render/widgets/elevation/**`

Relevant frontend areas:

- `app/src/features/widget-preview/**`
- `app/src/features/video-preview/**`

### 7. Remove Redundant Preview Pipelines

Once Wasm-backed preview rendering is stable across the targeted surfaces, the codebase should remove duplicate preview responsibilities.

Areas likely affected:

- `app/src/features/widget-preview/**`
- preview-only SVG helpers and preview geometry/model utilities that are no longer needed
- any temporary bridging code in `app/src/features/wasm-preview-debug/**`

Goal:

- one rendering source of truth
- frontend owns scheduling, state, and interaction
- Rust/Wasm owns preview rendering

This is the cleanup step that converts the migration into the new steady-state architecture.

## Most Dangerous Landmines

These are the main migration landmines, anchored to the specific code areas where the work is likely to get blocked or expand unexpectedly.

### 1. Font Resolution Is Hard-Wired to Filesystem and Native Font Lookup

Primary code area:

- `src-tauri/ovrley_core/src/render/text.rs`

Concrete problem points in that file:

- `resolve_font(...)`
- `resolve_typeface(...)`
- `load_typeface(...)`
- `load_first_bundled_typeface(...)`
- the global `OnceLock<Mutex<HashMap<String, Typeface>>>` cache
- fallbacks to `FontMgr::default().legacy_make_typeface(...)`

Why it is dangerous:

- the current render path expects `font_dirs: &[PathBuf]`
- it reads font bytes via `std::fs`
- it falls back to system font lookup through the native `FontMgr`
- it caches typefaces by name assuming those names are globally meaningful to the host

For a Wasm preview path, this is one of the biggest required refactors. The current code is not just drawing text; it is also acting as a native resource resolver.

### 2. `AppPaths` Is Embedded in the Preview Render Boundary

Primary code areas:

- `src-tauri/ovrley_core/src/paths.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/static_layer.rs`

Concrete problem points:

- `prepare_preview_assets(paths, ...)`
- `FrameRenderRequest { paths: &AppPaths, ... }`
- `render_frame_to_surface(..., paths, ...)`
- every call site that reaches into `paths.font_dirs`
- static-layer helpers that take `&AppPaths`

Why it is dangerous:

- `AppPaths` bundles runtime directories, templates, fonts, debug output, temp dirs, and user-data assumptions into one native-oriented struct
- that is convenient for native commands and preview PNG generation, but it is the wrong dependency shape for a Wasm-hosted preview render core
- if `AppPaths` is not split or abstracted, it will drag native runtime concerns into the Wasm seam

This is visible directly in the current render function signatures.

### 3. Preview Output Paths and File-Oriented Helpers Are Still Mixed Into Render Responsibilities

Primary code areas:

- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/render/surface.rs`

Concrete problem points:

- `render_preview_to_path(...)`
- `render_preview_with_report(...)`
- `PreviewRenderRequest { out_path: &Path, ... }`
- `write_surface_png(...)`
- `std::fs::write(...)` inside the surface helper

Why it is dangerous:

- these paths are not needed for the final Wasm preview, but they live in the same render module and influence the shape of the public API
- if the preview-to-file path is not cleanly separated from the frame-to-RGBA path, the migration can end up preserving the wrong boundary
- this can make the Wasm adapter larger and messier than it needs to be
- preview PNG generation is still useful for debugging, regression snapshots, and offline inspection, but it should remain explicitly secondary to the realtime Tauri preview path

The landmine is not PNG support itself. The landmine is letting file-output concerns define the render seam that Wasm has to implement.

### 4. Static Caches Exist, but They Are Tied to Native-Oriented Preparation Flow

Primary code areas:

- `src-tauri/ovrley_core/src/render/static_layer.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`
- widget prep under `src-tauri/ovrley_core/src/render/widgets/**`

Concrete problem points:

- `cached_labels_image(...)`
- `prepare_base_rgba(...)`
- global `OnceLock<Mutex<HashMap<u64, Image>>>` cache
- `prepared_assets.base_rgba`
- route / elevation / heading prepared state that is built assuming native host setup

Why it is dangerous:

- the good news is that static caching already exists
- the bad news is that the preparation lifecycle and cache ownership are currently embedded in the native render pipeline
- during migration, the team will need to decide what stays inside the Wasm module as persistent state and what the frontend invalidates explicitly

The landmine is that there is already caching, but it is currently owned by modules that were not designed as long-lived Wasm renderer state.

### 5. Route and Elevation Preview Logic Exists in Two Worlds Already

Primary code areas:

- Rust: `src-tauri/ovrley_core/src/render/widgets/route/**`
- Rust: `src-tauri/ovrley_core/src/render/widgets/elevation/**`
- Frontend: `app/src/features/widget-preview/hooks/useRoutePreviewGeometry.js`
- Frontend: `app/src/features/widget-preview/hooks/useElevationPreviewGeometry.js`
- Frontend: `app/src/features/widget-preview/components/RouteRenderer.jsx`
- Frontend: `app/src/features/widget-preview/components/ElevationRenderer.jsx`

Why it is dangerous:

- route and elevation are already partly backend-prepared and partly frontend-rendered
- the frontend currently still does per-frame preview shaping and SVG composition on top of Rust geometry results
- a Wasm migration must decide which responsibilities move fully into Rust/Wasm and which remain frontend-side

This is a real boundary hazard because it is easy to end up with a third mixed model instead of a clean replacement.

### 6. The Current Frontend Preview Stack Is Deeply Invested in SVG/Text-Specific Presentation Models

Primary code areas:

- `app/src/features/widget-preview/components/**`
- `app/src/features/widget-preview/hooks/**`
- `app/src/features/widget-preview/utils/**`

Examples already visible in code:

- `WidgetPreview.jsx` dispatches widget families and boxed display types
- route/elevation hooks build SVG-facing geometry models
- text/metric preview code depends on preview text measurement and SVG layout assumptions

Why it is dangerous:

- this code is not all disposable drawing code
- some of it is real orchestration and widget selection logic
- some of it is only there because the renderer is currently React/SVG based

The landmine is misjudging what can be deleted versus what must be retained as the Wasm host/controller layer.

### 7. The Toolchain Workaround Is Itself a Migration Dependency

Primary code areas:

- `src-tauri/.cargo/config.toml`
- `src-tauri/wasm_preview_poc/tools/emcc-linker.bat`
- `src-tauri/wasm_preview_poc/README.md`

Why it is dangerous:

- the current build relies on a specific Rust toolchain, Emscripten target, linker wrapper, and ABI workaround
- this is not just a POC convenience; it is currently the only verified way the Skia Wasm build works in this repo
- if the real renderer is moved into Wasm before this dependency is treated as a first-class build contract, upgrades and CI will become fragile
- rust-skia PR #1275 shows a possible future `wasm32-unknown-unknown` path with WASI/WebGL shims, but it is open upstream and is not a drop-in replacement for the current `skia-safe = 0.75` Emscripten binary-cache path

Follow-up spike findings:

- The rust-skia `0.93.1...0.97.0` range contains real Emscripten build-support changes:
  - Emscripten SDK discovery is routed through `skia_emsdk_dir`.
  - newer Emscripten sysroot include paths are handled.
  - Skia m148 wasm archive naming is handled by copying `*.wasm.a` archives to `*.a`.
- A narrow POC upgrade cannot happen while `wasm_preview_poc` remains in the same Cargo workspace as `ovrley_core` on `skia-safe = 0.75`, because both versions pull a `skia-bindings` crate with `links = "skia"`.
- Temporarily isolating the POC crate allowed `skia-safe = 0.97.0` to pass native `cargo check`.
- The Emscripten binary-cache path for `skia-safe = 0.97.0` and `0.97.2` failed before final linking because the downloaded archive contained `libskia.a`, while the build script expected `libskia.wasm.a`.
- Disabling default `skia-safe` features avoided that specific binary-cache archive, but then rust-skia fell back to a full Skia source build. On this Windows machine, that path failed while unpacking Skia m148 because creating symlinks requires privileges not available in the normal shell.
- As an experiment only, manually duplicating the downloaded target artifact from `libskia.a` to `libskia.wasm.a` let `skia-safe = 0.97.2` proceed to final linking.
- With Rust `1.85.0`, `skia-safe = 0.97.2`, Emscripten 6.0.0, the existing `-sSUPPORT_LONGJMP=emscripten` / initial-memory link args, and the temporary archive-name alias, the POC built successfully.
- In that successful `0.97.2` / Rust `1.85.0` setup, Cargo also built successfully when the linker was overridden directly to `H:\tools\emsdk\upstream\emscripten\emcc.bat`, bypassing `emcc-linker.py`.
- The same direct-upgrade experiment with latest stable Rust `1.94.1` reached final linking but failed with `__cpp_exception` symbols from Rust's precompiled Emscripten std, so the Rust toolchain version still matters.

Interpretation:

- The current `emcc-linker.py` wrapper is probably not a permanent requirement if the project moves to a newer rust-skia line and pins a compatible Rust/Emscripten toolchain.
- The wrapper cannot be removed from the current `skia-safe = 0.75` path without replacing the verified build contract.
- The immediate blocker for a clean `0.97.x` migration is not the wrapper itself; it is making the rust-skia Emscripten binary-cache/source-build path reproducible without manual target-directory artifact aliases or privileged Skia source unpacking.
- A real migration should be treated as a coordinated toolchain issue:
  - upgrade `ovrley_core` and `wasm_preview_poc` to one `skia-safe` version, or keep the POC outside the app workspace
  - pin Rust `1.85.0` or another verified Emscripten-compatible toolchain
  - resolve the `libskia.wasm.a` vs `libskia.a` cache mismatch through an upstream fix, local patched crate, or controlled binary cache
  - only then replace `src-tauri/wasm_preview_poc/tools/emcc-linker.*` with direct `emcc.bat` linker configuration

This is a concrete migration landmine because the adapter layer cannot exist without a stable build path.

## Recommended Working Assumption

Treat the current POC as a successful feasibility slice, not as a final design.

The safest assumption for next planning is:

- the Rust/Wasm path is viable enough to continue
- the next work should focus on boundary extraction and render-path migration, not on broad app replacement in one step
- the main architectural work is to move preview rendering responsibility from frontend drawing code into a Wasm-hosted Rust renderer inside the Tauri app
- the migration should stay incremental so redundant preview paths can be removed in a controlled order
- preview PNG generation should stay as a debug side-path rather than the primary render pathway

## Local Verification Snapshot

Most recent validated checks during this phase:

```powershell
cargo check --manifest-path src-tauri/wasm_preview_poc/Cargo.toml
```

Result: passed after the widget-local surface and cached static base changes.
