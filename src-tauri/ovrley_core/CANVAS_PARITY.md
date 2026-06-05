# Canvas Parity Test

Compares a Rust Skia-rendered frame against a Playwright-captured browser SVG
frame to detect pixel-level rendering differences between the two pipelines. Generates screenshots from both pipelines, computes SSIM scores, and produces a diff image highlighting mismatches, and a JSON summary of the results.

## How to run

```bash
cargo test -p ovrley_core --features canvas-parity --test canvas_parity_tests -- --nocapture
```

## Dependencies

| Tool                        | Required for                                     |
| --------------------------- | ------------------------------------------------ |
| Node.js                     | Running the Playwright screenshot script         |
| pnpm                        | Running Vite dev server (frontend app)           |
| Playwright browsers         | Headless Chromium for SVG capture                |
| ffmpeg                      | PNG decode, SSIM comparison, diff image encoding |
| ffprobe (ships with ffmpeg) | PNG dimension probing                            |

Before first run, from the repo root:

```bash
pnpm install
npx playwright install chromium
```

## Fixtures

The test reads two fixture files from `fixtures/`:

| Fixture  | Path                                     | Format                   |
| -------- | ---------------------------------------- | ------------------------ |
| Activity | `fixtures/activity/gpx-parse-debug.json` | Raw parsed activity JSON |
| Config   | `fixtures/config/test-template-4k.json`  | Ovrley template config   |

These must be valid inputs for `parse_activity_json` and `parse_template_json`
respectively.

## Pipeline

1. **Parse fixtures** → `ParsedActivity` + `RenderConfig`
2. **Build dense report** → interpolated frame data via `build_dense_activity_report`
3. **Render Skia** → `skia.png` via `render_preview_with_prepared_assets`
4. **Write mock data** → `template.json`, `activity.json`, `store-state.json`
   for the frontend app
5. **Start Vite** → dev server hosting the React editor app
6. **Playwright screenshot** → headless Chromium captures the widget layer → `canvas.png`
7. **SSIM** → ffmpeg compares the two PNGs (cropped to alpha union)
8. **Diff image + stats** → pixel-by-pixel comparison, `canvas-parity.png` + `summary.json`

## Output artifacts

All written to `src-tauri/ovrley_core/tests/canvas_parity/`:

| File                | Contents                                           |
| ------------------- | -------------------------------------------------- |
| `skia.png`          | Frame rendered by Rust Skia                        |
| `canvas.png`        | Frame captured from the browser via Playwright     |
| `canvas-parity.png` | Pixel diff overlay (red = significant mismatch     |
| )                   |
| `summary.json`      | SSIM scores + mismatch statistics + artifact paths |

Intermediate temp files (mock data, debug renders) go under
`src-tauri/target/canvas-parity/` and are not cleaned up after the run.

## Interpreting results

### SSIM threshold

The test passes if `combined >= 0.98`. This accounts for minor glyph
rasterization differences between Skia's FreeType and Chromium's FreeType.

### Mismatch categories

Printed to stdout and written to `summary.json`:

| Metric                  | What it means                                                                                             |
| ----------------------- | --------------------------------------------------------------------------------------------------------- |
| **Full frame**          | Any differing pixel, including transparent background. Inflated by empty area.                            |
| **Overlay only**        | Differing pixels excluding fully transparent pixels, i.e. only the overlay content.                       |
| **Threshold applied**   | Differing pixels outside of minor tolerance differences. Filters rasterization and compositing noise.     |
| **Clean (AA excluded)** | Excluding pixels at the edges due to anti-aliasing incosistencies. The MOST meaningful "real bug" metric. |
| **Preview exclusive**   | Content present in Canvas but not Skia.                                                                   |
| **Render exclusive**    | Content present in Skia but not Canvas.                                                                   |

Clean (AA excluded) metric along with SSIM are the most meaningful; but visual inspection of the diff image should always be a priority. Pixel is considered mismatched even if a single channel differs by a single point - because of this, a tiny threshold is applied to allow for rasterization/export artifacts.

Minor mismatch (<2% of pixels), even in AA-excluded results, **is expected and completely normal** due to rendering pipeline differences - e.g. slightly different character spacing in strings which cannot be perceived by a naked eye. Significant mismatch (>3% of pixels) likely indicates a real rendering bug, such as a missing widget, incorrect colors, or a compositing error.

### Diff image colours

- **Bright red** pixel = significant mismatch between the two renders.

### Threshold constants

Defined in `common/canvas_parity.rs`:

| Constant                     | Value | Purpose                                               |
| ---------------------------- | ----- | ----------------------------------------------------- |
| `ALPHA_MASK_THRESHOLD`       | 2     | Minimum alpha to count as "overlay"                   |
| `ONLY_PIXEL_ALPHA_THRESHOLD` | 96    | Alpha threshold for orphaned-pixel detection          |
| `DIFF_CHANNEL_TOLERANCE`     | 4     | Max per-channel delta before a pixel is "significant" |
| `EDGE_ALPHA_DELTA_THRESHOLD` | 0     | Neighbour alpha delta to qualify as an edge           |
| `EDGE_IGNORE_RADIUS`         | 0     | Radius around edges excluded from clean metric        |

## Troubleshooting

| Symptom                    | Likely cause                                                          |
| -------------------------- | --------------------------------------------------------------------- |
| Vite fails to start        | `pnpm install` not run; port already in use                           |
| Playwright fails           | Playwright browsers not installed (`npx playwright install chromium`) |
| SSIM always low            | Dimensions mismatch; Skia render may need scaling                     |
| "Canvas-only" pixels       | Widget not rendered by Skia (missing widget implementation)           |
| "Skia-only" pixels         | Widget visible in Skia but hidden/transparent in browser              |
| Terminal blocks after test | Vite process orphaned — fixed by `taskkill /T` in `ViteServer::drop`  |
