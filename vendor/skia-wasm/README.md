# Skia for the OVRLEY Wasm preview

This directory intentionally stores compiled third-party code in the repository.
The wasm-bindings crate uses a pinned rust-skia revision with Emscripten support
from PR #1336. Upstream release binaries do not supply this build contract.
Keeping the libraries and generated Rust declarations here makes ordinary Rust
preview edits independent of a full Skia source build, GN, Ninja and libclang.

`skia-binaries-<sha256>.tar.gz` is rust-skia's binary-cache format, containing
`libskia.a`, `libskia-bindings.a`, `bindings.rs` and upstream metadata. These are
WebAssembly static libraries, not host Windows libraries. `manifest.json`
records the source revision, toolchain, features and archive SHA-256. Both the
archive and manifest belong in version control. The content hash in the filename
also invalidates Cargo's cached import when the bundle changes. Generated application `.js` and
`.wasm` files stay in the ignored Cargo target directory.

Source: https://github.com/rust-skia/rust-skia/tree/1a80f6716b5ba787f6583443f7b62fa5f60e7084

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
