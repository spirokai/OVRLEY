Status: ready-for-agent

# 02 — Playwright Screenshot Script

## Parent

[Canvas-Frame Pixel Parity QA Gate PRD](../PRD.md)

## What to build

A self-contained Node.js script at the test fixtures location that launches a headless Chromium browser, loads the OVRLEY frontend from a Vite dev server, injects fixture store state and activity data, hides all UI chrome to isolate just the widget overlay layer, waits for font loading and SVG rendering to complete, then takes a single screenshot and writes it to a PNG file.

The script is runnable standalone (for debugging) and also spawned as a subprocess by the Rust integration test in Slice 3.

**Script interface**: The script accepts CLI arguments:
- `--mock-dir <path>` — directory containing three JSON files: `template.json` (ovrley-template envelope), `activity.json` (parsed activity), `store-state.json` (Zustand snapshot)
- `--vite-url <url>` — the Vite dev server URL (e.g. `http://localhost:5173`)
- `--out <path>` — output PNG file path

**Browser launch**: Headless Chromium with viewport matching the config dimensions (3840x2160), `deviceScaleFactor: 1` for 1:1 pixel mapping with Skia output, and `--disable-font-subpixel-positioning` for deterministic grayscale anti-aliasing across machines.

**State injection workflow**:
1. Read the three mock JSON files from `--mock-dir`.
2. Navigate to the Vite URL.
3. Poll for `window.__STORE__` availability (Zustand store mounted).
4. Call `page.evaluate()` to inject the store snapshot via `window.__STORE__.setState(state)`.
5. Call `page.evaluate()` to inject the activity data into the frontend cache (the `setCurrentActivityCache()` function from the activity cache module).

**Widget layer isolation**: The store state snapshot sets `backgroundMode: 'none'` (transparent background, no checkerboard), `gridVisible: false` (no editor grid), and `selectedWidgetId: null` (no resize handles). The OverlayCanvas renders only the widget SVG layer on transparent background.

**Render completion**: Before screenshotting, wait for `document.fonts.ready` to ensure all `@font-face` fonts are loaded, then wait a small `requestAnimationFrame` cycle for React to re-render with the injected state.

**Screenshot**: Target the OverlayCanvas container (the element containing all `OverlayCanvasWidget` instances, which occupies the full config dimensions). Clip to the container's bounding box. Write as PNG to `--out`.

## Acceptance criteria

- [ ] Running the script standalone produces a valid PNG: `node canvas_screenshot.mjs --mock-dir /tmp/mocks --vite-url http://localhost:5173 --out test.png`
- [ ] The output PNG dimensions match the config scene dimensions (3840x2160)
- [ ] The output PNG has a transparent background (no checkerboard, no grid lines, no UI chrome visible)
- [ ] The output PNG shows widget overlays rendered at their configured positions and sizes
- [ ] No Tauri IPC calls are made — the script runs against a plain Vite dev server, not the Tauri desktop app
- [ ] Fonts render correctly (Furore.otf from the bundled `/fonts/` directory via CSS `@font-face`)
- [ ] The script exits with code 0 on success, non-zero on failure, and prints error messages to stderr

## Blocked by

- [01 — Feature Gate, Dependencies, and Store Export](01-feature-gate-deps-store-export.md) (needs `window.__STORE__` to be present for store state injection)
