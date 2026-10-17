# Rust WASM SIMD experiment — 2026-10-17

The subsequent [alpha lookup experiment](alpha-lookup.md) removes the scalar
divisions with a code change. The measurements below isolate the earlier
Rust compiler-flag experiment and retain the division-based implementation.

Adding `-C target-feature=+simd128` to the Rust Emscripten target builds
successfully and produces SIMD instructions in the Rust widget crate. Two
full timing passes show small, mixed differences from the accepted SSE4.1
build, without a clear overall speedup. All 36 appearance cases are
pixel-identical. The Rust SIMD flag is retained alongside the lookup change.

The preceding SSE4.1 result is committed as `7b4bd385`
(`perf(wasm): select Skia SSE4.1 raster routines`). This experiment only changes
`src-tauri/wasm-bindings/.cargo/config.toml`. The C++ Skia archive is unchanged;
no LTO, codegen-unit, algorithm or rendering changes were added.

## Compiled code

Disassembling the compiled `ovrley_widgets` Rust library finds zero SIMD
instruction occurrences before the flag and 4,791 after it, across 343 function
bodies. These are pre-link object counts, not executed instructions or proof
that those functions dominate the benchmark.

The important readback function, `render::pixels::read_surface_rgba`, still
performs three scalar `i32.div_u` operations per partially transparent pixel.
Its six new SIMD instruction occurrences are `v128.load`/`v128.store` pairs
for metadata/result copies, not vectorized alpha conversion. Enabling the
feature therefore did not solve the previously identified conversion cost.
Evidence: [inspection](../../debug/rust-simd/inspection.json),
[baseline readback](../../debug/rust-simd/baseline-readback.txt) and
[SIMD readback](../../debug/rust-simd/simd-readback.txt).

| Artifact                      | SHA-256                                                            |
| ----------------------------- | ------------------------------------------------------------------ |
| Unchanged SSE4.1 Skia archive | `b7fa8f2b52adb915708d8be9794ec69709be086c72b825831af4de1dd57d4af2` |
| Baseline application WASM     | `d66bf7ac5b737c51ff90a7cfc49525943556df8de60af29c7edbe7f610893e21` |
| Rust SIMD application WASM    | `946763437471f66d3c92096766c3a38d603283c3192284c44b0893336e26c363` |

The application WASM shrank from 5,329,108 to 5,310,148 bytes (0.36%). The
baseline JS/WASM pair and configuration are saved locally under
`debug/rust-simd/baseline/`; candidate artifacts are in
`debug/rust-simd/candidate/`. Ordinary archive-consumer compilation succeeded
without rebuilding C++ Skia. No native or desktop application build ran.

## Timings

All runs used `node benchmarks/widgets/run.mjs --run --backend wasm` with the
same 24 cases, inputs, fonts, 200 warmup frames, 1,000 measured frames and
30 FPS timeline. Compilation, disassembly and appearance checks finished before
the timing passes. Values below are actual `wasm.render` milliseconds per
playback frame, including skipped redraws and readback, excluding JavaScript
pixel copying and browser canvas upload.

Baseline: [1792235208957-16652](../../debug/benchmarks/widgets/1792235208957-16652/summary.md).
Rust SIMD: [pass 1](../../debug/benchmarks/widgets/1792235421034-14672/summary.md)
and [pass 2](../../debug/benchmarks/widgets/1792235497071-8992/summary.md).

| Widget            | Baseline | Rust SIMD pass 1 | Rust SIMD pass 2 |
| ----------------- | -------: | ---------------: | ---------------: |
| Heading tape      |   2.6730 |           2.7357 |           2.6249 |
| Course            |   0.5208 |           0.5304 |           0.5372 |
| Elevation         |   3.3040 |           3.2391 |           3.2457 |
| G-force           |   1.3137 |           1.3886 |           1.3122 |
| Lap-log           |   1.3222 |           1.3418 |           1.3101 |
| Lean angle        |   0.6145 |           0.6527 |           0.6198 |
| Speed linear fill |   0.1417 |           0.1428 |           0.1392 |
| Speed linear bars |   0.1248 |           0.1311 |           0.1248 |
| Speed arc fill    |   0.8434 |           0.8532 |           0.8518 |
| Speed arc bars    |   0.7487 |           0.7679 |           0.7508 |
| Speed corner fill |   0.7913 |           0.7548 |           0.7629 |
| Speed corner bars |   0.7284 |           0.7104 |           0.7054 |
| Gradient          |   0.4645 |           0.4650 |           0.4657 |
| Current lap       |   1.1651 |           1.1699 |           1.1362 |
| Lap delta         |   0.0839 |           0.0844 |           0.0861 |

These sequential local passes do not establish a statistical margin of error.
Several changes reverse direction between passes. Elevation and corner gauges
improve slightly in both, while course and arc timings increase slightly; the
small differences are insufficient to establish a broad performance benefit.
All 24 cases retain their redraw counts and raster dimensions.
Full comparisons: [pass 1](../../debug/rust-simd/comparison-first-pass.json)
and [pass 2](../../debug/rust-simd/comparison.json).

## Appearance and upstream settings

All 36 browser raster comparisons and geometry JSON are identical. These cover
12 widget styles in normal, shadow-disabled and fractional/translucent/bordered
configurations at the selected diagnostic frames.
Evidence: [visual metrics](../../debug/rust-simd/visual/updated-visual.json).

The separate [upstream binary-build audit](rust-skia-build-flags.md) records
rust-skia's pinned WASM and Windows release settings. It also distinguishes
upstream's Rust ThinLTO setting from the disabled C++ LTO block; neither
optimization was added by this experiment.
