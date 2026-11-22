# Widget playback benchmark

`run.mjs` coordinates the run and writes results, `cases.rs` defines the cases with the editor's defaults and effective data, `native.rs` calls the export renderer, `preview.rs` runs the editor's preview session, and `summarize.mjs` reports existing results.

```sh
pnpm benchmark:widgets:build             # Compile the release binary; does not run playback
pnpm benchmark:widgets --list            # List cases; default operation
pnpm benchmark:widgets --plan            # Prepare input files; no rendering
pnpm benchmark:widgets --run             # Run export and preview, only after approval
pnpm benchmark:widgets --run --case course --backend native
pnpm benchmark:widgets --run --case lap-lap_log --backend native-preview
```

Every operation uses the prebuilt binary (`ovrley_app` bin `benchmark_widgets`); nothing builds or downloads automatically.

Each case gets one persistent widget, 200 warm-up frames, and **one 1,000-frame pass** through a consecutive 30 FPS timeline. `--frames` and `--warmup` override the counts. No repetitions, config edits, resize gestures, or scrubbing. All renderers run serially and use the same activity, resolved config, and frame times. Setup and warm-up are excluded from playback timings. The default `both` backend runs export and preview; `native` (export) and `native-preview` select one.

There are 52 widget cases plus an empty-scene baseline. They cover every standard widget and every display type: course, elevation, diving profile, rolling charts, gradient, label and raster; heading compass, heart zones, altitude tape and running splits; speed as text, linear, arc, and corner (including fill/bars), linear and arc again with a three-stop fill gradient (fill/bars, plus a flat-cap linear fill), GT gauge (fill only); heading tape; lean angle (solid and gradient); G-force; and four lap-timer modes. Time and backdrop each cover their two basic variants.

Sizes are the requested output pixels in the HUD's 4K scene at its scale of 2. Route is 1120 x 516, elevation and diving profile are 1236 x 326, heading tape 1200 x 200, circular gauges about 464 px across, and linear gauges 464 x 80. The recorded raster dimensions are the renderer's ink bounds with effects, so they differ from these. The raster case uses the bundled app icon.

The six `map-p<pitch>-<size>` cases cover pitch 0, 40 and 60 at 500 and 1000 px. The preview gets blank map frames at once, so it times the camera, marker, attribution and readback, not the map. Export skips these cases because it would need a map source.

General cases use `Evening_Ride-parse-debug.json` starting at 60 seconds. Altitude tape uses `Jul-14th-2026-12-45PM-Flight-Airdata-parse-debug.json` at 100 seconds, where the drone climbs from -5.6 m to 22.4 m. Lean angle uses `sample RaceChrono-parse-debug.json` at 60 seconds. G-force and laps use `sample Racebox-parse-debug.json` at 400 seconds; its playback window crosses the lap boundary at 427.24 seconds. Diving profile uses `Shearwater-Cloud-Dive-01-parse-debug.json` at 1560 seconds; its playback window crosses the stop-depth change from 6 m to 3 m at 1580 seconds. Running splits use `Evening_Ride-parse-debug.json` at 520 seconds with 1 km splits: three splits are complete when playback starts and the fourth completes at 549.7 seconds, inside the measured pass. The preview loads the split table that the native `--fonts` step derives with core, as the editor receives it; native export derives its own. These local files live under `debug/activities` and are not committed (a debug build writes one when it imports the activity); a case whose file is absent fails when it is selected, while `--case` runs only need their own file.

Results go to `debug/benchmarks/widgets/<timestamp>-<pid>/`: `native.json` and `native-preview.json` hold each renderer's samples, `results.json` also includes resolved inputs and environment details, and `summary.md` reports averages by widget and operation. JSON retains setup timings, raster dimensions, and preview redraw counts.

`environment.artifacts` records the binary's path, modification time, and SHA-256 hash. Source edits and Rust tests do not rebuild it; use an explicit build before measuring changed rendering code.

Regenerate Markdown from saved JSON without running playback, using only the run ID:

```sh
pnpm benchmark:widgets:summary 1792161096230-42800
```

The script reads available renderer JSON files from `debug/benchmarks/widgets/<run-id>/` and writes `summary.md` there. It ignores `results.json`, which may describe only one backend when runs were performed independently. Either renderer can be reported alone. Missing renderers and operations with no calls display `—`.

The preview renders into widget-sized premultiplied surfaces, with the editor's sampling, fonts, unchanged-value skips, RGBA readback, alpha conversion and geometry. Export draws directly into a straight-alpha 4K frame and includes full-scene restoration only in its total. The scopes differ and must not be read as a preview/export ratio.

Export's stages use the existing profiler, including full-frame restoration, surface wrapping, widget drawing, and `frame.draw`. The empty case exposes common host cost. `frame.draw` includes its child widget buckets; do not add them together. Export does not expose redraw counts.

Static widgets remain cached. No forced redraws, encoding, video decoding, or PNG output occurs in the measured loop. The other Markdown files beside this one are dated records of earlier work, much of it on the removed WASM preview: their paths, commands and artifacts are those of their day and mostly no longer exist.
