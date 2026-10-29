# Upstream rust-skia binary compilation flags

Inspected rust-skia revision `b7f043e0b1e2a850e702245de811f01df1f6b0ff`,
the 0.153.3 revision pinned by OVRLEY. The findings below describe its release
build recipes and the pinned Skia GN configuration. They are not recovered
compiler command lines from a published binary or claims about all versions.

`skia-safe` is the Rust API; its `skia-bindings` dependency builds and distributes
the C++ Skia libraries. Changing an application's Rust flags does not rebuild
the C++ contents of a downloaded archive.

## Release recipe

Both the WASM and Windows release workflows set `SKIA_DEBUG=0` and run
`cargo build -p skia-safe --release --features ... --target ...`. The builder
sets `is_official_build=true`, `is_debug=false` and `is_skia_standalone=false`.
Feature combinations are built separately. Upstream's workflows install or
update toolchains rather than pinning every compiler version, so reproducing
the exact code generation of an existing archive requires its original build
environment as well as these flags.

| Setting                 | WASM release                                                                   | Windows x64 release                                                                    |
| ----------------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| C++ optimization        | `-O3`, passed from Cargo's release optimization level and selected by GN       | `/O2`, `/Zc:inline` through GN                                                         |
| C++ compiler            | Emscripten clang                                                               | `clang-cl` selected through the installed LLVM directory                               |
| SIMD selection          | No explicit `-msimd128` or SSE option in the release workflow/platform builder | Baseline x64 code plus CPU-specific objects built with `/arch:AVX2` and `/arch:AVX512` |
| Floating-point settings | `-ffp-contract=off`                                                            | `/fp:precise`                                                                          |
| Code/data sections      | `-fdata-sections`, `-ffunction-sections`; GN also lists `-Wl,--gc-sections`    | GN lists `/OPT:ICF`, `/OPT:REF`                                                        |
| Size-optimization mode  | `skia_enable_optimize_size=false` by default                                   | Same default                                                                           |
| C++ LTO                 | The builder's `-flto` block is commented out                                   | Same block is commented out                                                            |
| Rust LTO                | Upstream workspace release profile sets `lto="thin"`                           | Same workspace setting                                                                 |

The linker flags listed above are GN configuration settings, not proof that
each is meaningful while producing a static archive or inherited by a consuming
Rust executable. The ordinary published bundle contains C++ static libraries
and generated bindings, rather than a finished browser application module.

Upstream's root `Cargo.toml` separately enables Rust ThinLTO for its workspace,
with a comment about building the `skia-org` example. Dependency workspace
profiles are not inherited by OVRLEY: our own application workspace controls
Rust LTO, and this setting does not enable C++ LTO in the archive.

The WASM workflow explicitly sets
`EMCC_CFLAGS="-s ERROR_ON_UNDEFINED_SYMBOLS=0 -s MAX_WEBGL_VERSION=2"`.
Skia's non-CanvasKit WASM GN configuration also adds `-pthread` and browser
link settings. This does not imply that OVRLEY's drawing work runs in parallel.
The WASM platform builder sets `target_cpu="wasm"`; it does not activate the
SSE raster implementations that our local SIMD build now selects.

Windows' AVX2/AVX-512 flags apply to specialized objects, not every source file.
Skia's `SkOpts::Init()` checks CPU capabilities before choosing those raster
routines. The configuration enables `SK_ENABLE_AVX512_OPTS`. This differs from
WASM, where enabling the vector implementation required our explicit flags.

## Implications for OVRLEY

OVRLEY's WASM Skia build already uses the upstream release optimization level,
then adds native WASM exceptions and `-msimd128 -msse4.1`. Upstream's release
recipe has no additional SIMD flag to copy into that build.

The commented LTO block explicitly says that LTO generated corrupt libraries on
host platforms with release builds. That is a historical source-code warning,
not evidence that every current LTO configuration fails. A later LTO experiment
must test the actual Rust/Emscripten combination and archive-consumer link.
Rust-only SIMD and Rust-only LTO are separate experiments from C++ Skia LTO.

## Pinned sources

- [Workspace release profile](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/Cargo.toml)
- [WASM binary-release workflow](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/.github/workflows/wasm-binaries.yaml)
- [Windows binary-release workflow](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/.github/workflows/windows-binaries.yaml)
- [GN argument construction and disabled LTO block](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/skia-bindings/build_support/skia/config.rs)
- [WASM platform configuration](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/skia-bindings/build_support/platform/emscripten.rs)
- [Windows platform configuration](https://github.com/rust-skia/rust-skia/blob/b7f043e0b1e2a850e702245de811f01df1f6b0ff/skia-bindings/build_support/platform/windows.rs)

The Skia sources were inspected in the pinned checkout at
`src-tauri/target/wasm-toolchain/rust-skia/skia-bindings/skia/`: optimization,
floating-point and WASM settings in `gn/skia/BUILD.gn`; defaults in
`gn/skia.gni`; specialized CPU objects in `BUILD.gn`; runtime raster selection
in `src/core/SkOpts.cpp`.
