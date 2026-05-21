# Phase 1 — Test Safety Net and Test Migration: Detailed Implementation Plan

[IMPORTANT!] Ignore this file completely, unless explicitely told to use this file.

## Overview

Phase 1 is the foundation of the entire refactor. Its purpose is to create a regression safety net before any architectural changes are made. No behavior may change. The only deliverables are: relocated tests, shared fixture infrastructure, new snapshot/golden tests, and verification that everything compiles and passes.

## Guiding Principles (from master plan)

- **Behavior preservation is mandatory** — observable behavior must not change
- **Tests come before refactors** — this phase comes first for a reason
- **Small, safe, independent changes** — each step has one clear purpose
- **All tests must live in dedicated `tests/` directories** — no inline `#[cfg(test)] mod tests` or `#[path = "tests/..."]` in production files
- **All tests must share a common test config** — no scattered `repo_root()` or `parent().unwrap()` chains

**Note on line numbers:** The line numbers in this document reflect the **current state of the codebase**, not the master plan. Several files have grown since the master plan was written (e.g., `ffmpeg_composite.rs` tests moved from line 228 to 448, `video_composite_pipeline.rs` tests from 549 to 806). Always verify line numbers before editing.

---

## Step 1: Create Test Infrastructure

### 1.1 Create directory structure

Create the following directories under `ovrley_core/`:

```
ovrley_core/tests/
├── common/
│   ├── mod.rs
│   └── test_config.rs
└── fixtures/
    ├── activity/
    ├── config/
    ├── ffprobe/
    │   ├── 4k.json
    │   └── 1080p.json
    ├── video/
    │   └── sample.mp4
    └── expected/
```

### 1.2 Create `tests/common/test_config.rs`

This is the single source of truth for all fixture paths. Every test file will import from here.

```rust
use std::path::{Path, PathBuf};

/// Returns the repository root directory.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("ovrley_core manifest dir should have a parent")
        .parent()
        .expect("ovrley_core parent should have a parent")
        .to_path_buf()
}

/// Returns the test fixtures directory root.
pub fn fixtures() -> PathBuf {
    repo_root().join("tests").join("fixtures")
}

/// Returns the path to the representative GPX activity fixture.
pub fn parsed_activity_path() -> PathBuf {
    fixtures().join("activity").join("Test_GPX-parse-debug.json")
}

/// Returns the path to the FIT activity fixture.
pub fn fit_activity_path() -> PathBuf {
    fixtures().join("activity").join("Test_FIT-parse-debug.json")
}

/// Returns the path to a simple render config fixture.
pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

/// Returns the path to the 4K ffprobe fixture.
pub fn ffprobe_4k_path() -> PathBuf {
    fixtures().join("ffprobe").join("4k.json")
}

/// Returns the path to the 1080p ffprobe fixture.
pub fn ffprobe_1080p_path() -> PathBuf {
    fixtures().join("ffprobe").join("1080p.json")
}

/// Returns the path to the sample video fixture.
pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("sample.mp4")
}

/// Resolves an arbitrary fixture path relative to the fixtures root.
pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    fixtures().join(relative)
}
```

### 1.3 Create `tests/common/mod.rs`

```rust
pub mod test_config;
```

### 1.4 Copy fixture files

Copy the following files from their current locations into the new fixture directories:

| Source                                       | Destination                                                     |
| -------------------------------------------- | --------------------------------------------------------------- |
| `debug/activities/Test_GPX-parse-debug.json` | `ovrley_core/tests/fixtures/activity/Test_GPX-parse-debug.json` |
| `debug/activities/Test_FIT-parse-debug.json` | `ovrley_core/tests/fixtures/activity/Test_FIT-parse-debug.json` |

Create a minimal config fixture at `ovrley_core/tests/fixtures/config/simple.json`:

```json
{
  "scene": {
    "fps": 30,
    "start": 0,
    "end": 60,
    "width": 1920,
    "height": 1080
  },
  "values": [{ "value": "speed", "x": 0, "y": 0 }]
}
```

**ffprobe fixtures:** Create stored ffprobe JSON outputs for video probe tests. These are used to test metadata extraction without requiring actual video files:

| File                          | Purpose                                                                    |
| ----------------------------- | -------------------------------------------------------------------------- |
| `fixtures/ffprobe/4k.json`    | ffprobe output for a 4K video (codec, resolution, duration, creation time) |
| `fixtures/ffprobe/1080p.json` | ffprobe output for a 1080p video                                           |

