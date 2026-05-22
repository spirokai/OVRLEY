# Phase 6 — Documentation and Final Polish: Detailed Implementation Plan

## Purpose

Add comprehensive documentation to every Rust source file under `src-tauri/`, including production code, binaries, `build.rs`, and all test files. Document every module, public API function, public type, threading construct, pipeline lifecycle, and test file across the OVRLEY Rust backend. Fix the two remaining `cfg!(debug_assertions)` runtime checks. Run final validation (`cargo fmt`, `cargo test`, `cargo clippy -- -D warnings`). Verify all user-facing flows.

This phase is the last step of the refactor. It assumes all prior phases (1–5) are complete: typed errors in `error.rs`, `MetricKind` in `types.rs`, `AppPaths` in `paths.rs`, consolidated RDP in `rdp.rs`, consolidated interpolation in `interpolation.rs`, split `render/widgets/common.rs` responsibilities, `RenderController` in `progress.rs`, request structs for large signatures, sibling pipeline isolation, explicit cache ownership, and all tests migrated to dedicated `tests/` directories using shared `test_config.rs`.

The documentation produced in this phase must follow the standards in Section 15 (Documentation Standards) of the master plan (`.agents/backend-refactor-v2.md`) and must not alter any executable code logic. Documentation must be sufficiently detailed to explain ownership, intent, invariants, lifecycle, and regressions guarded, but it must not become extremely verbose or repetitive. Prefer high-signal docs that explain why the code exists and what constraints it must preserve.

---

## Table of Contents

