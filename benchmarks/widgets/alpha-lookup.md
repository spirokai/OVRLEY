# Exact alpha-conversion lookup experiment — 2026-10-17

Replacing the three per-pixel integer divisions with table lookups improves
WASM preview rendering while preserving the output exactly. Two timing passes
put g-force at 0.836–0.841 ms/frame versus 1.341 ms before, about 1.6x faster.
Elevation, lap-log, lean angle and several gauges also improve.

## Implementation and correctness

The shared [pixel readback helper](../../src-tauri/ovrley_widgets/src/render/pixels.rs)
now uses a compile-time `[[u8; 256]; 256]` table indexed by alpha and channel.
Each entry retains the previous formula, `((channel * 255 + alpha / 2) / alpha)
as u8`. Transparent and opaque pixels retain their existing skip behavior.
There is no runtime initialization, allocation or approximation. The table
occupies 65,536 bytes of static data.

All three focused Rust tests passed with:

```text
cargo test --manifest-path src-tauri/Cargo.toml -p ovrley_widgets render::pixels::tests --locked
```

The new test exhaustively compares all 65,280 nonzero-alpha/channel byte pairs
against the previous integer formula. Existing tests still check surface
readback against Skia, alpha preservation, transparent/opaque pixels, repeated
readback without changing the drawing surface, and reused output buffers.

Disassembly of the Rust readback function confirms that its three `i32.div_u`
instructions are gone. The conversion uses byte loads from the table.
Evidence: [inspection](../../debug/alpha-lookup/inspection.json),
[division baseline](../../debug/alpha-lookup/baseline-readback.txt) and
[lookup version](../../debug/alpha-lookup/simd-readback.txt).

All 36 browser appearance comparisons are pixel-identical, with identical
geometry JSON. They cover 12 widget styles in normal, shadow-disabled and
fractional/translucent/bordered configurations at selected diagnostic frames.
Evidence: [visual metrics](../../debug/alpha-lookup/visual/updated-visual.json).

## Timings

The baseline already includes Skia SSE4.1 and Rust `+simd128`. Those compiler
settings and the Skia archive are unchanged. All runs use
`node benchmarks/widgets/run.mjs --run --backend wasm`: 24 identical cases,
inputs and fonts; 200 warmup frames; 1,000 measured frames; 30 FPS timeline.
Compilation, tests and visual/disassembly diagnostics finished before timing.

Values are actual `wasm.render` milliseconds per playback frame, including
skipped redraws and readback, excluding JavaScript copying and canvas upload.

Baseline: [1792235695087-3328](../../debug/benchmarks/widgets/1792235695087-3328/summary.md).
Lookup: [pass 1](../../debug/benchmarks/widgets/1792235764365-21052/summary.md)
and [pass 2](../../debug/benchmarks/widgets/1792235794141-16084/summary.md).

| Widget            | Division baseline | Lookup pass 1 | Lookup pass 2 |
| ----------------- | ----------------: | ------------: | ------------: |
| G-force           |            1.3414 |        0.8412 |        0.8355 |
| Elevation         |            3.2474 |        2.7190 |        2.7030 |
| Lap-log           |            1.3464 |        1.1377 |        1.1248 |
| Lean angle        |            0.6155 |        0.4863 |        0.4824 |
| Course            |            0.5444 |        0.4241 |        0.3998 |
| Heading tape      |            2.5909 |        2.5108 |        2.4940 |
| Gradient          |            0.4732 |        0.4401 |        0.4257 |
| Current lap       |            1.1489 |        1.0819 |        1.0686 |
| Lap delta         |            0.0854 |        0.0782 |        0.0803 |
| Speed linear fill |            0.1433 |        0.0920 |        0.0924 |
| Speed linear bars |            0.1247 |        0.0915 |        0.0877 |
| Speed arc fill    |            0.8526 |        0.7666 |        0.7612 |
| Speed arc bars    |            0.7468 |        0.6800 |        0.6863 |
| Speed corner fill |            0.7788 |        0.6968 |        0.6976 |
| Speed corner bars |            0.6982 |        0.6342 |        0.6266 |