**Video fixture:** Place a representative MP4 file at `fixtures/video/sample.mp4` for composite pipeline and probe integration tests. This can be a short (1-2 second) video with known properties.

**Expected outputs:** The `fixtures/expected/` directory stores golden outputs for snapshot comparisons (e.g., expected render plans, expected ffmpeg commands).

### 1.5 Add dev dependencies to `Cargo.toml`

Add `insta` for snapshot testing to the `[dev-dependencies]` section of `ovrley_core/Cargo.toml`:

```toml
[dev-dependencies]
insta = "1.41"
```

### What to keep in mind

- `CARGO_MANIFEST_DIR` points to `ovrley_core/`, so `parent().parent()` correctly resolves to the repo root
- The `test_config` module must be **test-only** — no production code may reference it
- Fixture files must be **copied**, not moved — the old locations must remain functional until all tests are migrated
- Keep the fixture helper functions trivially simple — no `OnceCell`, no lazy statics, no complex path resolution logic

---

## Step 2: Migrate `activity/mod.rs` inline tests

> **Note:** The master plan lists 9 test locations to migrate. This plan covers **13 total** — the 9 from the master plan plus 4 additional inline test modules discovered in `encode/` (`video_probe.rs`, `video.rs`, `progress.rs`, `codec_detect.rs`). All must be migrated for Phase 1 to be complete.

### 2.1 Source location

`ovrley_core/src/activity/mod.rs` lines 57-190 — inline `#[cfg(test)] mod tests { ... }`

### 2.2 Tests to migrate

| Test name                                         | Purpose                                                                         |
| ------------------------------------------------- | ------------------------------------------------------------------------------- |
| `builds_dense_report_for_full_fixture`            | Verifies GPX fixture densifies to expected frame count (147360 frames at 30fps) |
| `trims_non_integer_window_across_multiple_fps`    | Verifies non-integer trim windows produce stable frame counts at 24/30/60 fps   |
| `only_densifies_series_requested_by_template`     | Verifies only telemetry required by values/plots is densified                   |
| `trimmed_exports_keep_absolute_distance_progress` | Verifies trimmed plot progress remains absolute to full activity                |

### 2.3 Target location

`ovrley_core/tests/activity_tests.rs`

### 2.4 Migration steps

1. Create `ovrley_core/tests/activity_tests.rs`
2. Copy the test code from the inline module
3. Replace the local `repo_root()` and `fixture()` helpers with imports from `test_config`:
   ```rust
   mod common;
   use common::test_config;
   use ovrley_core::activity::{build_dense_activity_report, parse_activity_json};
   use ovrley_core::config::parse_config_json;
   use std::fs;
   ```
4. Update fixture loading to use `test_config::parsed_activity_path()` and `test_config::fit_activity_path()`:
   ```rust
   fn load_fixture(path: &PathBuf) -> String {
       fs::read_to_string(path)
           .unwrap_or_else(|error| panic!("Failed to read {}: {error}", path.display()))
   }
   ```
5. Remove the inline `#[cfg(test)] mod tests { ... }` block from `activity/mod.rs`

### 2.5 What to keep in mind

- The tests use **inline JSON strings** for config, not fixture files — preserve this behavior exactly
- The frame count assertion `147360` is a critical regression guard — do not change it
- The FIT fixture path must use `test_config::fit_activity_path()`, not the GPX path
- These tests are **integration tests** (they live in `tests/` and import from `ovrley_core` as an external crate) — use fully qualified paths like `ovrley_core::activity::...`

---

## Step 3: Migrate `config/mod.rs` path-included tests

### 3.1 Source location

`ovrley_core/src/config/mod.rs` line 615 — `#[path = "tests/config_tests.rs"] mod tests;`
Test file: `ovrley_core/src/config/tests/config_tests.rs`

### 3.2 Target location

`ovrley_core/tests/config_tests.rs`

### 3.3 Migration steps

1. Read the existing test file at `src/config/tests/config_tests.rs`
2. Create `ovrley_core/tests/config_tests.rs` with the same content
3. Update imports to use `test_config` for any fixture paths
4. Update imports to use `ovrley_core::config::...` (external crate access)
5. Remove the `#[path = "tests/config_tests.rs"] mod tests;` line from `config/mod.rs`
6. Delete the old `src/config/tests/config_tests.rs` file