1. [Pre-Flight Checklist](#1-pre-flight-checklist)
2. [Step 1 — Module-Level Documentation (`//!`)](#2-step-1--module-level-documentation-)
3. [Step 2 — Public API Documentation (`///` on `pub fn` and `pub struct`)](#3-step-2--public-api-documentation--on-pub-fn-and-pub-struct)
4. [Step 3 — Long Function Documentation (Layered Narrative Approach)](#4-step-3--long-function-documentation-layered-narrative-approach)
5. [Step 4 — Threading, Lifecycle, and Cancellation Documentation](#5-step-4--threading-lifecycle-and-cancellation-documentation)
6. [Step 5 — Test Documentation](#6-step-5--test-documentation)
7. [Step 6 — Fix `cfg!(debug_assertions)` → `#[cfg(debug_assertions)]`](#7-step-6--fix-cfgdebug_assertions--cfgdebug_assertions)
8. [Step 7 — Final Automated Checks](#8-step-7--final-automated-checks)
9. [Step 8 — Final Manual Verification](#9-step-8--final-manual-verification)
10. [Completion Criteria](#10-completion-criteria)

---

## 1. Pre-Flight Checklist

Before starting Phase 6, verify the following preconditions from earlier phases.

### 1.1 Phase 5 preconditions

- [ ] Global caches (`OnceLock<Mutex<HashMap>>`) are audited and either justified or restructured
- [ ] `video_composite_debug.rs` debug directory convention is documented (from Phase 5 Step 2 audit)
- [ ] Cache ownership is explicit — either through `RenderContext` or documented justification of why globals are acceptable
- [ ] Tests are deterministic (no state leakage between runs) — verified by running `cargo test` 3 consecutive times

### 1.2 Phase 4 preconditions

- [ ] `encode/ffmpeg_settings.rs` exists with `FfmpegSettings` and `build_ffmpeg_settings`
- [ ] `encode/progress.rs` owns `RenderController` (or re-exports it from old location)
- [ ] Request structs (`PreviewRenderRequest`, `FrameRenderRequest`, `MetricWidgetRequest`) are in place
- [ ] `commands/mod.rs` no longer contains `derive_composite_render_plan` (moved to `encode`)
- [ ] No `#[allow(clippy::too_many_arguments)]` on refactored render/encode functions
- [ ] Sibling pipelines are isolated (no cross-imports between `video_pipeline` and `video_composite_pipeline`)

### 1.3 Phase 3 preconditions

- [ ] `ovrley_core/src/paths.rs` exists with `AppPaths`
- [ ] `ovrley_core/src/interpolation.rs` is the single source of truth (f64-based)
- [ ] `ovrley_core/src/rdp.rs` exists with shared RDP logic
- [ ] `render/widgets/common.rs` responsibilities are reduced
- [ ] No `encode` → `commands` dependency

### 1.4 Phase 1–2 preconditions

- [ ] `ovrley_core/src/error.rs` exists with `CoreError` and `CoreResult`
- [ ] `ovrley_core/src/types.rs` exists with `MetricKind` enum
- [ ] All tests live in dedicated `tests/` directories
- [ ] `ovrley_core/tests/common/test_config.rs` exists and is used by all tests
- [ ] No `#[cfg(test)] mod tests` blocks in production source files (inline)
- [ ] No `#[path = "tests/..."] mod tests` directives in production source files

### 1.5 Baseline Before Changes

- [ ] Run `cargo test` from workspace root and record result: ___ passed, ___ failed, ___ ignored
- [ ] Run `cargo clippy -- -D warnings` from workspace root and record result
- [ ] Run `cargo fmt --check` to verify formatting is clean (record any unformatted files)
- [ ] Run a representative preview render (PNG output) and confirm it works
- [ ] Run a representative transparent overlay export and confirm it works
- [ ] Run a representative composite MP4 export and confirm it works

If any precondition fails, stop and resolve it before proceeding. Phase 6 is primarily additive (documentation + compile-time/lint fixes) and must not mask pre-existing issues.

---

## 2. Step 1 — Module-Level Documentation (`//!`)

**Purpose:** Ensure every Rust source file under `src-tauri/` has an appropriate file-level documentation strategy. For module files, this means a `//!` doc comment that explains ownership, disallowed dependencies, related modules, threading assumptions, and performance characteristics. For non-module entry points such as binaries and `build.rs`, add the most appropriate top-of-file documentation or comments so the file's role is explicit. This creates a navigable architectural map directly in the source.

### 2.1 Module Documentation Template

Every module doc must follow this template (from master plan Section 15.1):

```rust
//! # Module Name
//!
//! Owns: [what this module owns — the concepts and types it is authoritative for]
//! Does not own: [what related modules own instead]
//!
//! Allowed dependencies: [list of modules this may import]
//! Forbidden dependencies: [list of modules this must NOT import]
//!
//! Related modules: [cross-references to sibling/consumer modules]
//!
//! ## Thread Safety
//! [thread safety characteristics, if shared state; "Single-threaded" if none]
//!
//! ## Performance
//! [hot path or not, allocation characteristics, caching strategy]
```

### 2.2 Rust File Coverage

This phase must cover **every `.rs` file currently under `src-tauri/`**. At the time this plan was written, that inventory is **77 Rust files**:

- `src-tauri/build.rs`
- `src-tauri/src/*.rs`
- `src-tauri/src/bin/*.rs`
- `src-tauri/tests/*.rs`
- `src-tauri/ovrley_core/src/**/*.rs`
- `src-tauri/ovrley_core/tests/**/*.rs`

Do not stop after the files listed in the focused tables below. Those tables identify priority files and known gaps, but the acceptance criterion is broader: **every Rust file in the workspace must be reviewed and documented appropriately**, including:

- newly added support modules such as `src/bin_common.rs`
- all diagnostic binaries under `src/bin/`
- all widget helper modules such as `geometry.rs`, `marker.rs`, `polyline.rs`, and `transform.rs`
- all test support files such as `tests/common/mod.rs` and `tests/common/test_config.rs`
- all nested test module roots such as `render/widgets/tests/mod.rs`

Check the "Current State" column: files that already have a `//!` doc must be **reviewed and potentially enhanced** to meet the template; files that are missing `//!` docs must have them **written from scratch** where `//!` is applicable.

#### Core library — `ovrley_core/src/`

| File | Current State | Action |
|------|---------------|--------|
| `lib.rs` | Has `//!` (22 lines) — describes crate purpose | Review: add "Does not own", "Forbidden dependencies", "Thread Safety", "Performance" sections |
| `error.rs` | Has `//!` — describes `CoreError` and `CoreResult` | Review: add ownership, dependency, and threading sections |
| `types.rs` | Has `//!` — describes `MetricKind` and shared enums | Review: add full template sections |
| `paths.rs` | Has `//!` — describes `AppPaths` | Review: add dependency boundaries and threading notes |
| `rdp.rs` | Has `//!` — describes RDP simplification | Review: add full template sections |
| `interpolation.rs` | Has `//!` — describes consolidated interpolation | Review: add dependency boundaries, note that it uses `f64` precision |
| `activity/mod.rs` | Has `//!` — describes activity parsing domain | Review: enhance with full template |
| `activity/schema.rs` | **MISSING** `//!` | **Write from scratch** — owns `ParsedActivity`, `DenseActivityReport`, etc. |
| `activity/trim.rs` | Has `//!` — describes trimming | Review: add full template sections |
| `activity/interpolate.rs` | Has `//!` — describes densification | Review: note that authoritative interpolation lives in `crate::interpolation` |
| `config/mod.rs` | Has `//!` — describes config schema/validation | Review: enhance with full template |
| `commands/mod.rs` | Has `//!` — describes command orchestration | Review: document that this must remain thin, no deep domain logic |
| `debug/mod.rs` | Has `//!` — describes timing/profiling types | Review: add full template sections |
| `encode/mod.rs` | Has `//!` — describes encode module aggregation | Review: enhance with dependency matrix, pipeline overview |
| `encode/ffmpeg.rs` | Has `//!` — describes ffmpeg binary discovery | Review: add full template sections |
| `encode/ffmpeg_settings.rs` | Has `//!` — describes settings construction | Review: add full template sections |
| `encode/ffmpeg_composite.rs` | **MISSING** `//!` | **Write from scratch** — owns composite FFmpeg arg building |
| `encode/ffmpeg_composite_profiles.rs` | Has `//!` — describes encoder profile table | Review: add dependency boundaries |
| `encode/fps.rs` | Has `//!` — describes rational FPS type | Review: add full template sections |
| `encode/progress.rs` | Has `//!` — describes progress estimation + `RenderController` | Review: add state transition diagram, threading notes |
| `encode/video.rs` | **MISSING** `//!` | **Write from scratch** — owns render orchestration, segmented renders, `run_parallel_renders` |
| `encode/video_pipeline.rs` | Has `//!` — describes single-pass pipeline | Review: add "Forbidden dependencies: video_composite_pipeline" |
| `encode/video_composite_pipeline.rs` | Has `//!` — describes composite pipeline | Review: add "Forbidden dependencies: video_pipeline" |
| `encode/video_probe.rs` | **MISSING** `//!` | **Write from scratch** — owns video metadata probing via ffprobe |
| `encode/video_debug.rs` | Has `//!` — describes debug summaries | Review: add full template sections |
| `encode/video_composite_debug.rs` | Has `//!` — describes composite debug summaries | Review: add dependency boundaries |
| `encode/codec_detect.rs` | **MISSING** `//!` | **Write from scratch** — owns codec availability probing |
| `render/mod.rs` | Has `//!` — describes render orchestration | Review: enhance with threading, hot-path, and cache notes |
| `render/format.rs` | Has `//!` — describes metric formatting | Review: add full template sections |
| `render/surface.rs` | **MISSING** `//!` | **Write from scratch** — owns Skia surface creation + PNG export |
| `render/text.rs` | Has `//!` — describes text rendering + font cache | Review: enhance with cache ownership and threading notes |
| `render/widgets/mod.rs` | Has `//!` — describes widget preparation | Review: add full template sections |
| `render/widgets/types.rs` | Has `//!` — describes shared widget types | Review: add full template sections |
| `render/widgets/common.rs` | Has `//!` — describes reduced shared helpers | Review: add full template sections |
| `render/widgets/value.rs` | Has `//!` — describes metric value widgets | Review: enhance with SVG icon caching notes |
| `render/widgets/route.rs` | Has `//!` — describes route plot widget | Review: add full template sections |
| `render/widgets/elevation.rs` | Has `//!` — describes elevation profile widget | Review: add full template sections |

#### Tauri shell and entry points — `src-tauri/`

| File | Current State | Action |
|------|---------------|--------|
| `build.rs` | Likely minimal or undocumented | Document build-script purpose, link behavior, and why it must remain tiny |
| `main.rs` | Has `//!` (6 lines) — minimal binary entry | Review: add brief ownership note |
| `lib.rs` | Has `//!` — describes Tauri app setup | Review: add dependency boundaries, document that this is the Tauri boundary layer |
| `video_server.rs` | **MISSING** `//!` | **Write from scratch** — owns local HTTP preview server with range support |
| `bin_common.rs` | **MISSING** `//!` or equivalent top-of-file documentation | **Write from scratch** — owns shared CLI argument helpers for diagnostic binaries |
| `src/bin/*.rs` | Coverage not previously listed | Add top-of-file docs explaining each binary's purpose, inputs, and whether it is benchmark-, validation-, or render-oriented |
| `tests/video_server_tests.rs` | Coverage not previously listed here | Ensure top-of-file `//!` doc explains coverage, required fixtures/resources, and HTTP scenarios |

The counts in the earlier draft are intentionally superseded by this full-workspace requirement. If additional `.rs` files exist when Phase 6 starts, they are in scope automatically.

### 2.3 Verification for Step 1

After completing file/module docs for all in-scope files:

- [ ] Open every `.rs` file under `src-tauri/` and visually confirm it has appropriate file-level documentation
- [ ] Verify each module doc includes: Owns, Does not own, Allowed dependencies, Forbidden dependencies, Thread Safety, Performance
- [ ] Verify binaries and `build.rs` have concise top-of-file documentation/comments explaining purpose, inputs, and ownership boundaries
- [ ] `cargo doc --no-deps` generates without errors and produces module-level documentation
- [ ] `cargo doc --no-deps --document-private-items` does not show broken intra-doc links

```bash
# Verify module docs compile
cargo doc --no-deps 2>&1 | Select-String "error"

# Check there are no broken intra-doc links
cargo rustdoc -- -D warnings 2>&1 | Select-String "warning"
```

---

## 3. Step 2 — Public API Documentation (`///` on `pub fn` and `pub struct`)

**Purpose:** Add `///` docstrings to every public function and public struct that lacks one. Documentation must include purpose, errors, panics (if any), performance assumptions (if hot path), and threading assumptions (if shared state). Keep these docs detailed enough to be operationally useful, but avoid boilerplate and avoid narrating obvious one-line code.

### 3.1 Functions Missing Docstrings (Public API — High Priority)

These `pub fn` items are part of the library's public API surface and **must** be documented:

| File | Line | Function | What to Document |
|------|------|----------|------------------|
| `encode/video.rs` | 491 | `composite_output_frame_windows` | Purpose, input parameters, returned window ranges, how segment boundaries are computed, edge cases (single-segment, fractional frames) |
| `encode/codec_detect.rs` | 48 | `detect_codecs` | Purpose, how it probes every available encoder, timeout behavior (8s per probe), `# Performance` note (spawns many subprocesses, O(n) in codec count), `# Errors` (which probes fail silently vs fatally) |
| `encode/codec_detect.rs` | 516 | `parse_ffmpeg_filter_names` | Purpose, input format expected from `ffmpeg -filters`, return value semantics |
| `encode/video_probe.rs` | 34 | `probe_video` | Purpose, creation-time resolution priority (stream metadata → file metadata → ffprobe format tags), `# Errors` variants, path requirements |
| `encode/video_probe.rs` | 219 | `read_video_stream_duration` | Purpose, which ffprobe fields are read, fallback behavior |
| `src/video_server.rs` | 393 | `parse_range` | Purpose, supported range formats (bytes=start-end, bytes=start-, bytes=-suffix), unsatisfiable range response, `# Errors` |

### 3.2 Structs Missing Docstrings (Public API — High Priority)

| File | Line | Struct | What to Document |
|------|------|--------|------------------|
| `encode/video.rs` | 483 | `CompositeSegmentWindow` | Purpose (represents one parallel segment's frame range), field meanings |
| `encode/video_probe.rs` | 10 | `VideoMetadata` | Purpose, field meanings (especially `creation_time` priority semantics) |
| `encode/video_probe.rs` | 29 | `Resolution` | Purpose, width/height semantics |
| `encode/codec_detect.rs` | 14 | `AvailableCodecs` | Purpose, which codec families are covered, field meanings |
| `paths.rs` | 19 | `AppPaths` | Purpose (central path configuration), field meanings, how paths are derived |
| `src/video_server.rs` | 14 | `VideoServerHandle` | Purpose, ownership model (Arc<Mutex>), shutdown behavior, port allocation |
| `src/video_server.rs` | 35 | `PreviewVideoState` | Purpose, field meanings, state transitions |
| `src/video_server.rs` | 44 | `ByteRange` | Purpose, start/end semantics, inclusive vs exclusive bounds |

### 3.3 Internal Helpers Missing Docstrings (Medium Priority)

These functions are frequently used internally and benefit from documentation even though they are not the primary public API:

**`render/text.rs`** (10 functions) — font resolution, text measurement, drawing:
| Line | Function |
|------|----------|
| 79 | `label_style` |
| 111 | `value_style` |
| 148 | `draw_text` |
| 187 | `resolve_font` |
| ~193 | `baseline_for_alphabetical` (and 2 siblings) |
| ~224 | `measure_text` |
| ~230 | `measure_text_with_font` |
| ~245 | `parse_color` |

**`render/surface.rs`** (4 functions) — Skia surface/PixelRef creation, PNG export:
| Line | Function |
|------|----------|
| 11 | `create_surface` |
| 18 | `native_n32_image_info` |
| 31 | `wrap_native_surface` |
| 46 | `write_surface_png` |

**`render/widgets/value.rs`** (3 functions) — widget layout helpers:
| Line | Function |
|------|----------|
| 203 | `gradient_triangle_height` |
| 400 | `metric_vertical_metrics_text` |
| 421 | `metric_icon_top_from_value_layout` |

**`render/format.rs`** (2 functions):
| Line | Function |
|------|----------|
| 146 | `format_metric_parts` |
| 383 | `format_time_key` |

**`activity/interpolate.rs`** (1 function):
| Line | Function |
|------|----------|
| 172 | `densify_activity` |

**`src/bin_common.rs`** (7 functions) — CLI diagnostic helpers:
`repo_root`, `read_arg`, `read_optional_arg`, `read_positional`, `resolve_path`, `format_mmss`, `unix_timestamp`

**Requirement beyond the tables above:** after documenting the named priority items, sweep the rest of the workspace for remaining undocumented `pub` items. The plan is not complete until every public item in `src-tauri/` and `src-tauri/ovrley_core/` that benefits from docs has been reviewed.

### 3.4 Docstring Template for `pub fn`

```rust
/// Short one-line summary of what this function does.
///
/// Longer explanation of purpose, algorithm, or context. Explain WHY this
/// function exists, not just WHAT it does.
///
/// # Arguments
///
/// * `param_name` — What it represents, valid ranges, ownership expectations.
///
/// # Returns
///
/// Description of the return value and what the caller should do with it.
///
/// # Errors
///
/// Returns [`CoreError::Config`] if:
/// - specific condition A
/// - specific condition B
///
/// Returns [`CoreError::Ffmpeg`] if the subprocess exits non-zero.
///
/// # Performance
///
/// O(n) in number of config fields. Called once per render. Not a hot path.
///
/// # Panics
///
/// Does not panic. (Or: Panics if invariants are violated.)
pub fn function_name(...) -> CoreResult<...> { ... }
```

### 3.5 Verification for Step 2

- [ ] Every item in the tables above has a `///` docstring
- [ ] Remaining public items across all `.rs` files in `src-tauri/` have been reviewed; meaningful public APIs are documented even if they were not called out explicitly above
- [ ] Each docstring explains **why**, not just **what**
- [ ] Error conditions are enumerated (which `CoreError` variants and when)
- [ ] Performance notes exist for hot-path functions
- [ ] Threading notes exist for functions that touch shared state
- [ ] `cargo doc --no-deps` generates without errors
- [ ] No broken intra-doc links (use `[` `]` syntax where cross-referencing other items)

```bash
# Check for undocumented pub items (warnings from rustdoc)
cargo rustdoc -- -D warnings 2>&1
```

---

## 4. Step 3 — Long Function Documentation (Layered Narrative Approach)

**Purpose:** For every function identified as "long" (100+ lines, or multi-phase orchestration, or complex state management), apply a **three-layer documentation approach** that makes the function's internal logic transparent without changing any executable code.

This step implements the following instruction from the master plan:

> *In case of long functions, your task is to document them thoroughly using a layered narrative approach.*

### 4.1 Target Functions

The following functions were identified as long or complex enough to warrant the full layered treatment. For each, perform all three layers described below.

**Priority A — Multi-Phase Orchestration Functions (full lifecycle: parse → plan → execute → finalize → cleanup):**

| File | Line | Function | Lines | Description |
|------|------|----------|-------|-------------|
| `encode/video_pipeline.rs` | 55 | `render_video_single` | ~198 | Single-pass transparent overlay render |
| `encode/video_composite_pipeline.rs` | 196 | `render_composite_video_single` | ~273 | Full composite render orchestration |
| `encode/video.rs` | 248 | `render_composite_video_segmented` | ~232 | Parallel segmented composite render |
| `encode/video.rs` | 551 | `render_video_segmented` | ~184 | Parallel segmented transparent overlay render |

**Priority B — Large Logic-Heavy Functions:**

| File | Line | Function | Lines | Description |
|------|------|----------|-------|-------------|
| `encode/ffmpeg_settings.rs` | 39 | `build_ffmpeg_settings` | ~216 | Large match over codecs constructing FFmpeg args |
| `encode/codec_detect.rs` | 48 | `detect_codecs` | ~445 | Probes all available encoders via subprocesses |
| `render/widgets/elevation.rs` | 82 | `draw_elevation_widget` | ~203 | Per-frame elevation widget compositing |
| `render/widgets/elevation.rs` | 288 | `normalize_elevation_plot` | ~132 | Deep normalization of elevation plot options |
| `render/widgets/route.rs` | 135 | `normalize_route_plot` | ~75 | Normalization of route plot options |
| `render/widgets/value.rs` | 105 | `draw_gradient_value_widget` | ~87 | Gradient widget with triangle geometry |

**Priority C — Functions with Complex State/Threading/FFmpeg Ownership:**

| File | Line | Function | Description |
|------|------|----------|-------------|
| `encode/video_composite_pipeline.rs` | 664 | `spawn_composite_ffmpeg_process` | FFmpeg subprocess spawning (3-input composite) |
| `encode/video_pipeline.rs` | 328 | `spawn_ffmpeg_process` | FFmpeg subprocess spawning (single-pass transparent) |
| `encode/video_composite_pipeline.rs` | 486 | `terminate_composite_ffmpeg_after_cancel` | Cancellation + process kill with timeout |
| `encode/video.rs` | 33 | `run_parallel_renders` | Multi-config parallel render benchmark |
| `commands/mod.rs` | 83 | `backend_render` | Tauri command: validates, dispatches background render thread |
| `commands/mod.rs` | 140 | `backend_render_composite_phase3` | Tauri command: composite render dispatch |

**Total: 16 functions.**

### 4.2 Layer 1 — Top-Level Docstring

Before editing anything else, write a standard Rust docstring (`///`) for the function explaining:

```rust
/// One-line summary of the function's overall purpose.
///
/// Longer explanation of what this function accomplishes end-to-end.
/// Include context about when it is called and what it produces.
///
/// # Arguments
///
/// * `paths` — Central path configuration (fonts, templates, output directories).
/// * `config` — Validated render configuration with all scene/widget settings.
/// * `dense_activity` — Frame-aligned dense activity report with interpolated data.
/// * `controller` — Shared render state (progress, cancellation flag, render ID).
///             Cloning this shares the same underlying `Arc<Mutex<...>>` state.
/// * ... (document all parameters)
///
/// # Returns
///
/// On success, returns the path to the rendered output file.
/// Also writes debug timing summaries to the debug directory.
///
/// # Errors
///
/// Returns [`CoreError::Cancelled`] if the user cancelled during rendering.
/// Returns [`CoreError::Ffmpeg`] if the ffmpeg subprocess exits non-zero.
/// Returns [`CoreError::Render`] if a frame fails to render.
/// Returns [`CoreError::Io`] if output directory creation or file writing fails.
///
/// # Performance
///
/// This is a render hot path. Frame rendering and ffmpeg stdin writing happen
/// in a tight loop. Avoid allocations inside the loop.
///
/// # Thread Safety
///
/// Spawns a writer thread (ffmpeg stdin) and a monitor thread (ffmpeg stderr).
/// Both threads are joined before this function returns. The render loop runs
/// on the calling thread.
///
/// # Cancellation
///
/// Checks `controller.cancel_flag` between every frame. On cancellation:
/// 1. Stops producing new frames
/// 2. Closes ffmpeg stdin (so ffmpeg can finalize)
/// 3. Waits for ffmpeg to exit (with timeout, then kills)
/// 4. Joins writer and monitor threads
/// 5. Cleans up partial output file
/// 6. Returns `CoreError::Cancelled`
///
/// # Panics
///
/// Does not panic. All error paths return `CoreResult`.
pub fn render_video_single(...) -> CoreResult<PathBuf> { ... }
```

**Rules for Layer 1:**
- Every parameter must be documented
- Every error variant that can be returned must be enumerated
- If the function spawns threads, document which threads and their lifecycle
- If the function touches a hot path, add `# Performance` section
- If the function touches shared state, add `# Thread Safety` section
- If the function implements cancellation, add `# Cancellation` section
- Do not copy these sections mechanically — tailor them to the function

### 4.3 Layer 2 — Intermediate Step Architecture

After writing the top-level docstring, print (to the terminal / planning output, not into the source file) a brief bulleted list breaking the function into its major logical phases. Example for `render_video_single`:

```
Phase breakdown for render_video_single:

Phase 1: VALIDATION
  - Check cancellation before doing any work
  - Build FFmpeg settings from config (validates codec availability)

Phase 2: PREPARATION
  - Create debug output directory
  - Prepare Skia render assets (fonts, cached labels, route/elevation geometry)
  - Create frame buffer pool (reusable RGBA pixel buffers for the pipeline)

Phase 3: FFMPEG PROCESS SPAWN
  - Resolve ffmpeg binary path
  - Spawn ffmpeg child process with piped stdin (raw RGBA) and stderr
  - Assert that stdin and stderr handles were created successfully

Phase 4: THREAD SPAWN
  - Spawn monitor thread: reads ffmpeg stderr, parses "frame=N" status lines,
    updates shared AtomicU32 encoded_frames counter
  - Spawn writer thread: reads rendered frames from mpsc channel, writes raw RGBA
    to ffmpeg stdin, returns buffers to free pool, respects cancellation flag

Phase 5: HOT RENDER LOOP
  - For each frame index in [0, total_frames):
    a. Poll cancellation flag — if set, break out of loop
    b. Acquire frame buffer from pool (with timeout + cancellation check)
    c. Render one frame via render_frame_rgba() into the buffer
    d. Queue the frame to the writer thread via bounded mpsc channel
    e. Update progress (frame count, elapsed time)
  - On cancellation during loop: drop writer sender, join threads, kill ffmpeg

Phase 6: FINALIZATION
  - Set running = false on controller
  - Set progress state to Completed or Failed
  - Drop channel sender (signals writer thread to exit)
  - Join writer thread (collects any pipe-write errors)
  - Join monitor thread
  - Wait for ffmpeg child process to exit
  - Check ffmpeg exit status (non-zero → CoreError::Ffmpeg)
  - Validate encoded frame count vs expected frame count

Phase 7: DIAGNOSTICS & CLEANUP
  - Write timing summary JSON to debug directory
  - On error: remove partial output file
  - On success: return output path
```

**Rules for Layer 2:**
- This is NOT written into the source file — it is printed to the terminal as a planning artifact before editing
- Breaks the function into 5–8 named phases
- Each phase has a clear purpose and a bulleted list of sub-steps
- Edge cases and error paths are noted inline (e.g., "On cancellation during loop: ...")
- This serves as the architectural map that guides the inline comments in Layer 3

### 4.4 Layer 3 — Inline Commentary

After planning the phase breakdown, edit the function to add targeted inline comments (`//`) at non-obvious phase boundaries, ownership handoffs, tricky branches, and complex operations whose intent is not already clear from the code.

**CRITICAL RULES for inline comments:**

1. **Do NOT say what the code does.** Do not write `// increment i` or `// write to stdin`. Explain **WHY** it does it.

2. **Focus on invisible context:**
   - State changes that happened before this line and are assumed true
   - Assumptions about the data at this specific point (e.g., "At this point, `config.overlay_fps` is guaranteed to be a valid rational number because validation happened in `parse_config_json`")
   - Why a specific error fallback was chosen (e.g., "We kill instead of wait because if ffmpeg is hung, `wait()` would block indefinitely — the 2-second timeout via `try_wait` is a safety valve")
   - What edge case a branch is handling (e.g., "When the video is shorter than the activity, we clamp the render duration to the video length — otherwise ffmpeg would error on seeking past EOF")
   - Business logic dictating a particular choice (e.g., "We use `prores_ks` instead of `prores` because the former supports alpha channel, which is required for transparent overlays")

3. **Place comments where they add real value:**
   - Before phase transitions that are not obvious from the function shape alone
   - Before `thread::spawn` calls (explain the thread's role and shutdown contract)
   - Before `unsafe` blocks (explain the invariant being upheld)
   - Before non-obvious `match` branches or loops where the invariant, termination rule, or branch purpose is easy to miss
   - Before complex mathematical operations or coordinate/timing formulas
   - Before error-handling branches where the recovery strategy or failure mode is not self-evident
   - Do **not** add comments to every `match`, every loop, or every branch mechanically

4. **Do not refactor or change any executable code logic.** This step adds comments ONLY. The code must compile identically before and after.

**Example of good vs bad inline comments:**

```rust
// BAD — says what the code does:
// Loop over frames
for frame_index in 0..total_frames {
    // Check if cancelled
    if controller.is_cancelled() {
        // Break out of loop
        break;
    }
    // Render frame
    let frame = render_frame_rgba(&request)?;
    // Send to writer
    tx.send(frame)?;
}

// GOOD — explains why, what is assumed, what edge cases are handled:
// ── PHASE 5: HOT RENDER LOOP ──────────────────────────────────────
// At this point: ffmpeg is running with stdin open, the writer thread is
// draining the channel, and the monitor thread is parsing stderr. We own
// the render thread and must produce exactly `total_frames` frames.
// The bounded channel (capacity = 4) provides backpressure: if the writer
// falls behind, `send()` blocks here, preventing unbounded memory growth.
for frame_index in 0..total_frames {
    // Poll cancellation between every frame. We check here rather than
    // mid-frame because interrupting Skia drawing mid-operation could
    // leave surfaces in an inconsistent state. A cancelled render still
    // produces a valid partial output file (ffmpeg finalizes on stdin close).
    if controller.is_cancelled() {
        break;
    }

    // Render one frame of RGBA pixel data at the current frame index.
    // `render_frame_rgba` is the hot-path entry point — it composites
    // all visible widgets (metric values, route plot, elevation profile)
    // onto the base video frame for this exact timestamp.
    let frame = render_frame_rgba(&request)?;

    // Send the completed frame to the writer thread. If the channel is
    // full, this blocks until the writer drains a buffer — this is
    // intentional backpressure that keeps memory bounded.
    tx.send(frame)?;
}
```

### 4.5 Execution Order for Step 3

Process the functions in priority order:

1. **Priority A** (4 multi-phase orchestration functions) — these are the most complex and benefit most from layered documentation
2. **Priority B** (6 large logic-heavy functions) — apply the layered approach, but keep inline commentary selective and focused on non-obvious logic
3. **Priority C** (6 thread/state functions) — at minimum, ensure Layer 1 (top-level docstring) is complete; add Layer 3 inline comments only where threading/cancellation logic is non-obvious

For each function:
1. Read the full function body
2. Write Layer 1 (top-level docstring `///`)
3. Plan Layer 2 (print phase breakdown to terminal — this is the planning artifact)
4. Execute Layer 3 (add selective inline `//` comments only where they clarify intent, invariants, ownership, or tricky behavior)
5. Verify: `cargo build` succeeds with no new warnings
6. Verify: the function's behavior is unchanged (comments only, no logic changes)

### 4.6 Verification for Step 3

- [ ] All 16 functions have a complete Layer 1 docstring meeting the template
- [ ] All 16 functions have sufficient inline comments at non-obvious phase boundaries, ownership handoffs, tricky branches, and complex operations
- [ ] No executable code was changed — `git diff` shows only comment additions
- [ ] `cargo build` succeeds
- [ ] `cargo clippy -- -D warnings` does not produce new warnings from the documented functions
- [ ] Comments focus on WHY, not WHAT
- [ ] Edge cases, state assumptions, and error fallback reasoning are explained inline

```bash
# Verify only comments were added (no logic changes)
git diff -- src-tauri/ | Select-String "^[-+]" | Select-String -NotMatch "^[-+]\s*//"

# Should return nothing (or only the docstring additions which are also // style)
```

---

## 5. Step 4 — Threading, Lifecycle, and Cancellation Documentation

**Purpose:** For every piece of shared state (`RenderController`, `AtomicBool` flags, `Mutex` guards, thread handles, ffmpeg `Child` processes), document ownership, state transitions, thread safety guarantees, and shutdown behavior. This directly implements master plan Sections 8, 9, 15.3, and 15.5.

### 5.1 Document `RenderController` State Transitions

The `RenderController` (in `encode/progress.rs`) is the central coordination point for all render state. Its docstring must include a full state transition diagram:

```rust
/// Shared render state for progress polling and cancellation.
///
/// Clones share the same underlying state via `Arc<Mutex<...>>`.
/// Only one render may be active at a time — enforced by `running`
/// (AtomicBool with `compare_exchange`).
///
/// # State Transitions
///
/// ```text
/// Idle ──► Running ──► Completed
///   ▲        │  │
///   │        │  └──► Failed
///   │        │
///   └────────┴──────► Cancelled
/// ```
///
/// - **Idle**: `running == false`, `progress.status == Idle`. No render in flight.
/// - **Running**: `running == true`, `progress.status == Running`. A render thread
///   is actively producing frames. `cancel_flag` is false.
/// - **Completed**: `running == false`, `progress.status == Completed`. The render
///   finished normally; output file exists.
/// - **Failed**: `running == false`, `progress.status == Failed`. The render
///   encountered an unrecoverable error; partial output is cleaned up.
/// - **Cancelled**: `running == false`, `progress.status == Cancelled`. The user
///   called `cancel()`; partial output is cleaned up.
///
/// # Thread Safety
///
/// - `progress: Arc<Mutex<RenderProgress>>` — Mutex-protected; locked briefly
///   on each progress update (not per-frame in hot path — batched updates).
///   Also locked on frontend progress polling via `backend_progress` command.
/// - `cancel_flag: Arc<AtomicBool>` — Lock-free; read on every frame boundary
///   in the render loop. Written once by `cancel()`.
/// - `running: Arc<AtomicBool>` — Lock-free; used as a compare_exchange gate
///   to prevent concurrent render starts.
/// - `next_render_id: Arc<AtomicU32>` — Lock-free; monotonically increasing.
///
/// # Ownership
///
/// Created by `commands::backend_render` (or composite variant). One clone is
/// returned to the frontend for progress polling. The original is moved into
/// the background render thread. All clones observe the same state.
///
/// # Shutdown
///
/// On `cancel()`: sets `cancel_flag = true`. The render thread polls this flag
/// between frames and initiates shutdown (close ffmpeg stdin, join threads,
/// kill ffmpeg, clean up output). `running` is set to `false` after cleanup
/// completes. If the render thread panics, `running` remains `true` — this is
/// a known limitation (no watchdog thread).
pub struct RenderController { ... }
```

### 5.2 Document FFmpeg Process Lifecycle

In both pipeline files (`video_pipeline.rs` and `video_composite_pipeline.rs`), document the FFmpeg process lifecycle once at the most useful scope. Prefer a module-level explanation if multiple functions share the same lifecycle; use a function-level explanation only when the lifecycle meaningfully differs:

```rust
/// # FFmpeg Process Lifecycle
///
/// 1. **Spawn**: `spawn_ffmpeg_process()` creates the child process with piped
///    stdin (raw RGBA video frames) and piped stderr (progress parsing).
///    The child inherits no stdin from the parent.
///
/// 2. **Stdin ownership**: The writer thread takes `child.stdin.take()` and
///    holds exclusive ownership. It writes raw RGBA frames in a loop, then
///    drops the handle to signal EOF — this causes ffmpeg to finalize the
///    output file.
///
/// 3. **Stderr ownership**: The monitor thread takes `child.stderr.take()` and
///    holds exclusive ownership. It reads `frame=N` progress lines and updates
///    an `Arc<AtomicU32>` shared counter.
///
/// 4. **Wait**: After the writer finishes and stdin is closed, the main thread
///    calls `child.wait()` to collect the exit status.
///
/// 5. **Cancellation path**: If cancelled:
///    a. The render loop stops producing frames
///    b. The channel sender is dropped (signals writer to exit)
///    c. The writer finishes its current write, then exits
///    d. The main thread drops stdin → ffmpeg finalizes partial output
///    e. `child.try_wait()` with a short timeout (2s)
///    f. If still running: `child.kill()` (forceful termination)
///    g. `child.wait()` to reap the zombie process
///
/// 6. **Error path**: If ffmpeg fails (non-zero exit):
///    a. The stderr output is collected for diagnostics
///    b. The partial output file is removed
///    c. `CoreError::Ffmpeg { status, stderr }` is returned
///
/// # Writer Thread Panic
///
/// If the writer thread panics (e.g., broken pipe from ffmpeg crashing),
/// the panic is caught via `thread::join()` which returns `Err`. The main
/// thread treats this as an encoding failure, kills ffmpeg if still alive,
/// and returns an error.
pub fn render_video_single(...) -> CoreResult<PathBuf> { ... }
```

### 5.3 Document Thread Ownership Map

In `encode/mod.rs`, add a section documenting all thread types and their ownership:

```rust
//! ## Thread Map
//!
//! | Thread Type | Spawned By | Owns | Shutdown Signal | Joined By |
//! |------------|------------|------|-----------------|-----------|
//! | Writer thread | `render_video_single` / `render_composite_video_single` | ffmpeg stdin pipe | Channel sender dropped (EOF to ffmpeg) | Spawning function before returning |
//! | Monitor thread | `render_video_single` / `render_composite_video_single` | ffmpeg stderr pipe | ffmpeg exits → stderr EOF | Spawning function before returning |
//! | Render worker (segmented) | `render_video_segmented` / `render_composite_video_segmented` | Per-segment ffmpeg process + buffer pool | Child controller cancel flag | Aggregator loop via `thread::join()` |
//! | Parallel render worker | `run_parallel_renders` | Independent config + ffmpeg process | Work queue exhaustion | `run_parallel_renders` via `thread::join()` |
//! | Command dispatch thread | `backend_render` / `backend_render_composite_phase3` | The full render call | Render completion / cancel / error (updates shared controller) | Fire-and-forget (not joined) |
//! | Preview HTTP server | `VideoServer::start` | `tiny_http::Server` accept loop | `shutdown_flag` AtomicBool | `VideoServerHandle::stop()` via `thread::join()` |
```

### 5.4 Add `#[must_use]` Where Ignoring Return Would Be a Bug

Per master plan Section 15.5, audit and add `#[must_use]` to:

- `RenderController::progress()` — caller must use the snapshot, not discard it
- `RenderController::cancel()` — caller should check whether cancellation was already in progress
- `ProgressEstimator::estimate_fps()` — computed value is the whole point
- `probe_video()` — probe result is the entire purpose of the call
- `detect_codecs()` — expensive subprocess call; result must be consumed
- `parse_config_json()` — parsed config is the whole point
- `trim_activity()` — trimmed result is the whole point

**Rule:** Add `#[must_use]` only where ignoring the return value is probably a bug. Do not add it mechanically to every `pub fn`.

### 5.5 Verification for Step 4

- [ ] `RenderController` docstring includes full state transition diagram
- [ ] Both pipeline files document FFmpeg process lifecycle (spawn → stdin/stderr ownership → wait/kill → cleanup)
- [ ] `encode/mod.rs` includes a thread ownership map
- [ ] `#[must_use]` added to the listed functions where appropriate
- [ ] `cargo doc --no-deps` generates threading/lifecycle documentation
- [ ] No broken intra-doc links

```bash
# Verify doc generation
cargo doc --no-deps 2>&1 | Select-String "error|warning"
```

---

## 6. Step 5 — Test Documentation

**Purpose:** Every test file and every non-trivial `#[test]` function should be documented so that future contributors understand what behavior is being verified, why the test exists, what fixtures it uses, and what regressions it guards against. This requirement applies to every test-related `.rs` file in the workspace, including crate-level tests, nested test modules, and shared test helpers.

### 6.1 Test File Module-Level Documentation

Each test file must begin with a `//!` doc comment answering:

- **What this test file covers** (e.g., "Config parsing and normalization tests")
- **What fixtures it uses** (reference `test_config` paths)
- **What phases/flows are tested** (e.g., "Tests: valid configs, invalid configs, default handling, composite fields, serde round-trip")
- **Whether it's a unit test or integration test** and what real resources it needs (e.g., "Does not require ffmpeg" or "Requires sample MP4 at `test_config::sample_video_path()`")

Treat the list below as a minimum checklist, not the full scope. If any additional test `.rs` files exist when work begins, they must be documented too.

**Every test-related Rust file must have top-of-file documentation.** That includes test suites, test module roots, and shared helpers:

| File | What to Document |
|------|------------------|
| `activity_tests.rs` | Parsing of GPX-derived and FIT-derived JSON, field handling, debug payload wrapper |
| `cancellation_tests.rs` | Cancel lifecycle: start → cancel → progress state → cleanup → no stale running state |
| `codec_detect_tests.rs` | Codec probing behavior; fixture-based tests using stored probe output |
| `commands_tests.rs` | Command orchestration: input validation, render dispatch, progress polling, template listing |
| `common/mod.rs` | Test helpers; what each helper function does |
| `composite_profile_tests.rs` | Composite encoder profile resolution and template expansion |
| `config_tests.rs` | Config parsing: valid configs, invalid configs, default values, composite fields, serde round-trip |
| `error_display_tests.rs` | `CoreError` Display impl output; verifies frontend receives readable messages |
| `ffmpeg_composite_tests.rs` | Composite ffmpeg command construction for various codecs and settings |
| `ffmpeg_settings_tests.rs` | Transparent ffmpeg command construction; codec selection, filters, pixel formats |
| `format_tests.rs` | Metric formatting output: speed, heartrate, elevation, cadence, power, time formatting |
| `fps_tests.rs` | Rational FPS arithmetic: reduction, conversion, frame count calculations for NTSC rates |
| `metric_kind_behavior_tests.rs` | MetricKind method behavior: `requirements()`, `icon()`, `format()` |
| `metric_kind_serde_tests.rs` | Serialization compatibility: old string values deserialize correctly via `#[serde(rename)]` |
| `progress_tests.rs` | ProgressEstimator: EMA-based FPS calculation, ETA estimation, edge cases (NaN, Infinity) |
| `rdp_elevation_tests.rs` | RDP simplification applied to elevation data; verifies point reduction without visual degradation |
| `rdp_route_tests.rs` | RDP simplification applied to route/map data |
| `value_widget_tests.rs` | Metric value widget layout and rendering behavior |
| `video_composite_pipeline_tests.rs` | Composite pipeline planning: timing derivation, overrun guard, frame count verification |
| `video_probe_tests.rs` | Video metadata extraction from stored ffprobe JSON fixtures |
| `video_tests.rs` | Video/encode orchestration integration behavior |
| `widgets/tests/mod.rs` | Widget test module root; what sub-modules are declared |
| `video_server_tests.rs` | HTTP range request parsing, header construction, 404/416 responses |

### 6.2 Test Function Documentation

Each non-trivial `#[test]` function should have a short `///` docstring explaining:

- **What specific behavior/scenario is being tested**
- **What input/arrangement is used** (e.g., "Uses `test_config::simple_config_path()`")
- **What the expected outcome is** (e.g., "Expects `CoreResult::Ok` with 900 frames at 30fps for 30s duration")
- **What regression this test guards against**

Use function-level test docstrings for:

- tests whose setup, fixtures, or expected behavior are not obvious from the test name alone
- regression tests tied to a specific bug, race, parsing edge case, or lifecycle guarantee
- tests that exercise multistep orchestration, timing, cancellation, or external-tool interaction

Short, obvious tests with clear names may rely on a strong file-level `//!` doc instead of a repetitive per-test paragraph. Do not add boilerplate docstrings that merely restate the function name.

Examples of appropriate function-level test docs:

```rust
/// Verifies that parsing a valid GPX-derived activity JSON produces the expected
/// number of data points, with all required fields (latitude, longitude, timestamp,
/// elevation, heartrate) present and correctly typed.
///
/// Uses `test_config::parsed_activity_path()` — the gpx-parse-debug.json fixture.
///
/// Regressions guarded: this test catches changes to the JSON schema, field renaming
/// that breaks deserialization, and type mismatches (e.g., string where f64 expected).
#[test]
fn parse_valid_gpx_activity() { ... }
```

```rust
/// Verifies that cancellation during an active render correctly:
/// 1. Sets the cancellation flag (preventing new frames)
/// 2. Transitions progress state to Cancelled
/// 3. Resets `running` to false (allowing subsequent renders)
/// 4. Cleans up partial output files
///
/// This is a multi-threaded test — the render runs on a background thread
/// while the test thread calls cancel() after a short delay.
#[test]
fn cancel_mid_render_cleans_up_state() { ... }
```

### 6.3 Document `test_config.rs` and `common/mod.rs`

`tests/common/test_config.rs` already has a `//!` doc — review and enhance:

```rust
//! Central fixture-path resolver for all tests.
//!
//! Every test in the crate imports its fixture paths from this file.
//! There should be no `repo_root()`, `fixture_path()`, or `parent().unwrap()`
//! chains in individual test files — all path resolution lives here.
//!
//! To redirect all tests to a different fixture directory (e.g., CI mirror),
//! change the `fixtures()` function. No other test file needs modification.
//!
//! # Available fixtures
//!
//! - `parsed_activity_path()` → `tests/fixtures/activity/gpx-parse-debug.json`
//! - `simple_config_path()`   → `tests/fixtures/config/simple.json`
//! - `composite_config_path()`→ `tests/fixtures/config/composite.json`
//! - `invalid_config_path()`  → `tests/fixtures/config/invalid.json`
//! - `sample_video_path()`    → `tests/fixtures/video/test-1080p.mp4`
//! - `ffprobe_1080p_path()`   → `tests/fixtures/ffprobe/1080p.json`
//! - ... (list all)
```

### 6.4 Shared Test Helper Coverage

`tests/common/test_config.rs` and any `tests/common/mod.rs` file are also in scope. They are not optional helper docs; they are required documentation targets because they define how the entire test suite resolves fixtures and organizes support code.

### 6.5 Address Remaining Test Wiring

One `#[cfg(test)] mod tests;` directive remains at `render/widgets/mod.rs:39-40`. If this was intentionally kept for module-local test access (rdp_elevation_tests, rdp_route_tests), document why:

```rust
// Module-local tests for widget-specific RDP behavior that exercise internal
// types not available from crate-level integration tests. These tests are
// in a `tests/` subdirectory, not inline, and are gated by `#[cfg(test)]`.
#[cfg(test)]
mod tests;
```

### 6.6 Verification for Step 5

- [ ] Every test-related `.rs` file under `src-tauri/tests/`, `src-tauri/ovrley_core/tests/`, and nested `src/**/tests/` directories has top-of-file documentation
- [ ] Non-trivial `#[test]` functions have a `///` docstring; trivial tests with self-explanatory names may rely on a strong file-level `//!` doc
- [ ] Each test doc explains WHAT behavior, WHAT input, WHAT expected outcome, WHAT regression
- [ ] `test_config.rs` lists all available fixture paths
- [ ] `cargo test` passes with all tests still running
- [ ] `cargo doc --no-deps` generates test documentation (tests are private by default — verify at least that docs compile)

```bash
# Run all tests
cargo test

# Verify test docs don't break compilation
cargo test --no-run
```

---

## 7. Step 6 — Fix `cfg!(debug_assertions)` → `#[cfg(debug_assertions)]`

**Purpose:** Replace the two runtime `cfg!()` checks with compile-time `#[cfg]` attributes as specified in master plan Section 3.12. This prevents dead code from being compiled into release builds.

### 7.1 Site 1 — `src-tauri/src/lib.rs:31` (resource root)

**Current code:**
```rust
let resource_root = if cfg!(debug_assertions) {
    repo_root.clone()
} else {
    // resolve from Tauri resource dir
    ...
};
```

**Analysis:** Both branches need to compile in every build because the path resolution differs between dev (source checkout) and release (Tauri bundle). This is a legitimate use of `cfg!()` — the runtime check is correct here because the resolved path depends on the execution environment, not the compilation target. **Keep as-is.** Document why:

```rust
// NOTE: This uses cfg!(debug_assertions) intentionally, not #[cfg].
// In debug builds we resolve paths relative to the source checkout.
// In release builds we resolve relative to the Tauri resource directory.
// Both code paths must be compiled for both build types because the
// binary may be run from either location during development.
let resource_root = if cfg!(debug_assertions) { ... } else { ... };
```

### 7.2 Site 2 — `src-tauri/src/lib.rs:354` (Tauri log plugin)

**Current code:**
```rust
if cfg!(debug_assertions) {
    app.handle().plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Debug)
            .build(),
    )?;
}
```

**Action:** Replace `cfg!(debug_assertions)` with `#[cfg(debug_assertions)]`:

```rust
#[cfg(debug_assertions)]
{
    app.handle().plugin(
        tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Debug)
            .build(),
    )?;
}
```

**Why this is correct:** The Tauri log plugin should not exist in release builds at all — it adds runtime overhead and logging infrastructure. Using `#[cfg]` excludes the code (and the dependency, if feature-gated) from release compilation entirely.

**Verification:**
- Confirm `cargo build` succeeds in debug mode (log plugin present)
- Confirm `cargo build --release` succeeds (log plugin excluded)
- The `use tauri_plugin_log` import at the top of `lib.rs` may need to be gated with `#[cfg(debug_assertions)]` to avoid an unused-import warning in release builds:

```rust
#[cfg(debug_assertions)]
use tauri_plugin_log;
```

### 7.3 Verification for Step 6

- [ ] `cargo build` succeeds (debug mode)
- [ ] `cargo build --release` succeeds (release mode)
- [ ] `cargo clippy -- -D warnings` passes in both debug and release
- [ ] Comment added at site 1 explaining why `cfg!()` is intentionally kept
- [ ] Release binary no longer links `tauri_plugin_log`

```bash
# Verify both build modes
cargo build
cargo build --release

# Verify no unused-import warnings in release
cargo clippy --release -- -D warnings
```

---

## 8. Step 7 — Final Automated Checks

**Purpose:** Run the complete automated verification suite and ensure zero warnings, zero errors, all tests pass, and formatting is clean.

Unused-function and dead-code warnings need special handling in this phase. If a function is intentionally retained for API completeness, diagnostics, benchmarks, test seams, or platform-specific wiring and cannot be removed without violating the plan, fix the warning with the narrowest reasonable `allow` attribute such as `#[allow(dead_code)]` or `#[allow(unused)]`, placed on the specific item and accompanied by a short justification comment. Do not use broad crate-wide suppression, and do not hide genuinely accidental unused code.

### 8.1 Commands to Run

Run each command from the workspace root (`src-tauri/`):

```bash
# 1. Format check
cargo fmt --check
# If any files are unformatted, run: cargo fmt

# 2. Build check (debug)
cargo build
# Must succeed with 0 errors

# 3. Build check (release)
cargo build --release
# Must succeed with 0 errors

# 4. Clippy (debug)
cargo clippy -- -D warnings
# Must succeed with 0 warnings. If an intentionally-unused item is required,
# add a narrowly-scoped allow attribute with justification rather than leaving
# the warning unresolved.

# 5. Clippy (release)
cargo clippy --release -- -D warnings
# Same — 0 warnings or documented pre-existing.

# 6. Tests (debug)
cargo test
# All tests must pass. Record pass/fail/skip counts.

# 7. Tests (release, if different)
cargo test --release
# Should match debug results.

# 8. Documentation build
cargo doc --no-deps
# No errors. Warnings about broken intra-doc links are failures.

# 9. Deny check (if Cargo.toml has deny config)
cargo deny check
# If `deny.toml` exists, must pass.
```

### 8.2 Checklist

- [ ] `cargo fmt --check` — clean (0 unformatted files)
- [ ] `cargo build` — success (0 errors)
- [ ] `cargo build --release` — success (0 errors)
- [ ] `cargo clippy -- -D warnings` — success (0 warnings)
- [ ] `cargo clippy --release -- -D warnings` — success
- [ ] `cargo test` — all pass (record count: ___ passed, ___ failed, ___ ignored)
- [ ] `cargo test --release` — all pass (should match debug)
- [ ] `cargo doc --no-deps` — no errors, no broken intra-doc links
- [ ] `cargo deny check` — if configured, must pass
- [ ] No production source file contains inline `#[cfg(test)] mod tests { ... }`
- [ ] No production source file contains `#[path = "tests/..."] mod tests;`
- [ ] Any `#[allow(dead_code)]` / `#[allow(unused)]` added in Phase 6 is narrowly scoped to the specific item and justified by a short comment
- [ ] No `cfg!(debug_assertions)` remains where `#[cfg]` is correct (only the site 1 exception documented in Step 6)

### 8.3 If Errors Are Found

If any check fails:
1. Identify whether the failure is pre-existing or caused by Phase 6 changes
2. If pre-existing and it is an unused-item warning on code that must intentionally remain: fix it with the narrowest appropriate `allow` attribute plus a short justification comment, then rerun the check
3. If pre-existing and unrelated to Phase 6: document it and move on only if the phase owner explicitly accepts the deviation
4. If caused by Phase 6: fix immediately. The only Phase 6 changes that can affect compilation are: `#[cfg]` attribute changes (Step 6), `#[must_use]` additions (Step 4), intra-doc links (all steps), and targeted `allow` attributes for intentionally-unused items. Check these first.
5. Re-run the failing check after fixing.

---

## 9. Step 8 — Final Manual Verification

**Purpose:** Manually verify all user-facing flows to ensure no behavior was changed by any phase of the refactor. This is the final smoke test before declaring the refactor complete.

### 9.1 Manual Test Checklist

Execute each flow end-to-end. Do not skip any.

#### Video Import and Preview
- [ ] Import a video (MP4) — probe succeeds, metadata is correct
- [ ] Preview overlay renders correctly on the imported video
- [ ] Scrub through the video preview — overlay updates with correct metric values at each timestamp
- [ ] Preview server handles range requests correctly (seek to different positions)

#### Transparent Overlay Export
- [ ] Configure a scene with metric widgets (speed, heartrate, elevation, etc.)
- [ ] Export transparent overlay (prores_ks or qtrle codec)
- [ ] Verify the output file exists and has the expected frame count
- [ ] Verify the output plays correctly with the original video overlaid
- [ ] Verify cancellation works: start export, cancel mid-way, confirm state resets and no orphan processes
- [ ] Verify progress reporting updates during export

#### Composite MP4 Export
- [ ] Configure a composite scene with overlay + original video
- [ ] Export composite MP4
- [ ] Verify the output file exists and plays correctly
- [ ] Verify composite timing is correct (overlay aligned with video frames)
- [ ] Verify cancellation works for composite export
- [ ] Verify progress reporting updates during composite export

#### Templates and Configuration
- [ ] List available templates — all expected templates appear
- [ ] Load a saved template — all fields deserialize correctly
- [ ] Save a new template — serializes correctly and appears in the list
- [ ] Old config JSON (pre-refactor format) still parses correctly
- [ ] Unknown metric names produce clear, user-readable errors

#### System and Diagnostics
- [ ] `backend_health` returns healthy status
- [ ] `backend_current_os` returns correct OS
- [ ] `backend_list_system_fonts` returns font list
- [ ] Diagnostic binaries in `src/bin/` run correctly (if practical to test)

#### Error Handling
- [ ] Invalid config JSON produces a user-readable error (not raw Rust debug output)
- [ ] Invalid activity JSON produces a user-readable error
- [ ] Missing ffmpeg produces a clear error message
- [ ] Frontend receives readable error messages via Tauri IPC (`.to_string()` on `CoreError`)

#### Performance (Baseline Comparison)
- [ ] Representative preview render time is within 10% of Phase 1 baseline (or documented if different)
- [ ] Representative transparent export time is within 10% of baseline
- [ ] Representative composite export time is within 10% of baseline
- [ ] Memory usage during export is stable (no unbounded growth)

### 9.2 Cancellation Deep-Test

Perform a structured cancellation test:

1. Start a long export (30+ seconds)
2. Wait 5 seconds, then cancel
3. Verify: progress state shows `Cancelled`
4. Verify: `running` is `false` (can start a new render)
5. Verify: No ffmpeg.exe processes remain (check Task Manager / `Get-Process ffmpeg`)
6. Verify: No partial output files remain in the output directory
7. Start a new export — it must succeed (no stale state blocking it)
8. Repeat steps 1–7 for composite export

### 9.3 Verification Artifacts

After completing all manual tests, record:

```
Manual test results:
- Video import + preview: [PASS / FAIL]
- Transparent export: [PASS / FAIL]
- Composite export: [PASS / FAIL]
- Cancel + restart (transparent): [PASS / FAIL]
- Cancel + restart (composite): [PASS / FAIL]
- Template list/load/save: [PASS / FAIL]
- Old config compatibility: [PASS / FAIL]
- Error message readability: [PASS / FAIL]
- Performance regression: [NONE / <X>% change — documented]
- Orphan processes: [NONE / <N> found]
```

---

## 10. Completion Criteria

Phase 6 is complete when **all** of the following are true:

### Documentation Completeness
- [ ] Every `.rs` file under `src-tauri/` has appropriate file-level documentation; every module entry point and stand-alone module file has a `//!` doc following the template (Owns / Does not own / Allowed deps / Forbidden deps / Thread Safety / Performance)
- [ ] Every public function listed in Step 2 has a `///` docstring with purpose, errors, and context
- [ ] Every public struct listed in Step 2 has a `///` docstring
- [ ] Internal helpers in `render/text.rs`, `render/surface.rs`, `render/widgets/value.rs`, `render/format.rs` have docstrings
- [ ] All 16 long/complex functions have full layered documentation (top-level docstring + phase breakdown planning + inline commentary)
- [ ] All long-function inline comments explain WHY, not WHAT
- [ ] Documentation is detailed enough to explain intent and constraints, but not padded with repetitive or obvious commentary
- [ ] No executable code was changed by documentation additions except for the explicit compile-time/lint fixes called out elsewhere in this phase

### Threading and Lifecycle
- [ ] `RenderController` docstring includes full state transition diagram
- [ ] Both pipeline files document FFmpeg process lifecycle (spawn → stdin/stderr → wait/kill → cleanup)
- [ ] `encode/mod.rs` includes a thread ownership map
- [ ] `#[must_use]` added where ignoring return value would be a bug

### Test Documentation
- [ ] All test-related `.rs` files in the workspace have top-of-file documentation explaining coverage, fixtures/resources, and regression targets
- [ ] Non-trivial `#[test]` functions have a `///` docstring explaining WHAT behavior, WHAT input, WHAT expected outcome, and WHAT regression is guarded
- [ ] `test_config.rs` lists all available fixture paths
- [ ] Remaining `#[cfg(test)] mod tests;` is documented with justification

### Compile-Time Fixes
- [ ] `cfg!(debug_assertions)` at `lib.rs:354` replaced with `#[cfg(debug_assertions)]`
- [ ] `cfg!(debug_assertions)` at `lib.rs:31` kept intentionally, with comment explaining why
- [ ] Release builds exclude `tauri_plugin_log`

### Automated Checks (Green)
- [ ] `cargo fmt --check` — clean
- [ ] `cargo build` — success
- [ ] `cargo build --release` — success
- [ ] `cargo clippy -- -D warnings` — clean; no `dead_code` warnings
- [ ] `cargo clippy --release -- -D warnings` — clean; no `dead_code` warnings
- [ ] `cargo test` — all pass
- [ ] `cargo test --release` — all pass
- [ ] `cargo doc --no-deps` — no errors, no broken intra-doc links
- [ ] `cargo deny check` — pass (if configured)
- [ ] No unresolved dead code or unused-function warnings remain; any intentional exceptions use narrowly-scoped `allow` attributes with justification
- [ ] No commented-out `println!` or `eprintln!` statements in any production source file

### Architectural Integrity
- [ ] Dependency direction respected (config → activity → render → encode)
- [ ] `encode` does not depend on `commands`
- [ ] `render` does not depend on `encode`
- [ ] `video_pipeline` and `video_composite_pipeline` do not cross-import
- [ ] No `cfg!(debug_assertions)` remains where `#[cfg]` is correct

### Manual Verification
- [ ] All user-facing flows in Step 8 pass
- [ ] Cancellation deep-test passes (no orphan processes, state resets correctly)
- [ ] Performance within acceptable range of baseline
- [ ] Output paths, filenames, and formats unchanged

---

## Summary of Phase 6 Scope

| Area | Scope | Effort |
|------|-------|--------|
| File-level documentation sweep | All 77 current `.rs` files under `src-tauri/` | Large |
| Public API docstrings | All meaningful undocumented `pub` items, with named high-priority targets in Step 2 | Medium |
| Internal helper docstrings | Priority helpers plus any other non-obvious helpers discovered during the sweep | Medium |
| Long functions — full layered documentation (Priority A/B/C) | 16 named functions | Large |
| Thread/lifecycle documentation | `RenderController`, both pipelines, `encode/mod.rs` thread map | Medium |
| `#[must_use]` additions | Narrow, bug-preventing cases only | Small |
| Test file and test-function documentation | Every test-related `.rs` file and every meaningful `#[test]` function | Large |
| `cfg!(debug_assertions)` and lint cleanup | 1 required `#[cfg]` conversion, 1 intentional `cfg!()` exception, targeted `allow` fixes for intentional unused items | Small |
| Final automated checks | 9 commands | Small |
| Final manual verification | ~25 flows | Large |

**Estimated total effort:** 2–3 focused sessions.

---

## When in Doubt

1. **Document WHY, not WHAT** — the code already says WHAT; documentation must explain intent
2. **Preserve behavior** — Phase 6 adds documentation only (except the `#[cfg]` fix); do not touch logic
3. **Follow the template** — use the `//!` and `///` templates consistently
4. **Be detailed but concise** — explain intent, invariants, and ownership clearly, but skip trivial getters and avoid noise comments
5. **Keep hot paths clear** — inline comments must be valuable but not verbose enough to slow down reading the actual control flow
