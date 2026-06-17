# OVRLEY Wasm Preview POC

This crate is the isolated build proof for the Skia Wasm preview experiment.
It does not depend on `ovrley_core`, Tauri commands, ffmpeg, filesystem font
lookup, or the existing React/SVG preview implementation.

## Scope

- Renderer code path: standalone `wasm_preview_poc` crate.
- Skia backend label: `software-raster-rgba8888`.
- Browser presentation model: exported C ABI functions render into a Wasm-owned
  RGBA8888 buffer that a frontend can copy into `ImageData` for a 3840x2160
  canvas.
- Production impact: none intended. Existing React/SVG preview and native
  Rust/Skia export remain unchanged.

## Build command

From the repository root:

```powershell
pnpm wasm:preview:build
```

Equivalent direct command:

```powershell
cd src-tauri
cargo +1.84.0 build -p wasm_preview_poc --target wasm32-unknown-emscripten --profile wasm-preview
```

## Required toolchain

- Rust toolchain: `1.84.0`
- Rust target for that toolchain: `wasm32-unknown-emscripten`
- Emscripten SDK with `emcc` and `emcc.bat` available on `PATH`
- Existing repo Rust dependency set, including `skia-safe = 0.75` with the
  repository's `binary-cache` feature

Install/check commands:

```powershell
rustup toolchain install 1.84.0
rustup +1.84.0 target add wasm32-unknown-emscripten
H:\tools\emsdk\emsdk_env.bat
emcc --version
```

On this Windows machine, Emscripten 6.0.0 installed `emcc.exe` but Rust invoked
`emcc.bat`. A small compatibility shim was added at
`H:\tools\emsdk\upstream\emscripten\emcc.bat`:

```bat
@echo off
call "%~dp0emcc.exe" %*
exit /b %ERRORLEVEL%
```

## Expected artifact location

Cargo places the browser-consumable artifacts under:

```text
src-tauri/target/wasm32-unknown-emscripten/wasm-preview/
```

The current verified artifacts are:

```text
src-tauri/target/wasm32-unknown-emscripten/wasm-preview/wasm_preview_poc.js
src-tauri/target/wasm32-unknown-emscripten/wasm-preview/wasm_preview_poc.wasm
```

The next frontend mount slice should load that generated Emscripten JS/Wasm
runtime output and call the exported `ovrley_wasm_preview_*` functions.

## Emscripten linker wrapper

The POC uses `src-tauri/wasm_preview_poc/tools/emcc-linker.bat`, which delegates
to `emcc-linker.py`. The wrapper removes Rust's `-fwasm-exceptions` linker flag
before forwarding to `emcc.bat`, and the Cargo config adds
`-sSUPPORT_LONGJMP=emscripten`.

The browser mount also needs enough Wasm heap to allocate a 3840x2160 RGBA
frame buffer. The scoped Cargo config therefore passes
`-sINITIAL_MEMORY=134217728` for this target.

This is scoped to the `wasm32-unknown-emscripten` target in
`src-tauri/.cargo/config.toml`. It is a workaround for a known rust-skia
ABI mismatch where `skia-bindings` prebuilt Emscripten objects expect the
legacy Emscripten longjmp ABI while newer Rust toolchains pass Wasm exception
link flags. The build uses Rust 1.84.0, matching the repository pin, because
the latest stable Rust std for this target also introduced Wasm exception
references that conflict with the workaround.

## Local verification note

This POC intentionally uses only operational build/artifact checks. Do not add
unit tests for this crate unless a later issue introduces benchmark logic that
needs minimal correctness coverage.

### Verification on 2026-06-17

Native dependency check:

```powershell
cargo check --manifest-path src-tauri/wasm_preview_poc/Cargo.toml
```

Result: passed.

Wasm build:

```powershell
pnpm wasm:preview:build
```

Result: passed with Rust 1.84.0, Emscripten 6.0.0, the `emcc.bat`
compatibility shim, and the scoped linker wrapper. The build produced
`wasm_preview_poc.js` and `wasm_preview_poc.wasm` in the `wasm-preview` target
directory.