The substantial gains repeat, especially for g-force, elevation and lap-log.
These sequential local passes do not establish statistical confidence
intervals; the smaller movements should not be read as precise speedups.
All 24 cases retain their redraw counts and raster dimensions.
Full comparisons: [pass 1](../../debug/alpha-lookup/comparison-first-pass.json)
and [pass 2](../../debug/alpha-lookup/comparison.json).

## Artifacts and scope

| Application WASM  | SHA-256                                                            |
| ----------------- | ------------------------------------------------------------------ |
| Division baseline | `946763437471f66d3c92096766c3a38d603283c3192284c44b0893336e26c363` |
| Lookup            | `9b1693a069c64377a2fd9ef635703bb44c42452894c7e35032a24407fc62a161` |

The WASM grew from 5,310,148 to 5,375,677 bytes, an increase of 65,529 bytes
(1.23%). The original application JS/WASM pair and pixel-helper source are
saved locally in `debug/alpha-lookup/baseline/`.

The helper is shared with native preview. The production WASM module and native
release benchmark were rebuilt successfully. No desktop application build ran.

## Combined native and WASM run

The native division baseline used the existing release benchmark executable:
[1792235867948-3796](../../debug/benchmarks/widgets/1792235867948-3796/summary.md).
The native benchmark was then rebuilt with the lookup table using:

```text
cargo build --manifest-path src-tauri/Cargo.toml -p ovrley_core --release --bin benchmark_widgets --locked
node benchmarks/widgets/run.mjs --run --backend both
```

The combined run is [1792235973957-12940](../../debug/benchmarks/widgets/1792235973957-12940/summary.md).
It completed all 24 cases for native export, native preview and WASM preview
(72 result records). Hosts ran sequentially. Inputs, fonts, frame counts and
timeline match the earlier passes. The WASM artifact is unchanged from the two
lookup-only passes above.

| Widget            | Native division baseline | Native lookup | WASM lookup |
| ----------------- | -----------------------: | ------------: | ----------: |
| G-force           |                   0.8782 |        0.6015 |      0.8517 |
| Elevation         |                   2.7035 |        2.3724 |      2.6936 |
| Lap-log           |                   0.6499 |        0.5688 |      1.1130 |
| Lean angle        |                   0.3389 |        0.2798 |      0.4621 |
| Course            |                   0.5218 |        0.4591 |      0.4196 |
| Heading tape      |                   1.8072 |        1.8335 |      2.4964 |
| Gradient          |                   0.0801 |        0.0675 |      0.4384 |
| Current lap       |                   0.2084 |        0.1688 |      1.0702 |
| Lap delta         |                   0.0157 |        0.0129 |      0.0759 |
| Speed linear fill |                   0.1159 |        0.0885 |      0.0894 |
| Speed linear bars |                   0.1380 |        0.1124 |      0.0865 |
| Speed arc fill    |                   0.5081 |        0.4469 |      0.7628 |
| Speed arc bars    |                   0.4276 |        0.3891 |      0.6739 |
| Speed corner fill |                   0.4773 |        0.4411 |      0.7256 |
| Speed corner bars |                   0.4483 |        0.4118 |      0.6560 |

Values are preview render milliseconds per playback frame. Native preview and
WASM preview include the same sampling, skips, drawing, readback, conversion
and geometry work. Their font rasterizers can produce slightly different
metrics and bitmap sizes; cross-host pixel identity is not claimed. All 24
native cases retain their baseline raster dimensions and redraw counts, and
all native/WASM redraw counts agree.

The native measurements show g-force about 1.46x faster, elevation and lap-log
about 1.14x, and lean angle about 1.21x. Native heading tape is slightly slower
in this single pass (0.0263 ms/frame); small differences need more repeated
measurements before attribution. Native export draw and total timings are in
the combined summary's separate workflow table; they have a different scope
from preview timings and no new export before/after claim is made.

Native executable hashes:

- Division baseline: `f7d9b7d96b17d22b489803a8bc816d128a8235b1069d96358d31b97411696913`
- Lookup: `ea71e75811fe217c3f2f96d861ad8a5d72504549ad0b24d769590be900af1968`

The original native executable is saved in `debug/alpha-lookup/baseline/`.
Detailed comparison: [native-comparison.json](../../debug/alpha-lookup/native-comparison.json).
