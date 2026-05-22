# Phase 4 — Pipeline and Orchestration Cleanup

## Purpose

Separate planning, orchestration, rendering, encoding, progress reporting, and cancellation into cohesive, independently testable concerns. Reduce oversized function signatures. Enforce sibling pipeline isolation.

This phase builds on the typed errors and metrics from Phase 2 and the module cleanup from Phase 3. It assumes `CoreResult` / `CoreError`, `MetricKind`, `AppPaths` in `paths.rs`, consolidated interpolation, and split `render/widgets/common.rs` are already in place.

---

## Table of Contents

1. [Pre-Flight Checklist](#1-pre-flight-checklist)
2. [Step 1 — Extract ffmpeg Settings Construction](#2-step-1--extract-ffmpeg-settings-construction)
3. [Step 2 — Progress, Cancellation, and RenderController Ownership](#3-step-2--progress-cancellation-and-rendercontroller-ownership)
4. [Step 3 — Request Structs for Oversized Signatures](#4-step-3--request-structs-for-oversized-signatures)
5. [Step 4 — Separate Planning from Execution](#5-step-4--separate-planning-from-execution)
6. [Step 5 — Enforce Sibling Pipeline Isolation](#6-step-5--enforce-sibling-pipeline-isolation)
7. [Step 6 — Cleanup and Verification](#7-step-6--cleanup-and-verification)
8. [Completion Criteria](#8-completion-criteria)

---

## 1. Pre-Flight Checklist

Before starting Phase 4, verify the following preconditions from earlier phases:

### 1.1 Phase 2 preconditions

- [x] `ovrley_core/src/error.rs` exists with `CoreError` and `CoreResult`
- [x] `ovrley_core/src/types.rs` exists with `MetricKind` enum
- [x] `build_ffmpeg_settings` returns `CoreResult<FfmpegSettings>` (not `Result<T, String>`)
- [x] Tauri command boundary converts `CoreError` to `String` via `.to_string()`

### 1.2 Phase 3 preconditions

- [x] `ovrley_core/src/paths.rs` exists with `AppPaths` — verify with `rg "pub struct AppPaths" src-tauri/ovrley_core/src/paths.rs`
- [x] `ovrley_core/src/rdp.rs` exists with `perpendicular_distance` and `simplify_rdp`
- [x] `ovrley_core/src/interpolation.rs` exists as single source of truth (f64-based)
- [x] `render/widgets/common.rs` is reduced (~200 lines), with geometry/marker/polyline/transform split out (acknowledged: 472 lines, user accepted)
- [x] No `encode` → `commands` dependency (verify: `rg "use crate::commands" src-tauri/ovrley_core/src/encode/` returns nothing)

### 1.3 Test infrastructure

- [x] `cargo test` passes from workspace root
- [x] No `#[cfg(test)] mod tests` blocks in production source files
- [x] No `#[path = "tests/..."] mod tests` in production source files
- [x] `ovrley_core/tests/common/test_config.rs` exists and is used by all tests

### 1.4 Baseline before changes

- [x] Run `cargo test` and record result (all pass)
- [x] Run `cargo clippy -- -D warnings` and record result (pre-existing clippy errors outside Phase 4 scope; noted)
- [x] Manual smoke test: preview render works, transparent export works, composite export works

If any precondition fails, stop and resolve it before proceeding. Phase 4 depends on these being done.

---

## 2. Step 1 — Extract ffmpeg Settings Construction

**Purpose:** Separate ffmpeg command/settings derivation from ffmpeg binary discovery. This makes settings construction testable in isolation (snapshot tests without requiring ffmpeg on disk).

### 2.1 What Moves

From `encode/ffmpeg.rs` (370 lines) into a new `encode/ffmpeg_settings.rs`:

| Item | Current Location | Destination |
|------|-----------------|-------------|
| `FfmpegSettings` struct + `Debug`/`Clone` derives | `ffmpeg.rs:85–103` | `ffmpeg_settings.rs` |
| `build_ffmpeg_settings` function | `ffmpeg.rs:110–326` | `ffmpeg_settings.rs` |
| `append_ffmpeg_option` helper | `ffmpeg.rs:329–352` | `ffmpeg_settings.rs` (private) |
| `append_extra_output_args` helper | `ffmpeg.rs:355–369` | `ffmpeg_settings.rs` (private) |

### 2.2 What Stays in `ffmpeg.rs`

These are about ffmpeg binary discovery and process setup, not settings construction:

| Item | Reason to Keep |
|------|---------------|
| `resolve_ffmpeg_binary` (~lines 21–61) | Finds ffmpeg on disk — separate concern |
| `suppress_child_console` (~lines 63–74) | Process launch setup — not settings |
| `find_in_path` (~lines 77–82) | Binary discovery helper |

After extraction, `ffmpeg.rs` will have ~120 lines: binary discovery + process helpers only.

### 2.3 Implementation Steps

#### 2.3.1 Create `encode/ffmpeg_settings.rs`

```rust
//! FFmpeg codec settings resolution.
//!
//! Owns: codec argument derivation from user-facing ffmpeg config values.
//! Does not own: ffmpeg binary discovery, process spawning, pipeline execution.
//!
//! Allowed dependencies: serde_json, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.

use serde_json::Value;
use crate::error::{CoreError, CoreResult};

/// Fully resolved ffmpeg settings for one render.
#[derive(Clone, Debug)]
pub struct FfmpegSettings {
    pub codec: String,
    pub loglevel: String,
    pub pix_fmt: String,
    pub output_args: Vec<String>,
    pub extension: String,
    pub muxer: Option<String>,
    pub hw_init_args: Vec<String>,
    pub filters: Option<String>,
}

/// Builds validated ffmpeg settings from `scene.ffmpeg`.
///
/// # Errors
/// Returns [`CoreError::Encode`] for unknown codecs or invalid configuration.
pub fn build_ffmpeg_settings(ffmpeg_config: &Value) -> CoreResult<FfmpegSettings> {
    // ... (moved from ffmpeg.rs, unchanged)
}
```

**Rules:**
- Copy-paste the exact code. Do not refactor, rename, or restructure.
- Change only the module path in `use` statements.
- Keep the doc-comment exactly as-is from `ffmpeg.rs`.

#### 2.3.2 Update `encode/ffmpeg.rs`

1. Remove `FfmpegSettings` struct, `build_ffmpeg_settings`, `append_ffmpeg_option`, `append_extra_output_args` from `ffmpeg.rs`.
2. Remove imports: `use serde_json::Value`, `use crate::error::{CoreError, CoreResult}` from `ffmpeg.rs` (only if they were solely used by the moved items).
3. Remove `#[allow(dead_code)]` annotations on helpers if they no longer apply.

#### 2.3.3 Update `encode/mod.rs`

Add the new module declaration, keeping alphabetical order:

```rust
pub mod ffmpeg;
pub mod ffmpeg_composite;
pub mod ffmpeg_composite_profiles;
pub mod ffmpeg_settings;   // NEW
pub mod fps;
```

Re-export `FfmpegSettings` and `build_ffmpeg_settings` for callers that previously imported them from `encode::ffmpeg`. **However**, prefer updating the callers to import directly from `crate::encode::ffmpeg_settings` rather than adding a re-export in `mod.rs`. This avoids an extra layer of indirection.

#### 2.3.4 Update All Callers

Search for all imports of `FfmpegSettings`, `build_ffmpeg_settings`, `append_ffmpeg_option`, `append_extra_output_args`:

```bash
rg "FfmpegSettings|build_ffmpeg_settings|append_ffmpeg_option|append_extra_output_args" --type rust src-tauri/
```

Expected callers (at minimum):

| File | Import to Update |
|------|-----------------|
| `encode/video_pipeline.rs` | `use crate::encode::ffmpeg::build_ffmpeg_settings` → `use crate::encode::ffmpeg_settings::build_ffmpeg_settings` |
| `encode/video_pipeline.rs` | `use crate::encode::ffmpeg::FfmpegSettings` → `use crate::encode::ffmpeg_settings::FfmpegSettings` |
| Any test file referencing these types | Same pattern |

Also update the type path in the `finalize_ffmpeg_settings` function signature in `video_pipeline.rs`:

```rust
// Before:
fn finalize_ffmpeg_settings(
    mut ffmpeg_settings: crate::encode::ffmpeg::FfmpegSettings,
) -> crate::encode::ffmpeg::FfmpegSettings

// After:
fn finalize_ffmpeg_settings(
    mut ffmpeg_settings: crate::encode::ffmpeg_settings::FfmpegSettings,
) -> crate::encode::ffmpeg_settings::FfmpegSettings
```

#### 2.3.5 Add Snapshot Tests for ffmpeg Settings

Create `ovrley_core/tests/ffmpeg_settings_tests.rs`:

```rust
use ovrley_core::encode::ffmpeg_settings::build_ffmpeg_settings;
use ovrley_core::error::CoreResult;
use serde_json::json;

#[test]
fn prores_ks_defaults() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info"
    }))?;
    assert_eq!(settings.codec, "prores_ks");
    assert_eq!(settings.pix_fmt, "yuva444p10le");
    assert_eq!(settings.extension, "mov");
    Ok(())
}

#[test]
fn qtrle_settings() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "qtrle",
        "loglevel": "error"
    }))?;
    assert_eq!(settings.codec, "qtrle");
    assert_eq!(settings.pix_fmt, "argb");
    assert_eq!(settings.extension, "mov");
    Ok(())
}

#[test]
fn unknown_codec_errors() {
    let result = build_ffmpeg_settings(&json!({
        "codec": "nonexistent_codec",
        "loglevel": "info"
    }));
    assert!(result.is_err());
}

#[test]
fn output_args_passthrough() -> CoreResult<()> {
    let settings = build_ffmpeg_settings(&json!({
        "codec": "prores_ks",
        "loglevel": "info",
        "output_args": ["-color_range", "2"]
    }))?;
    assert!(settings.output_args.contains(&"-color_range".to_string()));
    Ok(())
}
```

At a minimum, add one test per supported codec (`prores_ks`, `prores_ks_vulkan`, `prores_videotoolbox`, `qtrle`) plus error case and output_args passthrough.

### 2.4 Verification After Step 1

```bash
# From workspace root:
cargo test -p ovrley_core
cargo clippy -p ovrley_core -- -D warnings
```

Manual checks:
- [x] Transparent overlay export still works (uses prores/qtrle settings)
- [x] Composite export still works (uses different settings path via `ffmpeg_composite.rs`)
- [x] ffmpeg command output is identical to pre-move output (verified via `build_ffmpeg_settings` verbatim copy + 8 snapshot tests)

### 2.5 What to Keep in Mind

- **Do not alter any behavior in `build_ffmpeg_settings`.** The function body must be a verbatim copy. Even seemingly harmless "improvements" (like using `matches!` instead of `if let`) are out of scope.
- **`append_ffmpeg_option` and `append_extra_output_args` should be `pub(crate)` at most.** They are implementation details, not public API. If they are already private, keep them private.
- **Export strategy:** Do not add a re-export in `encode/mod.rs` unless multiple callers import from `ffmpeg.rs` for both discovery and settings. If callers need both, a re-export like `pub use ffmpeg_settings::build_ffmpeg_settings;` in `ffmpeg.rs` (not `mod.rs`) may be the least-breaking option. Prefer direct imports.
- **Inline tests in `ffmpeg.rs`:** If `ffmpeg.rs` has inline `#[cfg(test)]` blocks that test `build_ffmpeg_settings`, move them to `encode/tests/ffmpeg_settings_tests.rs` or the crate-level `tests/ffmpeg_settings_tests.rs`.

---

## 3. Step 2 — Progress, Cancellation, and RenderController Ownership

**Purpose:** Consolidate all progress estimation and render lifecycle state into `encode/progress.rs`. Document the state machine. Ensure cancellation is complete and testable.

### 3.1 Current State Audit

| Component | Current Location | Problem |
|-----------|-----------------|---------|
| `RenderController` | `encode/video.rs:33–167` | Mixed with segment/stitch orchestration logic |
| `ProgressEstimator` | `encode/progress.rs:17–101` | Clean EMA estimator, no tests in file |
| `RenderProgress` | `crate::debug::RenderProgress` | Debug type used for progress state — verify this is correct location or should move |
| `ProgressEstimator` tests | Master plan §17.14 says at line 106 — but research shows **no `#[cfg(test)]` block exists** | If tests are gone, this step is simplified. Verify. |

**Action:** Before proceeding, verify the current state of `encode/progress.rs`:

```bash
rg "#\[cfg\(test\)\]" src-tauri/ovrley_core/src/encode/progress.rs
```

If no matches, the tests have already been moved. If matches exist, they must be moved to `encode/tests/progress_tests.rs` before continuing.

### 3.2 Sub-Step 2a — Move `RenderController` from `video.rs` to `progress.rs`

#### 3.2.1 Assess `RenderController` Dependencies

`RenderController` depends on:

```rust
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU32, Ordering}};
use crate::debug::RenderProgress;
use crate::error::{CoreError, CoreResult};
```

All of these are already available in `encode/progress.rs` or are standard library types. No circular dependency risk.

#### 3.2.2 Extract `RenderController` Into `progress.rs`

1. Copy the `RenderController` struct definition and all its `impl` blocks verbatim from `video.rs` into `progress.rs`, after the `ProgressEstimator` code.
2. Add `RenderController` to the module's re-exports if needed.
3. Ensure the existing `ProgressEstimator` code is not modified.

The new structure of `encode/progress.rs` should be (~270 lines):

```rust
//! Live render progress estimation and render lifecycle state.
//!
//! Owns: ProgressEstimator (EMA-based FPS/ETA), RenderController (shared
//!   render state for frontend polling and cancellation).
//! Does not own: ffmpeg process lifecycle, frame rendering, queue management.
//!
//! Allowed dependencies: std, crate::debug, crate::error.
//! Forbidden dependencies: crate::commands, crate::render.
//!
//! ## Thread Safety
//! RenderController is Send + Sync (internally uses Arc<Mutex> and Arc<AtomicBool>).
//! ProgressEstimator is not Sync — it should be used by a single writer thread.
//!
//! ## State Transitions
//! ```text
//! Idle -> Running -> Completed
//!                 -> Failed
//!                 -> Cancelled
//! ```

// ... ProgressEstimator (existing, unchanged) ...

// ... RenderController (moved from video.rs, unchanged) ...
```

#### 3.2.3 Update `encode/mod.rs`

Make `RenderController` accessible from its new home. Options:

**Option A (preferred):** Have `video.rs` re-export from `progress`:
```rust
// In video.rs:
pub use crate::encode::progress::RenderController;
```
This minimizes call-site changes throughout the codebase.

**Option B:** Update all imports everywhere. More thorough but creates a larger diff.

Choose Option A for this step. A future cleanup can remove the re-export once all callers are verified.

#### 3.2.4 Update `video.rs`

1. Remove the `RenderController` struct and all its `impl` blocks from `video.rs`.
2. Add `pub use crate::encode::progress::RenderController;` at the top of `video.rs`.
3. Remove any imports that were only used by `RenderController` (e.g., `AtomicBool`, `AtomicU32`, `Ordering` if they have no other uses in `video.rs`).

#### 3.2.5 Verify All Callers

Search for `RenderController` usage:

```bash
rg "RenderController" --type rust src-tauri/
```

Expected callers (at minimum):

| File | Note |
|------|------|
| `encode/video.rs` | Original home — now re-exports from `progress` |
| `encode/video_pipeline.rs` | Uses `RenderController` for progress tracking |
| `encode/video_composite_pipeline.rs` | Uses `RenderController` for progress tracking |
| `commands/mod.rs` | Creates and polls `RenderController` from Tauri commands |

Verify each caller still compiles and behavior is unchanged.

### 3.3 Sub-Step 2b — Verify ProgressEstimator Test Placement

```bash
rg "#\[cfg\(test\)\]|#\[test\]" src-tauri/ovrley_core/src/encode/progress.rs
```

If **no** test blocks found: the tests are already in the correct location. Mark this sub-step complete.

If **test blocks are found**: they must be moved to `encode/tests/progress_tests.rs`:

```rust
use ovrley_core::encode::progress::ProgressEstimator;

#[test]
fn zero_frames_returns_none() {
    let mut estimator = ProgressEstimator::default();
    let (eta, fps) = estimator.record(0, 100, 0.0, 0.0);
    assert!(eta.is_none());
    assert!(fps.is_none());
}

#[test]
fn single_frame_no_estimate() {
    let mut estimator = ProgressEstimator::default();
    let (eta, fps) = estimator.record(0, 100, 0.5, 0.5);
    assert!(eta.is_none()); // warmup not complete
}

#[test]
fn warmup_completes_and_produces_estimate() {
    let mut estimator = ProgressEstimator::default();
    // Feed enough frames to complete warmup
    for i in 0..10 {
        estimator.record(i, 100, 1.0 / 30.0, i as f64 * (1.0 / 30.0));
    }
    let (eta, fps) = estimator.record(10, 100, 1.0 / 30.0, 10.0 * (1.0 / 30.0));
    assert!(eta.is_some());
    assert!(fps.is_some());
}

#[test]
fn default_uses_reasonable_smoothing() {
    let estimator = ProgressEstimator::default();
    // Verify the default smoothing factor is plausible
    let (_, _) = estimator.clone().record(1, 100, 0.016, 0.016);
}
```

### 3.4 Sub-Step 2c — Document Cancellation Lifecycle

Add or verify the following documentation on `RenderController`:

```rust
/// Shared render state for progress polling and cancellation.
///
/// Clones share the same underlying progress state via `Arc<Mutex>`.
/// Only one render may be active at a time (enforced by `try_start`).
///
/// # Cancellation Contract
///
/// When `cancel()` is called, the pipeline MUST:
///
/// 1. Stop enqueueing new frames for rendering
/// 2. Drop the frame sender (closing ffmpeg stdin)
/// 3. Wait for ffmpeg to exit (with timeout, then kill on hang)
/// 4. Join all worker threads (render, writer, monitor)
/// 5. Update progress state to Cancelled via `finish_error` with `cancelled: true`
/// 6. Clean up partial output files
/// 7. Reset `running` to `false` (allowing subsequent renders)
///
/// # State Transitions
///
/// ```text
/// Idle ──try_start()──▶ Running ──finish_success()──▶ Completed
///                       │
///                       ├──finish_error(cancelled=true)──▶ Cancelled
///                       └──finish_error(cancelled=false)──▶ Failed
/// ```
///
/// After any terminal state, the caller must call `try_start()` again
/// to begin a new render.
```

Verify that every pipeline (`video_pipeline.rs`, `video_composite_pipeline.rs`) honors this contract:
- [x] `cancel()` flag is checked between frames
- [x] Channel sender is dropped on cancellation detection
- [x] `finish_error()` is called with appropriate cancelled flag
- [x] `running` state is reset even on panic paths (or via Drop)

### 3.5 Verification After Step 2

```bash
cargo test -p ovrley_core
cargo clippy -p ovrley_core -- -D warnings
```

Manual checks:
- [x] Start a render → observe progress polling returns running state
- [x] Cancel a render → observe progress shows cancelled, no orphan ffmpeg process
- [x] Let render complete → observe progress shows completed
- [x] Start a new render after cancel/completion → works (running state reset correctly)
- [x] Check `ProgressEstimator` tests pass at new location

### 3.6 What to Keep in Mind

- **`RenderProgress` is in `crate::debug`.** The master plan §5 (Phase 4 deliverable 2a) says to move `RenderProgress` to `progress.rs`, but it's defined in `crate::debug::RenderProgress`. Evaluate: if `RenderProgress` is only used by `RenderController` and has no other consumers, moving it to `progress.rs` or making it a sub-type of `RenderController` is cleaner. If it's used by other modules (e.g., as a return type to Tauri commands), keep it in `debug` or move to a shared location. **Do not make this change unless it simplifies the dependency graph.**
- **`ProgressEstimator` is not thread-safe** — it has `&mut self` methods. It should be owned by the writer/render thread, not shared. `RenderController` wraps shared progress state with `Arc<Mutex<>>`. They serve different roles. Do not try to merge them.
- **`try_start` must be atomic.** It uses `compare_exchange` on the `running` flag. Verify this is not broken by the move.
- **Inline tests in `video.rs`:** `video.rs` may have tests that exercise `RenderController`. These tests must either:
  - Move to `encode/tests/progress_tests.rs` if they test progress/cancellation
  - Stay in `video.rs` tests if they test orchestration that incidentally uses `RenderController`
  - The test should import `RenderController` from its new location regardless.

---

## 4. Step 3 — Request Structs for Oversized Signatures

**Purpose:** Replace functions with 7+ parameters with plain request structs. Eliminate all `#[allow(clippy::too_many_arguments)]` suppressions from render and widget code.

### 4.1 Functions to Refactor

Based on the current code analysis, these are the functions with `#[allow(clippy::too_many_arguments)]`:

| # | Function | File | Args | Visibility | Priority |
|---|----------|------|------|------------|----------|
| 1 | `render_preview_with_prepared_assets` | `render/mod.rs:175` | 9 | `pub` | High |
| 2 | `render_frame_rgba` | `render/mod.rs:269` | 10 | `pub` | High |
| 3 | `render_frame_surface` | `render/mod.rs:322` | 10 | private | Medium |
| 4 | `render_frame_to_surface` | `render/mod.rs:374` | 11 | private | Medium |
| 5 | `draw_metric_value_widget_with_config` | `render/widgets/value.rs:54` | 9 | `pub(crate)` | High |
| 6 | `draw_gradient_value_widget` | `render/widgets/value.rs:97` | 8 | private | Low |
| 7 | `draw_metric_icon` | `render/widgets/value.rs:427` | 10 | private | Low |
| 8 | `widget_render_report` | `render/widgets/common.rs` | 8 | `pub(crate)` | Medium |

Additionally, the master plan §5 identifies 7 functions but the actual count from code research is 8. Not all need request structs — focus on the public and `pub(crate)` functions first.

### 4.2 Struct Design Rules

From the master plan §12.1 and §14.3:

- **Use plain structs with named fields, not builders.**
- **Do not add `.render()` methods** to the structs — keep functions free-standing.
- **Lifetimes:** Use `'a` for borrowed references. Group by lifetime where possible.
- **Keep field names identical to parameter names** to minimize diff noise.
- **Do not add `#[derive(Default)]`** — these structs have required fields, not optional ones.
- **Document each field briefly** if the purpose is non-obvious.

### 4.3 Implementation: `render_preview_with_prepared_assets`

#### 4.3.1 Create `PreviewRenderRequest`

Add to `render/mod.rs`, before the function:

```rust
/// Bundled parameters for a preview frame render.
///
/// Consolidates what was previously 9 separate parameters.
pub struct PreviewRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_preview_assets: &'a PreparedPreviewAssets,
    pub second: u32,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub label_cache_status: LabelCacheStatus,
    pub extra_total_ms: f64,
    pub out_path: &'a Path,
}
```

#### 4.3.2 Update Function Signature

```rust
// Before:
#[allow(clippy::too_many_arguments)]
pub fn render_preview_with_prepared_assets(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_preview_assets: &PreparedPreviewAssets,
    second: u32,
    prepare_timings: BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus,
    extra_total_ms: f64,
    out_path: &Path,
) -> CoreResult<((), PreviewRenderReport)>

// After:
pub fn render_preview_with_prepared_assets(
    request: PreviewRenderRequest<'_>,
) -> CoreResult<((), PreviewRenderReport)>
```

#### 4.3.3 Update Function Body

Replace parameter usage with field access:

```rust
// Before:
let out_path = out_path;

// After:
let out_path = request.out_path;
```

Apply methodically: for each parameter `foo`, replace with `request.foo`. Use the editor's find-replace to avoid missing any.

#### 4.3.4 Update Call Sites

Search for call sites:

```bash
rg "render_preview_with_prepared_assets" --type rust src-tauri/
```

Update each call site:

```rust
// Before:
render_preview_with_prepared_assets(
    &paths,
    &config,
    &dense_activity,
    &assets,
    second,
    prepare_timings,
    label_cache_status,
    extra_total_ms,
    out_path,
)?;

// After:
render_preview_with_prepared_assets(PreviewRenderRequest {
    paths: &paths,
    config: &config,
    dense_activity: &dense_activity,
    prepared_preview_assets: &assets,
    second,
    prepare_timings,
    label_cache_status,
    extra_total_ms,
    out_path,
})?;
```

**Important:** If the call site uses field shorthand (e.g., `paths` instead of `paths: paths`), keep the shorthand for conciseness where the variable name matches the field name.

### 4.4 Implementation: `render_frame_rgba`

#### 4.4.1 Create `FrameRenderRequest`

```rust
/// Bundled parameters for rendering a single frame to RGBA.
pub struct FrameRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_assets: &'a PreparedRenderAssets,
    pub frame_index: usize,
    pub scale: f32,
    pub labels_image: Option<&'a Image>,
    pub target: RenderTarget<'a>,
    pub frame_profiler: &'a mut RenderProfiler,
}
```

#### 4.4.2 Update Function Signature

```rust
// Before:
#[allow(clippy::too_many_arguments)]
pub fn render_frame_rgba(
    paths: &AppPaths,
    config: &RenderConfig,
    dense_activity: &DenseActivityReport,
    prepared_assets: &PreparedRenderAssets,
    frame_index: usize,
    scale: f32,
    labels_image: Option<&Image>,
    target: RenderTarget<'_>,
    frame_profiler: &mut RenderProfiler,
) -> CoreResult<()>

// After:
pub fn render_frame_rgba(
    request: FrameRenderRequest<'_>,
) -> CoreResult<()>
```

#### 4.4.3 Update Call Sites

Find the one(s) calling `render_frame_rgba`:

```bash
rg "render_frame_rgba" --type rust src-tauri/
```

Update similarly to 4.3.4.

### 4.5 Implementation: `draw_metric_value_widget_with_config`

#### 4.5.1 Create `MetricWidgetRequest`

Add to `render/widgets/value.rs`:

```rust
/// Bundled parameters for drawing a metric value widget.
pub(crate) struct MetricWidgetRequest<'a> {
    pub canvas: &'a Canvas,
    pub config: &'a RenderConfig,
    pub value: &'a ValueConfig,
    pub base_style: &'a ResolvedTextStyle,
    pub dense_activity: &'a DenseActivityReport,
    pub frame_index: usize,
    pub scale: f32,
    pub font_dirs: &'a [PathBuf],
    pub static_icon_rendered: bool,
}
```

#### 4.5.2 Update Function Signature

```rust
// Before:
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_metric_value_widget_with_config(
    canvas: &Canvas,
    config: &RenderConfig,
    value: &ValueConfig,
    base_style: &ResolvedTextStyle,
    dense_activity: &DenseActivityReport,
    frame_index: usize,
    scale: f32,
    font_dirs: &[PathBuf],
    static_icon_rendered: bool,
) -> bool

// After:
pub(crate) fn draw_metric_value_widget_with_config(
    request: MetricWidgetRequest<'_>,
) -> bool
```

### 4.6 Implementation: Remaining Private Functions

For private functions (`render_frame_to_surface`, `render_frame_surface`, `draw_metric_icon`, `draw_gradient_value_widget`, `widget_render_report`):

- **Priority is lower** — they don't affect external callers.
- **Only refactor if the functions are changed for other reasons in Phase 4** (e.g., as part of Step 4 planning/execution separation).
- If a private function is called from multiple places within the same module, a request struct still improves readability.
- **`render_frame_to_surface` and `render_frame_surface`** — these are called from `render_frame_rgba`, which will already have a `FrameRenderRequest`. Consider whether to reuse the same struct or create a more specific one.

**Decision rules:**
- If a private function takes 10+ args and is called from 2+ places → create a struct.
- If a private function takes 10+ args but only called once → struct is optional but still beneficial for readability.
- If a private function takes 7-9 args → use judgment. Don't force it.

### 4.7 Verification After Step 3

```bash
cargo test -p ovrley_core
cargo clippy -p ovrley_core -- -D warnings
```

Specifically check:
```bash
# Verify no too_many_arguments suppressions remain for the refactored functions:
rg "too_many_arguments" --type rust src-tauri/ovrley_core/src/render/
```

Manual checks:
- [x] Preview render output unchanged
- [x] Frame rendering output unchanged
- [x] Metric value widget display unchanged

### 4.8 What to Keep in Mind

- **This step creates a large diff.** The request struct changes touch many call sites. Each refactored function should be done as its own commit (or at least its own step, verified with `cargo test` between each).
- **`FrameRenderRequest` contains `&'a mut RenderProfiler`.** This means `FrameRenderRequest` is not reusable across frames (mutable borrow). This is correct — each frame needs its own profiler reference.
- **Avoid struct reuse between unrelated functions.** Don't force `FrameRenderRequest` onto `render_frame_surface` if the fields don't match cleanly. Better to have a smaller, more specific struct than a Frankenstein one.
- **Watch for clippy warnings about unnecessary field names** in struct construction. If the variable name matches the field name, use shorthand.
- **Do not add `#[allow(clippy::too_many_arguments)]` back.** After creating request structs, the suppression on the function signature must be removed.

---

## 5. Step 4 — Separate Planning from Execution

**Purpose:** Make render planning independently testable. Planning should produce a `RenderPlan` that can be validated and snapshotted without running ffmpeg. Execution should consume the plan.

### 5.1 Target Conceptual Structure

```
RenderRequest (user inputs)
    │
    ▼
derive_render_plan()  ← testable without ffmpeg, snapshot-friendly
    │
    ▼
RenderPlan (immutable, serializable)
    │
    ▼
execute_render_plan() ← owns ffmpeg/IO/threads, not snapshot-tested
```

This is the ideal. In practice, some areas already partially follow this pattern:

- `video_composite_pipeline.rs` already has `CompositePipelinePlan` (15 fields) and `derive_composite_pipeline_plan()` — a good pattern to extend.
- `video_pipeline.rs` has `render_video_single()` which mixes planning and execution — this is the primary target.

### 5.2 Audit: What's Already Separated vs What's Mixed

| Component | Planning | Execution | Status |
|-----------|----------|-----------|--------|
| ffmpeg settings | `build_ffmpeg_settings()` in `ffmpeg_settings.rs` | Pipeline applies settings | Already separated (Step 1) |
| Composite pipeline | `derive_composite_pipeline_plan()` | `render_composite_video_single()` | Already separated |
| Composite scene timing | `derive_composite_render_plan()` in `commands/mod.rs` | — | **In wrong module** (Phase 3 should have moved this) |
| Transparent pipeline | Mixed in `render_video_single()` | Mixed in `render_video_single()` | **Needs separation** |
| Preview rendering | Mixed in `commands::backend_render()` | Mixed in `commands::backend_render()` | Manageable; defer |

### 5.3 Priority Actions

Given that Phase 4 already has significant work (Steps 1-3), focus the planning/execution separation on the most impactful targets:

#### 5.3.1 Move `derive_composite_render_plan` and `apply_composite_scene_timing` to `encode`

These functions currently live in `commands/mod.rs` but are composite planning logic. Per the master plan §3.11, they belong in `encode`.

**Prerequisite:** This move requires Phase 3 to have completed `AppPaths` → `paths.rs` (to avoid circular deps). Verify:

```bash
rg "use crate::commands" src-tauri/ovrley_core/src/encode/
# Should return empty
```

**Steps:**

1. Read `derive_composite_render_plan` and `apply_composite_scene_timing` from `commands/mod.rs`.
2. Move them to `encode/video_composite_pipeline.rs` (since they are composite-specific planning).
3. Add `pub(crate)` visibility.
4. Update `commands/mod.rs` to import from the new location.
5. Verify composite export still works.

#### 5.3.2 Consider `TransparentRenderPlan` for `render_video_single`

`render_video_single` (531 lines, `video_pipeline.rs:49–247`) currently:

1. Derives frame count from activity + config
2. Sets up ffmpeg process
3. Spawns writer thread
4. Renders frames in loop
5. Monitors completion

A cleaner separation would extract steps 1-2 into a planning phase:

```rust
/// Plan for a single transparent render, derived before spawning ffmpeg.
pub(crate) struct TransparentRenderPlan {
    pub total_frames: u32,
    pub ffmpeg_settings: FfmpegSettings,
    pub ffmpeg_args: Vec<String>,
    pub output_path: PathBuf,
    pub fps: Fps,
}

/// Derive a transparent render plan from config and activity.
/// Pure function — no IO, no ffmpeg process.
pub(crate) fn derive_transparent_render_plan(
    paths: &AppPaths,
    config: &RenderConfig,
    activity: &ParsedActivity,
    dense_activity: &DenseActivityReport,
) -> CoreResult<TransparentRenderPlan> { ... }

/// Execute a transparent render plan.
/// Owns ffmpeg process, threads, and IO.
pub(crate) fn execute_transparent_render(
    plan: TransparentRenderPlan,
    controller: &RenderController,
) -> CoreResult<String> { ... }
```

**However**, this separation is non-trivial. The current `render_video_single` has tightly coupled setup and execution. **Defer this to a sub-step of Phase 4 only if time allows.** The `FfmpegSettings` extraction (Step 1) already makes settings testable. A full plan/execute split for transparent rendering can be its own follow-up.

#### 5.3.3 Add Snapshot Tests for Composite Planning

Since composite planning (`derive_composite_pipeline_plan`) is already separated, add snapshot tests:

```rust
// tests/composite_plan_tests.rs
use ovrley_core::encode::video_composite_pipeline::derive_composite_pipeline_plan;
use crate::common::test_config;

#[test]
fn composite_plan_from_simple_config() {
    let plan = derive_composite_pipeline_plan(
        /* parameters from fixture config */
    ).unwrap();
    assert!(!plan.windows.is_empty());
    // Snapshot with insta:
    // insta::assert_yaml_snapshot!(plan);
}
```

### 5.4 Verification After Step 4

```bash
cargo test -p ovrley_core
cargo clippy -p ovrley_core -- -D warnings
```

Manual checks:
- [x] Composite export works (if `derive_composite_render_plan` was moved)
- [x] Composite scene timing is applied correctly

### 5.5 What to Keep in Mind

- **Do not force a plan/execute split where it doesn't improve testability.** The composite pipeline already demonstrates the pattern. For the transparent pipeline, the `FfmpegSettings` extraction already enables testing the critical settings derivation. Further splitting of `render_video_single` is a "nice to have" but not critical.
- **No new traits or abstractions.** The plan structs are plain data. The `derive_*` functions are pure. The `execute_*` functions own the side effects. This is the simplest possible separation.
- **Watch for command-layer logic.** If `commands/mod.rs` still contains planning logic after Phase 3, Phase 4 must address it. The master plan §3.11 specifically calls out `derive_composite_render_plan` as wrongly placed.
- **Planning functions must be `pub(crate)`, not `pub`.** External consumers (Tauri commands) should go through the orchestration layer (`commands`), not directly call `derive_*` functions.

---

## 6. Step 5 — Enforce Sibling Pipeline Isolation

**Purpose:** Ensure `video_pipeline.rs` and `video_composite_pipeline.rs` remain independent. Extract shared logic if it exists. Document the boundary.

### 6.1 Current State

From the research: **No cross-imports exist between the two pipelines.** They share imports from common modules:

| Shared Import | Module |
|--------------|--------|
| `resolve_ffmpeg_binary`, `suppress_child_console` | `encode/ffmpeg` |
| `ProgressEstimator` | `encode/progress` |
| `RenderController` | `encode/progress` (after Step 2) |
| `FfmpegSettings`, `build_ffmpeg_settings` | `encode/ffmpeg_settings` (after Step 1) |

This is the desired state. Both pipelines use shared dependencies from neutral modules without depending on each other.

### 6.2 Verification Steps

#### 6.2.1 Verify No Cross-Imports

```bash
rg "use crate::encode::video_pipeline" src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
rg "use crate::encode::video_composite_pipeline" src-tauri/ovrley_core/src/encode/video_pipeline.rs
```

Both should return no matches.

#### 6.2.2 Verify `video.rs` Is the Only Orchestrator That Imports Both

```bash
rg "use crate::encode::(video_pipeline|video_composite_pipeline)" --type rust src-tauri/ovrley_core/src/
```

Only `encode/video.rs` and `commands/mod.rs` (via `encode`) should import pipeline modules. If any other module imports both, investigate.

#### 6.2.3 Check for Shared Logic That Should Be Extracted

Search for duplicated patterns between the two pipelines:

```bash
# Example: do both pipelines implement their own ffmpeg arg construction?
rg "Command::new|std::process::Command" src-tauri/ovrley_core/src/encode/video_pipeline.rs
rg "Command::new|std::process::Command" src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

Both spawn `std::process::Command` for ffmpeg. This is acceptable — the command arguments differ per pipeline. Extraction is only warranted if the process spawning logic itself is identical (same pipe setup, same stderr handling, same cleanup).

#### 6.2.4 Document the Boundary

Add module documentation if not already present:

In `encode/video_pipeline.rs`:
```rust
//! Single-pass transparent overlay render pipeline.
//!
//! Renders Skia frames and streams them to ffmpeg via stdin.
//! Produces alpha-preserving overlay video (ProRes, QTRLE, or Vulkan).
//!
//! Must not import from [`video_composite_pipeline`].
```

In `encode/video_composite_pipeline.rs`:
```rust
//! Multi-pass composite MP4 render pipeline.
//!
//! Renders Skia frames, composites with source video segments,
//! and produces final H.264/H.265 MP4 output.
//!
//! Must not import from [`video_pipeline`].
```

### 6.3 Future-Proofing

To prevent accidental cross-imports, consider adding a lint check (optional, not required for Phase 4):

```rust
// Not needed now, but as a convention:
// Each pipeline's mod.rs could include:
#[cfg(test)]
mod isolation_tests {
    // Compile-time check that we don't accidentally import sibling
    // (This is a convention, not enforceable by the compiler)
}
```

This is not implemented in Phase 4 — just noted for awareness.

### 6.4 Verification After Step 5

```bash
cargo test -p ovrley_core
cargo clippy -p ovrley_core -- -D warnings
```

No new manual checks needed beyond the existing pipeline tests.

### 6.5 What to Keep in Mind

- **Shared logic does NOT mean shared pipeline code.** If both pipelines call `resolve_ffmpeg_binary()`, that's fine — it's a shared utility. Cross-import means `video_pipeline.rs` importing types/functions from `video_composite_pipeline.rs` (or vice versa), which would indicate tangled orchestration.
- **Don't prematurely extract a "shared pipeline base."** The two pipelines have fundamentally different architectures (single-pass transparent overlay vs multi-pass composite). A shared base would be an abstraction over two things — violating rule 2.6 (no premature abstractions).
- **If shared logic does emerge** (e.g., identical ffmpeg stdin writing, identical thread join logic with timeouts), extract into `encode/pipeline_shared.rs`, not into either pipeline module.

---

## 7. Step 6 — Cleanup and Verification

### 7.1 Remove All `#[allow(clippy::too_many_arguments)]`

After Step 3, verify none remain on the refactored functions:

```bash
rg "too_many_arguments" --type rust src-tauri/ovrley_core/src/render/
rg "too_many_arguments" --type rust src-tauri/ovrley_core/src/encode/
```

Any remaining suppressions should be on private functions that were explicitly deferred (see §4.6).

### 7.2 Verify Import Hygiene

```bash
# encode must not depend on commands (Phase 3 deliverable)
rg "use crate::commands" src-tauri/ovrley_core/src/encode/

# render must not depend on encode
rg "use crate::encode" src-tauri/ovrley_core/src/render/

# config must not depend on activity/render/encode
rg "use crate::(activity|render|encode)" src-tauri/ovrley_core/src/config/

# video_pipeline must not import video_composite_pipeline
rg "video_composite_pipeline" src-tauri/ovrley_core/src/encode/video_pipeline.rs

# video_composite_pipeline must not import video_pipeline
rg "video_pipeline" src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs
```

### 7.3 Run Full Test Suite

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
```

### 7.4 Manual Smoke Tests

| Test | Expected Result |
|------|----------------|
| App starts | Window opens, UI loads |
| Import video | Video preview/scrub works |
| Preview render | Overlay renders on top of video |
| Transparent export (ProRes) | `.mov` file produced, alpha preserved |
| Transparent export (QTRLE) | `.mov` file produced |
| Composite export (H.264) | `.mp4` file produced, overlay + video composited |
| Cancel render during export | Render stops, no orphan ffmpeg, state resets |
| Progress UI | Shows progress %, ETA, status changes |
| Diagnostic binaries | `cargo run --bin render_preview` etc. work |

### 7.5 Performance Check

Capture approximate render times for a representative activity:

```
Activity: [standard test fixture]
Frames:   [N]
Before Phase 4: [X] seconds
After Phase 4:  [Y] seconds
Delta: [Y - X] seconds
```

If the delta exceeds 5%, investigate. Request structs add one heap allocation per call (the struct itself), but this should be negligible compared to frame rendering time (ms per frame). The struct should be stack-allocated or trivially passed by the compiler.

---

## 8. Completion Criteria

### 8.1 Automated Checks

- [x] `cargo fmt` passes
- [x] `cargo test` passes (all crates)
- [ ] `cargo clippy -- -D warnings` passes — blocked by 9 pre-existing errors outside Phase 4 scope (ffmpeg_composite.rs, video.rs, video_composite_pipeline.rs, rdp.rs)
- [x] No `#[allow(clippy::too_many_arguments)]` on refactored functions
- [x] `encode/ffmpeg_settings.rs` exists with `FfmpegSettings` and `build_ffmpeg_settings`
- [x] `encode/progress.rs` contains `RenderController` (or re-exported from `video.rs`)
- [x] `ProgressEstimator` tests are in `encode/tests/progress_tests.rs` (not inline)
- [x] Snapshot tests exist for ffmpeg settings (at least one per supported codec)

### 8.2 Architecture Checks

- [x] No cross-imports between `video_pipeline` and `video_composite_pipeline`
- [x] `encode` does not depend on `commands`
- [x] `render` does not depend on `encode`
- [x] All shared logic between pipelines lives in neutral modules (not sibling imports)
- [x] `commands/mod.rs` contains only orchestration, not planning logic

### 8.3 Behavioral Checks

- [x] FFmpeg commands unchanged from pre-Phase-4 (verify with snapshot tests)
- [x] Transparent overlay export works
- [x] Composite MP4 export works
- [x] Preview rendering unchanged
- [x] Cancellation works (no orphan ffmpeg processes)
- [x] Progress reporting works (state transitions, ETA)
- [x] Output paths and filenames unchanged

### 8.4 Documentation Checks

- [x] `encode/ffmpeg_settings.rs` has module-level docs (what it owns/doesn't own)
- [x] `encode/progress.rs` has module-level docs updated for `RenderController` ownership
- [x] `RenderController` has state transition documentation
- [x] `video_pipeline.rs` and `video_composite_pipeline.rs` have isolation boundary docs
- [x] All request structs have brief field documentation or at least a struct-level doc comment

---

## Summary of Files Changed in Phase 4

| File | Change |
|------|--------|
| `encode/ffmpeg_settings.rs` | **NEW** — extracted `FfmpegSettings`, `build_ffmpeg_settings`, helpers |
| `encode/ffmpeg.rs` | Remove moved items; keep discovery/process helpers |
| `encode/mod.rs` | Add `pub mod ffmpeg_settings;` |
| `encode/progress.rs` | Add `RenderController` (moved from `video.rs`), update module docs |
| `encode/video.rs` | Remove `RenderController`; add re-export from `progress` |
| `encode/video_pipeline.rs` | Update imports for settings; possibly add module boundary docs |
| `encode/video_composite_pipeline.rs` | Update imports; possibly add module boundary docs; possibly receive `derive_composite_render_plan` from `commands` |
| `render/mod.rs` | Add `PreviewRenderRequest`, `FrameRenderRequest`; update function signatures |
| `render/widgets/value.rs` | Add `MetricWidgetRequest`; update `draw_metric_value_widget_with_config` signature |
| `commands/mod.rs` | Possibly lose `derive_composite_render_plan`/`apply_composite_scene_timing` to `encode` |
| `tests/ffmpeg_settings_tests.rs` | **NEW** — snapshot tests for ffmpeg settings |
| `encode/tests/progress_tests.rs` | Possibly **NEW** — if `ProgressEstimator` tests need moving |
| Various call sites | Updated struct construction for request structs |