### 3.4 What to keep in mind

- Path-included tests have access to **private module internals** — after migration to `tests/`, they can only access **public APIs**
- If any test relies on private internals, either:
  - Expose a narrow `pub(crate)` testing seam, or
  - Rewrite the test to use public behavior only
- This is the first test that will reveal if any tests are overfit to implementation details

---

## Step 4: Migrate `commands/mod.rs` path-included tests

### 4.1 Source location

`ovrley_core/src/commands/mod.rs` line 649 — `#[path = "tests/commands_tests.rs"] mod tests;`
Test file: `ovrley_core/src/commands/tests/commands_tests.rs`

### 4.2 Target location

`ovrley_core/tests/commands_tests.rs`

### 4.3 Migration steps

1. Read the existing test file
2. Create `ovrley_core/tests/commands_tests.rs`
3. Replace local `repo_root()` with `test_config::repo_root()`
4. Update all fixture paths to use `test_config` helpers
5. Update imports to `ovrley_core::commands::...`
6. Remove the `#[path]` declaration from `commands/mod.rs`
7. Delete the old test file

### 4.4 What to keep in mind

- The commands tests file contains its own `repo_root()` helper (lines 266-273) — this is exactly the duplication that `test_config` eliminates
- Commands tests may depend on activity/config fixtures — verify paths resolve correctly
- These tests likely exercise the full command pipeline — they are valuable regression tests

---

## Step 5: Migrate `encode/fps.rs` path-included tests

### 5.1 Source location

`ovrley_core/src/encode/fps.rs` line 104 — `#[path = "tests/fps_tests.rs"] mod tests;`
Test file: `ovrley_core/src/encode/tests/fps_tests.rs`

### 5.2 Target location

`ovrley_core/tests/fps_tests.rs`

### 5.3 Migration steps

1. Create `ovrley_core/tests/fps_tests.rs`
2. Update imports to `ovrley_core::encode::fps::...`
3. Remove the `#[path]` declaration from `fps.rs`
4. Delete the old test file

### 5.4 What to keep in mind

- FPS tests are likely pure unit tests — migration should be straightforward
- These protect rational FPS type behavior — critical for 29.97/59.94 NTSC rates

---

## Step 6: Migrate `encode/ffmpeg_composite.rs` path-included tests

### 6.1 Source location

`ovrley_core/src/encode/ffmpeg_composite.rs` line 448 — `#[path = "tests/ffmpeg_composite_tests.rs"] mod tests;`
Test file: `ovrley_core/src/encode/tests/ffmpeg_composite_tests.rs`

### 6.2 Target location

`ovrley_core/tests/ffmpeg_composite_tests.rs`

### 6.3 Migration steps

1. Create `ovrley_core/tests/ffmpeg_composite_tests.rs`
2. Update imports to `ovrley_core::encode::ffmpeg_composite::...`
3. Remove the `#[path]` declaration from `ffmpeg_composite.rs`
4. Delete the old test file

### 6.4 What to keep in mind

- These tests verify composite ffmpeg command generation — snapshot candidates for later
- May depend on config fixtures — update paths via `test_config`

---

## Step 7: Migrate `encode/video_composite_pipeline.rs` path-included tests

### 7.1 Source location

`ovrley_core/src/encode/video_composite_pipeline.rs` line 806 — `#[path = "tests/video_composite_pipeline_tests.rs"] mod tests;`
Test file: `ovrley_core/src/encode/tests/video_composite_pipeline_tests.rs`

### 7.2 Target location

`ovrley_core/tests/video_composite_pipeline_tests.rs`

### 7.3 Migration steps

1. Create `ovrley_core/tests/video_composite_pipeline_tests.rs`
2. Replace local `repo_root()` (lines 595-602) with `test_config::repo_root()`
3. Update all fixture paths
4. Update imports to `ovrley_core::encode::video_composite_pipeline::...`
5. Remove the `#[path]` declaration from `video_composite_pipeline.rs`
6. Delete the old test file

### 7.4 What to keep in mind

- This file contains its own `repo_root()` with `parent().unwrap()` chains — exactly what we're eliminating
- Composite pipeline tests are complex — verify they still compile after migration
- These tests are high-value — they protect the composite render pipeline behavior

---

## Step 8: Migrate `render/format.rs` inline tests

