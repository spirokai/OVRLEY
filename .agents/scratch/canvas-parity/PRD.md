Status: ready-for-agent

# Canvas-Frame Pixel Parity QA Gate

## Problem Statement

OVRLEY renders overlay widgets in two completely separate rendering pipelines: the Rust backend uses Skia (raster-level) for video export and preview thumbnails, while the frontend editor canvas uses React SVG components for live visual feedback. A rendering divergence between the frontend SVG preview and the backend Skia output would mean the user sees one thing in the editor and something different in the exported video.

There is currently no automated guard that verifies these two pipelines produce visually identical output for the same inputs. Rendering bugs, font regressions, widget positioning errors, or interpolation differences silently produce incorrect exports.

## Solution

Implement an opt-in Rust integration test that renders the same overlay frame using both pipelines — the Rust Skia backend and the frontend React SVG canvas (captured via Playwright) — and compares them using ffmpeg's SSIM (structural similarity) filter with a threshold of 0.98.

The test works exclusively for transparent overlay configurations (no imported video), uses a single rich 4K fixture exercising all widget types (labels, metrics, route, elevation), and produces a diff image on failure showing exactly which pixels differ.

## User Stories

1. As an OVRLEY developer, I want an automated test that verifies the frontend SVG preview and backend Skia render produce visually identical output, so that I catch rendering regressions before they reach users.
2. As an OVRLEY developer, I want the test to be opt-in (feature-gated), so that it does not slow down regular `cargo test` runs or fail in environments without Playwright installed.
3. As an OVRLEY developer, I want the test to report an SSIM score and highlight differing regions when the threshold is breached, so that I can quickly diagnose what differs between the two renderers.
4. As an OVRLEY developer, I want the test to use the same fixture data source for both sides, so that any mismatch is a genuine rendering divergence and not a data-preparation discrepancy.
5. As an OVRLEY developer, I want the test to exercise all widget types (labels, metric values, route/course, elevation profile) in a single frame, so that one test run covers the full rendering surface.
6. As an OVRLEY developer, I want the frontend to be served via Vite dev server and captured via headless Playwright, so that the test exercises the actual browser rendering pipeline the user sees.

## Implementation Decisions

### Architecture

The test is a Rust integration test in `ovrley_core/tests/canvas_parity_tests.rs`, gated behind a Cargo feature `canvas-parity`. It orchestrates three external processes: a Vite dev server serving the frontend, a Playwright script taking a screenshot, and ffmpeg computing SSIM.

### Feature gate

- A new Cargo feature `canvas-parity` is defined in `ovrley_core/Cargo.toml` with an empty feature set (`[]`).
- A `[[test]]` target entry is added with `required-features = ["canvas-parity"]` so the test binary is not compiled unless `--features canvas-parity` is explicitly passed.
- Run command: `cargo test -p ovrley_core --features canvas-parity --test canvas_parity_tests -- --nocapture`

### Rust test workflow

1. Parse activity fixture and config fixture from `fixtures/activity/gpx-parse-debug.json` and `fixtures/config/test-template-4k.json`.
2. Build dense activity report.
3. Call `prepare_preview_assets` once (shared asset preparation, label caching).
4. Call `render_preview_with_prepared_assets` for second 600, writing output to `skia.png`.
5. Serialize three mock data files to a temp directory:
   - The config in ovrley-template format (format/version/config envelope, as `backend_get_template` would return).
   - The parsed activity JSON (as the frontend cache would store).
   - A Zustand store snapshot (config, globalDefaults, selectedSecond, activitySummary, backgroundMode, gridVisible, selectedWidgetId).
6. Spawn Vite dev server on a random available port, wait for readiness.
7. Spawn the Playwright script with CLI args: `--mock-dir <temp_dir>`, `--vite-url http://localhost:<port>`, `--out <canvas.png>`.
8. Run ffmpeg SSIM filter comparing `skia.png` and `canvas.png`.
9. Assert combined SSIM score >= 0.98.
10. On failure: decode both PNGs to raw RGBA via ffmpeg, compute per-pixel diff, write a diff PNG to `target/canvas-parity/` with differing pixels highlighted in red, print the SSIM score and per-plane (Y, U, V) breakdown.

### Playwright script

A single `.mjs` file at `ovrley_core/tests/scripts/canvas_screenshot.mjs` that:
- Reads mock data JSON files from `--mock-dir`.
- Launches headless Chromium with viewport matching the config dimensions (3840x2160), `deviceScaleFactor: 1`, and `--disable-font-subpixel-positioning` for deterministic anti-aliasing.
- Navigates to the Vite dev server URL.
- Waits for the React app to render (polls for `window.__STORE__` availability).
- Reads the three mock JSON files from `--mock-dir` (template, activity, store snapshot).
- Injects the Zustand store state via `page.evaluate()` calling `window.__STORE__.setState()`.
- Injects activity data into the frontend cache via `page.evaluate()` calling the imported `setCurrentActivityCache()`.
- Hides background, grid, and selection handles via store state (`backgroundMode: 'none'`, `gridVisible: false`, `selectedWidgetId: null`).
- Waits for `document.fonts.ready` and a small animation-frame delay to ensure fonts are loaded and SVG has rendered.
- Screenshots the DOM element matching `[data-testid="widget-layer"]` or the OverlayCanvas container.
- Writes the screenshot PNG to the `--out` path.

