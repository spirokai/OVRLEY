# OVRLEY Backend Refactor — Phase 1: Test Safety Net and Test Migration

## Detailed Implementation Plan

> **Derived from:** `backend-refactor-v2.md` (Master Plan)  
> **Phase scope:** Test safety net and test migration. No behavior changes. No new features. No error-type migration. No module splitting.  
> **Goal:** Create a regression-safe testing foundation before any architectural changes touch production code.

---

## Table of Contents

1. [Scope and Constraints](#1-scope-and-constraints)
2. [What Phase 1 Will and Will Not Do](#2-what-phase-1-will-and-will-not-do)
3. [Current State Assessment](#3-current-state-assessment)
4. [Step-by-Step Execution Plan](#4-step-by-step-execution-plan)
5. [Step 1: Audit All Existing Tests](#step-1-audit-all-existing-tests)
6. [Step 2: Create Shared Test Infrastructure](#step-2-create-shared-test-infrastructure)
7. [Step 3: Migrate Module-Local Tests (#[path] style)](#step-3-migrate-module-local-tests-path-style)
8. [Step 4: Migrate Inline #[cfg(test)] Blocks](#step-4-migrate-inline-cfgtest-blocks)
9. [Step 5: Add Missing Snapshot / Regression Tests](#step-5-add-missing-snapshot--regression-tests)
10. [Step 6: Add Cancellation Lifecycle Tests](#step-6-add-cancellation-lifecycle-tests)
11. [Step 7: Final Verification and Cleanup](#step-7-final-verification-and-cleanup)
12. [Manual Validation Checklist](#manual-validation-checklist)
13. [Completion Criteria](#completion-criteria)
14. [Appendix A: File Inventory](#appendix-a-file-inventory)
15. [Appendix B: Common Pitfalls and How to Avoid Them](#appendix-b-common-pitfalls-and-how-to-avoid-them)
16. [Appendix C: Quick Reference — Test Migration Decision Tree](#appendix-c-quick-reference--test-migration-decision-tree)

---

## 1. Scope and Constraints

### What Phase 1 Must Preserve

- **All observable behavior** (Rule 2.1 from master plan)
- **All existing test assertions** — moving a test must not change what it asserts
- **Compilation** — every individual step must leave the crate compiling
- **Frontend JSON compatibility** — no schema changes
- **FFmpeg command construction** — no command changes
- **Performance** — no hot-path changes

### Constraints Inherited from Master Plan

| Rule | Application in Phase 1 |
|------|------------------------|
| 2.1 Behavior Preservation | Tests are moved, not rewritten. Assertions stay identical. |
| 2.2 Tests Come Before Refactors | Phase 1 *is* the safety net. We are building it. |
| 2.3 No Blind Refactoring | Every file to be touched is read in full before editing. |
| 2.4 Small, Safe, Independent Changes | One file's tests per commit/step. No bundling unrelated changes. |
| 2.5 All Tests in Dedicated `tests/` Directories | The core deliverable of this phase. |
| 2.9 Shared Test Config | All tests must use `tests/common/test_config.rs` for fixture paths. |

---

## 2. What Phase 1 Will and Will Not Do

### ✅ In Scope

1. Moving every existing test out of production source files
2. Creating shared test infrastructure (`tests/common/`, `tests/fixtures/`)
3. Adding fixture-based snapshot/golden tests for stable, deterministic outputs
4. Adding cancellation lifecycle tests where feasible without spawning real ffmpeg
5. Removing `#[cfg(test)]` and `#[path = "tests/..."]` declarations from production files
6. Ensuring `cargo test` discovers all tests without special flags

### ❌ Out of Scope (Forbidden in Phase 1)

- **Typed errors** — `Result<T, String>` stays as-is. Do not create `error.rs` yet.
- **`MetricKind` enum** — metric strings stay as strings. Do not create `types.rs` yet.
- **Module splitting** — do not split `common.rs`, `video.rs`, or any other module.
- **Cache/state changes** — do not touch `OnceLock<Mutex<HashMap>>` caches.
- **Function signature changes** — do not introduce request structs or reduce argument counts.
- **New abstractions** — no traits, no builders, no new crates.
- **Behavior changes** — no bug fixes disguised as refactors.
- **Code formatting-only changes** — do not run `cargo fmt` unless required for compilation.
- **Deletion of commented `println!`** — note them, but do not remove in Phase 1.

### Why These Restrictions Matter

Phase 1 is the safety net for everything that follows. If Phase 1 touches production logic, it loses its purpose as a neutral baseline. The only production-code changes allowed are:
- Removing `#[cfg(test)]` blocks
- Removing `#[path = "tests/..."]` declarations
- Adding `pub` or `pub(crate)` visibility where a test previously relied on `use super::*` access to private items

---

## 3. Current State Assessment

### 3.1 Test Locations (Inventory)

There are **13 production files** containing test code, across two patterns:

**Pattern A — `#[path = "tests/..."]` (module-local `tests/` directory):**

| Production File | Declaration Line | Test File | Current State |
|-----------------|------------------|-----------|---------------|
| `config/mod.rs` | 614-615 | `config/tests/config_tests.rs` | `#[path]` include + inline `#[cfg(test)]` wrapper |
| `commands/mod.rs` | 648-649 | `commands/tests/commands_tests.rs` | `#[path]` include + inline `#[cfg(test)]` wrapper |
| `encode/fps.rs` | 103-104 | `encode/tests/fps_tests.rs` | `#[path]` include + inline `#[cfg(test)]` wrapper |
| `encode/ffmpeg_composite.rs` | 447-448 | `encode/tests/ffmpeg_composite_tests.rs` | `#[path]` include + inline `#[cfg(test)]` wrapper |
| `encode/video_composite_pipeline.rs` | 805-806 | `encode/tests/video_composite_pipeline_tests.rs` | `#[path]` include + inline `#[cfg(test)]` wrapper |

**Pattern B — Pure inline `#[cfg(test)] mod tests` (no separate file):**

| Production File | Start Line | Content Type |
|-------------------|------------|--------------|
| `activity/mod.rs` | 57 | Unit tests for parsing, trimming, interpolation |
| `render/format.rs` | 500 | Unit tests for metric formatting |
| `render/widgets/value.rs` | 865 | Unit tests for widget value/icon matching |
| `video_server.rs` | 461 | Unit tests for HTTP range parsing |
| `encode/video.rs` | 899 | Unit tests for frame counting, timing |
| `encode/video_probe.rs` | 253 | Unit tests for probe metadata |
| `encode/codec_detect.rs` | 728 | Unit tests for codec detection |
| `encode/progress.rs` | 106 | Unit tests for `ProgressEstimator` |

**Total:** 13 production files contain test code — 5 via `#[path]` includes and 8 via pure inline blocks. Each must be extracted into a dedicated `tests/` directory.

### 3.2 Current Fixture Resolution (Brittle)

Multiple test files define their own `repo_root()` with fragile `parent().unwrap()` chains:

- `activity/mod.rs:65-73`
- `commands/tests/commands_tests.rs:266-273`
- `encode/tests/video_composite_pipeline_tests.rs:595-602`

These will be consolidated into a single `tests/common/test_config.rs`.

### 3.3 No Crate-Level Integration Tests Exist

The `ovrley_core/tests/` directory does not exist yet. The required end state
for this phase is crate-root integration tests under `ovrley_core/tests/`
and `src-tauri/tests/`. Do **not** assume that leaving files under
`src/*/tests/` will make Cargo discover them after removing the current
`#[path]` wiring.

---

## 4. Step-by-Step Execution Plan

### Ordering Principle

The order is designed to minimize risk:

1. **Infrastructure first** — create shared helpers before any test moves
2. **Leaf modules first** — `config`, `activity`, `debug` have few internal dependencies
3. **Complex modules last** — `encode`, `commands`, `render` have more interdependencies and may need visibility changes
4. **One file per step** — compile after each file to catch breakage early

### High-Level Step Order

```
Step 1: Audit all existing tests (read every test-containing file in full)
Step 2: Create shared test infrastructure (common/, fixtures/)
Step 3: Migrate module-local #[path] tests (5 files)
Step 4: Migrate inline #[cfg(test)] blocks (8 files + additional blocks)
Step 5: Add missing snapshot/regression tests
Step 6: Add cancellation lifecycle tests
Step 7: Final verification and cleanup
```

---

## Step 1: Audit All Existing Tests

### Purpose

Before moving anything, understand what each test does, what it imports from its `super` module, and whether it relies on private internals.

### Action

For each of the 13 production files listed in Section 3.1:

1. Read the **full file** from start to end
2. Identify the `#[cfg(test)]` block boundaries
3. List every `use super::*` or `use super::specific_item`
4. Note any access to `pub(crate)` or private items
5. Determine what minimal `pub(crate)` seam, if any, is required so the test
   can be migrated to a crate-level integration test.

### Audit Checklist Template

For each file, produce:

```markdown
### File: `path/to/file.rs`
- Test block lines: N-M
- Uses `use super::*`: yes/no
- Accesses private items: [list]
- Can become crate-level test: yes/no (reason)
- Recommended destination: `ovrley_core/tests/__tests.rs` or `src/module/tests/`
- Visibility changes needed: [list any `pub` additions required]
```

### Files to Audit (in this order)

1. `ovrley_core/src/config/mod.rs`
2. `ovrley_core/src/activity/mod.rs`
3. `ovrley_core/src/debug/mod.rs` (no tests currently — verify)
4. `ovrley_core/src/render/format.rs`
5. `ovrley_core/src/render/widgets/value.rs`
6. `ovrley_core/src/encode/fps.rs`
7. `ovrley_core/src/encode/progress.rs`
8. `ovrley_core/src/encode/video_probe.rs`
9. `ovrley_core/src/encode/codec_detect.rs`
10. `ovrley_core/src/encode/ffmpeg_composite.rs`
11. `ovrley_core/src/encode/video.rs`
12. `ovrley_core/src/encode/video_composite_pipeline.rs`
13. `ovrley_core/src/commands/mod.rs`
14. `src-tauri/src/video_server.rs`

### Keep in Mind

- Do not skip reading the full file. A test at line 500 may reference a private helper defined at line 50.
- The `commands` tests likely instantiate `AppPaths`, `RenderController`, and call `backend_render` — these may need `pub` visibility adjustments.
- The `render/widgets/value.rs` tests likely test `metric_icon_kind_for_value` or similar private helpers.
- The `video_server.rs` tests test HTTP range header parsing — these are self-contained and ideal for crate-level extraction.

### Expected Duration

This is a research step. Do not edit files yet. Produce the audit notes as a comment block in this document or as a separate scratch file.

---

## Step 2: Create Shared Test Infrastructure

### Purpose

Provide a single source of truth for fixture paths so that tests never define their own `repo_root()` or `parent().unwrap()` chains.

### 2.1 Create Directory Structure

```
ovrley_core/tests/
├── common/
│   ├── mod.rs
│   └── test_config.rs
└── fixtures/
    ├── config/
    ├── activity/
    ├── ffprobe/
    ├── video/
    └── expected/
```

### 2.2 Create `tests/common/mod.rs`

```rust
pub mod test_config;
```

### 2.3 Create `tests/common/test_config.rs`

Implement exactly as specified in master plan Rule 2.9:

```rust
use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    fixtures().join(relative)
}

pub fn parsed_activity_path() -> PathBuf {
    fixtures().join("activity").join("gpx-parse-debug.json")
}

pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("sample.mp4")
}
```

### 2.4 Populate Fixtures

For each fixture referenced by existing tests:

1. Locate the file currently used (often via `parent().unwrap()` traversal to repo root)
2. Copy or move it into `tests/fixtures/<category>/`
3. Update `test_config.rs` if new categories are needed

**Current known fixtures:**

- Activity JSON (GPX-derived debug output)
- Config JSON (simple transparent config)
- Config JSON (composite config with video fields)
- MP4 sample video (for probe/composite tests)
- ffprobe JSON dumps (for video_probe tests)

### 2.5 Verify Infrastructure

Before any test moves, ensure the infrastructure compiles:

```bash
cd src-tauri/ovrley_core
cargo test --test common  # or similar smoke test
cargo test --no-run       # compile all tests without running
```

### Keep in Mind

- `CARGO_MANIFEST_DIR` for `ovrley_core` is `src-tauri/ovrley_core/`. Fixtures live inside that directory.
- Do not use lazy statics or `OnceCell` in `test_config.rs` — keep it trivial.
- If a test currently references a fixture outside the crate (e.g., at repo root), copy it into `tests/fixtures/` rather than traversing upward. This keeps the test self-contained.
- No production code may reference `tests/common/`. It is test-only infrastructure.

---

## Step 3: Migrate Module-Local Tests (`#[path]` style)

### Purpose

These 5 files already have their tests in a `tests/` subdirectory, but the production file still knows about them via `#[path]` and `#[cfg(test)]`. We need to:

1. Remove the `#[cfg(test)]` wrapper and `#[path]` declaration from the production file
2. Ensure the test file can compile as a standalone module-level or crate-level test
3. Update imports (`use super::*` → `use ovrley_core::*` or `pub(crate)` visibility)

### Step 3.1: `config/mod.rs` + `config/tests/config_tests.rs`

**Production change:**

Remove from `config/mod.rs`:
```rust
#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
```

**Test file change:**

Current test file starts with `use super::parse_config_json;`. Since `parse_config_json` is already `pub`, this can become a crate-level integration test.

Move `config/tests/config_tests.rs` → `ovrley_core/tests/config_tests.rs` and change imports:

```rust
use ovrley_core::config::parse_config_json;
```

**Recommendation:** Move to `ovrley_core/tests/config_tests.rs` (crate-level) since `parse_config_json` is public.

**After change, run:**
```bash
cargo test -p ovrley_core config
```

### Step 3.2: `encode/fps.rs` + `encode/tests/fps_tests.rs`

**Production change:**

Remove `#[cfg(test)]` and `#[path]` from `fps.rs`.

**Test file change:**

The `fps` module exports a `Rational` FPS type and helpers. If these are `pub`, move test to `ovrley_core/tests/fps_tests.rs` with `use ovrley_core::encode::fps::*`.

If tests access private internals (e.g., `gcd` helper), either:
- Make the helper `pub(crate)` (minimal visibility change)
- Keep the final destination as `ovrley_core/tests/fps_tests.rs`

**After change, run:**
```bash
cargo test -p ovrley_core fps
```

### Step 3.3: `encode/ffmpeg_composite.rs` + `encode/tests/ffmpeg_composite_tests.rs`

**Production change:**

Remove `#[cfg(test)]` and `#[path]` from `ffmpeg_composite.rs`.

**Test file change:**

`ffmpeg_composite.rs` builds ffmpeg argument arrays for composite rendering. The tests likely snapshot/assert on command arrays. If functions are `pub`, extract to crate-level test.

**After change, run:**
```bash
cargo test -p ovrley_core ffmpeg_composite
```

### Step 3.4: `encode/video_composite_pipeline.rs` + `encode/tests/video_composite_pipeline_tests.rs`

**Production change:**

Remove `#[cfg(test)]` and `#[path]` from `video_composite_pipeline.rs`.

**Caution:** The `#[cfg(test)]` declaration at line 805 is the wrapper for the `#[path]` include at line 806. Verify the exact boundaries by reading the full file.

**Test file change:**

These tests likely need `AppPaths` and fixture video files. They will need `tests/common/test_config.rs`.

**After change, run:**
```bash
cargo test -p ovrley_core video_composite_pipeline
```

### Step 3.5: `commands/mod.rs` + `commands/tests/commands_tests.rs`

**Production change:**

Remove `#[cfg(test)]` and `#[path]` from `commands/mod.rs`.

**Test file change:**

The commands tests are the most complex. They likely:
- Instantiate `AppPaths` (currently private or module-local)
- Instantiate `RenderController`
- Call `backend_render` or `derive_composite_render_plan`
- Use synthetic activity and config fixtures

If `AppPaths`, `RenderController`, and command functions are `pub`, these can become crate-level tests. If not:

**Option A (preferred):** Add minimal `pub` or `pub(crate)` visibility to types/functions that tests need, then move to `ovrley_core/tests/commands_tests.rs`.

**Option B:** If private access blocks migration, add the narrowest possible
`pub(crate)` seam and keep the final destination as
`ovrley_core/tests/commands_tests.rs`.

**After change, run:**
```bash
cargo test -p ovrley_core commands
```

### Keep in Mind for All `#[path]` Migrations

- The `#[path]` attribute is the current reason these files compile. After
  removing that wiring, Cargo will **not** automatically pick up arbitrary
  `src/*/tests/*.rs` files as standalone tests.
- The safe end state is: move every migrated test to `ovrley_core/tests/` or
  `src-tauri/tests/`, and use narrow `pub(crate)` seams where private access is
  otherwise blocking that move.
- When moving a test, preserve **every assertion** exactly. Do not "improve" tests during migration.
- If a test uses `use super::*`, list exactly what it imports from `super` and verify those items are available at the new location.

---

## Step 4: Migrate Inline `#[cfg(test)]` Blocks

### Purpose

These 8 files have tests embedded directly in the production source. The tests must be extracted into separate files in `tests/` directories, and the `#[cfg(test)]` blocks removed from production.

### Step 4.1: `activity/mod.rs` (inline tests at line 57)

**Production change:**

Remove the entire `#[cfg(test)] mod tests { ... }` block from `activity/mod.rs`.

**Test extraction:**

Create `ovrley_core/tests/activity_tests.rs`.

The `activity` module exports `parse_activity_json`, `trim_activity`, `densify_activity`. If tests only use public functions, use crate-level.

If tests access private helpers inside `activity/mod.rs`, either:
- Make those helpers `pub(crate)`
- Keep the final destination as `ovrley_core/tests/activity_tests.rs`

**Fixture update:**

Current tests likely use `repo_root()` with `parent().unwrap()` chains. Replace with:

```rust
mod common;
use common::test_config;
```

**Resolution per master plan Rule 2.9:**

> "If tests need access to internals, prefer exposing a narrow `pub(crate)` testing seam or moving the test to a crate-level integration test if public behavior is sufficient."

**Action for `activity/mod.rs`:**

1. Read the test block to determine if it tests public API
2. If yes → move to `ovrley_core/tests/activity_tests.rs`
3. If no (tests private helpers) → add the smallest `pub(crate)` seam needed,
   then still move to `ovrley_core/tests/activity_tests.rs`

**After change, run:**
```bash
cargo test -p ovrley_core activity
```

### Step 4.2: `render/format.rs` (inline tests at line 500)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `render/format.rs`.

**Test extraction:**

`format.rs` exports `format_value`, `raw_value`, `format_metric_parts`. These are public. Move tests to `ovrley_core/tests/render_format_tests.rs` or `ovrley_core/tests/format_tests.rs`.

**After change, run:**
```bash
cargo test -p ovrley_core format
```

### Step 4.3: `render/widgets/value.rs` (inline tests at line 865)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `value.rs`.

**Test extraction:**

Tests likely cover `metric_icon_kind_for_value` and `draw_metric_value_widget_with_config`. If these are `pub`, extract to crate-level. If tests cover private drawing helpers, consider making the helpers `pub(crate)`.

**After change, run:**
```bash
cargo test -p ovrley_core value
```

### Step 4.4: `video_server.rs` (inline tests at line 461)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `src-tauri/src/video_server.rs`.

**Test extraction:**

This is in the **Tauri crate**, not `ovrley_core`. Create `src-tauri/tests/video_server_tests.rs`.

The Tauri crate's tests directory is `src-tauri/tests/`. The test should import from `ovrley_tauri::video_server::*` or the appropriate module path.

**After change, run:**
```bash
cargo test -p ovrley_tauri video_server
# or whatever the Tauri package name is
```

### Step 4.5: `encode/video.rs` (inline tests at line 899)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `encode/video.rs`.

**Test extraction:**

`video.rs` is the main encode orchestrator. Tests may cover:
- Frame count calculations
- Timing derivations
- Render plan logic

Some of these may test `pub` functions; others may test private planning helpers. If planning helpers are tested, those tests will be valuable when that logic moves in Phase 4. Preserve them exactly.

**After change, run:**
```bash
cargo test -p ovrley_core video
```

### Step 4.6: `encode/video_probe.rs` (inline tests at line 253)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `video_probe.rs`.

**Test extraction:**

`probe_video` and related functions are likely `pub`. Use stored ffprobe JSON fixtures in `tests/fixtures/ffprobe/`.

Replace any `repo_root()` chains with `test_config::fixture_path("ffprobe/1080p.json")`.

**After change, run:**
```bash
cargo test -p ovrley_core video_probe
```

### Step 4.7: `encode/codec_detect.rs` (inline tests at line 728)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `codec_detect.rs`.

**Test extraction:**

Codec detection functions may be `pub`. Move to crate-level tests.

**After change, run:**
```bash
cargo test -p ovrley_core codec_detect
```

### Step 4.8: `encode/progress.rs` (inline tests at line 106)

**Production change:**

Remove `#[cfg(test)] mod tests { ... }` from `progress.rs`.

**Test extraction:**

`ProgressEstimator` is a public type with methods `record()`, `estimate()`, etc. Move tests to `ovrley_core/tests/progress_tests.rs`.

**After change, run:**
```bash
cargo test -p ovrley_core progress
```

### Keep in Mind for All Inline Migrations

- **Preserve line-by-line assertion logic.** Copy-paste the test body exactly; only change imports and fixture paths.
- **Do not refactor test logic.** If a test has a helper function (`synthetic_activity()`, `transparent_config()`), move the helper with the test.
- **Visibility changes are allowed but must be minimal.** If a test needs a private function, prefer making it `pub(crate)` over keeping the test inline.
- **Watch for `use super::*` in nested modules.** If an inline test module is inside `render/widgets/value.rs`, `super` refers to `value.rs` module contents, not the crate root.

---

## Step 5: Add Missing Snapshot / Regression Tests

### Purpose

The master plan lists several areas that should have snapshot or golden tests. Phase 1 is the right time to add them because:
- We are not changing behavior yet, so snapshots capture the current "correct" output
- Future phases (error migration, module splitting) need these as regression guards

### Areas to Cover

| Area | What to Snapshot | Fixture Needed |
|------|------------------|----------------|
| Config parsing | Normalized `RenderConfig` JSON | `simple.json`, `composite.json` |
| Activity parsing | `ParsedActivity` struct (selected fields) | `gpx-parse-debug.json` |
| Interpolation | Frame counts at 24, 25, 29.97, 30, 59.94, 60 FPS | Activity fixture |
| Frame counting | Integer vs non-integer duration windows | Activity + config fixtures |
| FFmpeg commands (transparent) | `Vec<String>` output for prores, qtrle, vulkan | Config fixtures |
| FFmpeg commands (composite) | `Vec<String>` output for libx264 composite | Composite config + video fixture |
| Composite pipeline planning | Timing derivation, overrun guard | Composite config |
| Video probe metadata | Creation time, resolution, codec | ffprobe JSON fixtures |
| RDP simplification | Input/output point counts for route/elevation | Activity fixture |

### Snapshot Strategy

Two approaches are acceptable in Phase 1:

**Approach A — Direct assertions (no external library):**

```rust
let command = build_ffmpeg_settings(&config);
assert_eq!(command.args, vec!["-y", "-f", "rawvideo", ...]);
```

**Approach B — `insta` snapshot library (if team approves):**

```rust
let plan = build_render_plan(&request)?;
insta::assert_json_snapshot!(plan);
```

**Phase 1 recommendation:** Use Approach A for simplicity. If `insta` is already a dependency or trivial to add, Approach B is acceptable. Do not spend time evaluating snapshot libraries in Phase 1.

### Test File Naming

```
ovrley_core/tests/
├── config_snapshot_tests.rs
├── activity_snapshot_tests.rs
├── interpolation_regression_tests.rs
├── ffmpeg_command_tests.rs
├── render_plan_tests.rs
├── video_probe_tests.rs
└── rdp_regression_tests.rs
```

### Keep in Mind

- These tests are **new** but must test **existing** behavior. Do not change production code to make tests pass.
- If a function is hard to unit-test (e.g., spawns ffmpeg), test the planning/command-building functions that feed into it.
- For video probe tests, use stored ffprobe JSON output as input rather than calling `ffprobe` subprocess in tests. This keeps tests deterministic and fast.
- Snapshot tests for visual output (Skia pixels) are **explicitly out of scope** (master plan Section 6.4).

---

## Step 6: Add Cancellation Lifecycle Tests

### Purpose

Cancellation is critical user-facing behavior. The master plan requires tests for:

```
Idle -> Running -> Completed
                -> Failed
                -> Cancelled
```

### What Is Feasible in Phase 1

Full integration tests that spawn ffmpeg are out of scope for unit tests (master plan 6.4). However, we can test:

1. **`RenderController` state transitions** — start, cancel, progress state, reset
2. **ProgressEstimator behavior under cancellation** — `record()` after `cancel()` should not panic
3. **Render plan cancellation flags** — ensure `AtomicBool` is checked between frames

### Test Scenarios

```rust
#[test]
fn render_controller_start_sets_running() {
    let controller = RenderController::default();
    assert!(!controller.is_running());
    controller.start();
    assert!(controller.is_running());
}

#[test]
fn render_controller_cancel_sets_cancelled() {
    let controller = RenderController::default();
    controller.start();
    controller.cancel();
    assert!(controller.is_cancelled());
}

#[test]
fn render_controller_reset_allows_restart() {
    let controller = RenderController::default();
    controller.start();
    controller.cancel();
    controller.reset();
    assert!(!controller.is_running());
    assert!(!controller.is_cancelled());
    // Should be able to start again
    controller.start();
    assert!(controller.is_running());
}
```

### Keep in Mind

- These tests may require making `RenderController` methods `pub` or `pub(crate)`.
- Do not test actual ffmpeg process spawning in unit tests. That belongs in integration tests with controlled short renders.
- If `RenderController` lives in `encode/video.rs` and is not easily testable, add a TODO comment referencing Phase 4 (when `RenderController` moves to `progress.rs`).

---

## Step 7: Final Verification and Cleanup

### 7.1 Automated Verification Commands

Run these in order. Every command must pass.

```bash
# From repo root
cd src-tauri/ovrley_core

# 1. Compilation check
cargo check

# 2. All tests pass
cargo test

# 3. No remaining inline test blocks in production
cargo test --no-run  # Just to confirm discovery

# 4. Verify no #[cfg(test)] in src/ (except legit conditional compilation)
# Use grep to confirm zero matches in production files
grep -r "#\[cfg(test)\]" src/ | grep -v "tests/"
# Should return nothing

# 5. Verify no #[path = "tests/..."] in production
grep -r '#\[path = "tests/' src/
# Should return nothing
```

### 7.2 Production File Checklist

For every file listed in Section 3.1, verify:

- [ ] `#[cfg(test)]` block removed
- [ ] `#[path = "tests/..."]` removed (if present)
- [ ] No test-only imports remain (e.g., `use std::fs` added only for tests)
- [ ] No `#[cfg(test)]` conditional compilation on non-test code
- [ ] File compiles in isolation: `cargo check` succeeds

### 7.3 Test File Checklist

For every new test file:

- [ ] Located in `ovrley_core/tests/` or `src-tauri/tests/`
- [ ] Does not use `use super::*`; use explicit crate imports in the migrated test
- [ ] Uses `tests/common/test_config.rs` for fixture paths
- [ ] No hardcoded `parent().unwrap()` chains
- [ ] No `repo_root()` defined locally
- [ ] All assertions preserved from original test
- [ ] Compiles and passes: `cargo test <filter>` succeeds

### 7.4 Diff Review

Before marking Phase 1 complete, review the full diff:

```bash
git diff --stat
git diff
```

What to look for:
- No unexpected production logic changes
- No formatting-only changes mixed in
- No new dependencies (unless `insta` was approved)
- Test files moved, not duplicated (old inline tests should be gone, not copied)

---

## Manual Validation Checklist

After all automated checks pass, perform these manual validations to confirm no behavior was accidentally altered:

- [ ] App starts (`pnpm dev` or `cargo tauri dev`)
- [ ] Preview rendering works (open a video, see overlay preview)
- [ ] Transparent overlay export works
- [ ] Composite MP4 export works
- [ ] Video import/probe still works (drag-drop or import dialog)
- [ ] Cancellation still works (start render, click cancel, no hang)
- [ ] Progress UI still updates (percent and ETA change)
- [ ] Output paths and filenames unchanged

**If any manual test fails:**
1. Stop immediately
2. Do not proceed to Phase 2
3. Investigate whether the failure is pre-existing or caused by Phase 1
4. If caused by Phase 1, revert the offending change and fix

---

## Completion Criteria

Phase 1 is complete when **all** of the following are true:

### Automated Criteria

- [ ] `cargo test` passes with all tests green
- [ ] `cargo check` passes with zero errors
- [ ] **Zero** production source files contain inline `#[cfg(test)] mod tests { ... }`
- [ ] **Zero** production source files contain `#[path = "tests/..."] mod tests;`
- [ ] All tests are discoverable via standard `cargo test` (no `--test` flags required for basic discovery)
- [ ] All tests that reference fixtures use `tests/common/test_config.rs`
- [ ] No new dependencies were added (except optionally `insta` for snapshots, if approved)
- [ ] No production behavior was intentionally changed
- [ ] No production code was reformatted unless required for compilation

### Coverage Criteria

- [ ] All existing tests from Phase 0 (pre-refactor) are preserved in new locations
- [ ] At least one snapshot/regression test exists for: config parsing, activity parsing, interpolation, ffmpeg commands, video probe
- [ ] Cancellation lifecycle tests exist for `RenderController` state transitions

### Manual Criteria

- [ ] App starts successfully
- [ ] Preview rendering works
- [ ] Transparent overlay export works
- [ ] Composite MP4 export works
- [ ] Video import/probe works
- [ ] Cancellation works
- [ ] Progress reporting works

---

## Appendix A: File Inventory

### Production Files to Modify (Remove Test Blocks)

| # | File | Lines to Remove | Test Content Destination |
|---|------|-----------------|--------------------------|
| 1 | `ovrley_core/src/config/mod.rs` | 614-616 | `ovrley_core/tests/config_tests.rs` |
| 2 | `ovrley_core/src/activity/mod.rs` | 57-190 | `ovrley_core/tests/activity_tests.rs` |
| 3 | `ovrley_core/src/render/format.rs` | 500-737 | `ovrley_core/tests/format_tests.rs` |
| 4 | `ovrley_core/src/render/widgets/value.rs` | 865-919 | `ovrley_core/tests/widget_value_tests.rs` |
| 5 | `src-tauri/src/video_server.rs` | 461-705 | `src-tauri/tests/video_server_tests.rs` |
| 6 | `ovrley_core/src/encode/fps.rs` | 103-120 | `ovrley_core/tests/fps_tests.rs` |
| 7 | `ovrley_core/src/encode/progress.rs` | 106-183 | `ovrley_core/tests/progress_tests.rs` |
| 8 | `ovrley_core/src/encode/video_probe.rs` | 253-281 | `ovrley_core/tests/video_probe_tests.rs` |
| 9 | `ovrley_core/src/encode/codec_detect.rs` | 728-750 | `ovrley_core/tests/codec_detect_tests.rs` |
| 10 | `ovrley_core/src/encode/ffmpeg_composite.rs` | 447-470 | `ovrley_core/tests/ffmpeg_composite_tests.rs` |
| 11 | `ovrley_core/src/encode/video.rs` | 899-950 | `ovrley_core/tests/video_tests.rs` |
| 12 | `ovrley_core/src/encode/video_composite_pipeline.rs` | 805-807 | `ovrley_core/tests/video_composite_pipeline_tests.rs` |
| 13 | `ovrley_core/src/commands/mod.rs` | 648-650 | `ovrley_core/tests/commands_tests.rs` |

*Line ranges are approximate based on master plan. Read each file to confirm exact boundaries before editing.*

### New Files to Create

| # | File | Purpose |
|---|------|---------|
| 1 | `ovrley_core/tests/common/mod.rs` | Re-exports `test_config` |
| 2 | `ovrley_core/tests/common/test_config.rs` | Shared fixture path resolver |
| 3 | `ovrley_core/tests/config_tests.rs` | Config parsing tests (moved) |
| 4 | `ovrley_core/tests/activity_tests.rs` | Activity parsing tests (moved) |
| 5 | `ovrley_core/tests/format_tests.rs` | Metric formatting tests (moved) |
| 6 | `ovrley_core/tests/widget_value_tests.rs` | Widget value tests (moved) |
| 7 | `src-tauri/tests/video_server_tests.rs` | HTTP server tests (moved) |
| 8 | `ovrley_core/tests/fps_tests.rs` | FPS rational tests (moved) |
| 9 | `ovrley_core/tests/progress_tests.rs` | Progress estimator tests (moved) |
| 10 | `ovrley_core/tests/video_probe_tests.rs` | Video probe tests (moved) |
| 11 | `ovrley_core/tests/codec_detect_tests.rs` | Codec detection tests (moved) |
| 12 | `ovrley_core/tests/ffmpeg_composite_tests.rs` | Composite ffmpeg tests (moved) |
| 13 | `ovrley_core/tests/video_tests.rs` | Encode video tests (moved) |
| 14 | `ovrley_core/tests/video_composite_pipeline_tests.rs` | Composite pipeline tests (moved) |
| 15 | `ovrley_core/tests/commands_tests.rs` | Command orchestration tests (moved) |
| 16 | `ovrley_core/tests/config_snapshot_tests.rs` | NEW: config snapshot tests |
| 17 | `ovrley_core/tests/activity_snapshot_tests.rs` | NEW: activity snapshot tests |
| 18 | `ovrley_core/tests/interpolation_regression_tests.rs` | NEW: interpolation regression |
| 19 | `ovrley_core/tests/ffmpeg_command_tests.rs` | NEW: ffmpeg command snapshots |
| 20 | `ovrley_core/tests/render_plan_tests.rs` | NEW: render plan tests |
| 21 | `ovrley_core/tests/rdp_regression_tests.rs` | NEW: RDP simplification tests |
| 22 | `ovrley_core/tests/cancellation_tests.rs` | NEW: cancellation lifecycle tests |

### Fixture Files to Create / Populate

| # | File | Source (if existing) |
|---|------|----------------------|
| 1 | `ovrley_core/tests/fixtures/config/simple.json` | Existing test fixture or create minimal valid config |
| 2 | `ovrley_core/tests/fixtures/config/composite.json` | Existing test fixture or create minimal composite config |
| 3 | `ovrley_core/tests/fixtures/config/invalid.json` | Create intentionally malformed config |
| 4 | `ovrley_core/tests/fixtures/activity/gpx-parse-debug.json` | Existing fixture from repo root or test data |
| 5 | `ovrley_core/tests/fixtures/activity/fit-parse-debug.json` | If available |
| 6 | `ovrley_core/tests/fixtures/ffprobe/1080p.json` | Store ffprobe output for sample video |
| 7 | `ovrley_core/tests/fixtures/ffprobe/4k.json` | If needed |
| 8 | `ovrley_core/tests/fixtures/video/sample.mp4` | Short representative MP4 for probe/composite tests |

---

## Appendix B: Common Pitfalls and How to Avoid Them

### Pitfall 1: Forgetting to Remove the Old `#[cfg(test)]` Block

**Symptom:** Test is duplicated; production file still contains test wiring.

**Prevention:** After moving a test, `grep` the original file for `#[cfg(test)]` to confirm it's gone.

### Pitfall 2: `use super::*` Breaks After Moving

**Symptom:** Compilation error "cannot find `super` in this scope" or "unresolved import".

**Fix:** Replace `use super::*` with explicit crate imports, e.g.
`use ovrley_core::module::function`.

### Pitfall 3: Private Item Access Lost

**Symptom:** "function `helper` is private" after moving test.

**Fix:** Add `pub(crate)` visibility to the helper. Do not make it fully `pub` unless it should be public API. Document with a comment:

```rust
// Visible for tests in Phase 1. Not intended as public API.
pub(crate) fn helper() { ... }
```

### Pitfall 4: Fixture Paths Break After Moving Test

**Symptom:** File not found errors in tests.

**Fix:** Ensure all tests use `tests/common/test_config.rs`. Never hardcode paths.

### Pitfall 5: Accidentally Changing Behavior While "Improving" Tests

**Symptom:** Test passes but assertions changed; manual validation reveals different output.

**Prevention:** Copy-paste assertions exactly. If a test looks wrong, add a TODO comment but do not fix it in Phase 1. Phase 1 is for safety nets, not bug fixes.

### Pitfall 6: Leaving a Migrated Test Under `src/*/tests/`

**Symptom:** The `#[path]` line is gone, but the old file no longer runs under
`cargo test`.

**Fix:** Move the migrated test to `ovrley_core/tests/` or `src-tauri/tests/`.
Do not add replacement production-side test wiring or a test-only utility
module under `src/` just to preserve the old layout.

### Pitfall 7: Tauri Crate Tests vs ovrley_core Tests

**Symptom:** Tests in `src-tauri/` cannot find `ovrley_core` types.

**Fix:** The Tauri crate depends on `ovrley_core` as a workspace member. Tests in `src-tauri/tests/` should import from the ovrley_core crate explicitly: `use ovrley_core::...`.

---

## Appendix C: Quick Reference — Test Migration Decision Tree

```
Does the test only use public API?
├── YES → Can it be a crate-level integration test?
│         └── YES → Move to ovrley_core/tests/<name>_tests.rs
│         └── NO  → Move to ovrley_core/tests/<name>_tests.rs anyway
│                     (crate-level tests can only use public API by design)
└── NO  → Does it need a private helper?
          ├── YES → Can the helper be made pub(crate)?
          │         ├── YES → Make pub(crate), move test to crate-level
          │         └── NO  → Stop and document the blocker; do not invent a
          │                     new production-side test wiring path in Phase 1
          └── NO  → Does it need a struct field?
                    ├── YES → Consider a minimal non-public test seam
                    └── NO  → Re-evaluate whether the test is really targeting
                                public behavior
```

**Default rule:** Use crate-level tests only. If a test cannot be migrated
cleanly, prefer a narrow `pub(crate)` seam and document the rationale.

---

## Cross-Reference to Master Plan

This Phase 1 plan implements the following sections of `backend-refactor-v2.md`:

| Master Plan Section | Phase 1 Implementation |
|---------------------|------------------------|
| 2.2 Tests Come Before Refactors | Phase 1 *is* this principle in action |
| 2.4 Small, Safe, Independent Changes | One file per step |
| 2.5 All Tests in Dedicated `tests/` Directories | Core deliverable |
| 2.9 Shared Test Config | Step 2 |
| 3.1 Test Placement and Test Structure | Steps 3-4 |
| 3.3 Fixture Resolution and Test Portability | Step 2 + all test moves |
| 5. Refactor Phases → Phase 1 Deliverables | All deliverables addressed |
| 6. Testing Strategy | Steps 5-6 |
| 16.1 Automated Checks | Step 7 |
| 16.2 Behavioral Checks | Manual Validation Checklist |

### What Phase 1 Deliberately Does NOT Touch

| Master Plan Item | Deferred To |
|------------------|-------------|
| 3.2 `Result<T, String>` → typed errors | Phase 2 |
| 3.4 Stringly-typed metric keys | Phase 2 |
| 3.5 Oversized function signatures | Phase 4 |
| 3.6 Duplicated RDP | Phase 3 |
| 3.7 Duplicate interpolation | Phase 3 |
| 3.8 Large `render/widgets/common.rs` | Phase 3 |
| 3.9 Global caches | Phase 5 |
| 3.10 Bin crate boilerplate | Phase 3 |
| 3.11 Command layer logic | Phase 4 |
| 3.12 `cfg!(debug_assertions)` → `#[cfg]` | Phase 6 |
| 3.13 Commented debug output | Phase 3 |
| 3.14 `encode` depends on `commands` | Phase 3 |
| 4.1 Target Core Layout (`error.rs`, `types.rs`, `paths.rs`) | Phases 2-3 |
| 9. Pipeline/Threading/Cancellation docs | Phase 4 |
| 15. Documentation Standards | Phase 6 |

---

*End of Phase 1 Implementation Plan*