### 8.1 Source location

`ovrley_core/src/render/format.rs` line 500 — inline `#[cfg(test)] mod tests { ... }`

### 8.2 Target location

`ovrley_core/tests/format_tests.rs`

### 8.3 Migration steps

1. Read the inline test module
2. Create `ovrley_core/tests/format_tests.rs`
3. Update imports to `ovrley_core::render::format::...`
4. Remove the inline test module from `format.rs`

### 8.4 What to keep in mind

- Format tests likely test `formatValue`, `raw_value`, `format_metric_parts` — these use stringly-typed metric keys that will change in Phase 2
- Preserve the exact test assertions — they are regression guards for Phase 2 migration

---

## Step 9: Migrate `render/widgets/value.rs` inline tests

### 9.1 Source location

`ovrley_core/src/render/widgets/value.rs` line 865 — inline `#[cfg(test)] mod tests { ... }`

### 9.2 Target location

`ovrley_core/tests/widget_value_tests.rs`

### 9.3 Migration steps

1. Read the inline test module
2. Create `ovrley_core/tests/widget_value_tests.rs`
3. Update imports to `ovrley_core::render::widgets::value::...`
4. Remove the inline test module from `value.rs`

### 9.4 What to keep in mind

- Widget value tests may depend on Skia surface creation — verify they work as integration tests
- If tests require private internals, consider whether they should test public behavior instead

---

## Step 10: Migrate `encode/video_probe.rs` inline tests

### 10.1 Source location

`ovrley_core/src/encode/video_probe.rs` line 253 — inline `#[cfg(test)] mod tests { ... }`

### 10.2 Target location

`ovrley_core/tests/video_probe_tests.rs`

### 10.3 Migration steps

1. Read the inline test module
2. Create `ovrley_core/tests/video_probe_tests.rs`
3. Update imports to `ovrley_core::encode::video_probe::...`
4. Remove the inline test module from `video_probe.rs`

### 10.4 What to keep in mind

- Video probe tests may use stored ffprobe JSON fixtures — check if fixture paths need updating
- These tests protect metadata extraction behavior (creation time priority, codec detection)

---

## Step 11: Migrate `encode/video.rs` inline tests

### 11.1 Source location

`ovrley_core/src/encode/video.rs` line 899 — inline `#[cfg(test)] mod tests { ... }`

### 11.2 Target location

`ovrley_core/tests/video_tests.rs`

### 11.3 Migration steps

1. Read the inline test module
2. Create `ovrley_core/tests/video_tests.rs`
3. Update imports to `ovrley_core::encode::video::...`
4. Remove the inline test module from `video.rs`

### 11.4 What to keep in mind

- `video.rs` contains `RenderController` — tests may involve cancellation/progress state
- These tests are high-value for protecting render lifecycle behavior

---

## Step 12: Migrate `encode/progress.rs` inline tests

### 12.1 Source location

`ovrley_core/src/encode/progress.rs` line 106 — inline `#[cfg(test)] mod tests { ... }`

### 12.2 Target location

`ovrley_core/tests/progress_tests.rs`

### 12.3 Tests to migrate

| Test name                                                    | Purpose                                                   |
| ------------------------------------------------------------ | --------------------------------------------------------- |
| `returns_none_during_warmup`                                 | Verifies EMA estimator returns None during 5-frame warmup |
| `reports_immediately_when_progress_and_elapsed_time_exist`   | Verifies reporting after warmup                           |
| `clamps_optimistic_ema_to_wall_clock_throughput`             | Verifies EMA is clamped to wall clock throughput          |
| `uses_ema_when_it_is_slower_than_wall_clock_throughput`      | Verifies EMA is used when slower                          |
| `can_report_output_equivalent_fps_from_scaled_frame_seconds` | Verifies FPS calculation accuracy                         |

### 12.4 Migration steps

1. Create `ovrley_core/tests/progress_tests.rs`
2. Copy test code from inline module
3. Update imports to `ovrley_core::encode::progress::ProgressEstimator`
4. Remove the inline test module from `progress.rs`

### 12.5 What to keep in mind

- `ProgressEstimator` tests are pure unit tests — straightforward migration
- These protect the EMA-based FPS/ETA calculation — critical for progress UI
- The warmup period (5 frames) and alpha (0.90) are implementation details the tests encode

---

## Step 13: Migrate `encode/codec_detect.rs` inline tests

