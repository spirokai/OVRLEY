# Phase 1 — Test Safety Net & Test Migration: Detailed Implementation Plan

## 0. Document Purpose

This document is the **step-by-step execution plan** for Phase 1 of the
[OVRLEY Backend Refactor](./backend-refactor-v2.md). It expands the master
plan's Phase 1 section into actionable work items with rationale, preconditions,
and per-step verification. It is intended to be read *alongside* the master
plan, not instead of it.

Every step references the relevant master-plan rule or section so that
nothing is missed. The goal is to produce a regression safety net that all
subsequent phases can rely on.

---

## 0.1 Scope

Phase 1 covers **exactly** two concerns:

1. Relocate every test to a dedicated `tests/` directory (no inline
   `#[cfg(test)] mod tests`, no `#[path = "tests/..."]`).
2. Add a shared fixture/config helper and new snapshot/golden tests so the
   safety net has meaningful coverage **before** structural changes begin.

Phase 1 does **not** touch error types, metrics, module splits, pipeline
reorganization, or cache ownership. Those belong to Phases 2–5.

---

## 0.2 Non-Negotiable Rules That Govern Phase 1

From the master plan, the following rules are **hard constraints** during
every step of this phase:

| Rule | Master Plan Reference | Summary |
|------|----------------------|---------|
| Behavior preservation | §2.1 | Keep observable behavior exactly the same |
| Tests before refactors | §2.2 | Add coverage before structural changes |
| Small, independent changes | §2.4 | One concern per step; compile + pass after each |
| All tests in `tests/` dirs | §2.5 | Forbid inline `#[cfg(test)]` and `#[path]` |
| Shared test config | §2.9 | Single fixture-path source; no `parent().unwrap()` chains |
| No blind refactoring | §2.3 | Inspect file, callers, API, tests before editing |
| Cohesion over line count | §2.8 | Don't split test files mechanically |

---

## 0.3 Pre-Flight Check

Before starting any work, **manually verify the app works**:

- [ ] `pnpm dev` — app starts, frontend loads
- [ ] Import a GPX/FIT activity — parse succeeds
- [ ] Open preview scrubber — preview renders
- [ ] Export transparent overlay — output file produced
- [ ] Import a video + export composite MP4 — output file produced
- [ ] Cancel a render mid-way — cancellation works
- [ ] Progress UI updates during render

Record a baseline of `cargo test` output:

```bash
cargo test --package ovrley_core 2>&1 | Out-File phase1_baseline_tests.txt
cargo test --package app 2>&1 | Out-File phase1_baseline_app_tests.txt
```

---

## 0.4 Complete Inventory of All Test Locations

A fresh grep of the entire workspace produced the following inventory.
**Every entry must be migrated before Phase 1 is complete.**

### Category A — `#[path = "tests/..."]` includes (5 locations)

| # | Production File | Line | Test File |
|---|----------------|------|-----------|
| A1 | `ovrley_core/src/config/mod.rs` | 615 | `config/tests/config_tests.rs` |
| A2 | `ovrley_core/src/commands/mod.rs` | 649 | `commands/tests/commands_tests.rs` |
| A3 | `ovrley_core/src/encode/fps.rs` | 104 | `encode/tests/fps_tests.rs` |
| A4 | `ovrley_core/src/encode/ffmpeg_composite.rs` | 448 | `encode/tests/ffmpeg_composite_tests.rs` |
| A5 | `ovrley_core/src/encode/video_composite_pipeline.rs` | 806 | `encode/tests/video_composite_pipeline_tests.rs` |

### Category B — inline `#[cfg(test)] mod tests { ... }` (8 locations)

| # | Production File | Line | Approx. Lines | Notes |
|---|----------------|------|---------------|-------|
| B1 | `ovrley_core/src/activity/mod.rs` | 57 | 133 | Uses `parent().unwrap().parent().unwrap()` |
| B2 | `ovrley_core/src/render/format.rs` | 500 | 237 | Uses `super::*` imports |
| B3 | `ovrley_core/src/render/widgets/value.rs` | 865 | 54 | Tests icon parsing |
| B4 | `src-tauri/src/video_server.rs` | 461 | 244 | HTTP range parsing + integration tests |
| B5 | `ovrley_core/src/encode/video_probe.rs` | 253 | — | Tests ffprobe JSON parsing |
| B6 | `ovrley_core/src/encode/video.rs` | 899 | — | Tests render controller |
| B7 | `ovrley_core/src/encode/progress.rs` | 106 | 77 | EMA progress estimator tests |
| B8 | `ovrley_core/src/encode/codec_detect.rs` | 728 | — | Codec detection tests |

Total: **13 test locations** across **12 production files**.

> **Note:** The master plan's Phase 1 table lists 9 locations (A1–A5 + B1–B4).
> B5–B8 were discovered during the implementation-plan audit and represent tests
> in newer modules. They must be migrated per the non-negotiable rule §2.5 just
> like any other location.

---

## 1. Step 1 — Create Shared Test Config & Fixture Directory

**Duration estimate:** 30 min

### 1.1 Create directory structure

