# Group interaction profiling

```sh
pnpm benchmark:interactions
pnpm benchmark:interactions --label before --frames 300 --burst 4
pnpm benchmark:interactions --label after --frames 300 --burst 4 --compare debug/benchmarks/widget-interactions/<before-run>/results.json
pnpm benchmark:interactions --workload mixed --operation resize --widgets 15
pnpm benchmark:interactions --label rect-stacks --workload mixed --operation drag --frames 30 --rect-stacks
```

The default runs group dragging and resizing for 15 text widgets, then a mixed group of 15 widgets. Each case warms up for 30 input frames and measures 180 frames. The mixed group includes text, labels, gauges, heading tape, backdrops, route, and elevation. The scene uses the existing 4K futuristic HUD defaults and the local afternoon ride fixture, paused at 60 seconds. `--burst` controls mouse moves per animation frame so coalescing can be checked under faster input. `--warmup 0` explicitly disables warm-up.

This is a separate browser page using the production editor hooks, draft store, canvas, WASM session, selection/badge geometry, Moveable, and inspector widget derivation. Synthetic mouse events go through Moveable; the benchmark verifies that a group gesture began, every selected widget presented changed geometry, movement continued during measurement, and each gesture committed exactly once. Config uploads and resize calls are counted, but are allowed to disappear after an optimization. Toolbar, sidebar controls, video playback, and desktop IPC are excluded. Only resource loading is replaced with local fixture transport. Production source is unchanged.

The runner requires the existing WASM artifact, native `benchmark_widgets` executable (only for preparing serialized fonts), local ride fixture, and installed Playwright Chromium. It never builds or downloads anything. The normal app development server rebuilds WASM on demand; this runner serves existing artifacts directly. Rebuild explicitly before measuring Rust changes.

Results are saved under `debug/benchmarks/widget-interactions/<timestamp>-<label>-<pid>/`. `results.json` retains raw samples, protocol, browser/CPU/viewport, Git state, and the WASM hash; `summary.md` reports averages, p95, maximums, operation counts, pixel traffic, and optional before/after stage deltas. `fixtures.json` records the canonical configs. A comparison requires the same full protocol.

`--rect-stacks` adds rectangle-read counts grouped by caller stack and element to `results.json`. Use it to distinguish Moveable reads from badge/border work. Stack collection adds overhead, so use separate normal runs for timing; this flag cannot be combined with `--compare`.

What to compare after each optimization:

- Scheduler changes: first input to all group geometry, draft callback counts, and preview callback counts. The first-response probe runs after idle and waits for all selected widgets before sending another input, avoiding attribution to a newer pointer event.
- Placement/cache changes: WASM config-update/render time, pixel-copy and canvas-upload counts/bytes.
- Batch resizing: resize/config call counts, JSON time, and preview callback time.
- Draft feedback changes: extra preview callbacks, serialization, and React render duration.
- Selection changes: explicit Moveable `updateRect` time and DOM rectangle-read time.

Timers intentionally overlap. `raf.render` includes config synchronization, WASM rendering, copying, and canvas submission; `moveable.updateRect` includes its DOM reads. Do not sum nested stages. Frame intervals include the browser's refresh pacing, so a faster change may reduce CPU time without changing FPS. The >25 ms interval count is a rough 60 Hz indicator, not a display-independent dropped-frame count. Canvas upload records API submission, and geometry events do not measure physical display latency. Run a few times with the same browser, machine load, workload, and artifact settings before interpreting small differences. These numbers describe instrumented Chromium development code, not the shipping Tauri WebView.
