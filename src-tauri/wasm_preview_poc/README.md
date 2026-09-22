# Wasm preview POC

This is the isolated first step for moving widget rendering into Rust/Skia/Wasm.
It proves the complete path before any production widget is migrated:

1. Rust renders with `skia-safe`.
2. Emscripten links the Rust/Skia module.
3. The browser loads the generated Emscripten JavaScript/Wasm pair.
4. Rust renders RGBA pixels into Wasm memory.
5. The frontend copies those pixels into a canvas and benchmarks the result.

The POC is currently software-rasterized RGBA8888 output. It is not yet the
production widget renderer.

## Migration boundary

Transfer the behavior below as one unit. The named files are merge points, **not
instructions to replace the target branch's files**. Read the target versions,
apply only the relevant changes, and preserve their other workspace members,
routes, Vite plugins, Tauri hooks, and release steps. Copy a new POC-only file
when it does not exist there; merge it if it already does.

1. **Isolate the Rust crate.** Carry over `src-tauri/wasm_preview_poc/Cargo.toml`,
   its `Cargo.lock`, and `src/lib.rs` and `src/main.rs`. The POC manifest must
   retain `[workspace]`, the `wasm-preview` profile, and the pinned rust-skia
   dependency below. In the target `src-tauri/Cargo.toml`, remove
   `wasm_preview_poc` from the native workspace members if present; keep the
   native app and other members untouched. Resolve the target branch's native
   `src-tauri/Cargo.lock` normally; do not replace it with the POC lockfile or
   import the POC's newer Skia dependency into the native app.

2. **Carry over the one-shot build.** Add or merge `scripts/build-wasm.mjs` and
   set `package.json`'s `wasm:preview:build` script to
   `node scripts/build-wasm.mjs`, preserving other scripts. The builder loads
   the local environment, prepares pinned Skia/GN/Ninja when needed, and runs
   Cargo with `CARGO_TARGET_DIR=src-tauri/target`. Keep that output location in
   sync with the Vite artifact path and CI download path. Skia source and Cargo
   output are cached under `src-tauri/target`; no separate `dev.mjs`,
   `wasm:preview:watch`, or WASM logic in `scripts/tauri.mjs` is required.

3. **Keep target linker settings with the POC.** Carry over
   `src-tauri/wasm_preview_poc/.cargo/config.toml` with the
   `wasm32-unknown-emscripten` target's `em++` linker, `emar` archiver,
   `-sINITIAL_MEMORY=134217728`, and `-sEXPORTED_RUNTIME_METHODS=HEAPU8`.
   Cargo finds it because the build script runs from the POC directory. Do
   not place these POC-only settings in the native `src-tauri/.cargo` config
   or carry over the old `emcc-linker.*` wrapper and legacy longjmp flag.

4. **Connect local Tauri commands.** Merge these values into the target
   `src-tauri/tauri.conf.json` build hooks, retaining any other work those hooks
   perform:

   ```json
   {
     "beforeDevCommand": "pnpm wasm:preview:build && pnpm dev:frontend",
     "beforeBuildCommand": "pnpm wasm:preview:build && pnpm build:frontend"
   }
   ```

   The first build must finish before Vite serves the debug route. Local
   `pnpm build` also needs an artifact; CI does not build it for a developer's
   machine. The WASM script is one-shot, so Vite—not Tauri or that script—
   handles subsequent source changes.

5. **Merge the Vite integration.** In the target `app/vite.config.js`, add the
   `wasmPreviewArtifactPlugin()` behavior to its existing plugins. Its dev
   middleware serves only `wasm_preview_poc.js` and `wasm_preview_poc.wasm`
   from `src-tauri/target/wasm32-unknown-emscripten/wasm-preview` at
   `/debug/wasm-preview-artifacts`, with the correct JS/WASM content types and
   no-store caching. It also uses Vite's existing watcher for POC `.rs`,
   `.toml`, `.lock`, and the POC `.cargo/config.toml` changes; it reruns the
   one-shot script, avoids overlapping builds, reloads on success, and shows a
   dev error/503 instead of silently serving stale artifacts after failure.
   Preserve the target's other plugins, middleware, and server configuration.

