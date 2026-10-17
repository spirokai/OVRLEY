# Skia for the OVRLEY Wasm preview

This directory intentionally stores compiled third-party code in the repository.
Both native and WASM pin rust-skia release 0.153.3. Native uses the published
binary; WASM uses a local source build with native WASM exceptions because the
published WASM archive fails to link with this SDK (`emscripten_longjmp`).
Keeping the libraries and generated Rust declarations here makes ordinary Rust
preview edits independent of a full Skia source build, GN, Ninja and libclang.

`skia-binaries-<sha256>.tar.gz` is rust-skia's binary-cache format, containing
`libskia.a`, `libskia-bindings.a`, `bindings.rs` and upstream metadata. These are
WebAssembly static libraries, not host Windows libraries. `manifest.json`
records the source revision, toolchain, features and archive SHA-256. Both the
archive and manifest belong in version control. The content hash in the filename
also invalidates Cargo's cached import when the bundle changes. Generated application `.js` and
`.wasm` files stay in the ignored Cargo target directory.

Source: https://github.com/rust-skia/rust-skia/tree/b7f043e0b1e2a850e702245de811f01df1f6b0ff

`LICENSE.skia` (BSD-3-Clause) and `LICENSE.rust-skia` (MIT) are the two
primary licenses; `THIRD_PARTY_NOTICES.txt` consolidates them plus the notices
for dependencies in this bundle's GN compile graph: Expat, FreeType, libjpeg-turbo,
libpng, Wuffs and zlib, including separately licensed code embedded in FreeType.
All three are regenerated together by `pnpm wasm:skia:build`. Retain them with
distributions of the bundle. Links alone do not replace required notices.
Unused optional dependencies, tests and general documentation are excluded
(ICU, HarfBuzz, brotli and libwebp are not linked by this build).
Re-audit this list when changing the Skia revision or enabled features.

Regenerate with `pnpm wasm:skia:build`, then run `pnpm wasm:build` to verify
consumption of the stored archive. See
[the migration setup guide](../../src-tauri/wasm-bindings/README.md).

## SIMD rendering

The source build enables `-msimd128 -msse2`. Emscripten translates the SSE2
intrinsics to WebAssembly SIMD; the SSE2 macro selects this Skia revision's
vector raster pipeline. The application linker also receives `-msimd128`.
Rust's own code generation flags are unchanged, keeping the alpha-conversion
loop outside this experiment. The generated manifest records the C++ flags.
The source builder applies `scripts/patches/skcms-emscripten-intrinsics.patch`
to omit skcms's extra Clang header imports on Emscripten; its `immintrin.h`
already includes the enabled SSE2 headers. No drawing code is patched.

The SIMD bundle is adopted following the measured gains and appearance checks
in [the experiment report](../../benchmarks/widgets/skia-simd.md). The temporary
scalar backups were removed; the original archive remains available in Git history.
