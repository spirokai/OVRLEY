# Skia WASM SIMD experiment — 2026-10-17

The SIMD source build succeeded and is adopted in `vendor/skia-wasm`.
Heading tape improved from 10.39 to 2.91 ms/frame (3.56x). Most other drawing-heavy
widgets improved, but gradient, current-lap and lap delta regressed. The largest
absolute regression is current-lap at 0.29 ms/frame. These remaining text-heavy
costs can be investigated separately.

## Build

The pinned rust-skia revision, Rust/Emscripten versions, library features and
widget drawing code are unchanged. Skia's C++ build receives `-msimd128 -msse2`.
This revision does not select a vector raster pipeline from `-msimd128` alone;
`-msse2` selects its SSE2 implementation, which Emscripten translates to WASM
SIMD. The application linker also receives `-msimd128`; Rust target-feature
flags are unchanged. See [Emscripten's SIMD documentation](https://emscripten.org/docs/porting/simd.html).

The first build failed because skcms unconditionally imports additional x86
headers under Clang, including unsupported AVX-512 headers. The source builder
now applies [a two-line header-guard patch](../../scripts/patches/skcms-emscripten-intrinsics.patch)
that excludes these extra imports on Emscripten. Its `immintrin.h` already
includes the enabled SSE2 headers. No drawing algorithms were patched.

Disassembling `core.SkOpts.o` finds 14,736 SIMD instruction occurrences, including
the `sse2::bilinear_*`, pixel load/store and low-precision raster stages. The
scalar object has zero. Disassembly of the final linked module finds 68,652
SIMD instruction occurrences (`v128`, `f32x4`, `i32x4`, `i16x8`, `i8x16`). These
are static counts, not runtime instruction counts or CPU profile samples.
Evidence: [inspection log](../../debug/skia-simd/inspection.log) and
[raster-pipeline disassembly](../../debug/skia-simd/skia-bindings-fea9b00334bead74-SkOpts.txt).

The temporary scalar vendor and generated JS/WASM backups were removed after
accepting the SIMD result. The original vendor archive remains in Git history;
the scalar benchmark and appearance evidence are retained under `debug/`.

| Artifact                | SHA-256                                                            |
| ----------------------- | ------------------------------------------------------------------ |
| Scalar Skia archive     | `c73a14119702c79e153b64a36b76e82f446dafa45dc2a72442415e30f228b891` |
| SIMD Skia archive       | `42eaa042f816449dd92b3a23edeba0e81249c5d991c497a3ff3534ad13167be3` |
| Scalar application WASM | `9e25e06b60721654bc9c8f3a647afed4c983e9aef6be6642f9b427339a006e12` |
| SIMD application WASM   | `f2685a9b1dabf3f32dd40120f4587a1f87fc178a4a6f5c97e5b5f5aeae24aaef` |

The application WASM grew from 4,976,878 to 5,346,755 bytes (7.4%).

## Timings

All runs use `node benchmarks/widgets/run.mjs --run --backend wasm`, with the
same 24 cases, 200 warmup frames, 1,000 measured frames, fonts, inputs and 30 FPS
timeline. The table measures actual `wasm.render` time per playback frame,
including skipped redraws and readback, excluding JavaScript pixel copying and
canvas upload. No native benchmark or desktop application was rebuilt.

The scalar baseline is [1792231919596-8564](../../debug/benchmarks/widgets/1792231919596-8564/summary.md).
Two SIMD passes completed. The first overlapped with disassembly/appearance
checks during its early cases; the table uses the second, timing-only pass,
[1792232286886-21792](../../debug/benchmarks/widgets/1792232286886-21792/summary.md).
The first pass's heading, gradient and current-lap timings were respectively
2.89, 0.454 and 1.147 ms/frame, agreeing with the second pass's mixed result.
These sequential local passes do not establish statistical confidence intervals.

| Widget            | Scalar ms/frame | SIMD ms/frame |      Scalar / SIMD |
| ----------------- | --------------: | ------------: | -----------------: |
| Heading tape      |         10.3857 |        2.9138 |              3.56x |
| Course            |          1.1228 |        0.5426 |              2.07x |
| Elevation         |          4.1623 |        3.2410 |              1.28x |
| G-force           |          1.8735 |        1.3177 |              1.42x |
| Lap-log           |          1.6083 |        1.3287 |              1.21x |
| Lean angle        |          0.7945 |        0.6272 |              1.27x |
| Speed linear fill |          0.2157 |        0.1425 |              1.51x |
| Speed linear bars |          0.2049 |        0.1268 |              1.62x |
| Speed arc fill    |          1.0930 |        0.8590 |              1.27x |
| Speed arc bars    |          1.0243 |        0.7591 |              1.35x |
| Speed corner fill |          1.1430 |        0.7642 |              1.50x |
| Speed corner bars |          1.0569 |        0.7083 |              1.49x |
| Gradient          |          0.3072 |        0.4556 | 0.67x (48% slower) |
| Current lap       |          0.8706 |        1.1602 | 0.75x (33% slower) |
| Lap delta         |          0.0628 |        0.0913 | 0.69x (45% slower) |

All 24 cases retain their redraw counts and raster dimensions. The text-heavy
regressions repeat in both SIMD passes; their cause has not been profiled.
The Rust alpha-conversion loop was not separately optimized. There is no new
native comparison here and no claim about live video/GPU presentation.
Full comparison: [comparison.json](../../debug/skia-simd/comparison.json).

## Appearance

The existing visual diagnostic was reused with separate output under
`debug/skia-simd/visual`, preserving the previous shadow experiment's evidence.
It checks 11 widget styles in normal, shadow-disabled and fractional/translucent/
bordered configurations, for 33 browser raster comparisons in total.

All geometry JSON is identical. Twenty-two cases are pixel-identical; eleven
have rounding differences. Maximum alpha and premultiplied RGB differences are
both 2/255; opaque foreground RGB is unchanged. Side-by-side inspection of the
fractional/translucent g-force case shows equivalent appearance. This does not
claim pixel identity for every configuration or cover every animation frame.
Evidence: [visual metrics](../../debug/skia-simd/visual/updated-visual.json),
[g-force comparison](../../debug/skia-simd/visual/comparison-g-force-fractional-translucent-border.png),
and [heading comparison](../../debug/skia-simd/visual/comparison-heading-tape-fractional-translucent-border.png).
