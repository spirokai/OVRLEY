# Skia WASM SSE4.1 experiment — 2026-10-17

Changing Skia from `-msimd128 -msse2` to `-msimd128 -msse4.1` reduced heading
tape's render time from 2.8831 to 2.6373–2.6429 ms/frame in two passes, an
8.3–8.5% reduction. Other changes are small and mixed. Gradient, current-lap
and lap delta remain approximately as expensive as in the SSE2 build.
The SSE4.1 bundle is accepted and active; the preceding SSE2 checkpoint
is commit `e8a854c2` (`perf(wasm): enable Skia SIMD raster pipeline`).

## Build and verification

Only the C++ instruction-selection flag changed. Skia source revision, the
existing skcms header patch, library features, toolchain, Rust flags, link flags
and widget code are unchanged. Source compilation and ordinary archive-consumer
linking both succeeded. No native benchmark or desktop application build ran.

`core.SkOpts.o` now contains `sse41::` raster routines. Disassembly shows direct
`f32x4.floor` instructions in place of SSE2's rounding sequences. Static SIMD
instruction occurrences fell from 14,736 to 13,814 in that object, and from
68,652 to 67,979 in the final application module. These counts establish code
generation differences; they are not runtime instruction counts or an isolated
measurement of why heading improved.
Evidence: [inspection log](../../debug/skia-sse41/inspection.log) and
[raster-pipeline disassembly](../../debug/skia-sse41/SkOpts.txt).

| Artifact                | SHA-256                                                            |
| ----------------------- | ------------------------------------------------------------------ |
| SSE2 Skia archive       | `42eaa042f816449dd92b3a23edeba0e81249c5d991c497a3ff3534ad13167be3` |
| SSE4.1 Skia archive     | `b7fa8f2b52adb915708d8be9794ec69709be086c72b825831af4de1dd57d4af2` |
| SSE2 application WASM   | `f2685a9b1dabf3f32dd40120f4587a1f87fc178a4a6f5c97e5b5f5aeae24aaef` |
| SSE4.1 application WASM | `d66bf7ac5b737c51ff90a7cfc49525943556df8de60af29c7edbe7f610893e21` |

The application WASM shrank from 5,346,755 to 5,329,108 bytes (0.33%).
A local copy of the SSE2 vendor bundle and generated JS/WASM pair is retained
under `debug/skia-sse41/baseline-sse2/`. Both scalar backup directories from
the initial experiment were removed before committing the SSE2 result.

## Timings

All runs used `node benchmarks/widgets/run.mjs --run --backend wasm` with the
same 24 cases, inputs, fonts, 200 warmup frames, 1,000 measured frames and 30 FPS
timeline. No compilation or appearance/disassembly checks ran alongside these
timing passes. Values are actual `wasm.render` milliseconds per playback frame,
including skipped redraws and readback, excluding JavaScript pixel copying and
canvas upload.

The fresh SSE2 baseline is [1792233163090-3364](../../debug/benchmarks/widgets/1792233163090-3364/summary.md).
SSE4.1 passes are [1792233835236-2476](../../debug/benchmarks/widgets/1792233835236-2476/summary.md)
and [1792233865136-9020](../../debug/benchmarks/widgets/1792233865136-9020/summary.md).
These sequential local passes do not establish statistical confidence intervals.

| Widget            |   SSE2 | SSE4.1 pass 1 | SSE4.1 pass 2 |
| ----------------- | -----: | ------------: | ------------: |
| Heading tape      | 2.8831 |        2.6373 |        2.6429 |
| Course            | 0.5977 |        0.5702 |        0.5585 |
| Elevation         | 3.2503 |        3.2785 |        3.2392 |
| G-force           | 1.3536 |        1.3323 |        1.3335 |
| Lap-log           | 1.3639 |        1.3358 |        1.3618 |
| Lean angle        | 0.6034 |        0.6345 |        0.6266 |
| Speed linear fill | 0.1457 |        0.1391 |        0.1548 |
| Speed linear bars | 0.1262 |        0.1262 |        0.1293 |
| Speed arc fill    | 0.8667 |        0.8922 |        0.8819 |
| Speed arc bars    | 0.7589 |        0.8021 |        0.7775 |
| Speed corner fill | 0.7703 |        0.7890 |        0.8187 |
| Speed corner bars | 0.7460 |        0.7200 |        0.7418 |
| Gradient          | 0.4696 |        0.4714 |        0.4651 |
| Current lap       | 1.1883 |        1.1591 |        1.1946 |
| Lap delta         | 0.0866 |        0.0860 |        0.0872 |

Heading's gain repeats in both passes. Several gauge/lean-angle timings
increase slightly; the largest increase in this table is 0.0484 ms/frame for
corner fill in pass 2. The smaller movements need more repeated measurements
before attributing them to the compiler flag. No broad speedup is claimed.
All 24 cases retain their redraw counts and raster dimensions.
Full comparisons: [pass 1](../../debug/skia-sse41/comparison-first-pass.json)
and [pass 2](../../debug/skia-sse41/comparison.json).

## Appearance

All 36 browser raster comparisons are pixel-identical to SSE2, with identical
geometry JSON. They cover 12 widget styles, including lap delta, in normal,
shadow-disabled and fractional/translucent/bordered configurations. This checks
the selected diagnostic frames rather than every frame or configuration.
Evidence: [metrics](../../debug/skia-sse41/visual/updated-visual.json),
[heading comparison](../../debug/skia-sse41/visual/comparison-heading-tape-fractional-translucent-border.png)
and [lap-delta comparison](../../debug/skia-sse41/visual/comparison-lap-delta-normal.png).