6. **Merge the browser debug path.** Carry over
   `app/src/features/wasm-preview-debug/` and its tests in
   `app/src/tests/features/wasm-preview-debug/`. Add the development-only
   `#/debug/wasm-preview` route in the target `app/src/App.jsx` without
   replacing its other routes. The loader in `wasmPreviewRenderer.js` must
   request the same artifact URL Vite serves. Also merge Vite's
   `fontServingPlugin()` behavior for `/fonts/JetBrains Mono.ttf` and carry
   over that font (or intentionally update both the middleware and the fetch
   in `WasmPreviewDebug.jsx`); the benchmark loads its bytes into Wasm.

7. **Merge CI and ignore rules.** Carry over
   `.github/workflows/wasm-preview.yml` and merge the release workflow's
   dependency on it, artifact download into the same target directory, and
   `WASM_PREVIEW_PREBUILT=1` on the native build step. The prebuilt flag
   validates and reuses the downloaded pair instead of compiling Skia again.
   Keep the target workflow's existing release/packaging steps. Ensure
   `.gitignore` excludes `.env.wasm.local` and generated `src-tauri/target/`.
   Do not commit a machine-specific env file, Skia checkout, or generated
   JavaScript/Wasm pair.

If the target only needs a compile proof, step 6 can wait. Steps 1–5 and 7 are
required for the integrated local-dev and release flow described here.

## Pinned build contract

The current verified combination is:

| Component | Version or value |
| --- | --- |
| Rust | `1.98.1` |
| Rust target | `wasm32-unknown-emscripten` |
| Emscripten | `6.0.9` |
| `skia-safe` | `0.153.4` from rust-skia PR #1336 |
| `skia-bindings` | `0.153.3` from the same Git revision |
| rust-skia revision | `1a80f6716b5ba787f6583443f7b62fa5f60e7084` |
| Skia backend | software raster, RGBA8888 |

The dependency declarations must remain pinned to the rust-skia Git revision
until the branch intentionally moves to a released version containing the same
changes:

```toml
skia-safe = { git = "https://github.com/rust-skia/rust-skia", rev = "1a80f6716b5ba787f6583443f7b62fa5f60e7084", version = "=0.153.4", features = ["binary-cache"] }
```

Do not reintroduce the old `src-tauri/wasm_preview_poc/tools/emcc-linker.*`
wrapper or `-sSUPPORT_LONGJMP=emscripten`. The verified build uses direct
`em++`/`emar`, `-fwasm-exceptions`, and source-built Skia.

## Toolchain requirements

### Local Windows build

Create `.env.wasm.local` at the repository root on each development machine;
it is intentionally ignored and must not be migrated or committed. Put in the
paths to that machine's installed SDK and libclang directory. This machine's
working file uses:

```dotenv
EMSDK=E:/emsdk
LIBCLANG_PATH=E:/llvm-23.1.0/LLVM/bin
```

There is no committed `.env.wasm.example` in this branch. On another machine,
replace these values with its actual installation paths, or set `EMSDK` and
`LIBCLANG_PATH` in the shell. The build script loads `.env.wasm.local` for
`pnpm dev`, `pnpm build`, and `pnpm wasm:preview:build`; existing shell
variables take precedence. CI supplies its own variables and does not need
this file.

The script checks out the pinned rust-skia revision under
`src-tauri/target/wasm-preview-toolchain`, fetches GN, and syncs Skia's
dependencies on first use. On Windows it also downloads Ninja 1.13.1 there
when Ninja is unavailable on `PATH`. Subsequent runs reuse these cached tools
and Cargo's Skia build output. `SKIA_SOURCE_DIR` and `NINJA` remain optional
overrides for externally managed installations.

The current successful local setup used Emscripten 6.0.9, LLVM/libclang
23.1.0, and Ninja. The script resolves Windows `.exe` tools and passes the
real `emcc.exe`, `em++.exe`, and `emar.exe` paths to GN. The small `emcc`
presence marker used by the unpatched PR #1336 detection code is created by
`scripts/build-wasm.mjs`; no rust-skia source patch is required.

### Linux CI build

The committed workflow at `.github/workflows/wasm-preview.yml` is the
reproducible CI path. It installs or restores:

- Rust 1.98.1 and `wasm32-unknown-emscripten`;
- Emscripten 6.0.9;
- clang/libclang, LLVM, Python, and Ninja;
- rust-skia at the pinned revision;
- GN through Skia's `fetch-gn` script;
- Skia third-party dependencies through `git-sync-deps`.

