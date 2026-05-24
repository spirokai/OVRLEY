Status: ready-for-agent

# 01 — Feature Gate, Dependencies, and Store Export

## Parent

[Canvas-Frame Pixel Parity QA Gate PRD](../PRD.md)

## What to build

Set up all infrastructure required before any testing can begin. This slice adds the Cargo feature gate, the Node.js dependency, and the frontend store export — three small, independent changes that collectively make the test compilable and the Playwright script injectable.

**Cargo feature gate**: Add a `[features]` section to `ovrley_core/Cargo.toml` with an empty `canvas-parity` feature (`canvas-parity = []`). Add a `[[test]]` target entry with `name = "canvas_parity_tests"` and `required-features = ["canvas-parity"]` so the test binary is not compiled unless `--features canvas-parity` is explicitly passed. Without the feature, the test file does not exist to Cargo — no compilation, no "ignored" message.

**Node.js dependency**: Add `@playwright/test` to root `package.json` devDependencies. The Playwright script is spawned via `npx` from the Rust test, so the dependency lives at the project root where `npx` resolves.

**Frontend store export**: In the Zustand store creation file, add a one-line conditional: when `import.meta.env.DEV` is true, attach the store to `window.__STORE__`. This lets the Playwright script call `window.__STORE__.setState(...)` to inject the fixture state snapshot. Stripped from production builds automatically by Vite's dead-code elimination.

## Acceptance criteria

- [ ] `ovrley_core/Cargo.toml` has `[features] canvas-parity = []` and a `[[test]]` target with `required-features = ["canvas-parity"]`
- [ ] Running `cargo test -p ovrley_core` does NOT compile the canvas parity test binary (feature not enabled)
- [ ] Running `cargo test -p ovrley_core --features canvas-parity --test canvas_parity_tests` compiles the test binary (an empty stub test is acceptable)
- [ ] `package.json` includes `@playwright/test` in devDependencies
- [ ] `window.__STORE__` is accessible in the browser console when running `pnpm dev:frontend` (it is an object with `.setState`, `.getState`, `.subscribe` methods matching the Zustand store API)
- [ ] `window.__STORE__` is undefined when running `pnpm build:frontend` and inspecting the production bundle

## Blocked by

None — can start immediately.
