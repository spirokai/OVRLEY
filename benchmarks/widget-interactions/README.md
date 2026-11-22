# Group interaction profiling

```sh
cargo build -p ovrley_app --release --bin benchmark_interactions --locked   # the runner never builds
pnpm benchmark:interactions
pnpm benchmark:interactions --label before --frames 300 --burst 4
pnpm benchmark:interactions --label after --frames 300 --burst 4 --compare debug/benchmarks/widget-interactions/<before-run>/results.json
pnpm benchmark:interactions --workload mixed --operation resize --widgets 15
```

The default runs group dragging and resizing for 15 text widgets, then a mixed group of 15 widgets. Each case warms up for 30 input frames and measures 180 frames. The mixed group includes text, labels, gauges, heading tape, backdrops, route, and elevation. The scene uses the 4K futuristic HUD defaults and the committed `test-ride.fit` fixture, paused at 60 seconds, in a 1600 × 1000 editor at density 1. `--burst` controls pointer moves per frame, so coalescing can be checked under faster input. `--warmup 0` disables warm-up.

`native.rs` (`ovrley_app` bin `benchmark_interactions`) runs each case in its own process, without a window. It uses the app's store, gesture and draft actions, update loop and native preview. A pointer move is one dispatch, and the preview redraws once per frame, as the window schedules it. The benchmark verifies that every selected widget presented changed geometry on the first move, that the document was unchanged until the release, and that the release committed the gesture.

What is measured:

- `update`: one pointer move through the update loop (the action, the reconcile steps, the history step).
- `preview`: one redraw (config sync, layout, Skia raster).
- A frame: its moves and its redraw.
- The first move until the whole group has new geometry, and the release that commits the gesture.

The window is not measured: projections, pixel uploads, and presentation.

Results are saved under `debug/benchmarks/widget-interactions/<timestamp>-<label>-<pid>/`. `results.json` retains raw samples, the protocol, CPU, Git state, and the binary's hash; `summary.md` reports averages, p95, maximums, redraw counts and bytes, and optional before/after deltas. A comparison requires the same full protocol. Run a few times with the same machine load before interpreting small differences.