### 13.1 Source location

`ovrley_core/src/encode/codec_detect.rs` line 728 — inline `#[cfg(test)] mod tests { ... }`

### 13.2 Target location

`ovrley_core/tests/codec_detect_tests.rs`

### 13.3 Migration steps

1. Read the inline test module
2. Create `ovrley_core/tests/codec_detect_tests.rs`
3. Update imports to `ovrley_core::encode::codec_detect::...`
4. Remove the inline test module from `codec_detect.rs`

### 13.4 What to keep in mind

- Codec detect tests may probe actual ffmpeg — verify they work in CI environment
- These tests protect codec availability detection (490 lines of probe logic)

---

## Step 14: Migrate `video_server.rs` inline tests

### 14.1 Source location

`src-tauri/src/video_server.rs` line 461 — inline `#[cfg(test)] mod tests { ... }`

### 14.2 Target location

`src-tauri/tests/video_server_tests.rs`

### 14.3 Migration steps

1. Read the inline test module
2. Create `src-tauri/tests/video_server_tests.rs`
3. Update imports to use the `tauri_app` crate's public API
4. Remove the inline test module from `video_server.rs`

### 14.4 What to keep in mind

- `video_server.rs` is in the **Tauri shell crate**, not `ovrley_core` — tests go in `src-tauri/tests/`
- HTTP server tests may test range headers, 404/416 responses — preserve behavior exactly
- The Tauri crate may need its own `Cargo.toml` `[dev-dependencies]` for test utilities

---

## Step 15: Add Snapshot/Golden Tests

### 15.1 Config parsing snapshots

Create `ovrley_core/tests/config_snapshot_tests.rs`:

**What to snapshot:**

- Valid config parsing and normalization output
- Default field population
- Composite field handling

**Test approach:**

```rust
let config = parse_config_json(&fixture_json).unwrap();
let normalized = serde_json::to_string_pretty(&config).unwrap();
insta::assert_snapshot!("simple_config_parses", normalized);
```

### 15.2 Activity parsing snapshots

Create `ovrley_core/tests/activity_snapshot_tests.rs`:

**What to snapshot:**

- GPX fixture parse output (field presence, series length)
- FIT fixture parse output
- Densified report structure for small fixtures

**Test approach:**

```rust
let activity = parse_activity_json(&fixture_content).unwrap();
let summary = summarize_activity(&activity); // narrow summary for snapshot
insta::assert_snapshot!("gpx_activity_parse", summary);
```

### 15.3 Interpolation/densification snapshots

Add to activity tests:

**What to test:**

- Frame counts at common FPS values (24, 25, 30, 50, 60)
- 29.97 and 59.94 NTSC-rate frame counts
- Frame elapsed seconds for known durations

**Test approach:**

```rust
// Verify frame count at 29.97 fps for a 30-second window
let config = parse_config_json(&r#"{
    "scene": {"fps": 29.97, "start": 0, "end": 30},
    "values": [{"value": "speed", "x": 0, "y": 0}]
}"#).unwrap();
let report = build_dense_activity_report(&activity, &config).unwrap();
assert_eq!(report.frame_count, 899); // 30 * 29.97 ≈ 899.1
```

### 15.4 Frame counting tests

Add explicit tests for:

- Integer duration at integer FPS (e.g., 30s at 30fps = 900 frames)
- Non-integer duration at integer FPS
- Integer duration at non-integer FPS (29.97)
- Non-integer duration at non-integer FPS (59.94)

### 15.5 FFmpeg command generation snapshots

Create `ovrley_core/tests/ffmpeg_command_tests.rs`:

**What to snapshot:**

- Transparent render ffmpeg command (prores, qtrle, vulkan)
- Composite render ffmpeg command (libx264)
- FFmpeg settings struct serialization

**Test approach:**

```rust
let settings = build_ffmpeg_settings(&config).unwrap();
let command_json = serde_json::to_string_pretty(&settings).unwrap();
insta::assert_snapshot!("transparent_prores_command", command_json);
```

### 15.6 Composite pipeline planning snapshots

Add to composite pipeline tests:

**What to snapshot:**

- Timing derivation output
- Overrun guard behavior
- Render plan structure

### 15.7 Video probe metadata snapshots

Add to video probe tests:

**What to snapshot:**

- Probe output from stored ffprobe JSON fixtures
- Creation time priority fallback behavior
- Codec detection results

