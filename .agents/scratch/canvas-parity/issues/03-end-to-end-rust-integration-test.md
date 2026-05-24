Status: ready-for-agent

# 03 — End-to-End Rust Integration Test

## Parent

[Canvas-Frame Pixel Parity QA Gate PRD](../PRD.md)

## What to build

The complete Rust integration test file that orchestrates the full comparison pipeline: renders the same overlay frame using the Rust Skia backend, captures the same frame from the frontend React SVG canvas via Playwright, and compares both outputs using ffmpeg's SSIM filter.

The test is feature-gated behind `canvas-parity` (set up in Slice 1). When the feature is enabled, the test binary compiles and the test function runs. When the feature is disabled, the test binary does not exist.

**Test workflow — 10 steps in order**:

1. **Parse fixtures**: Read `fixtures/activity/gpx-parse-debug.json` and `fixtures/config/test-template-4k.json`. Parse into `ParsedActivity` and `RenderConfig` using the existing `parse_activity_json` and `parse_config_json` functions.

2. **Build dense activity report**: Call `build_dense_activity_report(&activity, &config)` to produce the frame-aligned telemetry.

3. **Prepare shared preview assets**: Call `prepare_preview_assets(&paths, &config, &activity, &dense_activity)` once for label caching and widget preparation.

4. **Render Skia PNG**: Call `render_preview_with_prepared_assets` with `second: 600` and `out_path` pointing to a temp file `skia.png`. This writes the Skia-rendered PNG — the backend's reference output.

5. **Generate mock data files**: Write three JSON files to a temp directory:
   - `template.json`: The ovrley-template envelope wrapping the parsed config (`{ format: "ovrley-template", version: 2, name: "test", savedAt: "...", config: <RenderConfig>, settings: { globalDefaults: <derived from config> } }`). This mimics what `backend_get_template` returns.
   - `activity.json`: The parsed activity data, as the frontend JS parser would produce on file import. The same debug fixture JSON used in step 1.
   - `store-state.json`: A Zustand store snapshot containing:
     - `config`: the normalized RenderConfig (the parsed config object)
     - `globalDefaults`: default font, color, opacity, scale settings
     - `selectedSecond`: 600
     - `activitySummary`: metadata extracted from the parsed activity (duration, distance, sample count)
     - `backgroundMode`: `"none"` (transparent, no checkerboard)
     - `gridVisible`: false
     - `selectedWidgetId`: null (no selection handles)
     - `editorZoomLevel`: 1.0

6. **Spawn Vite dev server**: Start the Vite dev server on a randomly chosen free port (`npx vite --port <port> --strictPort`). Wait for the server to be ready by polling or parsing stdout for the "Local:" URL line. The server process must be killed when the test completes (on drop).

7. **Spawn Playwright script**: Run the script from Slice 2: `node <script-path> --mock-dir <temp-dir> --vite-url http://localhost:<port> --out <canvas.png>`. Wait for completion, check exit code.

8. **Run ffmpeg SSIM**: Spawn ffmpeg with the SSIM filter: `ffmpeg -i skia.png -i canvas.png -lavfi "ssim" -f null -`. Parse stderr output for the combined score: `SSIM ... All:0.XXXX`. Convert to f64.

9. **Assert threshold**: Check that the combined SSIM score is `>= 0.98`. If it passes, the test succeeds.

10. **On failure — generate diff image**: Decode both `skia.png` and `canvas.png` to raw RGBA byte buffers via a ffmpeg subprocess (pipe to `rawvideo` format). Iterate pixel-by-pixel (4 bytes per pixel: R, G, B, A). For each pixel where any channel differs, write a red pixel (255, 0, 0, 255) to a diff buffer; for identical pixels, write the original pixel dimmed. Encode the diff buffer back to PNG via ffmpeg piped input. Write the diff PNG to `target/canvas-parity/failures/`. Print the combined SSIM score, per-plane scores (Y luminance, U/V chroma), pixel mismatch count, and diff image path.

**Error handling**: Each external process spawn (ffmpeg, Vite, Playwright) should produce clear error messages on failure. Missing prerequisites (ffmpeg not found, Node.js not found, Vite fails to start) should print actionable messages pointing to the prerequisite.

**Cleanup**: The Vite server process and any temp files should be cleaned up when the test function returns (use Rust's RAII via a wrapper struct with `Drop` implementation).

## Acceptance criteria

- [ ] `cargo test` (without `--features canvas-parity`) does NOT compile or run this test
- [ ] `cargo test -p ovrley_core --features canvas-parity --test canvas_parity_tests -- --nocapture` runs the test end-to-end
- [ ] The test produces `skia.png` from the Rust Skia renderer (visually inspectable in the temp directory)
- [ ] The test produces `canvas.png` from the Playwright screenshot (visually inspectable in the temp directory)
- [ ] The test prints a combined SSIM score (e.g. `SSIM: 0.9912 (threshold: 0.98) — PASS`)
- [ ] The test passes when SSIM >= 0.98
- [ ] When SSIM < 0.98: the test fails, prints per-plane scores, pixel mismatch count, and writes a diff PNG to `target/canvas-parity/failures/`
- [ ] Vite server is killed and temp files cleaned after the test, regardless of pass or fail
- [ ] All three mock JSON files (template, activity, store-state) are valid JSON that could be consumed by the frontend

## Blocked by

- [01 — Feature Gate, Dependencies, and Store Export](01-feature-gate-deps-store-export.md) (needs Cargo feature gate to compile the test binary at all)
- [02 — Playwright Screenshot Script](02-playwright-screenshot-script.md) (needs the `.mjs` script to spawn from Rust)
