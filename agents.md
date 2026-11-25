# OVRLEY — Agent Guide

## Non-negotiable code rules

These rules are mandatory for all new and modified code:

- Required data MUST be consumed as a strict contract and malformed config input MUST fail loudly. Fallbacks are permitted only for documented optional absence; they MUST NOT repair malformed present data. This concerns ONLY user config/input, NOT external activity data which we have no control over.
- The codebase MUST use one canonical naming scheme and data shape. API mismatches MUST be fixed at their owner, never hidden behind aliases, remapped objects, compatibility branches, or secondary adapters.
- Validation and normalization MUST happen once at ingress. Consumers MUST NOT repeat coercion or defensive checks such as `typeof`, `Boolean`, `Number.isFinite`, or `Number.isInteger` for validated fields.
- Components MUST be presentational. Stateful and reusable presentation logic MUST live in hooks; pure computation, formatting, and transformation MUST live in utilities.
- Conditionals MUST represent product behavior or documented optionality. Broken required state MUST NOT degrade into defaults, generated identities, `null`, or empty output.
- Adapters are permitted only at genuine external-system boundaries and MUST translate once into the internal canonical model.

## Project

Native desktop app (Rust, Slint, Skia, wgpu) that turns activity data (`.fit`, `.gpx`, `.tcx`, CSV, camera telemetry and more) into customizable video overlays (speed, HR, elevation, maps, etc.).

**Product name in config/bundles: OVRLEY.** Package name in root `package.json`: `ovrley`. The cargo binary is `ovrley`; packages rename it.

## Architecture

```
crates/
  ovrley_ui/        Slint markup (ui/), projections and bindings (src/features/), platform glue, the binary
  ovrley_app/       store, actions, reconcile steps, services traits; no Slint
  ovrley_mcp/       MCP server: tool catalog, protocol, the socket to the window
  ovrley_services/  projects, rasters, map tiles and styles, video frames, caches, runtime, updater
  ovrley_playback/  MDK player and its GPU interop
  ovrley_core/      activity parsing, normalization, export renderer, FFmpeg
  ovrley_widgets/   widgets (Skia): model, catalog, preview session, drawing
  ovrley_map/       map vocabulary, wgpu renderer (`gpu`), style validator (`validate`)
assets/             widget manifests, map styles, icons, keyboard shortcuts
locales/            translations: ovrley.pot and <lang>.po
packaging/          installers' inputs; see packaging/README.md
app/                the former React app: a frozen reading reference. Nothing reads it and it does not run. Do not edit it.
```

- One update loop: an action runs on the UI thread, then the reconcile steps, the history step and the projections (`ovrley_app/src/store/`). One action is at most one undo step.
- UI and MCP are the only entry points, and both dispatch the same actions. A tool never has logic of its own that the UI lacks.
- `ovrley_app`, `ovrley_services`, `ovrley_mcp` and the crates below them never depend on Slint.
- See `docs/ARCHITECTURE.md`.

## Commands (run from repo root)

| Command                                                   | What it does                                                          |
| --------------------------------------------------------- | --------------------------------------------------------------------- |
| `pnpm dev:native`                                         | The app: `cargo run -p ovrley_ui --bin ovrley`                        |
| `pnpm dev:native:live`                                    | The app with Slint live preview (`live-preview` feature)              |
| `cargo check -p <crate>`                                  | Compile check of the crate you touched                                |
| `cargo test --workspace --features ovrley_ui/dev-windows` | The whole suite                                                       |
| `pnpm test:native-ui`                                     | `ovrley_app`, `ovrley_ui`, `ovrley_services` and `ovrley_mcp` tests   |
| `pnpm build`                                              | Release build and packages for this OS (`scripts/package.mjs`)        |
| `pnpm install:packager`                                   | cargo-packager at the pinned revision                                 |
| `pnpm prepare:packaging`                                  | The notices and the player's libraries, in `target/packaging/`        |
| `pnpm release`                                            | semantic-release; CI runs it                                          |

ABSOLUTELY NEVER run build without the user's permission!

## Code style (Rust)

- Toolchain pinned in `rust-toolchain.toml`; edition 2021.
- **Never run `cargo fmt` over existing files.** Many files are not rustfmt-formatted and there is no rustfmt config, so it rewrites untouched lines. Match the surrounding style.
- A module starts with a `//!` comment saying what it owns. Name modules by their role, not by the library they wrap.
- No subfolder for a single file: `foo_tests.rs` lies beside `foo.rs`.
- Actions are synchronous. Blocking work runs on `services.workers` and comes back as an action. Side effects go through the traits in `ovrley_app/src/store/services.rs`, which have fakes for tests.
- Widget data changes go through `ovrley_widgets::model::document`, UI formatting through `ovrley_app/src/lib/format.rs`. Do not add a second home for either.
- Skia is `skia-safe` with prebuilt binaries only (`no-compile`). Slint is pinned exactly; its patched renderer is in `patches/` (see `patches/README.md`).

## Code style (Slint)