### Frontend store access

- The Zustand store in `app/src/store/useStore.js` is conditionally exposed: `if (import.meta.env.DEV) window.__STORE__ = useStore`.
- This is stripped from production builds and has no runtime cost.

### Mock data generation

All mock data is generated by the Rust test from the same parsed fixture data used to render the Skia PNG. This eliminates mock drift — if a schema changes, the test fails at compile time (Rust type mismatch or frontend parse error), never silently.

- **Activity JSON**: the raw debug fixture JSON, as the frontend JS parser would produce on file import. Injected directly into the frontend cache.
- **Template JSON**: the ovrley-template envelope `{ format, version, name, config, settings }` wrapping the `test-template-4k.json` config. The frontend's `normalizeTemplateFilePayload` and `hydrateTemplateState` functions run on it.
- **Store snapshot**: a JSON object matching the Zustand store shape — `config` (normalized RenderConfig), `globalDefaults`, `selectedSecond: 600`, `activitySummary` (metadata from parsed activity), `backgroundMode: 'none'`, `gridVisible: false`, `selectedWidgetId: null`, `editorZoomLevel: 1.0`.

### Fixtures

- **Config**: `fixtures/config/test-template-4k.json` — 3840x2160, scale 2, labels (Furore.otf text), 7 metric values (cadence, temperature, heartrate, power, speed, gradient, time), elevation plot, course/route plot. No composite video fields.
- **Activity**: `fixtures/activity/gpx-parse-debug.json` — 4817 samples, 4912s duration, full telemetry. Same fixture used by the existing render baseline suite.
- **Second**: 600. Mid-activity, all metrics have interpolated values, route is partially completed, elevation fill is partially visible. Maximum rendering coverage in a single frame.

### Fonts

Both the Rust Skia renderer and the browser load fonts from `/fonts/`:
- Rust: `AppPaths.font_dirs` includes `repo_root.join("fonts")`. Skia's `load_typeface` searches this directory for `.ttf`/`.otf` files.
- Browser: CSS `@font-face` rules in `app/src/index.css` use `url('../../fonts/...')` relative paths, which resolve to the same directory via Vite.
- The 4K fixture uses `Furore.otf` (all widgets) and `Arial.ttf` (scene default). Both are bundled.

### Chromium launch configuration

- `viewport: { width: 3840, height: 2160 }` — matches config scene dimensions.
- `deviceScaleFactor: 1` — 1:1 pixel mapping with Skia.
- `headless: true` — CI-compatible, no display server needed.
- Chromium args: `--disable-font-subpixel-positioning` — forces grayscale anti-aliasing, eliminating subpixel AA variance across machines.

### SSIM threshold

The ffmpeg `ssim` filter is used with threshold `>= 0.98`. This accounts for minor glyph rasterization differences between Skia's FreeType and Chromium's FreeType while catching significant rendering divergences (wrong widget position, missing element, wrong color, broken text spacing).

### Failure output

- Print the combined SSIM score and per-plane scores (Y: luminance, U/V: chroma).
- Generate a diff PNG where differing pixels are colored red. The diff image is written to `target/canvas-parity/failures/`.
- Print the diff image path and pixel mismatch count/percentage.

### Out of scope

- Testing composite/imported-video rendering paths. This test is transparent-overlay-only.
- Pixel-exact comparison. SSIM >= 0.98 is perceptual parity, not pixel parity.
- CI enforcement. The test is feature-gated and opt-in only. It is not run in release workflows.
- Multiple-second testing. Only second 600 is tested.
- Performance benchmarking. No timing reports are generated.

### Dependencies

- `@playwright/test` added to root `package.json` devDependencies. The Rust test spawns it via `npx`.
- No new Rust crate dependencies. ffmpeg is spawned as a subprocess (already available via the project's bundled ffmpeg).

## Testing Decisions

The test itself IS the deliverable — there is no additional test-of-a-test layer. The integration test verifies external rendering behavior end-to-end:

- **What makes a good test**: the test compares actual rendered pixels between two independent rendering engines, using a single source of truth for fixture data. It asserts a quantitative similarity metric, not an implementation detail.
- **Prior art**: the existing `render_baseline_suite.rs` tests Skia output against saved golden PNGs. This test extends that pattern to cross-engine comparison (Skia vs browser SVG).
- **No unit tests needed** for the mock data generator, Vite server manager, or ffmpeg SSIM runner — they are exercised by the integration test and trivial to debug if they fail.

## Further Notes

- The existing `render_baseline_suite` compares current Skia output to historically saved golden files (regression guard). This new test is orthogonal: it compares Skia output to browser SVG output (cross-platform rendering fidelity guard).
- The `test-template-4k.json` fixture has `scale: 2`. Both Skia and the frontend multiply widget positions and font sizes by this factor internally. The output resolution (3840x2160) already accounts for it.
- The Playwright script should be runnable standalone for debugging: `node canvas_screenshot.mjs --mock-dir /tmp/mock --vite-url http://localhost:5173 --out debug.png`.

## Issues

- [01 — Feature Gate, Dependencies, and Store Export](issues/01-feature-gate-deps-store-export.md)
- [02 — Playwright Screenshot Script](issues/02-playwright-screenshot-script.md)
- [03 — End-to-End Rust Integration Test](issues/03-end-to-end-rust-integration-test.md)