### 15.8 RDP simplification tests

Create `ovrley_core/tests/rdp_tests.rs`:

**What to test:**

- Route simplification output for known point sets
- Elevation simplification output
- Tolerance parameter effects

**Test approach:**

```rust
let points = vec![(0.0, 0.0), (1.0, 0.5), (2.0, 0.0), (3.0, 1.0)];
let simplified = simplify_rdp(&points, 0.1);
insta::assert_snapshot!("rdp_tolerance_0.1", format!("{:?}", simplified));
```

### 15.9 What to keep in mind

- **Snapshot tests are regression guards** — the first run creates the baseline snapshots
- Run `cargo insta review` after initial snapshot creation to verify snapshots are correct
- Snapshots should capture **public behavior**, not internal implementation details
- Use narrow summaries for large structs — don't snapshot entire 147k-frame reports
- Snapshots must be **deterministic** — no timestamps, random values, or path-dependent output

---

## Step 16: Add Cancellation Lifecycle Tests

### 16.1 Test: Start → Cancel → Cleanup → No Stale Running State

Create in `ovrley_core/tests/cancellation_tests.rs`:

**Test scenario:**

```rust
// 1. Create a RenderController
// 2. Start a render (or simulate running state)
// 3. Call cancel()
// 4. Verify progress state transitions to "cancelled"
// 5. Verify running state is reset (subsequent render can start)
// 6. Verify no stale "running" state remains
```

**What to verify:**

- `cancel()` returns true when running
- Progress status becomes "cancelled"
- `running` flag is reset to false
- Subsequent render can be started without error

### 16.2 What to keep in mind

- Cancellation tests may be difficult to unit test without actual ffmpeg
- Focus on **state machine behavior** — the controller's state transitions
- Document the expected state transition diagram in test comments:
  ```
  Idle -> Running -> Completed
                  -> Failed
                  -> Cancelled
  ```

---

## Step 17: Verify and Validate

### 17.1 Run all tests

```bash
cd src-tauri
cargo test
```

**Expected outcome:** All tests pass, including migrated tests and new snapshot tests.

### 17.2 Verify no inline tests remain

```bash
# Check for inline test modules
grep -r '#\[cfg(test)\]' ovrley_core/src/ --include="*.rs"
grep -r '#\[path = "tests/' ovrley_core/src/ --include="*.rs"
```

**Expected outcome:** No matches in production source files.

### 17.3 Verify test discoverability

```bash
cargo test -- --list
```

**Expected outcome:** All tests are discoverable through `cargo test` without special configuration.

### 17.4 Verify behavior preservation

Manual tests (as specified in master plan):

- [ ] App starts
- [ ] Preview rendering works
- [ ] Transparent overlay export works
- [ ] Composite MP4 export works
- [ ] Video import/probe still works
- [ ] Cancellation still works
- [ ] Progress UI still updates
- [ ] Output paths and filenames unchanged

### 17.5 Run clippy

```bash
cargo clippy -- -D warnings
```

**Expected outcome:** No warnings.

### 17.6 Run fmt

```bash
cargo fmt
```

**Expected outcome:** No formatting changes needed.

---

## Completion Criteria Checklist

Phase 1 is complete when **all** of the following are true:

