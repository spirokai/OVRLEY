# Frontend preview migration setup

This crate is OVRLEY's Emscripten binding boundary. It is an isolated Cargo
workspace so its toolchain and Skia version do not change the native renderer.
The initial diagnostic draws text and an animated bar into caller-owned RGBA;
production widgets have not been migrated.

## Daily development

1. Install Rust `1.98.1` with `wasm32-unknown-emscripten` and activate Emscripten
   `6.0.9` in your existing emsdk installation.
2. Copy root `.env.wasm.example` to `.env.wasm.local` and set `EMSDK`.
   Set `PYTHON` if the Python on PATH is older than 3.10. Local paths are ignored
   by Git; shell variables take precedence.
3. Run `pnpm dev:frontend` (or `pnpm dev` for Tauri), then open
   `http://localhost:5173/#/debug/wasm-preview`.

The first debug asset request runs `pnpm wasm:build`. Subsequent Rust, Cargo,
toolchain, build-script or stored-bundle changes trigger serialized rebuilds
through Vite's existing watcher and reload the page. Edits arriving during a
build queue another build. Requests wait for the current build; a failed build
returns HTTP 503 and a Vite error overlay, never old artifacts. React keeps its
standard HMR and releases renderer allocations on effect cleanup. Tauri ignores
this crate for native restarts. No extra watcher process or watch dependency is
needed. Ordinary editor development does not compile Wasm until this debug path
is opened. The Tauri development CSP explicitly permits Wasm compilation;
the standalone debug route skips desktop-only preference hydration.

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

- PR #1336 probes a literal `emcc` file; the script creates an empty presence
  marker in the SDK while using the actual `.exe` tools for compilation.
- GN receives explicit `emcc.exe`, `em++.exe` and `emar.exe` paths.
- The selected Python is put first on PATH. A cached `python3.cmd` maps Skia's
  hard-coded command to that Python on Windows.
- Bindgen receives Emscripten's compatibility include directory.
- Native Wasm exceptions (`-fwasm-exceptions`) are enabled. No legacy longjmp
  wrapper, exception stripping or undefined-symbol suppression is used.

## Runtime contract and migration boundary

The Emscripten output is an ES module factory, so there is no shared global
`window.Module`. Each debug mount owns a renderer handle, font and persistent
pixel buffer. Initialization checks ABI version 1. Fonts are supplied as bytes,
with invalid fonts reported as errors and no system-font fallback. Teardown
frees the handle and pixels, including initialization/unmount races.

The C ABI is internal: pointers must refer to live allocations in this module;
callers must not double-free or reuse destroyed handles. Rendering is synchronous
and single-threaded. The diagnostic is opaque RGBA; before rendering transparent
widgets, define conversion between Skia premultiplied RGBA and ImageData's
straight alpha explicitly.

This setup stops before widget migration. The debug route and asset server are
development-only. Production build hooks and release workflows do not yet
depend on Wasm. Before enabling a production preview, add:

1. A host-neutral render core with injected font/resource bytes instead of
   filesystem paths and native global caches.
2. Session-owned activity state and per-widget configuration/invalidation;
   frame/playhead inputs should be the normal per-frame traffic.
3. CI generation and bundling of the JS/Wasm pair, shipped third-party notices,
   and the Tauri production CSP permission for Wasm compilation.

Do not import the old branch's native Cargo lockfile or replace the existing
React preview orchestration. The `wasm-poc` README and report were references;
their benchmark widget and production release wiring are not this migration's
runtime API.
