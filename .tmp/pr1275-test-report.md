# rust-skia PR #1275 test

## What was tested

- Checked out PR #1275 (`dc8f08ac`) into `.tmp/rust-skia-pr1275`.
- Built the existing `wasm_preview_poc` source through a small isolated wrapper crate.
- Used `wasm32-unknown-unknown` with `skia-safe` default features disabled.

## Required for a successful build

- Ninja 1.13.2 on `PATH`.
- The PR's automatically downloaded WASI SDK 32.0 (Clang, `llvm-ar`, sysroot, and `libclang.dll`).
- The test-only Skia GN change recorded in `.tmp/pr1275-skia-toolchain.patch`, removing the unresolved Emscripten toolchain activation dependency.
- `LIBCLANG_PATH` pointed at the WASI SDK `bin` directory.
- `BINDGEN_EXTRA_CLANG_ARGS` set to the WASI SDK Clang resource directory (`lib/clang/22`), so bindgen could find `stddef.h`.

## Result

The build succeeded and produced `wasm_preview_poc.wasm` (~3.2 MB). The module instantiated in Node and the CPU render smoke test returned success.

The PR checkout is committed as a submodule pinned to `dc8f08ac`. After cloning, initialize submodules recursively and apply the recorded patch before building:

```text
git submodule update --init --recursive
git apply --directory=.tmp/rust-skia-pr1275/skia-bindings/skia .tmp/pr1275-skia-toolchain.patch
```

This removes the project's Emscripten-specific `emcc.bat`/linker workaround, but PR #1275 still supplies internal allocator and WASI shims. The main branch was not changed; all test files and artifacts remain under `.tmp`.