- [ ] `ovrley_core/tests/common/test_config.rs` exists and is the single source of truth for fixture paths
- [ ] `ovrley_core/tests/fixtures/` directory exists with activity and config fixtures
- [ ] All tests from `activity/mod.rs` inline module migrated to `ovrley_core/tests/activity_tests.rs`
- [ ] All tests from `config/tests/config_tests.rs` migrated to `ovrley_core/tests/config_tests.rs`
- [ ] All tests from `commands/tests/commands_tests.rs` migrated to `ovrley_core/tests/commands_tests.rs`
- [ ] All tests from `encode/tests/fps_tests.rs` migrated to `ovrley_core/tests/fps_tests.rs`
- [ ] All tests from `encode/tests/ffmpeg_composite_tests.rs` migrated to `ovrley_core/tests/ffmpeg_composite_tests.rs`
- [ ] All tests from `encode/tests/video_composite_pipeline_tests.rs` migrated to `ovrley_core/tests/video_composite_pipeline_tests.rs`
- [ ] All tests from `render/format.rs` inline module migrated to `ovrley_core/tests/format_tests.rs`
- [ ] All tests from `render/widgets/value.rs` inline module migrated to `ovrley_core/tests/widget_value_tests.rs`
- [ ] All tests from `encode/video_probe.rs` inline module migrated to `ovrley_core/tests/video_probe_tests.rs`
- [ ] All tests from `encode/video.rs` inline module migrated to `ovrley_core/tests/video_tests.rs`
- [ ] All tests from `encode/progress.rs` inline module migrated to `ovrley_core/tests/progress_tests.rs`
- [ ] All tests from `encode/codec_detect.rs` inline module migrated to `ovrley_core/tests/codec_detect_tests.rs`
- [ ] All tests from `video_server.rs` inline module migrated to `src-tauri/tests/video_server_tests.rs`
- [ ] No production source file contains `#[cfg(test)] mod tests { ... }`
- [ ] No production source file contains `#[path = "tests/..."] mod tests;`
- [ ] No test file contains its own `repo_root()` or `parent().unwrap()` chains (all use `test_config`)
- [ ] Snapshot tests added for: config parsing, activity parsing, interpolation, frame counting, ffmpeg commands, composite planning, video probe, RDP simplification
- [ ] Cancellation lifecycle test added
- [ ] `cargo test` passes with all tests discoverable
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt` passes
- [ ] No production behavior changed (manual validation complete)

---

## Risk Assessment and Mitigations

### Risk 1: Tests rely on private internals

**Impact:** Tests fail to compile after migration to `tests/` directory (external crate access).

**Mitigation:**

- Before migration, audit each test file for access to private items
- If tests need internals, expose a narrow `pub(crate)` testing seam in the module
- Prefer rewriting tests to use public behavior where possible

### Risk 2: Fixture paths break after migration

**Impact:** Tests fail at runtime with "file not found" errors.

**Mitigation:**

- Use `test_config` helpers consistently — single point of failure
- Test the `test_config` helpers first with a simple sanity test
- Keep old fixture files in place until all tests pass with new paths

### Risk 3: Snapshot tests create incorrect baselines

**Impact:** Future refactors pass against wrong baselines, hiding regressions.

**Mitigation:**

- Review every snapshot with `cargo insta review` before committing
- Cross-reference snapshots with known-good manual test results
- Document what each snapshot is protecting

### Risk 4: Tauri crate tests have different structure

**Impact:** `video_server.rs` tests don't migrate cleanly.

**Mitigation:**

- The Tauri crate (`src-tauri/`) is a separate crate — tests go in `src-tauri/tests/`
- May need to add test dependencies to `src-tauri/Cargo.toml`
- HTTP server tests should be straightforward unit tests

### Risk 5: `insta` dependency adds complexity

**Impact:** Additional dev dependency, snapshot files in repo.

**Mitigation:**

- `insta` is the standard Rust snapshot testing library — well-maintained
- Snapshot files are human-readable and reviewable
- Can use simple `assert_eq!` tests instead if preferred, but snapshots are better for regression protection

---

## Order of Execution

The steps above are ordered for safety and independence:

1. **Steps 1-14** can be executed in any order within their groups (infrastructure first, then migrations)
2. **Step 15** (snapshot tests) should come after migrations — snapshot the behavior that the migrated tests protect
3. **Step 16** (cancellation tests) is independent and can be done anytime
4. **Step 17** (verification) is last — everything must be in place

Recommended execution order:

1. Step 1 (infrastructure) — **must be first**
2. Steps 2-14 (migrations) — in any order, smallest files first for confidence
3. Step 15 (snapshot tests) — after migrations are verified
4. Step 16 (cancellation tests) — anytime
5. Step 17 (verification) — **must be last**

---

## Notes for Future Phases

Phase 1 sets up the safety net that enables all subsequent phases:

- **Phase 2** (typed errors, typed metrics) — the migrated tests will catch any behavior changes during error type migration
- **Phase 3** (module cleanup, RDP extraction, interpolation consolidation) — the RDP snapshot tests added in Step 15.8 protect the extraction
- **Phase 4** (pipeline cleanup) — cancellation tests and ffmpeg command snapshots protect orchestration changes
- **Phase 5** (cache cleanup) — tests ensure cache changes don't affect output
- **Phase 6** (documentation) — tests serve as living documentation of expected behavior

**Do not** start any subsequent phase until Phase 1 is fully complete and verified.
