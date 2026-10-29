# Frontend preview migration setup

This crate is OVRLEY's Emscripten binding boundary. It is an isolated Cargo
workspace to isolate its Emscripten toolchain and output. Native and WASM both
pin rust-skia 0.153.3 at `b7f043e0b1e2a850e702245de811f01df1f6b0ff`.
OverlayEditor uses the shared `ovrley_widgets` renderer for every widget family.

## Daily development

1. Install Rust `1.98.1` with `wasm32-unknown-emscripten` and activate Emscripten
   `6.0.9` in your existing emsdk installation.
2. Copy root `.env.example` to `.env.local` and set `EMSDK`.
   Set `PYTHON` if the Python on PATH is older than 3.10. Local paths are ignored
   by Git; shell variables take precedence.
3. Run `pnpm dev` from the repository root to test OverlayEditor in Tauri.
   Both the Tauri launcher and WASM scripts load `.env.local` automatically.
   No checkpoint launcher or native Skia source-tool paths are needed.

App bootstrap initializes the module under the existing “OVRLEY is starting”
screen and waits for it before showing the editor. The first WASM
asset request runs `pnpm wasm:build`. Subsequent Rust, Cargo,
toolchain, build-script or stored-bundle changes trigger serialized rebuilds
through Vite's existing watcher and reload the page. Edits arriving during a
build queue another build. Requests wait for the current build; a failed build
returns HTTP 503 and a Vite error overlay, never old artifacts. React keeps its
standard HMR and releases renderer allocations on effect cleanup. Tauri ignores
this crate for native restarts. No extra watcher process or watch dependency is
needed. Ordinary bindings builds reuse the stored Skia archive; they need
Emscripten and Python, but no Skia source, Ninja or libclang. The Tauri
development CSP explicitly permits Wasm compilation.

`pnpm wasm:build` also runs independently. It verifies the stored bundle's pins
and SHA-256 and forces rust-skia's local binary-cache import with `no-compile`.
It cannot fall back to source-compiling Skia. Output:

```
src-tauri/target/wasm-bindings/wasm32-unknown-emscripten/wasm/wasm_bindings.js
src-tauri/target/wasm-bindings/wasm32-unknown-emscripten/wasm/wasm_bindings.wasm
```

## Regenerating the stored Skia bundle

Only needed when changing the pinned Skia/toolchain contract:

```powershell
# In your existing emsdk directory:
git pull --ff-only
.\emsdk.bat install 6.0.9
.\emsdk.bat activate 6.0.9

# In the app repository:
rustup toolchain install 1.98.1 --profile minimal --target wasm32-unknown-emscripten
pnpm wasm:skia:build
pnpm wasm:build
```

Set `LIBCLANG_PATH` to a libclang directory. LLVM **23.1.0 is verified**, not an
asserted minimum; prefer a libclang major matching Emscripten's clang. Install
Python 3.10+ and Git. The script downloads Ninja 1.13.1 on Windows if `NINJA` is
unset; on other hosts install Ninja or supply `NINJA`. Windows also requires
the usual MSVC Rust build tools. Skia compilation needs several GB of disk space
and network access on first use.

The script prepares pinned rust-skia/Skia, GN and Skia dependencies beneath
`src-tauri/target/wasm-toolchain`, compiles with native Wasm exceptions, exports
the bundle to `vendor/skia-wasm`, and copies upstream licenses. That source
build cache is separate from daily bindings builds. Review and commit the
archive, manifest and notices together after regeneration.

Windows fixes carried forward from the experiment and validated here:

- GN receives explicit `emcc.exe`, `em++.exe` and `emar.exe` paths and the SDK
  sysroot, replacing this release's assumptions about `.bat` launchers.
- The selected Python is put first on PATH. A cached `python3.cmd` maps Skia's
  hard-coded command to that Python on Windows.
- Bindgen receives Emscripten's compatibility include directory.
- Native Wasm exceptions (`-fwasm-exceptions`) are enabled. No legacy longjmp
  wrapper, exception stripping or undefined-symbol suppression is used.

## Runtime contract and migration boundary

The Emscripten output is an ES module factory. OverlayEditor caches that module
and owns a disposable preview session with activity, fonts and widget surfaces.
Native supplies immutable activity snapshots and serialized Skia font faces.
Rust owns sampling and drawing; JS coalesces the latest snapshot per animation
frame and presents the pixels.

The C ABI is internal: pointers must refer to live allocations in this module;
callers must not double-free or reuse destroyed handles. Rendering is synchronous
and single-threaded. Widget pixels use ImageData's straight-alpha RGBA format.

Vite builds and emits the JS/WASM pair for production and serves it during
development. Further widget families, interaction support and representative
performance measurements remain described in
[the migration notes](../../docs/scratch/wasm-migration-vertical-slice.md).