It caches the Emscripten SDK, prepared rust-skia/Skia source, Cargo downloads,
and the Cargo target directory. It uploads the two browser artifacts as a
workflow artifact.

The CI job is intentionally Linux-only. The release workflow calls it once and
downloads the generated JavaScript/Wasm pair into each native build job. Native
release jobs validate the pair with `WASM_PREVIEW_PREBUILT=1`, so they do not
need to source-build Skia again.

## Build and run

The POC is an isolated Cargo workspace beside the native Tauri workspace. The
native `app` crate does not depend on it. Tauri's `beforeDevCommand` runs
`pnpm wasm:preview:build` once before starting Vite. Vite's existing watcher
then rebuilds the artifact when POC Rust or Cargo inputs change and reloads
the page; failed rebuilds appear in the dev overlay and terminal. Tauri's
`beforeBuildCommand` runs the same one-shot build before the frontend bundle.
This applies to `pnpm dev`, `pnpm tauri dev`, `pnpm build`, and
`pnpm tauri build`.

`scripts/tauri.mjs` is the existing Tauri CLI wrapper for portable ZIP
packaging; it contains no Wasm build logic. Cargo keeps the Skia build in
`src-tauri/target`, so edits to POC Rust code do not recompile Skia when its
inputs are unchanged.

From the repository root, after the required environment is prepared:

```powershell
pnpm wasm:preview:build
```

The build invokes Cargo with:

- `FORCE_SKIA_BUILD=1`;
- `EMCC_CFLAGS=-fwasm-exceptions`;
- direct Emscripten `em++` and `emar` tools;
- the Emscripten compatibility include path for bindgen;
- the configured GN and Ninja commands.

The target linker configuration in
`src-tauri/wasm_preview_poc/.cargo/config.toml` also passes:

```text
-sINITIAL_MEMORY=134217728
-sEXPORTED_RUNTIME_METHODS=HEAPU8
```

The first flag provides enough heap for the renderer's RGBA buffer. The second
exposes Emscripten 6's heap view to the browser loader. No undefined-symbol
suppression or exception-flag stripping is part of the strict build.

Generated files:

```text
src-tauri/target/wasm32-unknown-emscripten/wasm-preview/wasm_preview_poc.js
src-tauri/target/wasm32-unknown-emscripten/wasm-preview/wasm_preview_poc.wasm
```

Start the frontend dev server in another terminal:

```powershell
pnpm dev:frontend
```

Open:

```text
http://localhost:5173/#/debug/wasm-preview
```

Click **Run Benchmark**. Do not open the generated JavaScript or Wasm directly
with `file://`; the Emscripten runtime and Vite artifact middleware require an
HTTP server.

## Transfer acceptance gate

Before starting widget migration on the new branch, all of these must pass:

1. The branch contains the complete migration boundary listed above.
2. `pnpm wasm:preview:build` succeeds with the pinned toolchain.
3. Both generated artifacts exist in the `wasm-preview` target directory.
4. The generated Wasm contains no legacy `emscripten_longjmp` references:

   ```powershell
   rg -a -i "emscripten_longjmp" src-tauri/target/wasm32-unknown-emscripten/wasm-preview
   ```

   No matches are expected.

5. The debug route loads without `HEAPU8` or module-initialization errors.
6. The benchmark completes and reports its final timing summary.
7. While Vite is running, a POC Rust edit triggers a WASM rebuild and page
   reload; a failed rebuild is visible in the terminal and dev overlay.
8. The frontend and benchmark tests pass.
9. The release workflow consumes the pair from the WASM workflow with
   `WASM_PREVIEW_PREBUILT=1`, without requiring Emscripten on native runners.

Only after this gate passes should the standalone POC ABI be expanded to carry
real widget data or production widget drawing code.

## Files that must not be transferred

Do not commit or copy these local/generated items:

```text
.tmp-rust-skia-pr1336/
src-tauri/target/
wasm_preview_poc.js
wasm_preview_poc.wasm
local Emscripten/LLVM/Ninja installation directories
```

The CI workflow recreates the source checkout and toolchain, and Cargo creates
the generated artifacts.

`report.md` is historical context and troubleshooting evidence. It is useful
when changing the build contract, but it is not required for compiling or
running the POC.