- No std-widgets: use `ui/components/`.
- One global per feature (`ui/features/<feature>/index.slint`), written by that feature's projection (`src/features/<feature>.rs`). Markup stays presentational.
- Projections write a property only when its value changed (`set_if_changed`, `replace_if_changed`).
- Use the `Theme` tokens. Sizes are in rem.
- Every user-visible string in markup is `@tr("<key>" => "<English text>")`.

## Translations

- `locales/ovrley.pot` holds the English text (msgid) under each key (msgctxt), and `locales/<lang>.po` each translation. Both Slint's `@tr` (bundled by `ovrley_ui/build.rs`) and Rust's runtime strings (`ovrley_app::i18n`) read them.
- New string: `@tr` or a catalog key in Rust, plus its entry in `ovrley.pot` and the catalogs.
- `crates/ovrley_ui/tests/translations.rs` checks every `@tr` against the template, that every entry is used, and that the catalogs follow the template.
- MCP tool names, descriptions and errors are English and not in the catalogs.

## Dependencies

- **FFmpeg** (pinned build) auto-downloaded by `postinstall` to `vendor/ffmpeg/` (`scripts/install-ffmpeg.mjs`; skip with `OVRLEY_SKIP_FFMPEG_INSTALL=1`). Export and previews run it as a subprocess; `OVRLEY_FFMPEG` overrides the binary.
- **MDK SDK** (nightly; video playback) auto-downloaded by `postinstall` to `vendor/mdk-sdk/` (`scripts/install-mdk.mjs`; skip with `OVRLEY_SKIP_MDK_INSTALL=1`, another archive with `OVRLEY_MDK_ARCHIVE_URL`). The root `.cargo/config.toml` points `MDK_SDK` there; an `MDK_SDK` in the environment wins. `ovrley_playback` and, through it, `ovrley_ui` need it.
- **pnpm 10.25.0** for the scripts (`packageManager` in `package.json`; install hooks reject other package managers). Install from the root with `pnpm install --frozen-lockfile`. `app/` is not part of it.
- **Node 24** in CI.

## Build & release

- `scripts/package.mjs` builds and packages; cargo-packager's config is `[package.metadata.packager]` in `crates/ovrley_ui/Cargo.toml`.
- The workspace's version is the app's. `scripts/release/set-version.mjs` stamps it; the committed one is a placeholder.
- Workflows in `.github/workflows/`: `workflow_release.yml` runs the Windows, macOS and AppImage ones and drafts the release; `release-linux.yml` (deb), `release-flatpak.yml` and `release-ms-store.yml` are separate. Never trigger them.
- semantic-release tags pushes to `main`, `rust-rewrite` and `rust-wasm` (`.releaserc.json`).
- The updater (`ovrley_services::updater`) is off in debug builds and wherever the `.ovrley-portable` marker lies beside the executable.

## Debugging

- `ovrley --mcp` is the MCP server; its standard output carries only the protocol. See `docs/MCP.md`.
- Debug builds: `OVRLEY_SNAPSHOT=<dir>` renders a tour of app states to PNGs through Slint's `take_snapshot()` (no screen-recording permission needed) and quits; `OVRLEY_SNAPSHOT_SIZE=<width>x<height>` sizes it. It reads but never writes `ovrley-settings.json`.
- Debug builds: `MDK_DECODERS=<name>[,<name>…]` overrides the decoders.
- Debug builds write an imported activity's parse to `debug/activities/`.
- `OVRLEY_FRAME_RATE=1` logs the window's rendered frames per second; `OVRLEY_SKIA_FULL_REDRAW=1` turns off the patched renderer's partial redraw, to compare.
- Windows plays and decodes through D3D11. The D3D12 path is in the code but not wired in (`WindowsTarget::ACTIVE`).

## Testing

- Unit tests are inline `#[cfg(test)] mod tests` or files beside their module. `ovrley_core`'s integration tests are one binary: `cargo test -p ovrley_core --test it <module>::`.
- `ovrley_ui`'s component tests (`crates/ovrley_ui/tests/`) need the `dev-windows` feature, which also compiles their harness windows (`crates/ovrley_ui/tests/harness/`). Slint's testing platform is process-wide and bound to one thread, so each such file has a single `#[test]` that runs its cases through `common::run_cases`.
- The render baselines are ignored and run by hand: `crates/ovrley_core/BASELINES.md`.
- A test of the updater's manifest runs `node`.

## Agent skills

### Issue tracker

Local markdown files under `.agents/scratch/`. See `.agents/agents/issue-tracker.md`.

### Review-friendly edits

- Prefer small, surgical `apply_patch` hunks over full-file rewrites.
- Re-read a file immediately before any follow-up patch to the same file.
- Avoid delete-and-readd file edits unless a full rewrite is truly necessary.
- When possible, preserve surrounding code structure so human review can stay diff-based.

### Triage labels

Five canonical roles: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. Recorded as `Status:` in issue frontmatter. See `.agents/agents/triage-labels.md`.

### Domain docs

`docs/ARCHITECTURE.md` for the architecture, `docs/MCP.md` for the MCP server, `packaging/README.md` for packages. See `.agents/agents/domain.md`.

## Documentation

- Check a claim against the code before writing it down, and correct a doc in the change that makes it false.
- `docs/scratch/` holds plans, and the reports in `benchmarks/*/` other than each README are dated records: neither describes the current code.
