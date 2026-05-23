# OVRLEY — Agent Guide

## Project

Desktop app that turns `.fit`/`.gpx` activity data into customizable video overlays (speed, HR, elevation, map route, etc.).

**Product name in config/bundles: OVRLEY.** Package name in root `package.json`: `ovrley`.

## Architecture

```
app/              React 19 + Vite frontend (JSX, NOT TypeScript)
src-tauri/        Tauri v2 desktop shell
src-tauri/ovrley_core/   Standalone Rust crate — Skia rendering, ffmpeg encoding, activity parsing
```

- **No Python backend.** The old `backend/` directory & Python sidecar are gone. All rendering is Rust (Skia) + ffmpeg subprocess.
- Tauri IPC commands are defined in `src-tauri/src/lib.rs` and `ovrley_core/src/commands/`. Frontend calls them via `@/api/backend.js`.
- `@/` path alias resolves to `app/src/` (configured in `vite.config.js` and `jsconfig.json`).

## Commands (run from repo root)

| Command             | What it does                                                        |
| ------------------- | ------------------------------------------------------------------- |
| `pnpm dev`          | Full dev: builds frontend + launches Tauri window                   |
| `pnpm dev:frontend` | Vite dev server only (port 5173)                                    |
| `pnpm build`        | Production build (runs `pnpm tauri build` via wrapper)              |
| `pnpm lint`         | ESLint on `app/` (flat config v9, Prettier-integrated)              |
| `pnpm format`       | Prettier on `app/`                                                  |
| `pnpm release`      | semantic-release (auto-tags on `skia-render-backend` branch pushes) |

`pnpm tauri build` is wrapped by `scripts/tauri.mjs` — it runs Tauri with `--no-bundle`, then creates a standalone `.zip` portable archive containing the binary + `vendor/ffmpeg/` + `fonts/` + `templates/`.

## Code style (frontend)

- **Prettier** (enforced via ESLint `prettier/prettier: error`): no semicolons, single quotes, trailing commas, 150 print width, LF line endings.
- **ESLint**: `react/prop-types: off`, `react-hooks/exhaustive-deps: error` (flat config at `app/eslint.config.js`).
- **No TypeScript.** Use JSDoc `@param`/`@returns` on exported functions for type documentation.
- **Zustand** for global state with Immer middleware (store slices in `app/src/store/slices/`). Use `useShallow` for object selectors.
- **shadcn/ui components** live in `app/src/components/ui/`. Use existing Radix primitives when adding new UI.

## State management patterns

- Zustand store created with `create()` + `immer` + `subscribeWithSelector` + `devtools` middleware.
- Store slices: `createEditorSlice`, `createMediaSlice`, `createTemplateSlice`, `createVideoImportSlice`.
- Component store access goes through selector hooks in `app/src/hooks/` (e.g. `useAppStoreSelectors.js`). Avoid direct `useStore()` in leaf components.

## Frontend refactoring conventions

When asked to refactor React code, follow the detailed process in `.agents/refactor-guide.md`:

- Extract in order: constants → utils → presentational components → hooks → container hooks
- Only refactor one feature at a time
- No behavioral changes, no bug fixes, no feature additions
- Extract `data/` (constants only, no functions) vs `utils/` (pure functions) vs `components/` vs `hooks/`

## Rust backend

- Rust 1.84.0 (`.tool-versions`). `edition = "2021"`.
- `ovrley_core` crate has Skia (`skia-safe 0.75`), serde, chrono.
- Tauri crate (`src-tauri/`) depends on `ovrley_core` as a workspace member.
- On Windows: links `msvcprt` via `build.rs`.

## Dependencies

- **FFmpeg 8.1+** (full build) auto-downloaded by `postinstall` script to `vendor/ffmpeg/`. Required for video encoding. Tauri bundle resources include this path.
- **pnpm 10.25.0** (enforced via `package.json` `packageManager` field).
- **Node 24** used in CI; any modern LTS should work.

## Build & release

- CI: `pnpm install --frozen-lockfile` → `pnpm tauri build --bundles <type>`.
- Release workflow (`.github/workflows/release.yml`): manual trigger with tag input, builds Windows (NSI/MSI + portable) and macOS (DMG + portable).
- semantic-release auto-creates tags on pushes to `skia-render-backend` branch using `@semantic-release/commit-analyzer`.
- The portable archive script (`scripts/package-portable.mjs`) packages: Tauri binary renamed to `OVRLEY(.exe)`, `vendor/ffmpeg/`, `fonts/`, `templates/`.

## Testing

No test framework is set up. No test files exist. Manual verification is the only testing approach.

## Agent skills

### Issue tracker

Local markdown files under `.agents/scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical roles: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. Recorded as `Status:` in issue frontmatter. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `.agents/agents.md` for project overview and glossary, `docs/ARCHITECTURE.md` for detailed architecture. See `docs/agents/domain.md`.

## Stale documentation to be aware of

- `.agents/refactor-guide.md` is for frontend-only refactoring work — best consulted explicitly when refactoring is requested.
- `docs/`, `gpu-render.md`, `mp4-*`, `phase-5-*` files may describe old or aspirational architecture. Prefer reading code and config over these docs.