```
ovrley_core/tests/
├── common/
│   ├── mod.rs                  # re-exports
│   └── test_config.rs          # single source of truth for fixture paths
├── fixtures/
│   ├── config/
│   │   ├── simple.json
│   │   ├── composite.json
│   │   └── invalid.json
│   ├── activity/
│   │   ├── gpx-parse-debug.json
│   │   └── fit-parse-debug.json
│   ├── ffprobe/
│   │   └── (stored ffprobe JSON outputs)
│   └── video/
│       └── sample.mp4
├── config_tests.rs
├── activity_tests.rs
├── ffmpeg_command_tests.rs
├── render_plan_tests.rs
└── video_probe_tests.rs
```

### 1.2 Write `tests/common/test_config.rs`

```rust
//! Central fixture-path resolver for all tests.
//!
//! Every test imports paths from here. No test file should contain
//! `repo_root()`, `fixture_path()`, or `parent().unwrap()` chains.

use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

pub fn parsed_activity_path() -> PathBuf {
    fixtures().join("activity").join("gpx-parse-debug.json")
}

pub fn fit_activity_path() -> PathBuf {
    fixtures().join("activity").join("fit-parse-debug.json")
}

pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

pub fn composite_config_path() -> PathBuf {
    fixtures().join("config").join("composite.json")
}

pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("sample.mp4")
}
```

### 1.3 Write `tests/common/mod.rs`

```rust
pub mod test_config;
```

### 1.4 Copy fixtures from current locations

The current fixtures live under `debug/activities/`. Copy (don't move yet —
existing tests still reference the old location) the key fixtures:

```bash
# Copy activity fixtures
Copy-Item "debug\activities\Test_GPX-parse-debug.json" "src-tauri\ovrley_core\tests\fixtures\activity\gpx-parse-debug.json"
Copy-Item "debug\activities\Test_FIT-parse-debug.json" "src-tauri\ovrley_core\tests\fixtures\activity\fit-parse-debug.json"
```

**What to keep in mind:**
- Master plan §2.9 (common test config is non-negotiable)
- Master plan §3.3 (eliminate `parent().unwrap()` chains)
- `/tests/` at crate root is automatically an integration test directory to cargo
- `/tests/common/` is **not** compiled as a test binary — it's a helper module
- Fixtures go into the `ovrley_core` crate, not `src-tauri`
- The `video_server.rs` tests live in `src-tauri/`, which is a **different
  crate**. Its test fixture config needs to go in `src-tauri/tests/common/`.
  See Step 10.

### 1.5 Verification after Step 1

