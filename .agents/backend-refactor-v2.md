# OVRLEY Backend Refactor Guide & Execution Plan

## Purpose

This document is the canonical refactor guide for the OVRLEY Rust backend. It is an execution plan, architectural ruleset, safety checklist, and issue catalog — not a general style guide.

The goal is to modernize the Rust backend for clarity, maintainability, testability, and long-term extensibility while preserving behavior.

The refactor must improve structure **without changing how the application works**.

---

## Table of Contents

1. [Current Architecture Overview](#1-current-architecture-overview)
2. [Non-Negotiable Refactor Rules](#2-non-negotiable-refactor-rules)
3. [Current Technical Debt and Issue Coordinates](#3-current-technical-debt-and-issue-coordinates)
4. [Target Architecture](#4-target-architecture)
5. [Refactor Phases](#5-refactor-phases)
6. [Testing Strategy](#6-testing-strategy)
7. [Error Handling Strategy](#7-error-handling-strategy)
8. [Module Architecture and Dependency Boundaries](#8-module-architecture-and-dependency-boundaries)
9. [Pipeline, Threading, Cancellation, and Ownership](#9-pipeline-threading-cancellation-and-ownership)
10. [Logging and Diagnostics](#10-logging-and-diagnostics)
11. [Performance and Hot-Path Rules](#11-performance-and-hot-path-rules)
12. [Code Style and Refactor Patterns](#12-code-style-and-refactor-patterns)
13. [Best Practices with Examples](#13-best-practices-with-examples)
14. [Antipatterns to Avoid](#14-antipatterns-to-avoid)
15. [Documentation Standards](#15-documentation-standards)
16. [Final Validation Checklist](#16-final-validation-checklist)
17. [Appendix: File-by-File Analysis Summary](#17-appendix-file-by-file-analysis-summary)

---

# 1. Current Architecture Overview

## 1.1 Workspace Layout

```
src-tauri/                    # Tauri application shell
├── Cargo.toml                # Workspace root; members include "." and "ovrley_core"
├── build.rs                  # Links msvcprt on Windows, then tauri_build::build()
├── capabilities/
│   └── default.json          # Tauri v2 capability permissions
├── tauri.conf.json           # App window, CSP, bundle config
├── src/
│   ├── main.rs               # Binary entry point (6 lines)
│   ├── lib.rs                # Tauri command handlers + app setup (360 lines)
│   ├── video_server.rs       # HTTP preview/range server (705 lines)
│   └── bin/                  # Standalone diagnostic binaries
│       ├── parallel_render.rs
│       ├── render_preview.rs
│       ├── render_video.rs
│       └── validate_activity.rs
└── ovrley_core/              # Core library crate
    ├── Cargo.toml            # deps: chrono, serde, serde_json, skia-safe
    └── src/
        ├── lib.rs            # Module declarations (22 lines)
        ├── activity/         # Activity ingestion, trimming, interpolation
        │   ├── mod.rs        # parse_activity_json, build_dense_activity_report
        │   ├── schema.rs     # ParsedActivity, DenseActivityReport, TrimmedActivity
        │   ├── trim.rs       # trim_activity
        │   └── interpolate.rs# Interpolation and densification
        ├── commands/         # Framework-agnostic command logic (654 lines)
        │   ├── mod.rs
        │   └── tests/commands_tests.rs
        ├── config/           # Template/config schema and validation (616 lines)
        │   ├── mod.rs
        │   └── tests/config_tests.rs
        ├── debug/            # Progress/profiling/timing types (113 lines)
        │   └── mod.rs
        ├── encode/           # Video encoding, ffmpeg, pipelines (~4200 lines)
        │   ├── mod.rs
        │   ├── video.rs      # RenderController, render_video, parallel renders
        │   ├── video_pipeline.rs     # Single-pass frame producer/ffmpeg consumer
        │   ├── video_composite_pipeline.rs  # MP4 compositing pipeline
        │   ├── video_composite_debug.rs     # Composite timing/debug summaries
        │   ├── video_probe.rs        # ffprobe metadata extraction
        │   ├── video_debug.rs        # Debug summaries, sample frames, concat
        │   ├── ffmpeg.rs             # ffmpeg discovery + codec settings
        │   ├── ffmpeg_composite.rs   # Composite ffmpeg arg builder
        │   ├── ffmpeg_composite_profiles.rs  # Composite encoder profile templates
        │   ├── fps.rs                # Rational FPS type
        │   ├── progress.rs           # Live progress estimation (ProgressEstimator)
        │   ├── codec_detect.rs       # Codec availability detection (490 lines)
        │   └── tests/
        │       ├── fps_tests.rs
        │       ├── ffmpeg_composite_tests.rs
        │       └── video_composite_pipeline_tests.rs
        └── render/           # Skia overlay rendering and widgets (~4500 lines)
            ├── mod.rs        # prepare_preview_assets, render_frame_rgba
            ├── format.rs     # Metric formatting (737 lines)
            ├── surface.rs    # Skia surface helpers (48 lines)
            ├── text.rs       # Font resolution, text drawing (323 lines)
            └── widgets/
                ├── mod.rs    # Widget preparation entry
                ├── types.rs  # Shared widget types
                ├── common.rs # Drawing/interpolation helpers (774 lines)
                ├── value.rs  # Metric value widgets + SVG icon parser (832 lines)
                ├── route.rs  # Route plot widget (605 lines)
                └── elevation.rs # Elevation profile widget (933 lines)
```

## 1.2 Current Dependency Graph

```
tauri-app (lib.rs)
  ├── video_server (independent)
  └── ovrley_core
        ├── config        — leaf, no internal deps
        ├── debug         — leaf, no internal deps
        ├── activity      — depends on config
        ├── render        — depends on config, activity
        ├── encode        — depends on config, activity, render, debug, commands
        └── commands      — depends on config, activity, render, encode, debug
```

**Problem**: `encode` depends on `commands`. This should not exist — command-level types needed by `encode` should move to a neutral module (`paths.rs`, `types.rs`).

## 1.3 Current Data Flow

```
Frontend JSON
  -> src-tauri/src/lib.rs (Tauri command wrapper)
    -> ovrley_core::commands/*
      -> config::parse_config_json          -> RenderConfig
      -> activity::parse_activity_json      -> ParsedActivity
        -> trim_activity                    -> TrimmedActivity
        -> densify_activity                 -> DenseActivityReport
          -> render::prepare_preview_assets
          -> render::render_frame_rgba (per-frame)
          -> encode::video::render_video
            -> ffmpeg process / output file
```

## 1.4 Current Main Concerns

The backend already has a reasonable high-level domain split, but several areas reduce maintainability:

- fallible functions return `Result<T, String>` everywhere
- metric keys are stringly typed across 5+ files
- tests are inconsistently structured (inline + `#[path]`)
- render/encode orchestration functions are too large (200+ lines, 7+ `#[allow(clippy::too_many_arguments)]`)
- ffmpeg settings, process ownership, progress, and cancellation are mixed together
- `render/widgets/common.rs` has too many responsibilities (774 lines)
- shared geometry/RDP logic is duplicated between route.rs and elevation.rs
- duplicate interpolation logic exists in `activity/interpolate.rs` and `render/widgets/common.rs`
- caches are global and implicit (`OnceLock<Mutex<HashMap>>`)
- logging/debugging is inconsistent (commented `println!` in `video_probe.rs`)
- performance hot paths need explicit guardrails
- command layer contains logic that belongs deeper in core modules
- new composite modules (`ffmpeg_composite_profiles.rs`, `video_composite_debug.rs`) use `Result<T, String>` and `progress.rs` has inline tests violating Phase 1 targets
- `cfg!(debug_assertions)` used where `#[cfg]` would be safer
- binaries duplicate CLI parsing boilerplate (4 files, same `read_arg` helper)

---

# 2. Non-Negotiable Refactor Rules

## 2.1 Behavior Preservation Is Mandatory

The refactor must preserve observable behavior unless a change is explicitly marked as a bug fix.

**Behavior includes:**

- ffmpeg command construction
- render timing and frame count calculation
- timestamp interpolation and telemetry alignment
- queue pacing
- cancellation behavior and state transitions
- output file naming and format
- video metadata extraction
- preview rendering behavior
- composite rendering behavior
- frontend JSON compatibility
- error strings visible to the frontend (unless intentionally migrated)
- progress reporting semantics

**If a change may alter behavior, preserve the old behavior.**

If a behavior change is intentional, document it as:

```
Intentional behavior change:
- Previous behavior:
- New behavior:
- Reason:
- Tests added:
- Manual validation:
```

## 2.2 Tests Come Before Refactors

Before changing error types, modules, pipelines, or caches, add or preserve regression tests. Do not perform large structural changes without a safety net.

**The phase order is intentional:**

1. Test safety net and test migration
2. Typed errors and typed metrics
3. Module cleanup and duplication reduction
4. Pipeline/orchestration cleanup
5. Cache/state cleanup
6. Documentation and final polish

## 2.3 No Blind Refactoring

Before editing any file, the refactoring agent must inspect:

- the full file
- its callers
- its public API
- its tests
- its performance role (is this a hot path?)
- its threading role
- whether its behavior is externally observable

Do not make mechanical transformations without understanding the file.

## 2.4 Small, Safe, Independent Changes

Each step must:

- have one clear purpose (error migration **or** module splitting **or** formatting)
- affect a limited surface area
- compile after completion
- preserve tests and behavior
- avoid unrelated cleanup

Do not combine error migration, module splitting, formatting, algorithm changes, performance changes, and naming changes in the same patch.

## 2.5 All Tests Must Live in Dedicated `tests/` Directories

This is non-negotiable.

**Acceptable locations:**

```
ovrley_core/tests/                        # crate-level integration (preferred)
ovrley_core/src/activity/tests/           # module-local (if internals needed)
ovrley_core/src/render/widgets/tests/
ovrley_core/src/encode/tests/
```

**Forbidden:**

```rust
#[cfg(test)]
mod tests { ... }

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
```

The production file must not know about test files.

**Important**: Do not use module-local `tests/` directories together with `#[path]` includes from production files. If tests need access to internals, prefer exposing a narrow `pub(crate)` testing seam or moving the test to a crate-level integration test if public behavior is sufficient.

## 2.6 Do Not Introduce Premature Abstractions

Do not introduce:
- generic plugin systems, service locators, dependency injection frameworks
- unnecessary traits, speculative GPU renderer abstractions
- micro-crates, over-generalized pipeline traits
- framework-like architecture
- builders for simple required fields

**Abstractions are allowed** when they remove proven duplication, clarify ownership, or enforce a necessary boundary.

## 2.7 Hot Paths Must Stay Hot

Render and encode paths are performance-sensitive. Avoid adding these inside frame/render/write loops:
- heap allocations
- unnecessary clones
- repeated `String` formatting
- repeated `HashMap` construction
- unnecessary `Arc` cloning
- mutex locking
- dynamic dispatch
- allocation-heavy iterator chains where simple loops are clearer/faster

If a refactor touches a hot path, benchmark before and after.

## 2.8 Prefer Cohesion Over File Size Rules

Do not split files only because they exceed a line count. Split when:
- a file has multiple unrelated reasons to change
- imports become incoherent
- tests become awkward
- the file mixes domains
- navigation becomes difficult
- ownership is unclear

## 2.9 All Tests Must Share a Common Test Config

All integration and module-level tests **must** source their test fixture paths from a single shared configuration file (`tests/common/mod.rs` or `tests/common/test_config.rs`). This file defines the paths to:

- a representative parsed activity JSON fixture
- a representative template/config JSON fixture  
- a representative MP4 video fixture (for probe/composite tests)

No test file should hardcode its own `repo_root()` or `fixture_path()` calls with literal relative paths. Instead, each test references the shared config:

```rust
use crate::common::test_config;

let activity = test_config::parsed_activity_path();
let config = test_config::simple_config_path();
let video = test_config::sample_video_path();
```

**Why this is required:**
- a single redirect point when fixtures are reorganized or relocated
- eliminates brittle `parent().unwrap().parent().unwrap()` chains scattered across files
- ensures all tests use the same source data, preventing silent divergence where different tests exercise slightly different inputs
- enables running the full test suite against a different dataset (e.g., CI fixture mirror) by changing one file

The config file must expose simple functions returning `PathBuf`, not lazy statics or `OnceCell` — keep it trivially simple:

```rust
// tests/common/test_config.rs
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

pub fn simple_config_path() -> PathBuf {
    fixtures().join("config").join("simple.json")
}

pub fn sample_video_path() -> PathBuf {
    fixtures().join("video").join("sample.mp4")
}
```

No production code may reference this test config. It is test-only infrastructure.

---

# 3. Current Technical Debt and Issue Coordinates

## 3.1 Test Placement and Test Structure (HIGH PRIORITY)

**Current Problem:** Tests are co-located with source via `#[path = "tests/..."]` (e.g., `config/mod.rs:614-616`) or inline `#[cfg(test)] mod tests { ... }` (e.g., `activity/mod.rs:57-190`, `render/format.rs:500-737`, `render/widgets/value.rs:814-832`).

**Why This Is a Problem:**
- source modules must know about test file layout
- tests may silently rely on private module internals
- test wiring adds noise to production modules
- test discovery is nonstandard

**Target:** Move all tests into dedicated `tests/` directories. Prefer crate-level integration tests for public behavior.

## 3.2 `Result<T, String>` Everywhere (HIGH PRIORITY)

**Current Problem:** Nearly every fallible function returns `Result<T, String>`. Examples: `parse_config_json`, `parse_activity_json`, `trim_activity`, `densify_activity`, `render_frame_rgba`, `build_ffmpeg_settings`, `probe_video`.

**Why This Is a Problem:**
- lose error kind information — cannot distinguish validation vs IO vs ffmpeg failure
- prevent pattern matching on error type
- encourage fragile string-based error inspection (`error.to_lowercase().contains("cancelled")` in `commands/mod.rs:209`)
- make diagnostics weaker
- make error conversion inconsistent

**Target:** Introduce structured errors with `thiserror`. The migration must be incremental. Do not rewrite all error handling in one patch.

## 3.3 Fixture Resolution and Test Portability (HIGH PRIORITY)

**Current Problem:** Tests resolve fixtures by walking parent directories from `CARGO_MANIFEST_DIR`:

```rust
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap().to_path_buf()
}
```

This appears identically in `activity/mod.rs:65-73`, `commands/tests/commands_tests.rs:266-273`, and `encode/tests/video_composite_pipeline_tests.rs:595-602`.

**Why This Is a Problem:** Brittle `parent().unwrap()` chains break when test files move. `CARGO_MANIFEST_DIR` is acceptable when pointing to a stable crate-local fixture root — but avoid fragile parent traversal.

**Target:** Use a single shared test config file as mandated by [Rule 2.9](#29-all-tests-must-share-a-common-test-config):

```
ovrley_core/tests/
├── common/
│   ├── mod.rs
│   └── test_config.rs       # Single source of truth for fixture paths
├── fixtures/
│   ├── config/
│   ├── activity/
│   ├── ffprobe/
│   ├── video/
│   └── expected/
```

All tests import from `test_config` instead of defining their own resolvers:

```rust
use crate::common::test_config;

let activity_path = test_config::parsed_activity_path();
let config_path = test_config::simple_config_path();
let video_path = test_config::sample_video_path();
```

No test file should contain `repo_root()`, `fixture_path()`, or `parent().unwrap()` chains — that logic lives in exactly one place.

## 3.4 Stringly-Typed Metric Keys (HIGH PRIORITY)

**Current Problem:** Metric strings such as `"speed"`, `"heartrate"`, `"elevation"`, `"time"`, `"gradient"`, `"cadence"`, `"power"`, `"temperature"` are scattered as raw string literals across:

- `config/mod.rs` — match arms in `render_data_requirements` (lines 541-553)
- `render/format.rs` — match arms in `format_value` (lines 62-76), `raw_value` (lines 93-137), `format_metric_parts` (lines 150-241)
- `render/text.rs` — gradient check in `value_style` (line 111)
- `render/widgets/value.rs` — match in `metric_icon_kind_for_value` (lines 467-477)
- `activity/trim.rs` — boolean requirements for each metric

**Why This Is a Problem:**
- typos are silent bugs
- supported metrics are not discoverable
- logic is duplicated across 5+ files
- compiler cannot help with refactors
- validation is fragmented

**Target:** Introduce a typed `MetricKind` enum. Must preserve frontend JSON compatibility exactly via `#[serde(rename = "...")]`.

## 3.5 Oversized Function Signatures (MEDIUM PRIORITY)

**Current Problem:** At least **7 functions** suppress `#[allow(clippy::too_many_arguments)]`:

| Function | File | Args |
|----------|------|------|
| `render_preview_with_prepared_assets` | `render/mod.rs:177-188` | 9 |
| `render_frame_rgba` | `render/mod.rs:271-282` | 8 |
| `render_frame_to_surface` | `render/mod.rs:379-391` | 10 |
| `render_frame_surface` | `render/mod.rs:324-335` | 9 |
| `draw_metric_value_widget_with_config` | `render/widgets/value.rs:52-63` | 10 |
| `widget_render_report` | `render/widgets/common.rs:712-723` | 8 |
| `draw_metric_icon` | `render/widgets/value.rs:367-379` | 10 |

**Target:** Use plain request/context structs. Do not automatically introduce builders — simple structs with named fields are sufficient.

## 3.6 Duplicated RDP Simplification (MEDIUM PRIORITY)

**Current Problem:** Identical `perpendicular_distance` function exists in:

- `render/widgets/route.rs:436-446` (inline inside `simplify_route_samples`)
- `render/widgets/elevation.rs:813-823` (inline inside `simplify_elevation_samples_segment`)

Additionally, recursive RDP splitting logic is duplicated between the two modules.

**Target:** Extract shared geometry/RDP utility into `ovrley_core/src/rdp.rs`. Add tests before extraction. Prefer simple free functions over a `Point2D` trait unless generic behavior is proven necessary.

## 3.7 Duplicate Interpolation Logic (MEDIUM PRIORITY)

**Current Problem:** Similar or identical interpolation helpers exist in:

- `activity/interpolate.rs` — `interpolate_points`, `interpolate_numeric_series_value`, `collect_valid_numeric_points`
- `render/widgets/common.rs` — `interpolate_numeric_points`, `interpolate_numeric_series_many`, `interpolate_optional_numeric_series`

These may drift. The `render/widgets/common.rs` versions use `f32` while `activity/interpolate.rs` uses `f64`, creating subtle precision differences.

**Target:** Consolidate interpolation into a single utility module. The `activity/interpolate.rs` versions are the authoritative implementations (they use `f64` for precision). Render code should delegate to them.

## 3.8 Large `render/widgets/common.rs` (MEDIUM PRIORITY)

**Current Problem:** 774 lines containing too many unrelated responsibilities:
- interpolation helpers (duplicated from `activity/interpolate.rs`)
- polyline/area drawing helpers
- marker drawing helpers
- widget transforms and rotation
- geometry/layout/fitting helpers
- progress calculation helpers

**Target:** Split by cohesive responsibility, not arbitrary size:

```
render/widgets/
├── common.rs       # reduced to ~200 lines: only shared constants + tiny helpers
├── geometry.rs     # point/rect/fitting/layout helpers
├── marker.rs       # marker/dot drawing
├── polyline.rs     # polyline/area drawing
├── transform.rs    # rotation/projection transforms
└── progress.rs     # frame progress calculation
```

Only extract modules when cohesion improves. Do not split mechanically.

## 3.9 Global Caches with `OnceLock<Mutex<HashMap>>` (MEDIUM PRIORITY)

**Current Problem:** Two global caches:

- `render/text.rs:277` — font/typeface cache: `static CACHE: OnceLock<Mutex<HashMap<String, Typeface>>>`
- `render/mod.rs:464` — label cache: `static CACHE: OnceLock<Mutex<HashMap<u64, Image>>>`

**Why This May Be a Problem:**
- hidden lifecycle — no way to reset between tests
- possible state leakage between test runs
- hard to measure hit rate or size
- hard to configure (max entries, TTL)

**Important Clarification:** Global caches are not automatically wrong. Do not replace them with complex infrastructure unless there is a concrete reason.

**Target:** First consider a minimal explicit context:

```rust
pub struct RenderContext {
    pub font_cache: FontCache,
    pub label_cache: LabelCache,
}
```

Only add TTL, LRU, max entries, or metrics if needed.

## 3.10 Bin Crate Boilerplate (LOW PRIORITY)

**Current Problem:** Four binaries in `src/bin/` each duplicate the same `read_arg()`, `read_optional_arg()`, and `repo_root()` helpers:

- `src/bin/parallel_render.rs:7-13` — `repo_root()`
- `src/bin/render_preview.rs:14-19` — `read_arg()`, lines 21-27 `repo_root()`, lines 29-33 `read_optional_arg()`
- `src/bin/render_video.rs:9-14` — `read_arg()`, lines 22-28 `repo_root()`
- `src/bin/validate_activity.rs:6-11` — `read_arg()`

**Target:** Extract shared helpers into `src-tauri/src/bin/common.rs`. Diagnostic binaries may use `anyhow`. Core library code should use typed errors.

## 3.11 Command Layer Contains Logic That Belongs Elsewhere (MEDIUM PRIORITY)

**Current Problem:** `commands/mod.rs` contains:
- `derive_composite_render_plan` — composite timing/planning logic (lines 302-380)
- `apply_composite_scene_timing` — mutates render config (lines 386-391)
- `is_composite_render` — composite mode detection (lines 225-227)
- Template listing, path resolution, file operations

**Why This Is a Problem:** `commands` should remain a thin orchestration boundary. Planning/mutation logic belongs in `encode` or a neutral planning module.

**Target:** Move reusable render/composite planning logic out of `commands` when doing so does not create circular dependencies. `commands` should parse inputs, delegate work, and serialize responses — nothing more.

## 3.12 `cfg!(debug_assertions)` Instead of `#[cfg]` (LOW PRIORITY)

**Current Problem:** `src/lib.rs:348`:

```rust
if cfg!(debug_assertions) {
    app.handle().plugin(tauri_plugin_log::Builder::default()...)?;
}
```

**Why This Is a Problem:** `cfg!()` is a runtime macro — both branches are compiled into every build. For compile-time feature gating (where the log plugin shouldn't exist in release binaries), use `#[cfg(debug_assertions)]`.

**Target:** Use `#[cfg(debug_assertions)] { ... }` for compile-time exclusion where the excluded code should not be compiled in release builds.

## 3.13 Commented-Out Debug Output (LOW PRIORITY)

**Current Problem:** `encode/video_probe.rs` contains multiple commented-out `println!` statements identifying debug info about creation-time resolution (lines 155, 163, 170, 177, 185, 191, 197).

**Target:** Remove or replace with `tracing::debug!`.

## 3.14 `encode` Depends on `commands` (MEDIUM PRIORITY)

**Current Problem:** `encode/mod.rs` declares `pub mod video`, and `encode/video.rs` imports `use crate::commands::AppPaths`. This creates a circular-ish dependency where `encode` depends on `commands` which depends on `encode`.

**Why This Is a Problem:** `AppPaths` is a path config type that both `commands` and `encode` need. It should live in a neutral location.

**Target:** Move `AppPaths` to a neutral module such as `ovrley_core/src/paths.rs`. Neither `commands` nor `encode` should depend on each other.

---

# 4. Target Architecture

## 4.1 Target Core Layout

```
ovrley_core/
├── src/
│   ├── lib.rs
│   ├── error.rs              # CoreError and CoreResult
│   ├── types.rs              # MetricKind and shared finite-domain enums
│   ├── paths.rs              # AppPaths (moved from commands)
│   ├── rdp.rs                # Shared RDP simplification
│   ├── interpolation.rs      # Shared interpolation (consolidated)
│   ├── activity/
│   │   ├── mod.rs
│   │   ├── schema.rs
│   │   ├── trim.rs
│   │   └── tests/
│   ├── config/
│   │   ├── mod.rs
│   │   └── tests/
│   ├── commands/
│   │   ├── mod.rs
│   │   └── tests/
│   ├── debug/
│   │   └── mod.rs
│   ├── encode/
│   │   ├── mod.rs
│   │   ├── ffmpeg.rs         # ffmpeg discovery
│   │   ├── ffmpeg_settings.rs# NEW (TBD): ffmpeg command/settings derivation
│   │   ├── ffmpeg_composite.rs
│   │   ├── ffmpeg_composite_profiles.rs # EXISTING: composite encoder templates
│   │   ├── fps.rs
│   │   ├── progress.rs       # EXISTING: ProgressEstimator (RenderController still in video.rs)
│   │   ├── video.rs
│   │   ├── video_composite_debug.rs     # EXISTING: composite debug summaries
│   │   ├── video_composite_pipeline.rs
│   │   ├── video_probe.rs
│   │   ├── video_debug.rs
│   │   └── tests/
│   └── render/
│       ├── mod.rs
│       ├── format.rs
│       ├── surface.rs
│       ├── text.rs
│       └── widgets/
│           ├── mod.rs
│           ├── types.rs
│           ├── common.rs     # reduced: only shared constants + tiny helpers
│           ├── geometry.rs   # NEW: point/rect/fitting/layout helpers
│           ├── marker.rs     # NEW: marker/dot drawing
│           ├── polyline.rs   # NEW: polyline/area drawing
│           ├── transform.rs  # NEW: rotation/projection transforms
│           ├── value.rs
│           ├── route.rs
│           ├── elevation.rs
│           └── tests/
└── tests/
    ├── common/
    │   └── mod.rs            # fixture helpers
    ├── fixtures/
    │   ├── config/
    │   ├── activity/
    │   ├── ffprobe/
    │   └── expected/
    ├── config_tests.rs
    ├── activity_tests.rs
    ├── ffmpeg_command_tests.rs
    ├── render_plan_tests.rs
    └── video_probe_tests.rs
```

## 4.2 Re-Export Strategy

```rust
// lib.rs
pub mod activity;
pub mod commands;
pub mod config;
pub mod debug;
pub mod encode;
pub mod error;
pub mod render;
pub mod types;
pub mod paths;

pub use error::{CoreError, CoreResult};
pub use types::MetricKind;
```

Avoid re-exporting internal helper modules unless they are part of the public core API.

## 4.3 DTO vs Domain Types

When input boundary structs become too loose, consider splitting:

```rust
RawRenderConfig    // serde input from frontend (with serde(default))
RenderConfig       // validated domain config
RenderPlan         // derived immutable execution plan
```

Do not do this everywhere automatically. Use it only when it improves correctness, clarity, or validation.

---

# 5. Refactor Phases

## Phase 1 — Test Safety Net and Test Migration

**Purpose:** Create regression safety before changing architecture. This phase comes first.

### Deliverables

**1. Move all tests into `tests/` directories**

Remove inline `#[cfg(test)] mod tests { ... }` and `#[path = "tests/..."] mod tests;` declarations from source files. Move tests to:

```
ovrley_core/tests/                    # crate-level integration (preferred)
ovrley_core/src/*/tests/              # module-local if internals needed
```

Files to migrate:

| Current Location | Source Line |
|-----------------|-------------|
| `config/mod.rs` | `#[path = "tests/config_tests.rs"] mod tests;` at line 614 |
| `commands/mod.rs` | `#[path = "tests/commands_tests.rs"] mod tests;` at line 652 |
| `encode/fps.rs` | `#[path = "tests/fps_tests.rs"] mod tests;` at line 104 |
| `encode/ffmpeg_composite.rs` | `#[path = "tests/ffmpeg_composite_tests.rs"] mod tests;` at line 228 |
| `encode/video_composite_pipeline.rs` | `#[path = "tests/video_composite_pipeline_tests.rs"] mod tests;` at line 549 |
| `activity/mod.rs` | inline `#[cfg(test)] mod tests {}` at line 57 |
| `render/format.rs` | inline `#[cfg(test)] mod tests {}` at line 500 |
| `render/widgets/value.rs` | inline `#[cfg(test)] mod tests {}` at line 814 |
| `video_server.rs` | inline `#[cfg(test)] mod tests {}` at line 461 |

**2. Add fixture helper infrastructure**

```rust
// tests/common/mod.rs
use std::path::{Path, PathBuf};

pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join(relative)
}
```

**3. Add snapshot/golden tests for:**

- config parsing and normalization
- activity parsing (GPX, FIT fixtures)
- interpolation/densification (frame counts at common FPS values)
- frame counting (integer and non-integer durations)
- ffmpeg command generation (transparent, composite, qtrle, prores)
- composite pipeline planning (timing derivation, overrun guard)
- video probe metadata extraction (from stored ffprobe JSON)
- RDP simplification output

**4. Add cancellation lifecycle tests where feasible:**

```rust
// Test: start -> cancel -> progress state -> cleanup -> no stale running state
```

**5. Capture performance baseline:**

- representative preview render (ms/frame)
- representative transparent export
- representative composite export

### Manual Tests After Phase 1

- app starts
- preview rendering works
- transparent overlay export works
- composite MP4 export works
- video import/probe still works
- cancellation still works
- progress UI still updates
- output paths and filenames unchanged

### Completion Criteria

- all tests pass and are discoverable through `cargo test`
- no production behavior changed
- no source file contains inline `#[cfg(test)] mod tests`
- no source file contains `#[path = "tests/..."] mod tests`

---

## Phase 2 — Typed Errors and Typed Metrics

**Purpose:** Replace weak string-based boundaries with structured types.

### Deliverables

**1. Add `error.rs`**

Introduce a flat `CoreError` (for a mid-sized crate, nested sub-errors are unnecessary complexity):

```rust
use std::path::PathBuf;
use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Activity parse error: {0}")]
    Activity(String),

    #[error("Render error: {0}")]
    Render(String),

    #[error("Encoding error: {0}")]
    Encode(String),

    #[error("IO error at {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },

    #[error("FFmpeg error (exit {status}): {stderr}")]
    Ffmpeg { status: std::process::ExitStatus, stderr: String },

    #[error("FFmpeg not found: {0}")]
    FfmpegNotFound(String),

    #[error("Render cancelled")]
    Cancelled,

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

// Avoid adding an `Other(String)` catch-all — if used temporarily, add TODO to migrate later.
```

**2. Migrate errors incrementally**

Recommended order (each step = one PR):
```
config -> activity -> render helpers -> encode helpers -> commands -> Tauri boundary
```

**3. Preserve Tauri boundary strings**

```rust
#[tauri::command]
async fn backend_render(...) -> Result<String, String> {
    commands::backend_render(...)
        .map_err(|error| error.to_string())
        .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
}
```

**4. Add `MetricKind` enum**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    #[serde(rename = "speed")]       Speed,
    #[serde(rename = "heartrate")]   Heartrate,
    #[serde(rename = "elevation")]   Elevation,
    #[serde(rename = "time")]        Time,
    #[serde(rename = "gradient")]    Gradient,
    #[serde(rename = "cadence")]     Cadence,
    #[serde(rename = "power")]       Power,
    #[serde(rename = "temperature")] Temperature,
}
```

**5. Move metric behavior onto `MetricKind`**

```rust
impl MetricKind {
    pub fn requirements(self) -> RenderDataRequirements { /* ... */ }
    pub fn icon(self) -> Option<MetricIconKind> { /* ... */ }
    pub fn format(self, config, value_config, dense, frame_index) -> String { /* ... */ }
}
```

### Manual Tests After Phase 2

- old frontend configs still parse
- existing saved templates still parse
- unknown metrics produce clear errors
- frontend receives readable error messages (not raw Rust debug output)
- rendering output unchanged
- ffmpeg commands unchanged

### Completion Criteria

- no unnecessary behavior changes
- high-value functions use typed errors
- metric string matching eliminated from business logic (retained only at serde boundary)
- serde compatibility tests pass

---

## Phase 3 — Module Cleanup and Duplication Reduction

**Purpose:** Reduce duplication and improve cohesion before deeply restructuring pipelines.

### Deliverables

**1. Extract shared RDP logic into `rdp.rs`**

Before extraction: add snapshot tests for route simplification and elevation simplification.

Extract as simple free functions:

```rust
// rdp.rs
pub fn perpendicular_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 { ... }
pub fn simplify_rdp(points: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> { ... }
```

Only introduce a `Point2D` trait if more than tuple points are needed. Prefer simpler.

**2. Consolidate interpolation logic**

Move authoritative `f64`-based interpolation from `activity/interpolate.rs` to a shared `ovrley_core/src/interpolation.rs`. Have `render/widgets/common.rs` delegate to it instead of maintaining a separate `f32` copy.

**3. Split `render/widgets/common.rs`**

Suggested extraction targets (create only when cohesion improves):

```
geometry.rs     # point/rect/math helpers
marker.rs       # marker/dot drawing
polyline.rs     # polyline/area drawing
transform.rs    # rotation/projection transforms
fit.rs          # layout/fitting helpers
```

Reduce `common.rs` from 774 lines to ~200 lines (shared constants + tiny helpers only).

**4. Consolidate CLI helpers**

Create `src-tauri/src/bin/common.rs`:

```rust
pub fn read_arg(flag: &str, args: &[String]) -> anyhow::Result<String> { ... }
pub fn read_optional_arg(flag: &str, args: &[String]) -> Option<String> { ... }
pub fn repo_root() -> anyhow::Result<PathBuf> { ... }
```

Diagnostic binaries may use `anyhow`. Core library code must use typed errors.

**5. Remove commented-out debug code**

Replace useful diagnostics with `tracing::debug!`. Delete stale comments.

**6. Integrate new composite modules into refactor**

`encode/ffmpeg_composite_profiles.rs` and `encode/video_composite_debug.rs` already exist. Both use `Result<T, String>` and need typed error migration. Additionally:

- `ffmpeg_composite_profiles.rs` — well-structured data-driven profile table; migrate errors, consider snapshot tests for profile resolution.
- `video_composite_debug.rs` — composite-only debug summary writer; migrate to `CoreResult`, move inline tests if any are added.
- Consider whether `video_composite_debug.rs` should be folded into `video_debug.rs` with shared debug logic, or kept separate as a composite-specific sibling.

**7. Move `AppPaths` to neutral module**

Extract `AppPaths` from `commands/mod.rs` into `ovrley_core/src/paths.rs`. Update all imports.

### Manual Tests After Phase 3

- route widget output unchanged
- elevation widget output unchanged
- rendered preview unchanged
- diagnostic binaries still work
- no measurable hot-path regression

### Completion Criteria

- duplicated RDP logic removed
- interpolation consolidated into single source of truth
- `common.rs` responsibilities reduced
- `AppPaths` moved to neutral module
- CLI boilerplate eliminated
- tests pass, behavior preserved

---

## Phase 4 — Pipeline and Orchestration Cleanup

**Purpose:** Separate planning, orchestration, rendering, encoding, progress, and cancellation.

### Deliverables

**1. Extract ffmpeg settings construction into `ffmpeg_settings.rs`**

Move `build_ffmpeg_settings`, `FfmpegSettings`, and supporting helpers from `ffmpeg.rs` into `ffmpeg_settings.rs`. This makes ffmpeg command snapshot tests easier.

**2. Progress estimation and cancellation ownership**

`encode/progress.rs` already exists with a `ProgressEstimator` (EMA-based FPS/ETA calculation) but `RenderController` and `RenderProgress` remain in `encode/video.rs`. Two sub-deliverables:

- **2a.** Move `RenderController` and `RenderProgress` from `encode/video.rs` into `encode/progress.rs` to complete the separation.
- **2b.** Review `ProgressEstimator` — it has inline `#[cfg(test)] mod tests` that must be moved to a `tests/` directory.

Document lifecycle:
```
Idle -> Running -> Completed
                -> Failed
                -> Cancelled
```

**3. Use request structs for large signatures**

Replace functions with 7+ parameters:

```rust
// Before:
pub fn render_preview_with_prepared_assets(
    paths: &AppPaths, config: &RenderConfig, dense_activity: &DenseActivityReport,
    assets: &PreparedPreviewAssets, second: u32, prepare_timings: BTreeMap<String, TimingBucket>,
    label_cache_status: LabelCacheStatus, extra_total_ms: f64, out_path: &Path,
) -> CoreResult<PreviewRenderReport>

// After:
pub struct PreviewRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub assets: &'a PreparedPreviewAssets,
    pub second: u32,
    pub prepare_timings: BTreeMap<String, TimingBucket>,
    pub label_cache_status: LabelCacheStatus,
    pub extra_total_ms: f64,
    pub out_path: &'a Path,
}

pub fn render_preview_with_prepared_assets(
    request: PreviewRenderRequest<'_>,
) -> CoreResult<PreviewRenderReport>
```

**Do not add a `.render()` method unless it genuinely improves the API. Avoid builder pattern unless fields are optional or defaults are meaningful.**

Functions to refactor:

| Current | Suggested Struct |
|---------|-----------------|
| `render_preview_with_prepared_assets` (9 args) | `PreviewRenderRequest` |
| `render_frame_rgba` (8 args) | `FrameRenderRequest` |
| `render_frame_to_surface` (10 args) | `FrameRenderRequest` |
| `draw_metric_value_widget_with_config` (10 args) | `MetricWidgetRequest` |

**4. Separate planning from execution**

Target conceptual structure:

```
RenderRequest -> RenderPlan -> ExecuteRenderPlan
```

Planning should be easy to test without running ffmpeg. Execution may own IO/process/thread behavior.

**5. Keep sibling pipelines independent**

`video_pipeline.rs` and `video_composite_pipeline.rs` must not import each other. If they share logic, extract to `encode/ffmpeg_settings.rs` or `encode/shared.rs`.

### Manual Tests After Phase 4

- transparent export works
- composite export works
- cancellation works (no orphan processes)
- progress reporting works
- ffmpeg child process cleanup works
- snapshot ffmpeg commands unchanged

### Completion Criteria

- orchestration functions smaller and clearer
- process ownership documented
- progress/cancellation ownership documented
- no `video_pipeline` / `video_composite_pipeline` cross-import
- no `#[allow(clippy::too_many_arguments)]` remaining

---

## Phase 5 — Cache and State Management

**Purpose:** Make hidden state explicit where it improves testability or correctness.

### Deliverables

**1. Audit all global caches**

Identify:
- what is cached (typeface references, rendered label images)
- why it is cached (performance — font loading and label rendering are expensive)
- whether it is bounded (unbounded — grows with unique configs/fonts)
- how it is reset in tests (not reset — state leaks between tests)
- whether it affects output (yes for labels, no for fonts — fonts affect rendering)
- whether it affects performance (yes, significantly)

**1b. Audit composite-only state**

`video_composite_debug.rs` writes debug artifacts to `debug_render/phase_7/` on every composite render. This is intentional diagnostic output, not cache state — but the directory naming convention and cleanup policy should be documented.

**2. Introduce explicit render context only if justified**

```rust
pub struct RenderContext {
    pub font_cache: FontCache,
    pub label_cache: LabelCache,
}
```

Do not add TTL/LRU/max-entry logic unless required.

**3. Add cache metrics only if useful**

Potential metrics: hit count, miss count, cache size. Do not add metrics if they complicate hot paths (cache lookups happen on every frame).

### Manual Tests After Phase 5

- preview render unchanged
- exports unchanged
- memory usage acceptable
- no test state leakage
- no performance regression

### Completion Criteria

- cache ownership is clear
- hidden global state reduced or justified
- tests remain deterministic

---

## Phase 6 — Documentation and Final Polish

### Deliverables

**1. Add module-level docs for major modules**

Each major module should explain:
- what it owns
- what it does not own
- allowed dependencies
- related modules
- threading/lifecycle assumptions

```rust
//! # Encode Pipeline
//!
//! Owns ffmpeg process orchestration, queue management, progress reporting,
//! and encode pipeline execution.
//!
//! Does not own widget rendering internals or configuration schema validation.
//!
//! Allowed dependencies: config, activity, render, debug.
//! Forbidden dependencies: commands.
//!
//! ## Thread Safety
//! RenderController is Send + Sync (uses Arc<Mutex>).
//! ffmpeg child processes are !Send and owned by the spawning thread.
```

**2. Add public API docs for meaningful `pub` items**

Include: purpose, errors, panics, performance assumptions (if hot path), threading assumptions (if shared state). Do not document trivial getters.

**3. Add threading and lifecycle docs**

For shared controllers, queues, and cancellation state, document:
- ownership
- state transitions
- thread safety
- shutdown behavior

**4. Fix `cfg!(debug_assertions)` → `#[cfg(debug_assertions)]`**

In `src-tauri/src/lib.rs:348`, replace the runtime check with a compile-time attribute so the log plugin is excluded from release builds.

**5. Run final checks**

```bash
cargo fmt
cargo test
cargo clippy -- -D warnings
cargo deny check   # if configured
```

### Manual Tests After Phase 6

Verify all user-facing flows:
- import video, preview overlay, scrub preview
- render transparent overlay, render composite MP4
- cancel render, inspect progress
- run diagnostic binaries

### Completion Criteria

- `cargo fmt`, `cargo test`, `cargo clippy -- -D warnings` all pass
- all public APIs documented
- threading/lifecycle documented for shared state
- no `cfg!(debug_assertions)` remaining
- no dead code
- no commented-out `println!` statements

---

# 6. Testing Strategy

## 6.1 Test Categories

| Priority | Area | Examples |
|----------|------|----------|
| P0 | Config parsing | Valid configs, invalid configs, defaults, composite fields |
| P0 | Activity parsing | GPX/FIT-derived JSON, missing fields, debug payload wrapper |
| P0 | Interpolation | Frame timestamps, dense reports, 29.97/59.94 FPS |
| P0 | Frame counting | fps, duration, integer/non-integer windows |
| P1 | FFmpeg commands | Transparent (prores, qtrle, vulkan), composite (libx264) |
| P1 | Render plans | Preview, export, composite plan derivation |
| P1 | Cancellation | Start/cancel/reset lifecycle, progress states |
| P2 | Video probe | Metadata extraction, creation time priority fallback |
| P2 | HTTP server | Range headers (full, partial, suffix, unsatisfiable), 404, 416 |
| P3 | Visual regression | Representative rendered frames (optional, manual) |

## 6.2 Snapshot Testing

Use snapshot/golden testing for deterministic text/JSON outputs. Good candidates:

- ffmpeg command arrays (as strings or `Vec<String>`)
- ffmpeg settings structs (serialize to JSON)
- normalized configs
- render plans
- dense activity reports for small fixtures
- video probe results from stored ffprobe JSON

Example patterns:

```rust
// Direct assertion:
let command = build_ffmpeg_settings(&config);
assert_eq!(command.loglevel, "info");
assert!(command.output_args.contains(&"-c:v".to_string()));

// Snapshot library (insta):
let plan = serde_json::to_string_pretty(&plan)?;
insta::assert_snapshot!(plan);
```

## 6.3 Fixture Strategy

All tests share fixture paths through a single `tests/common/test_config.rs` file (see [Rule 2.9](#29-all-tests-must-share-a-common-test-config)). This prevents scattered `parent().unwrap()` chains and makes redirecting all test data as easy as editing one file.

```
ovrley_core/tests/
├── common/
│   ├── mod.rs
│   └── test_config.rs       # Shared fixture path resolver
├── fixtures/
│   ├── config/
│   │   ├── simple.json
│   │   ├── composite.json
│   │   └── invalid.json
│   ├── activity/
│   │   ├── gpx-parse-debug.json
│   │   └── fit-parse-debug.json
│   ├── ffprobe/
│   │   ├── 4k.json
│   │   └── 1080p.json
│   └── video/
│       └── sample.mp4       # Representative MP4 for probe/composite tests
├── config_tests.rs
├── activity_tests.rs
├── ffmpeg_command_tests.rs
├── render_plan_tests.rs
└── video_probe_tests.rs
```

The test config pattern (required by rule 2.9):

```rust
// tests/common/test_config.rs
use std::path::PathBuf;

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures")
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

## 6.4 What Not To Unit Test

- arbitrary Skia pixel output (use dedicated visual regression tooling if needed)
- full real ffmpeg encoding in normal unit tests (use integration tests with short renders)
- Tauri wrapper internals (test the command helpers instead)

Instead: test render planning, ffmpeg command construction, and process wrappers with controlled fakes where possible.

## 6.5 Tests Must Protect Behavior, Not Implementation Style

Tests should verify: same output, same derived plans, same ffmpeg commands, same timing calculations, same metadata extraction — not overfit to temporary internal function names.

---

# 7. Error Handling Strategy

## 7.1 Core Rule

Core library code must not return raw `String` errors. Use `CoreResult<T>` and convert to strings only at UI/Tauri boundaries.

## 7.2 Flat Error First

For this crate, a flat `CoreError` is acceptable initially. Do not create many sub-error enums until the domain demands it. If a domain grows enough, split later into `ConfigError`, `RenderError`, `EncodeError` — but do not overcomplicate the first migration.

## 7.3 Add Context to IO Errors

```rust
// Good:
CoreError::Io { path: path.to_path_buf(), source }

// Bad:
std::io::Error  // no path context
```

## 7.4 Avoid Meaningless `Other(String)` Catch-All

If used temporarily during migration, add a TODO comment and migrate later.

## 7.5 Tauri Boundary Conversion

```rust
#[tauri::command]
async fn backend_render(...) -> Result<String, String> {
    commands::backend_render(...)
        .map_err(|error| error.to_string())
}
```

Do not leak internal error implementation details to the frontend unless intended. The `Display` impl of `CoreError` should produce user-readable messages.

---

# 8. Module Architecture and Dependency Boundaries

## 8.1 Dependency Direction

```
config -> activity -> render -> encode
                    \-> commands
```

**Forbidden:**
- `render` importing `encode`
- pipelines importing sibling pipelines
- `config` importing runtime systems
- `commands` becoming a dumping ground for encode logic
- `encode` depending on `commands`

## 8.2 Import Matrix

| Module | May Import | Must Not Import |
|--------|-----------|----------------|
| `config` | std, serde, local helpers, `error`, `types` | `activity`, `render`, `encode`, `commands` |
| `activity` | `config`, `error`, `types`, `debug` (if justified) | `render`, `encode`, `commands` |
| `render` | `config`, `activity`, `debug`, `error`, `types` | `encode`, `commands` |
| `render::widgets` | `render::format`, `render::text`, `config`, `activity` | `encode`, `commands` |
| `encode::video_pipeline` | `config`, `activity`, `render`, `encode::ffmpeg`, `encode::ffmpeg_settings`, `encode::fps`, `encode::progress`, `error` | `commands`, `encode::video_composite_pipeline` |
| `encode::video_composite_pipeline` | `config`, `activity`, `render`, `encode::ffmpeg`, `encode::ffmpeg_composite`, `encode::fps`, `encode::progress`, `error` | `commands`, `encode::video_pipeline` |
| `encode::video` (orchestrator) | Everything in `encode/` + `config`, `activity`, `commands`, `error` | `render` directly (go through `video_pipeline`) |
| `commands` | `config`, `activity`, `render`, `encode`, `debug`, `error` | Tauri-specific APIs |
| `debug` | std, serde | `config`, `render`, `encode` (unless justified) |
| `src-tauri/src/lib.rs` | `ovrley_core`, `video_server`, tauri | Core implementation details |

## 8.3 Crate Split Rule

Do not split into separate crates (`encoding_transparent`, `encoding_composite`, `rendering_cpu`, `rendering_gpu`).

**Reasons:**
- shared types (`AppPaths`, `RenderConfig`, `DenseActivityReport`) would require yet another crate
- pipelines share render/config/activity concepts — the dependency graph becomes a DAG of crates
- GPU path is future-facing — premature abstraction
- crate split would increase build complexity before boundaries are stable

Use module boundaries first. If GPU rendering materializes, introduce a `trait OverlayRenderer { fn render_frame(...) }` within the existing crate, then optionally extract later.

## 8.4 Feature Flags

Do not add feature flags unless there is a real build/test/deployment reason (e.g., compile-time exclusion of composite pipeline for faster CI). If needed:

```toml
[features]
default = ["encode-transparent", "encode-composite"]
encode-transparent = []
encode-composite = []
```

But defer until proven useful.

## 8.5 Commands Boundary

`commands` must be framework-agnostic. `src-tauri/src/lib.rs` owns Tauri-specific concerns. `ovrley_core::commands` must not import Tauri.

If a command helper becomes large, move domain logic into the relevant module and keep only orchestration in `commands`.

---

# 9. Pipeline, Threading, Cancellation, and Ownership

## 9.1 Explicit Ownership Required

The code must clearly define ownership of:

- ffmpeg child process
- stdin pipe to ffmpeg
- stderr monitor thread
- writer thread (ffmpeg stdin)
- render worker thread
- frame queue (bounded channel)
- cancellation flag (`AtomicBool`)
- progress state (`Arc<Mutex<RenderProgress>>`)
- temporary output file (segment renders)
- final output file

## 9.2 Cancellation Must Be Complete

Cancellation must:

1. Set cancellation flag (preventing new frames from being rendered)
2. Stop sending frames to ffmpeg
3. Close ffmpeg stdin intentionally (so ffmpeg can finalize output)
4. Wait for ffmpeg to exit (with timeout, then kill)
5. Join worker threads (render, writer, monitor)
6. Update progress state to `cancelled`
7. Clean up partial output files
8. Reset `running` state (allowing subsequent renders)

**Must not:**
- orphan ffmpeg child processes
- deadlock on join (use timeouts)
- leave `running = true` after failure

## 9.3 Process Lifecycle Documentation

Any function spawning ffmpeg must document:

- who owns the child process
- who writes to stdin
- who reads stderr
- who waits for exit
- what happens on cancellation
- what happens on writer thread panic
- what happens on ffmpeg failure
- how temporary files are cleaned up

## 9.4 Progress Updates

Progress updates should:
- be thread-safe (use atomic counter for encoded frames, mutex for full state snapshot)
- avoid high-frequency lock contention (don't lock per-frame, batch updates)
- avoid per-frame expensive formatting (format strings only on state change)
- be reset on completion/error/cancel
- expose clear state transitions

State model:
```
Idle -> Running -> Completed
                -> Failed
                -> Cancelled
```

## 9.5 Thread Shutdown

- Writer threads must exit when the channel closes (sender dropped).
- Monitor threads must exit when stderr EOF is reached (ffmpeg exits).
- Render threads must check cancellation flag between frames (not mid-frame).
- Thread join should use reasonable timeouts to detect hangs.

---

# 10. Logging and Diagnostics

## 10.1 No `println!` in Core Library Logic

Use structured logging from the `log` crate (already a dependency, or add `tracing`).

```rust
// Good:
log::info!("Render started: {render_id}");
log::warn!("FFmpeg exited with non-zero status: {status}");

// Bad:
println!("Render started");
eprintln!("FFmpeg failed");
```

Diagnostic binaries may print user-facing output (e.g., timing summaries to stdout), but core library code must not use ad-hoc prints.

## 10.2 Include Useful Context

Good context for log messages:
- render id
- pipeline type (transparent, composite)
- input/output paths
- frame index (for frame-level diagnostics)
- ffmpeg exit status
- cancellation reason
- probe source used for creation time (for `video_probe.rs`)

```rust
log::warn!(
    "Video creation time missing for {}; falling back to file metadata",
    path.display()
);
```

## 10.3 Avoid Hot-Path Log Spam

Do not log every frame, interpolation, cache lookup, or queue operation unless explicitly debug-gated behind a configuration flag.

## 10.4 Replace Commented Debug Code

```rust
// BAD:
// println!("probing video: {path}");
// println!("[OVRLEY] Final selected creation time: {:?}", metadata.creation_time);

// GOOD:
log::debug!("Probing video: {}", path);
// or remove entirely
```

---

# 11. Performance and Hot-Path Rules

## 11.1 Hot Path Examples

Treat these as hot paths:
- frame rendering (`render_frame_rgba`, `render_frame_to_surface`)
- widget drawing (`draw_route_widget`, `draw_elevation_widget`, `draw_metric_parts`)
- activity interpolation (`interpolate_points`, `densify_activity`)
- ffmpeg stdin writing (`writer_worker`)
- queue send/receive loops (`queue_frame`, `acquire_frame_buffer`)
- text layout if called per frame
- route/elevation simplification if called per-frame (currently precomputed — keep it that way)

## 11.2 Avoid in Hot Paths

Avoid:
- unnecessary heap allocation (reuse buffers via pool)
- unnecessary cloning (pass references where possible)
- repeated `String` formatting (format once, cache)
- repeated serde operations (serialize once)
- repeated `HashMap` construction (reuse or cache)
- repeated font lookup (already cached — maintain this)
- unnecessary mutex locks (use atomics where possible)
- dynamic dispatch without proven need

## 11.3 Benchmark Before/After

For render/encode changes, capture:
- ms/frame
- total export time
- memory usage (process RSS)
- queue wait time
- encode time (ffmpeg wall clock)

## 11.4 Performance Is a Refactor Constraint

A refactor that makes code cleaner but significantly slower is not acceptable unless explicitly justified.

If performance changes, document:
```
Performance impact:
- Before: 12ms/frame, 30s export
- After:  14ms/frame, 35s export
- Reason: additional bounds check in interpolation
- Accepted? No — refactor to avoid regression
```

---

# 12. Code Style and Refactor Patterns

## 12.1 Request Structs Over Builders

**Bad:**

```rust
#[allow(clippy::too_many_arguments)]
pub fn render_preview(paths: &AppPaths, config: &RenderConfig, ..., out_path: &Path) -> ...
```

**Better (plain struct):**

```rust
pub struct PreviewRenderRequest<'a> {
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub activity: &'a DenseActivityReport,
    pub assets: &'a PreparedPreviewAssets,
    pub second: u32,
    pub out_path: &'a Path,
}

pub fn render_preview(request: PreviewRenderRequest<'_>) -> CoreResult<PreviewRenderReport>
```

**Do not add builder APIs unless optional/default fields justify them.** Simple request structs are preferred — they are clear, testable, and require no boilerplate.

## 12.2 DTO vs Domain Types

If boundary structs become too loose, split:

```rust
RawRenderConfig    // serde input from frontend (relaxed validation)
RenderConfig       // validated domain config (invariants guaranteed)
RenderPlan         // derived immutable execution plan
```

Do not do this everywhere automatically. Use it where it improves correctness.

## 12.3 Use Enums for Finite Domains

Use enums for:
- metric kinds (`MetricKind`)
- codec choices (replace string matching in `build_ffmpeg_settings`)
- export format, render state (`RenderProgress.status`)
- pipeline mode (transparent vs composite)

Keep strings only at serialization boundaries.

## 12.4 Import Order

```rust
// 1. Standard library
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// 2. External crates (alphabetical)
use serde::{Deserialize, Serialize};
use thiserror::Error;

// 3. Internal crate modules
use crate::config::RenderConfig;
use crate::error::CoreResult;
```

## 12.5 Trait Derives

Derive only what is needed:

```rust
// Good for data types:
#[derive(Clone, Debug, PartialEq)]
pub struct ByteRange { pub start: u64, pub end: u64 }

// Only at serialization boundaries:
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderConfig { ... }
```

Do not derive `Serialize`, `Deserialize`, or `Clone` everywhere by default. Boundary DTOs need serde; runtime execution objects often should not.

---

# 13. Best Practices with Examples

## 13.1 Typed Metrics

**Bad (current):**

```rust
match value.value.as_str() {
    "speed" => requirements.speed = true,
    "elevation" => requirements.elevation = true,
    _ => {}
}
```

**Better:**

```rust
match value.metric {
    MetricKind::Speed => requirements.speed = true,
    MetricKind::Elevation => requirements.elevation = true,
    MetricKind::Gradient => {
        requirements.distance = true;
        requirements.elevation = true;
    }
    _ => {}
}
```

## 13.2 Shared Algorithms

**Bad (before refactor):**

```rust
// route.rs
fn perpendicular_distance(point, start, end) -> f32 { /* ... */ }

// elevation.rs
fn perpendicular_distance(point, start, end) -> f32 { /* IDENTICAL */ }
```

**Better (after refactor):**

```rust
// rdp.rs
pub fn perpendicular_distance(point: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 { /* ... */ }

// route.rs and elevation.rs call rdp::perpendicular_distance
```

Only extract after tests exist and prove behavior preservation.

## 13.3 Compile-Time vs Runtime Feature Gating

```rust
// If code should not compile into release:
#[cfg(debug_assertions)]
{
    app.handle().plugin(log_plugin)?;
}

// If both branches are safe to compile and only runtime behavior differs:
if is_development_environment() { ... }
```

## 13.4 Keep Planning Testable

```rust
// Prefer:
let plan = build_render_plan(&request)?;
let command = build_ffmpeg_command(&plan)?;

// Over:
spawn_ffmpeg_and_render_everything_in_one_function(&request)?;
```

## 13.5 Preserve JSON Compatibility

When replacing strings with enums, ensure serde names match existing frontend data:

```rust
// Must match the frontend serialization exactly:
#[serde(rename = "heartrate")]
Heartrate,    // NOT "heartRate" unless frontend is migrated intentionally
```

## 13.6 Parameter Object Pattern

When a function has 7+ parameters that always appear together, bundle them into a struct:

```rust
// Before (render_frame_to_surface: 10 params):
fn render_frame_to_surface(
    canvas, paths, config, dense_activity, prepared_assets,
    frame_index, scale, labels_image, base_layer_restored, frame_profiler,
) -> ...

// After:
pub struct FrameRenderContext<'a> {
    pub canvas: &'a skia_safe::Canvas,
    pub paths: &'a AppPaths,
    pub config: &'a RenderConfig,
    pub dense_activity: &'a DenseActivityReport,
    pub prepared_assets: &'a PreparedRenderAssets,
    pub frame_index: usize,
    pub scale: f32,
    pub labels_image: Option<&'a Image>,
    pub base_layer_restored: bool,
    pub frame_profiler: &'a mut RenderProfiler,
}
```

---

# 14. Antipatterns to Avoid

## 14.1 Giant Orchestration Functions

**Antipattern:** A single function that does all of: parse config, prepare assets, spawn ffmpeg, start threads, render frames, write frames, monitor progress, handle cancellation, clean files. Example: `render_video_single` (lines 53-236 in `video_pipeline.rs`).

**Fix:** Split into planning, orchestration, execution, monitoring, and cleanup phases.

## 14.2 Stringly-Typed Business Logic

**Antipattern:** Repeated `match some_string.as_str()` for known finite domains across 5+ files.

**Fix:** Use enums with serde rename attributes for JSON compatibility.

## 14.3 Over-Abstracted Builders

**Antipattern:** Builder pattern for simple parameter grouping where all fields are required.

**Fix:** Use a plain struct with named fields. Add a builder only when fields are optional or defaults are meaningful.

## 14.4 Arbitrary Line Count Caps

**Antipattern:** Splitting files purely because they exceed an arbitrary line count (e.g., "max 200 lines").

**Fix:** Split when responsibilities diverge, testing becomes difficult, imports become incoherent, or navigation becomes difficult — not because of a number.

## 14.5 Hidden Mutable Global State

**Antipattern:** `static CACHE: OnceLock<Mutex<HashMap<...>>>` with no documented ownership, no reset mechanism, unbounded growth.

**Fix:** Either justify why it's acceptable (document reasoning), or make ownership explicit through an injectable context struct.

## 14.6 Misleading Error Messages

**Antipattern:**

```rust
"Failed to read import ID"
// when the actual failure is URL parsing
```

**Fix:** Describe the actual failure, not the symptom:

```rust
format!("Preview URL {preview_url} has no valid import id segment")
```

## 14.7 Swallowing Semantics With `unwrap_or_default`

**Antipattern:**

```rust
value.as_str().unwrap_or_default().to_ascii_lowercase()
```

 `None` and `""` (Some("")) may have different meanings. Handle the distinction explicitly.

## 14.8 Test Wiring in Production Source

**Antipattern:**

```rust
#[cfg(test)]
#[path = "tests/whatever.rs"]
mod tests;
```

The production file must not know about test file layout.

## 14.9 Detached Threads Without Shutdown Ownership

**Antipattern:**

```rust
std::thread::spawn(move || { loop { ... } });
// handle discarded — thread is fire-and-forget
```

**Fix:** Join handles must be stored, cancellable, or intentionally detached with documented lifecycle.

## 14.10 Per-Frame Logging

**Antipattern:**

```rust
for frame in frames {
    log::debug!("rendering frame {frame}");  // thousands of lines per render
}
```

**Fix:** Log at coarser granularity (per-second, per-10%, or on state change) unless explicitly debugging.

---

# 15. Documentation Standards

## 15.1 Module Documentation

Every `mod.rs` (and every module root) should include:

```rust
//! # Module Name
//!
//! Owns: [what this module owns]
//! Does not own: [what related modules own instead]
//!
//! Allowed dependencies: [list of modules this may import]
//! Forbidden dependencies: [list of modules this must NOT import]
//!
//! Related modules: [cross-references to sibling/consumer modules]
//!
//! ## Thread Safety
//! [thread safety characteristics, if shared state]
//!
//! ## Performance
//! [hot path or not, allocation characteristics, caching strategy]
```

## 15.2 Function Documentation

Document public functions when they are meaningful API. Include:
- purpose (what the function does)
- errors (which error variants can be returned and why)
- panics (if any — ideally none)
- performance assumptions (if hot path)
- threading assumptions (if shared state)

```rust
/// Parses and validates render configuration JSON.
///
/// # Errors
///
/// Returns [`CoreError::Config`] if:
/// - JSON is malformed
/// - required scene fields are missing
/// - fps/update-rate constraints are invalid
///
/// # Performance
///
/// O(n) in number of config fields. Called once per render.
/// Not a hot path.
pub fn parse_config_json(input: &str) -> CoreResult<RenderConfig> { ... }
```

## 15.3 Type Documentation

Document types that encode important state or ownership:

```rust
/// Shared render state for progress polling and cancellation.
///
/// Clones share the same underlying progress state via `Arc<Mutex>`.
/// Only one render may be active at a time.
///
/// # State Transitions
///
/// ```text
/// Idle -> Running -> Completed
///                 -> Failed
///                 -> Cancelled
/// ```
#[derive(Clone)]
pub struct RenderController { ... }
```

## 15.4 Intra-Doc Links

Use intra-doc links for related core flows:

```rust
/// Uses [`trim_activity`](crate::activity::trim::trim_activity)
/// before building the dense frame-aligned report via
/// [`densify_activity`](crate::activity::interpolate::densify_activity).
```

## 15.5 `#[must_use]`

Use where ignoring the return value is probably a bug:

```rust
#[must_use]
pub fn progress(&self) -> RenderProgress { ... }

#[must_use]
pub fn cancel(&self) -> bool { ... }
```

Do not add `#[must_use]` everywhere mechanically.

## 15.6 Avoid Documentation Noise

Avoid:

```rust
/// Gets the width.
pub fn width(&self) -> u32
```

Document meaning, invariants, lifecycle, and non-obvious behavior — not trivial getters.

---

# 16. Final Validation Checklist

Before considering the refactor complete, verify all of the following:

## 16.1 Automated Checks

- [ ] `cargo fmt` passes
- [ ] `cargo test` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] All snapshot/golden tests pass
- [ ] No production source file contains inline `#[cfg(test)] mod tests`
- [ ] No production source file contains `#[path = "tests/..."] mod tests`
- [ ] No new unnecessary `#[allow(...)]` suppressions were added
- [ ] No new arbitrary abstractions were introduced
- [ ] No `cfg!(debug_assertions)` remaining in production code (use `#[cfg]`)

## 16.2 Behavioral Checks

- [ ] Existing config JSON still parses
- [ ] Saved templates still parse
- [ ] Old metric strings still deserialize (serde rename compatibility)
- [ ] FFmpeg command snapshots unchanged (unless intentionally changed)
- [ ] Interpolation output unchanged (frame counts, timestamp values)
- [ ] Render plans unchanged (unless intentionally changed)
- [ ] Metadata extraction unchanged (creation time priority, codec detection)
- [ ] Preview output visually unchanged
- [ ] Transparent overlay export works
- [ ] Composite MP4 export works
- [ ] Cancellation works and resets state correctly
- [ ] Progress reporting works (state transitions, estimated time)
- [ ] Output paths and filenames unchanged

## 16.3 Threading and Process Checks

- [ ] FFmpeg child processes are not orphaned on cancel or error
- [ ] FFmpeg stdin is closed intentionally on completion
- [ ] Writer threads are joined or intentionally owned
- [ ] Monitor threads are joined or intentionally owned
- [ ] Cancellation does not deadlock
- [ ] Failed renders reset `running` state
- [ ] Cancelled renders reset `running` state
- [ ] Temporary output files are cleaned up on failure

## 16.4 Performance Checks

- [ ] Representative preview render has no significant regression
- [ ] Representative transparent export has no significant regression
- [ ] Representative composite export has no significant regression
- [ ] Hot paths do not introduce obvious extra allocation or locking
- [ ] Memory usage remains acceptable (no unbounded cache growth)

## 16.5 Architecture Checks

- [ ] Dependency direction is respected (config -> activity -> render -> encode)
- [ ] `encode` does not depend on `commands`
- [ ] `render` does not depend on `encode`
- [ ] `config` remains a leaf domain module
- [ ] `commands` remains thin (orchestration only, no deep domain logic)
- [ ] Tauri-specific code remains outside `ovrley_core`
- [ ] `video_pipeline` and `video_composite_pipeline` do not cross-import
- [ ] `AppPaths` lives in a neutral module (`paths.rs`), not in `commands`

---

# 17. Appendix: File-by-File Analysis Summary

## 17.1 `src-tauri/src/main.rs` (6 lines)

**Status:** Good. Simple binary entry point.
**Action:** No major action needed.

## 17.2 `src-tauri/src/lib.rs` (360 lines)

**Status:** Moderate refactor needed.
**Issues:**
- Tauri command wrappers should remain thin — convert typed errors to frontend strings only at boundary
- Contains `cfg!(debug_assertions)` that should be `#[cfg(debug_assertions)]` (line 348)
- Do not move core business logic into this file
**Action:** Keep as boundary layer. Fix `cfg!` → `#[cfg]`.

## 17.3 `src-tauri/src/video_server.rs` (705 lines)

**Status:** Reasonable quality.
**Issues:**
- Tests must be moved to dedicated `tests/` directory (inline `#[cfg(test)] mod tests` at line 461)
- `expect()` on static headers (lines 364-365, 373-374) — acceptable if invariant is clear
- Range parsing behavior should be snapshot/integration tested
**Action:** Move tests. Preserve behavior exactly. Do not rewrite the server.

## 17.4 `src-tauri/src/bin/*.rs` (4 files)

**Status:** Useful diagnostic binaries with duplicated boilerplate.
**Issues:**
- `read_arg()`, `read_optional_arg()`, `repo_root()` repeated identically across all 4 files
**Action:** Extract into `src-tauri/src/bin/common.rs`. Use `anyhow` in binaries.

## 17.5 `ovrley_core/src/lib.rs` (22 lines)

**Status:** Clean module declaration point.
**Action:** Add `pub mod error;`, `pub mod types;`, `pub mod paths;` as needed. Avoid excessive re-exports.

## 17.6 `ovrley_core/src/activity/` (~768 lines)

**Status:** Generally strong domain separation.
**Issues:**
- Tests must move (inline `mod tests` in `mod.rs:57`)
- Errors should migrate from `String`
- Interpolation and frame timestamp behavior must be heavily protected (29.97/59.94 FPS)
**Action:** Add regression tests before refactoring. Protect NTSC-rate behavior.

## 17.7 `ovrley_core/src/config/mod.rs` (616 lines)

**Status:** Important schema/validation module.
**Issues:**
- Stringly-typed metric values (lines 541-553)
- `#[path = "tests/config_tests.rs"]` at line 614
- Config schema may mix raw DTO and validated domain concerns
**Action:** Introduce `MetricKind`. Preserve serde compatibility exactly. Move tests.

## 17.8 `ovrley_core/src/debug/mod.rs` (113 lines)

**Status:** Clean and focused.
**Action:** Keep lightweight. Avoid letting debug types depend on render/encode internals.

## 17.9 `ovrley_core/src/commands/mod.rs` (654 lines)

**Status:** Mixed — contains logic that belongs elsewhere.
**Issues:**
- `derive_composite_render_plan` and `apply_composite_scene_timing` belong in `encode`
- `AppPaths` should live in a neutral `paths.rs`
- Must remain framework-agnostic (no Tauri imports)
**Action:** Move planning logic to `encode`. Move `AppPaths` to `paths.rs`. Keep commands thin.

## 17.10 `ovrley_core/src/encode/` (~4200 lines)

**Status:** Largest complexity hotspot. Three new modules have been added since the original plan.

**Issues:**
- `video.rs` — `RenderController` should be in `progress.rs` (partially done — `ProgressEstimator` already extracted)
- `ffmpeg.rs` — `build_ffmpeg_settings` + `FfmpegSettings` should be in `ffmpeg_settings.rs`
- `video_pipeline.rs` — 200-line `render_video_single` mixes orchestration with execution
- `video_composite_pipeline.rs` — must not import from `video_pipeline`
- `encode` currently depends on `commands` (for `AppPaths`)
- Command generation should be snapshot-tested
- ffmpeg process lifecycle must be documented
- `ffmpeg_composite_profiles.rs` (new) — data-driven encoder profile table, no tests, `Option` return instead of typed errors
- `video_composite_debug.rs` (new) — composite debug summary writer, uses `Result<T, String>`, no tests
- `progress.rs` (new) — `ProgressEstimator` with inline tests, `RenderController` separation not yet complete
**Action:** Extract `ffmpeg_settings.rs`. Complete `RenderController` migration to `progress.rs`. Enforce sibling pipeline isolation. Add tests for new modules. Preserve ffmpeg behavior exactly.

## 17.11 `ovrley_core/src/render/` (~4500 lines)

**Status:** Good domain separation overall, but some large shared helpers.
**Issues:**
- `widgets/common.rs` (774 lines) too broad
- Duplicated geometry/RDP logic in `route.rs` and `elevation.rs`
- Duplicated interpolation logic with `activity/interpolate.rs`
- Stringly-typed metric/icon behavior
- Global caches (`OnceLock<Mutex<HashMap>>`) need ownership review
- Hot path performance must be protected
**Action:** Split `common.rs` by cohesive responsibility. Consolidate RDP into `rdp.rs`. Consolidate interpolation into `interpolation.rs`. Add tests before extracting algorithms.

## 17.12 `ovrley_core/src/encode/video_probe.rs` (227 lines)

**Status:** Important metadata extraction module.
**Issues:**
- Commented-out `println!` debug output (lines 155, 163, 170, 177, 185, 191, 197)
- Creation time resolution priority should be documented
**Action:** Replace debug prints with `tracing` or remove. Add fixture tests using stored ffprobe JSON.

## 17.13 `ovrley_core/src/encode/codec_detect.rs` (490 lines)

**Status:** Highly repetitive probe pattern.
**Issues:**
- Each codec probe repeats the same `probe_codec` call pattern with different args
- 20+ codec probe calls with nearly identical structure
**Action:** The probe pattern is already extracted into `probe_codec` — this is acceptable. The repetition is the list of codecs, not the logic. Consider a data-driven approach only if new codecs are added regularly.

## 17.14 `ovrley_core/src/encode/progress.rs` (183 lines)

**Status:** Clean EMA-based progress estimator, well-tested, already extracted as a separate file.

**Issues:**
- `#[cfg(test)] mod tests` at line 106 must be moved to a dedicated `tests/` directory (violates Phase 1 target)
- Uses `f64` for frame timing — ensure overflow/NaN/Infinity are handled (currently checked in `record()` at line 53)
- No integration with `RenderController` — `RenderController` is still in `video.rs`

**Action:** Move inline tests to `encode/tests/`. Consider whether `ProgressEstimator` should be integrated into `RenderController` state or remain independent.

## 17.15 `ovrley_core/src/encode/ffmpeg_composite_profiles.rs` (248 lines)

**Status:** Well-structured data-driven profile table. Predefined profiles for software (libx264/5), NVENC (H.264/HEVC), QSV (H.264/HEVC), VAAPI (H.264/HEVC), and VideoToolbox (H.264/HEVC) with HWA filter chains.

**Issues:**
- Returns `Option<CompositeProfile>` — should use `CoreResult` for unknown profiles with a descriptive error
- No tests for profile resolution (`composite_profile_template`)
- All filter strings are `&'static str` with `{width}`/`{height}` placeholders — no validation that format strings are correct
- Profile name aliasing (e.g., `"h264_nvenc"` → `"nvgpu_h264"`) duplicates ffmpeg codec logic from `codec_detect.rs`

**Action:** Add tests for profile lookup + template expansion. Consider unifying codec naming with `codec_detect.rs`. Add typed error for unknown profiles.

## 17.16 `ovrley_core/src/encode/video_composite_debug.rs` (253 lines)

**Status:** Composite-only debug/timing summary writer. Produces `timing_summary.json` under `debug_render/phase_7/`.

**Issues:**
- Uses `Result<T, String>` for all fallible operations (lines 100-166)
- `composite_debug_id` strips `"video_composited_"` prefix from output filename — brittle if naming convention changes
- Debug directory layout (`phase_7/`) hardcoded — should reference a shared constant
- No tests

**Action:** Migrate to `CoreResult`. Extract debug directory path into shared constant. Add tests for summary serialization and debug ID derivation.

---

## Final Instruction

Do not optimize for making the diff look impressive. Optimize for making the backend **safer, clearer, and easier to maintain**.

When in doubt:

1. **Preserve behavior** — if uncertain, keep the original
2. **Add tests first** — never refactor without a safety net
3. **Make the smallest useful change** — one concern per step
4. **Avoid speculative abstraction** — solve today's problem, not next year's
5. **Protect hot paths** — performance matters in render/encode loops
6. **Keep module boundaries explicit** — document what each module owns and what it must not import
