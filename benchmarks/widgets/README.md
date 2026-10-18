# Widget playback benchmark

`run.mjs` coordinates the run and writes results, `browser.js` defines cases and exercises the production WASM ABI, `native.rs` calls the native export renderer, `preview.rs` runs the same preview implementation natively, and `summarize.mjs` reports existing results. The only frontend change exports the existing font-request helper.

```sh
pnpm benchmark:widgets:build             # Compile native + WASM; does not run playback
pnpm benchmark:widgets --list            # List cases; default operation
pnpm benchmark:widgets --plan            # Prepare input files; no rendering
pnpm benchmark:widgets --run             # Run both backends, only after approval
pnpm benchmark:widgets --run --case course --backend wasm
pnpm benchmark:widgets --run --case lap-lap_log --backend native-preview
```

The build uses the existing Rust/Skia/Emscripten setup. Execution uses prebuilt artifacts and an installed Playwright Chromium browser; it never builds or downloads dependencies automatically.

Each case gets one persistent widget, 200 warm-up frames, and **one 1,000-frame pass** through a consecutive 30 FPS timeline. `--frames` and `--warmup` override the counts. No repetitions, config edits, resize gestures, or scrubbing. All renderers run serially and use the same activity, resolved config, and frame times. Setup and warm-up are excluded from playback timings. The default `both` backend runs native export, native preview, and WASM; `native`, `native-preview`, and `wasm` select one workflow.

There are 24 widget cases plus an empty-scene baseline: all standard families, including diving profile; speed as text, linear, arc, and corner (including fill/bars); heading tape; lean angle; G-force; and four lap-timer modes. Time and backdrop each cover their two basic variants.

Sizes come from the HUD's 4K scene and scale of 2. Route is 1120 x 516, elevation and diving profile are 1236 x 326, heading tape 1200 x 200, circular gauges about 464 px across, and linear gauges 464 x 80. The recorded raster dimensions include the renderer's effects/ink bounds. The raster case uses the bundled app icon.

General cases use `afternoon-ride-parse-debug.json` starting at 60 seconds. Lean angle uses `sample RaceChrono-parse-debug.json` at 60 seconds. G-force and laps use `sample Racebox-parse-debug.json` at 400 seconds; its playback window crosses the lap boundary at 427.24 seconds. Diving profile uses `Shearwater-Cloud-Dive-01-parse-debug.json` at 1560 seconds; its playback window crosses the stop-depth change from 6 m to 3 m at 1580 seconds. These local files must be present under `debug/activities`.

Results go to `debug/benchmarks/widgets/<timestamp>-<pid>/`: `native.json`, `native-preview.json`, and `wasm.json` hold each renderer's samples, `results.json` also includes resolved inputs and environment details, and `summary.md` reports averages by widget and operation. JSON retains setup timings, raster dimensions, and preview redraw counts.

`environment.artifacts` records the actual binary paths, modification times, and SHA-256 hashes. Source edits and Rust tests do not rebuild the production WASM artifact; use an explicit build before measuring changed rendering code.

Regenerate Markdown from saved JSON without running playback, using only the run ID:

```sh
pnpm benchmark:widgets:summary 1792161096230-42800
```

The script reads available renderer JSON files from `debug/benchmarks/widgets/<run-id>/` and writes `summary.md` there. It ignores `results.json`, which may describe only one backend when runs were performed independently. Any renderer can be reported alone. The first widget table compares native preview with WASM preview and reports their ratio. Both use `PreviewSession` with the same widget-sized premultiplied surfaces, sampling, fonts, unchanged-value skips, RGBA readback, alpha conversion, and geometry serialization. Native calls the session directly; WASM also crosses its browser ABI. Neither preview render timer includes JavaScript pixel copying or canvas upload.

A separate workflow table reports native export drawing, native export total, and WASM playback. Native export draws directly into a straight-alpha 4K frame and includes full-scene restoration only in its total. These workflow scopes differ and must not be used as an isolated native/WASM backend ratio. The operation table includes averages per playback frame and per operation call. Missing renderers and operations with no calls display `—`.

WASM stages cover rendering, geometry handling, RGBA copying, and canvas upload. Rust retains the real widget caches and unchanged-value skips. The browser harness exercises these calls directly; React scheduling and editor DOM placement are excluded. Canvas upload measures API submission rather than eventual display completion.

Native stages use the existing profiler, including full-frame restoration, surface wrapping, widget drawing, and `frame.draw`. The empty case exposes common host cost. Native output is a complete 4K frame, so compare widget draw buckets as well as total time. `frame.draw` includes its child widget buckets; do not add them together. Native export does not expose redraw counts.

Static widgets remain cached. No forced redraws, encoding, video decoding, PNG output, or Tauri IPC occurs in the measured loop. Fine-grained profiling can follow if these results identify a hotspot. Existing runs can gain a comparable native preview measurement without rebuilding WASM: after compiling the updated native benchmark, run `src-tauri/target/release/benchmark_widgets.exe --run-preview debug/benchmarks/widgets/<run-id>/plan.json`, then regenerate that run's summary.