```bash
cargo test --package ovrley_core
```
Must compile and all pre-existing tests must still pass (their fixture paths
haven't changed yet).

---

## 2. Step 2 — Create Crate-Level Test Stub Files

**Duration estimate:** 15 min

Create empty (or near-empty) crate-level integration test files so the
directory structure is ready before migration:

```
ovrley_core/tests/
├── common/                (already created in Step 1)
├── fixtures/              (already created in Step 1)
├── config_tests.rs        (new, empty or with placeholder)
├── activity_tests.rs      (new, empty)
├── format_tests.rs        (new, empty)
├── value_widget_tests.rs  (new, empty)
├── fps_tests.rs           (new, empty)
├── ffmpeg_composite_tests.rs    (new, empty)
├── video_composite_pipeline_tests.rs  (new, empty)
├── video_probe_tests.rs   (new, empty)
├── progress_tests.rs      (new, empty)
├── codec_detect_tests.rs  (new, empty)
└── cancellation_tests.rs  (new, empty — for Step 8)
```

Also create:
```
src-tauri/tests/
└── video_server_tests.rs  (new, empty)
```

These files will be filled in during Steps 3–10.

---

## 3. Step 3 — Migrate `#[path]` Tests: config

**Duration estimate:** 20 min

### Source file: `ovrley_core/src/config/mod.rs`

Current state:
```rust
// line 614-616
#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
```

### Actions

**3a.** Read the full test file at `ovrley_core/src/config/tests/config_tests.rs`
(140 lines). Understand what it tests:
- Transparent config without composite fields
- Config with all composite fields
- Composite fields skipped in serialization
- Various other config parsing assertions

**3b.** This test file uses `use super::parse_config_json;` — it depends on
access to a `pub(crate)` or `pub` function. `parse_config_json` is already
`pub`, so this test can live at the crate level.

**3c.** Copy the test content into `ovrley_core/tests/config_tests.rs`.
Adjust imports:
- Replace `use super::parse_config_json;` with `use ovrley_core::config::parse_config_json;`
- Verify no other `super::` imports need fixing

**3d.** Remove lines 614–616 from `ovrley_core/src/config/mod.rs`.

**3e.** Verify: `cargo test --package ovrley_core config_tests`

**3f.** Do NOT delete the old `config/tests/config_tests.rs` file yet —
keep it until the migrated crate-level test passes and the production-file
`#[path]` line has been removed. Delete the old file only after full
verification.

**What to keep in mind:**
- Master plan §2.5: production file must not know about test files
- The `parse_config_json` function is `pub` — check `mod.rs` to confirm
- If any private helper is used by tests, expose the narrowest possible
  `pub(crate)` seam with a comment explaining why it exists for test migration.
  Do not rely on `src/*/tests/` being discovered automatically after removing
  the `#[path]` include.

**3g. If internal access is needed:** Keep the final destination as
`ovrley_core/tests/config_tests.rs`. If migration reveals private-item access,
fix that with minimal `pub(crate)` visibility or a small public-behavior
rewrite of the test. Do **not** leave the test stranded under
`src/config/tests/` after removing the `#[path]` line; Cargo will not pick
that file up automatically as a standalone test target.

---

## 4. Step 4 — Migrate `#[path]` Tests: commands

**Duration estimate:** 20 min

### Source file: `ovrley_core/src/commands/mod.rs`

Current state:
```rust
// line 648-650
#[cfg(test)]
#[path = "tests/commands_tests.rs"]
mod tests;
```

### Actions

**4a.** Read `ovrley_core/src/commands/tests/commands_tests.rs` (300 lines).
This is a large test file with:
- A `repo_root()` helper with `parent().unwrap().parent().unwrap()`
- A `test_paths()` helper constructing `AppPaths`
- `fixture_activity()` loading from debug/activities
- Integration-level tests that exercise the full render pipeline

**4b.** This file currently uses `use super::*` — it needs access to
`commands/mod.rs` internals. Check which items are actually used:
- If only `pub` items are used → migrate to `ovrley_core/tests/commands_tests.rs`
- If `pub(crate)`/private items are used → add needed `pub(crate)` seams so
  the migrated test can live under `ovrley_core/tests/commands_tests.rs`

**4c.** Replace the `repo_root()` and fixture loading with imports from
`crate::common::test_config`.

**4d.** Remove lines 648–650 from `commands/mod.rs`.

**4e.** Verify: `cargo test --package ovrley_core commands_tests`

**What to keep in mind:**
- This test exercises the full pipeline (render + encode) — it's important to
  keep it passing
- The `parent().unwrap().parent().unwrap()` chain here is a known anti-pattern
  (§3.3) — this is the moment to fix it
- If the test uses `RenderController`, `AppPaths`, or other types that are
  currently only accessible via `super::*`, expose only the minimal surface
  needed for the crate-level test. Do not add production `#[cfg(test)]`
  wiring or keep the test in `src/commands/tests/` as an unwired file.

---

## 5. Step 5 — Migrate `#[path]` Tests: encode (fps, ffmpeg_composite, video_composite_pipeline)

**Duration estimate:** 40 min

Three test files under `encode/` use the `#[path]` pattern:

| Production File | Test File |
|----------------|-----------|
| `encode/fps.rs:104` | `encode/tests/fps_tests.rs` |
| `encode/ffmpeg_composite.rs:448` | `encode/tests/ffmpeg_composite_tests.rs` |
| `encode/video_composite_pipeline.rs:806` | `encode/tests/video_composite_pipeline_tests.rs` |

### 5a. fps tests

Read `encode/tests/fps_tests.rs`. FPS is a small, pure domain type. Tests are
likely to use only `pub` items. Migrate to `ovrley_core/tests/fps_tests.rs`.

### 5b. ffmpeg_composite tests

Read `encode/tests/ffmpeg_composite_tests.rs`. These test ffmpeg command
construction for the composite pipeline. Likely use `pub` items only.

**Important:** These tests constitute the snapshot tests referenced in the
master plan §6.2 (ffmpeg command generation). Preserve them exactly. If they
test command-array construction, consider adding explicit snapshot assertions
(e.g., `assert_eq!` on key parts of the command array).

### 5c. video_composite_pipeline tests

Read `encode/tests/video_composite_pipeline_tests.rs` (921 lines — the largest
test file). This file:
- Has its own `repo_root()` with `parent().unwrap().parent().unwrap()` (lines
  595–602)
- Constructs full `AppPaths` with test directories
- Exercises the entire composite render pipeline

Migrate to `ovrley_core/tests/video_composite_pipeline_tests.rs`. Replace the
local `repo_root()` with `crate::common::test_config::repo_root()`.

**What to keep in mind:**
- These are the most valuable existing tests — they exercise ffmpeg command
  construction, FPS math, and composite pipeline planning
- Do not change assertion logic, only fixture path resolution and import paths
- The video_composite_pipeline tests may require a real video fixture at
  `tests/fixtures/video/sample.mp4` — copy a suitable test video or adjust the
  test to skip if the fixture is absent (use `#[ignore]` or conditional skip)

### 5d. Remove `#[path]` lines

After each migration, remove the corresponding `#[cfg(test)] #[path = "..."]`
block from the production file.

### 5e. Verify

```bash
cargo test --package ovrley_core fps_tests ffmpeg_composite_tests video_composite_pipeline_tests
```

---

## 6. Step 6 — Migrate Inline Tests: activity

**Duration estimate:** 20 min

### Source file: `ovrley_core/src/activity/mod.rs:57–190`

### Actions

**6a.** Read the test block. It contains 4 tests:
1. `builds_dense_report_for_full_fixture` — uses GPX fixture, checks frame count
2. `trims_non_integer_window_across_multiple_fps` — 24/30/60 FPS frame counts
3. `only_densifies_series_requested_by_template` — sparse series check
4. `trimmed_exports_keep_absolute_distance_progress` — distance progress

**6b.** These tests use `super::build_dense_activity_report` and
`super::parse_activity_json` — both are already `pub` functions.

**6c.** Copy tests to `ovrley_core/tests/activity_tests.rs`. Adjust imports:
- `use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};`
- `use ovrley_core::config::parse_config_json;`
- Use `crate::common::test_config` for fixture paths

**6d.** Remove lines 57–190 from `ovrley_core/src/activity/mod.rs`.

**6e.** Verify: `cargo test --package ovrley_core activity_tests`

**What to keep in mind:**
- These tests cover interpolation/densification — critical behavior per master
  plan §6.1 (P0 priority)
- Frame counts for 24/30/60 FPS with non-integer windows must remain exactly
  the same
- The `fixture()` helper uses `parent().unwrap().parent().unwrap()` — replace
  with `test_config::parsed_activity_path()` and `test_config::fit_activity_path()`

---

## 7. Step 7 — Migrate Inline Tests: render (format.rs, value.rs)

**Duration estimate:** 25 min

### 7a. `render/format.rs:500–737` (237 lines of tests)

Tests for metric formatting (`format_metric_parts`, `format_time_key`,
`MetricIconKind`). These import from `super::*` and may use private helpers.

Check which items are used:
- `format_metric_parts` — is this `pub`?
- `format_time_key` — is this `pub`?
- `MetricIconKind` — is this `pub`?

If all used items are `pub` → migrate to `ovrley_core/tests/format_tests.rs`.

If private helpers are used → make them `pub(crate)` with a `// test seam`
comment so the migrated test can live under `ovrley_core/tests/format_tests.rs`.

### 7b. `render/widgets/value.rs:865–919` (54 lines of tests)

Tests for SVG icon parsing (`parse_svg_path`, `parse_svg_transform`).

Check if these functions are `pub`. If not, add `pub(crate)` visibility.

Migrate to `ovrley_core/tests/value_widget_tests.rs`.

### 7c. Remove inline `#[cfg(test)] mod tests` blocks from both files.

### 7d. Verify: `cargo test --package ovrley_core format value_widget`

**What to keep in mind:**
- Master plan §2.5: the end state for this phase is crate-root integration
  tests plus minimal visibility seams where needed. Do not assume unwired
  `src/*/tests/` files are discoverable.
- The `format.rs` tests are P1 priority per §6.1 (render plans)
- Do not refactor or rename test functions — only move them

---

## 8. Step 8 — Migrate Inline Tests: encode (video_probe, video, progress, codec_detect)

**Duration estimate:** 40 min

These four files were NOT listed in the master plan's Phase 1 table but were
discovered during the implementation-plan audit. They must be migrated.

### 8a. `encode/video_probe.rs:253` — ffprobe JSON parsing tests

Tests likely parse stored ffprobe JSON output and verify metadata extraction
(creation time, resolution, codec).

- If tests reference real ffprobe JSON files → copy those files into
  `tests/fixtures/ffprobe/`
- Migrate to `ovrley_core/tests/video_probe_tests.rs` if using `pub` items

### 8b. `encode/video.rs:899` — RenderController tests

Tests exercise render state transitions, progress polling, and cancellation.

- These are the cancellation lifecycle tests referenced in master plan §5
  Phase 1 deliverable #4
- If they access `RenderController` internals → add `pub(crate)` seams
- Migrate to `ovrley_core/tests/video_tests.rs`

### 8c. `encode/progress.rs:106–183` — ProgressEstimator tests

Tests the EMA-based FPS/ETA estimator (warmup period, convergence, NaN/Infinity
handling).

- `ProgressEstimator` and its methods appear to be `pub`
- Migrate to `ovrley_core/tests/progress_tests.rs`

### 8d. `encode/codec_detect.rs:728` — Codec detection tests

Tests for codec availability probing.

- Likely calls `probe_codec` or related functions
- Migrate to `ovrley_core/tests/codec_detect_tests.rs`

### 8e. Remove all inline `#[cfg(test)] mod tests` blocks from these files.

### 8f. Verify

```bash
cargo test --package ovrley_core video_probe progress codec_detect
```

**What to keep in mind:**
- These are newer modules (master plan §17.14–17.16) — their tests may use
  `Result<T, String>` patterns that will change in Phase 2. Do not refactor
  error types now.
- `progress.rs` tests rely on precise EMA math — do not change assertion
  values
- `codec_detect.rs` tests may spawn ffmpeg subprocesses — ensure they work
  in CI or use `#[ignore]` if ffmpeg is not guaranteed

---

## 9. Step 9 — Migrate Inline Tests: video_server (src-tauri crate)

**Duration estimate:** 25 min

### Source file: `src-tauri/src/video_server.rs:461–705` (244 lines)

This is in the **Tauri app crate**, not `ovrley_core`. It has its own test
requirements.

### Actions

**9a.** The `video_server.rs` tests include:
- Range header parsing unit tests
- Integration tests that start a tiny_http server and make real HTTP requests

**9b.** Create `src-tauri/tests/video_server_tests.rs`.

**9c.** The tests currently use `use super::*;` — they import private items
from `video_server.rs` (like `parse_range`, `ByteRange`, `valid`). Options:
- Make needed items `pub(crate)` or `pub`
- Move tests to `src-tauri/src/video_server_tests.rs` (a module-level test,
  but separate file — NOT inside `video_server.rs`)

Since `video_server.rs` is not a module directory (it's a standalone file),
the simplest approach: extract test functions to `src-tauri/tests/` and
add `pub(crate)` visibility to `parse_range`, `ByteRange`, `valid`, and
any other items used by tests.

**9d.** If `src-tauri/tests/` needs fixture paths, create
`src-tauri/tests/common/mod.rs` with its own `test_config.rs` (the Tauri
crate has a different `CARGO_MANIFEST_DIR` than `ovrley_core`).

**9e.** Remove lines 461–705 from `src-tauri/src/video_server.rs`.

**9f.** Verify: `cargo test --package app video_server`

**What to keep in mind:**
- Master plan §2.5 applies to ALL crates, not just `ovrley_core`
- The `video_server` is in `src-tauri/` which is a **different crate** (`app`)
- The shared test config (§2.9) is per-crate — `src-tauri/tests/common/` is
  separate from `ovrley_core/tests/common/`
- Range header parsing tests (suffix range, unsatisfiable range) are P2 priority
  per §6.1 — preserve them exactly

---

## 10. Step 10 — Fix Fixture Resolution in All Migrated Tests

**Duration estimate:** 30 min

Now that all tests are in their new locations, systematically replace every
instance of the anti-pattern with the shared test config.

### Anti-pattern to remove

```rust
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap().to_path_buf()
}

fn fixture(name: &str) -> String {
    let path = repo_root().join("debug").join("activities").join(name);
    fs::read_to_string(&path).unwrap_or_else(...)
}
```

### Replacement

```rust
use crate::common::test_config;

let activity_json = std::fs::read_to_string(test_config::parsed_activity_path()).unwrap();
```

### Locations to audit

From the master plan §3.3 and the inventory above:
- `activity/mod.rs` tests (now in `activity_tests.rs`) — lines 65–73
- `commands/tests/commands_tests.rs` (now in `commands_tests.rs`) — lines 266–273
- `encode/tests/video_composite_pipeline_tests.rs` — lines 595–602

Also check any other test that constructs paths from `CARGO_MANIFEST_DIR` with
parent traversal.

### Verification

```bash
# Search for remaining parent().unwrap() chains in test files
rg "parent\(\).*unwrap\(\)" src-tauri/ovrley_core/tests/
rg "parent\(\).*unwrap\(\)" src-tauri/tests/
```

**What to keep in mind:**
- Master plan §2.9: a single redirect point for all fixtures
- Master plan §3.3: eliminated brittle `parent().unwrap()` chains
- Do NOT modify the `debug/activities/` directory — it may be used by
  diagnostic binaries. Fixtures are **copied**, not moved.
- If a test file needs a fixture that doesn't exist in `tests/fixtures/`,
  add it there first

---

## 11. Step 11 — Add New Snapshot / Golden Tests

**Duration estimate:** 1–2 hours

Now that the test infrastructure is in place, add new tests to strengthen the
safety net. These are described in the master plan §5 Phase 1 deliverable #3.

### 11a. Config parsing snapshot tests

File: `ovrley_core/tests/config_tests.rs` (or extend the existing migrated
config tests)

Tests to add:
- Valid minimal config → verify all defaults filled
- Valid composite config → verify composite fields preserved
- Invalid JSON → verify error message
- Missing required fields → verify error
- Unknown metric strings → verify error (Phase 2 will improve this)
- Config round-trip: parse → serialize → parse → assert equal

Use the fixture files in `tests/fixtures/config/`:
- `simple.json` — minimal transparent config
- `composite.json` — full composite config
- `invalid.json` — intentionally broken JSON

### 11b. Activity parsing snapshot tests

File: `ovrley_core/tests/activity_tests.rs`

Tests to add:
- GPX fixture → verify parsed activity structure (point count, bounds, timestamps)
- FIT fixture → verify same
- Missing optional fields → verify defaults
- Malformed JSON → verify error handling
- Debug payload wrapper → verify unwrapped correctly

### 11c. Interpolation / densification snapshot tests

File: `ovrley_core/tests/activity_tests.rs`

Tests to add (expand the existing migrated ones):
- Frame count at common FPS values: 24, 25, 29.97, 30, 50, 59.94, 60
- Non-integer durations (e.g., 7.3s at 29.97fps)
- Edge cases: zero-duration window, start > end, start == end
- NTSC-rate behavior: verify frame counts for 30000/1001 and 60000/1001

### 11d. Frame counting tests

File: `ovrley_core/tests/fps_tests.rs`

Tests to add (FPS is a rational type — `Fps::new(num, den)`):
- Integer FPS: 30/1, 60/1 → as_f64(), frame count for duration
- NTSC FPS: 30000/1001, 60000/1001 → precision, rounding behavior
- Division: 60fps / 2 = 30fps, 59.94fps / 2 = 29.97fps
- Invalid: denominator = 0 → error
- Duration-to-frame-count: exact integer windows, fractional windows
- Equality and comparison

### 11e. FFmpeg command generation snapshot tests

File: `ovrley_core/tests/ffmpeg_command_tests.rs` or extend
`ffmpeg_composite_tests.rs`

Tests to add:
- Transparent prores command → verify key args (-c:v prores_ks, -profile:v, -pix_fmt)
- Transparent qtrle command → verify key args (-c:v qtrle, -pix_fmt argb)
- Composite libx264 command → verify key args (-c:v libx264, -preset, -crf)
- Codec filter chain presence: -vf with scale, format, etc.
- Input args: -f rawvideo, -s WxH, -r fps, -pix_fmt
- Output args: -y (overwrite), output path

**Approach:** These tests should construct an ffmpeg settings struct and assert
on the command-line string or argument array. Do NOT actually spawn ffmpeg —
test command construction only.

### 11f. Composite pipeline planning tests

File: `ovrley_core/tests/video_composite_pipeline_tests.rs`

Tests to add:
- Timing derivation: sync_offset, render_duration → frame ranges
- Overrun guard: render_duration exceeding video_duration → capped correctly
- Widget update rate division: 60fps source ÷ 2 update rate → 30fps overlay
- FPS matching: overlay FPS must be integer-divisible from source FPS

### 11g. Video probe metadata extraction tests

File: `ovrley_core/tests/video_probe_tests.rs`

Tests to add:
- Parse stored ffprobe JSON (4K, 1080p) → verify resolution, codec, FPS
- Creation time priority: `com.apple.quicktime.creationdate` → `creation_time` → file metadata
- Missing fields → graceful degradation
- Multiple streams → video stream selection

**Prerequisite:** Store representative ffprobe JSON output in
`tests/fixtures/ffprobe/`. Generate by running:
```bash
ffprobe -v quiet -print_format json -show_format -show_streams sample.mp4 > 1080p.json
```

### 11h. RDP simplification output tests

File: `ovrley_core/tests/rdp_tests.rs` (new)

Even though RDP extraction happens in Phase 3, add tests NOW while we have
the duplicated code. This protects behavior before extraction.

Tests to add:
- Straight line (2 points) → returns both points
- Collinear points → middle points removed
- Single point → returns the point
- Empty input → returns empty
- Real route data → verify simplification ratio at given tolerance
- Elevation profile → verify peaks preserved

These tests should exercise the `perpendicular_distance` function and the
RDP simplification loop in both `route.rs` and `elevation.rs` to verify
they produce identical output before consolidation.

---

## 12. Step 12 — Add Cancellation Lifecycle Tests

**Duration estimate:** 45 min

File: `ovrley_core/tests/cancellation_tests.rs` (new)

Per master plan §5 Phase 1 deliverable #4, add tests that exercise the full
cancellation lifecycle.

### Tests to add

```rust
// Test 1: Start -> immediate cancel
// - Begin a render
// - Immediately call cancel()
// - Assert progress state transitions to Cancelled
// - Assert no stale running state remains
// - Assert output file is cleaned up
//
// Test 2: Start -> render partial -> cancel
// - Begin a render
// - Wait for some progress (e.g., 10% complete)
// - Call cancel()
// - Assert progress state transitions to Cancelled
// - Assert frame count < total frame count
// - Assert ffmpeg process is not orphaned
// - Assert partial output file is cleaned up
//
// Test 3: Start -> complete normally
// - Begin a render, wait for completion
// - Assert progress state transitions to Completed
// - Assert output file exists
// - Assert no stale running state
//
// Test 4: Double cancel (idempotent)
// - Begin a render
// - Call cancel() twice
// - Assert no panic, no deadlock
// - Assert state is Cancelled
//
// Test 5: Cancel resets state for next render
// - Begin, cancel, wait for state reset
// - Begin a new render
// - Assert the second render starts cleanly
```

**Prerequisites:** These tests require a render controller that supports
synchronous or polling-based progress checks. If `RenderController` only
works with async Tauri commands, consider:
- Using `std::thread::spawn` for the render, polling progress in the test thread
- Making a test-only synchronous render entry point if needed
- Marking tests `#[ignore]` if they require ffmpeg and it may not be available
  in all environments

**What to keep in mind:**
- Master plan §9.2: cancellation must be complete (8 steps)
- Do NOT write tests that leak ffmpeg processes — always join/wait with timeout
- These tests may need a very short video fixture (1–2 seconds) to keep test
  duration reasonable
- If the current architecture makes cancellation testing impractical, document
  the gap and file it as a Phase 4 dependency

---

## 13. Step 13 — Final Verification

**Duration estimate:** 30 min

### 13a. Compilation check

```bash
cargo build --package ovrley_core
cargo build --package app
```

Must compile with zero errors and zero warnings.

### 13b. All tests pass

```bash
cargo test --package ovrley_core
cargo test --package app
```

### 13c. No forbidden test patterns remain

```bash
# Must produce ZERO matches in production source (not test files)
rg "#\[cfg\(test\)\]" src-tauri/ovrley_core/src/
rg "#\[cfg\(test\)\]" src-tauri/src/

# Must produce ZERO matches
rg "#\[path.*=.*tests" src-tauri/ovrley_core/src/
rg "#\[path.*=.*tests" src-tauri/src/
```

### 13d. No brittle fixture resolution remains

```bash
# Must produce ZERO matches in test files
rg "parent\(\).*unwrap\(\).*parent\(\).*unwrap\(\)" src-tauri/ovrley_core/tests/
rg "parent\(\).*unwrap\(\).*parent\(\).*unwrap\(\)" src-tauri/tests/
```

### 13e. Manual regressions tests (from master plan §5 Phase 1)

- [ ] App starts (`pnpm dev`)
- [ ] Preview rendering works (scrub through timeline)
- [ ] Transparent overlay export works (prores and/or qtrle)
- [ ] Composite MP4 export works
- [ ] Video import/probe still works
- [ ] Cancel a render mid-way — still works
- [ ] Progress UI still updates during render
- [ ] Output paths and filenames unchanged

### 13f. Compare `cargo test` output against baseline

Compare with `phase1_baseline_tests.txt` from Step 0.3. All previously passing
tests must still pass. Any new failures must be investigated.

---

## 14. Phase 1 Completion Criteria (from master plan §5)

- [x] All tests pass and are discoverable through `cargo test`
- [x] No production behavior changed
- [x] No source file contains inline `#[cfg(test)] mod tests`
- [x] No source file contains `#[path = "tests/..."] mod tests`
- [x] All tests share fixture paths from a single `test_config.rs` per crate
- [x] Snapshot/golden tests added for the 8 categories listed in Step 11
- [x] Cancellation lifecycle tests added
- [x] Manual regression verification passes

---

## 15. Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Test breaks because of import path change | Medium | Run `cargo test` after EACH migration step |
| Private item needed by test but not exposable | Low | Use `pub(crate)` test seam; document why |
| video_composite_pipeline tests need real MP4 fixture | Medium | Provide a small (1s) test video or `#[ignore]` if absent |
| cancellation tests leave orphan ffmpeg processes | Medium | Use `#[ignore]` for CI; add timeout guards |
| `src-tauri` crate tests need separate config | Low | Create `src-tauri/tests/common/` mirror |
| Test migration causes merge conflicts with ongoing work | Low | Phase 1 touches only test files and removal of test wiring lines; surface area is small |
| New snapshot tests are too brittle | Low | Test behavior (output values, key invariants), not implementation details |

---

## 16. Files Touched (Summary)

### Production files (test wiring removal only)

| File | Change |
|------|--------|
| `ovrley_core/src/config/mod.rs` | Remove lines 614–616 (`#[path]` include) |
| `ovrley_core/src/commands/mod.rs` | Remove lines 648–650 (`#[path]` include) |
| `ovrley_core/src/encode/fps.rs` | Remove lines 103–104 (`#[path]` include) |
| `ovrley_core/src/encode/ffmpeg_composite.rs` | Remove lines 447–448 (`#[path]` include) |
| `ovrley_core/src/encode/video_composite_pipeline.rs` | Remove lines 805–806 (`#[path]` include) |
| `ovrley_core/src/activity/mod.rs` | Remove lines 57–190 (inline test module) |
| `ovrley_core/src/render/format.rs` | Remove lines 500–737 (inline test module) |
| `ovrley_core/src/render/widgets/value.rs` | Remove lines 865–919 (inline test module) |
| `src-tauri/src/video_server.rs` | Remove lines 461–705 (inline test module) |
| `ovrley_core/src/encode/video_probe.rs` | Remove inline test module |
| `ovrley_core/src/encode/video.rs` | Remove inline test module |
| `ovrley_core/src/encode/progress.rs` | Remove lines 106–183 (inline test module) |
| `ovrley_core/src/encode/codec_detect.rs` | Remove inline test module |

### New test files

| File | Contents |
|------|----------|
| `ovrley_core/tests/common/mod.rs` | Re-exports `test_config` |
| `ovrley_core/tests/common/test_config.rs` | Shared fixture path functions |
| `ovrley_core/tests/config_tests.rs` | Migrated + new config tests |
| `ovrley_core/tests/activity_tests.rs` | Migrated + new activity tests |
| `ovrley_core/tests/format_tests.rs` | Migrated format tests |
| `ovrley_core/tests/value_widget_tests.rs` | Migrated value widget tests |
| `ovrley_core/tests/fps_tests.rs` | Migrated + new FPS tests |
| `ovrley_core/tests/ffmpeg_composite_tests.rs` | Migrated + new command tests |
| `ovrley_core/tests/video_composite_pipeline_tests.rs` | Migrated + new pipeline tests |
| `ovrley_core/tests/video_probe_tests.rs` | Migrated + new probe tests |
| `ovrley_core/tests/progress_tests.rs` | Migrated progress tests |
| `ovrley_core/tests/codec_detect_tests.rs` | Migrated codec detect tests |
| `ovrley_core/tests/cancellation_tests.rs` | New cancellation lifecycle tests |
| `ovrley_core/tests/rdp_tests.rs` | New RDP behavior tests |
| `src-tauri/tests/video_server_tests.rs` | Migrated video_server tests |
| `src-tauri/tests/common/mod.rs` | Tauri test config (if needed) |

### New fixture files

| File | Source |
|------|--------|
| `ovrley_core/tests/fixtures/activity/gpx-parse-debug.json` | Copy from `debug/activities/Test_GPX-parse-debug.json` |
| `ovrley_core/tests/fixtures/activity/fit-parse-debug.json` | Copy from `debug/activities/Test_FIT-parse-debug.json` |
| `ovrley_core/tests/fixtures/config/simple.json` | New (or copy from existing template) |
| `ovrley_core/tests/fixtures/config/composite.json` | New (or copy from existing template) |
| `ovrley_core/tests/fixtures/config/invalid.json` | New (deliberately broken JSON) |
| `ovrley_core/tests/fixtures/ffprobe/1080p.json` | Generate with ffprobe |
| `ovrley_core/tests/fixtures/ffprobe/4k.json` | Generate with ffprobe |
| `ovrley_core/tests/fixtures/video/sample.mp4` | Small test video (1-2s) |

---

## 17. What NOT to Do in Phase 1

The following are explicitly **out of scope** and must be deferred to later
phases:

- ❌ Introduce `thiserror` or typed error enums (Phase 2)
- ❌ Create `MetricKind` enum or touch metric string matching (Phase 2)
- ❌ Extract RDP logic into `rdp.rs` (Phase 3)
- ❌ Consolidate interpolation logic (Phase 3)
- ❌ Split `render/widgets/common.rs` (Phase 3)
- ❌ Move `AppPaths` to `paths.rs` (Phase 3)
- ❌ Extract `ffmpeg_settings.rs` (Phase 4)
- ❌ Create request structs for large signatures (Phase 4)
- ❌ Change cache ownership or introduce `RenderContext` (Phase 5)
- ❌ Fix `cfg!(debug_assertions)` → `#[cfg]` (Phase 6)
- ❌ Replace commented-out `println!` with `tracing::debug!` (Phase 6, though
  removal in Phase 3 is also acceptable per §3.13)
- ❌ Add module documentation (Phase 6)
- ❌ Any behavioral changes to rendering, encoding, parsing, or command
  construction

---

## 18. Cross-Reference: Phase 1 vs Master Plan

| Master Plan Reference | Phase 1 Deliverable | Covered In |
|----------------------|---------------------|------------|
| §5 Phase 1 deliverable #1 | Move all tests to `tests/` | Steps 3–9 |
| §5 Phase 1 deliverable #2 | Fixture helper infrastructure | Step 1 |
| §5 Phase 1 deliverable #3 | Snapshot/golden tests (8 categories) | Step 11 |
| §5 Phase 1 deliverable #4 | Cancellation lifecycle tests | Step 12 |
| §2.5 | No inline `#[cfg(test)]` or `#[path]` | Steps 3–9, verified in Step 13c |
| §2.9 | Shared test config, no `parent().unwrap()` | Steps 1, 10, verified in Step 13d |
| §3.1 | Test placement and structure | Entire phase |
| §3.3 | Fixture resolution and portability | Step 10 |
| §6.1 | Test categories (P0–P2 priorities) | Step 11 (tests categorized) |
| §6.2 | Snapshot testing approach | Step 11 |
| §6.3 | Fixture strategy | Step 1 (directory layout) |
| §6.4 | What not to unit test | Step 11 (guidance notes) |
| §6.5 | Tests protect behavior, not implementation | Step 11 (guidance notes) |
| §14.8 | Antipattern: test wiring in production source | Steps 3–9 |
| §17.x | Per-file analysis notes on test migration | Steps 3–9 |

---

## 19. Execution Order Dependency

```
Step  1 (test config + fixtures)
  ↓
Step  2 (crate-level test stubs)
  ↓
Steps 3–9 (migrate individual test files — can be parallelized)
  ↓
Step 10 (fix fixture resolution — after all migrations)
  ↓
Step 11 (add new snapshot tests)
  ↓
Step 12 (add cancellation tests)
  ↓
Step 13 (final verification)
```

Steps 3–9 are independent of each other and can be done in any order or in
parallel. Each must be verified individually before moving to Step 10.

---

## Appendix A: Quick-Reference Grep Commands

Use these to verify compliance during and after the phase:

```bash
# Find remaining inline tests in production source
rg "#\[cfg\(test\)\]" src-tauri/ovrley_core/src/
rg "#\[cfg\(test\)\]" src-tauri/src/

# Find remaining #[path] test includes in production source
rg "#\[path.*=.*tests" src-tauri/ovrley_core/src/
rg "#\[path.*=.*tests" src-tauri/src/

# Find brittle fixture paths in tests
rg "parent\(\).*unwrap\(\)" src-tauri/ovrley_core/tests/
rg "parent\(\).*unwrap\(\)" src-tauri/tests/

# Count test functions
rg "#\[test\]" src-tauri/ovrley_core/tests/ --count
rg "#\[test\]" src-tauri/tests/ --count
```

---

## Appendix B: If Something Goes Wrong

### A test fails after migration

1. Compare the migrated test file against the original (diff)
2. Check that all `use` imports are correct — `super::` → `ovrley_core::module::`
3. Check that fixture paths resolve correctly — use `dbg!()` to print paths
4. If the test used private items, add `pub(crate)` visibility and a `// test seam` comment
5. If the test cannot be migrated without structural changes → stop, document
   the blocker, and prefer the smallest `pub(crate)` seam or public-behavior
   rewrite that still preserves Phase 1 scope. Do not rely on an unwired
   `src/*/tests/` file as the final state.

### A test needs access to private internals

Use the escape hatch from master plan §2.5:
> If tests need access to internals, prefer exposing a narrow `pub(crate)`
> testing seam or moving the test to a crate-level integration test if public
> behavior is sufficient.

Pattern:
```rust
// In the production file, near the tested item:
pub(crate) fn internal_helper_for_tests() -> ... { ... }  // test seam
```

### A test is flaky (timing-dependent)

Mark with `#[ignore]` and file an issue. Do not delete the test. Flaky tests
that depend on real ffmpeg or real HTTP servers are expected.

### A migration causes a merge conflict

Phase 1 touches only:
- Test wiring lines in production files (2–3 lines per file, removal)
- New files in `tests/` directories

Merge conflicts should be minimal. If they occur, prefer the migrated version
and re-apply any new tests that were added since the branch point.
