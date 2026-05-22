# Phase 3 — Module Cleanup and Duplication Reduction: Detailed Implementation Plan

## Purpose

Reduce duplication (RDP, interpolation, CLI boilerplate), improve cohesion (split `common.rs`, move `AppPaths`), clean up dead code, and integrate the newer composite modules into the refactored error/structure conventions — all without changing production behavior.

---

## Prerequisites

Before starting Phase 3, Phase 2 must be complete:

- [ ] `error.rs` exists with `CoreError` and `CoreResult<T>`
- [ ] `types.rs` exists with `MetricKind`
- [ ] All functions in `ovrley_core` return `CoreResult<T>` (no `Result<T, String>`)
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass
- [ ] No production behavior changed

If Phase 2 is incomplete, stop and complete it first. Phase 3 relies on typed errors (`CoreResult`) for the composite module migration (deliverable 6) and `MetricKind` for safe refactoring of any metric-related logic in `common.rs`.

---

## Rules Carried Forward from Master Plan

The following master plan rules apply during Phase 3. Re-read them before starting each step.

| Rule | Summary |
|------|---------|
| [§2.1] | Behavior preservation is mandatory — RDP simplification output, interpolation frame counts, rendered output must be identical |
| [§2.3] | Inspect the full file, its callers, its public API, its tests, and its performance role before editing |
| [§2.4] | One clear purpose per step — do not mix RDP extraction with interpolation consolidation or module splitting |
| [§2.5] | All tests in dedicated `tests/` directories — any new tests added in Phase 3 go there |
| [§2.7] | Hot paths must stay hot — RDP simplification is called once per render (not per-frame), but interpolation IS per-frame for progress lookup; do not add allocation inside interpolation loops |
| [§2.8] | Split files by cohesion, not line count — only extract when responsibilities diverge |
| [§3.6] | Duplicated RDP in `route.rs:439-471` and `elevation.rs:816-849` — extract to `rdp.rs` |
| [§3.7] | Duplicate interpolation in `activity/interpolate.rs` (f64) and `render/widgets/common.rs` (f32) — consolidate, f64 is authoritative |
| [§3.8] | `render/widgets/common.rs` is 774 lines — split by cohesive responsibility |
| [§3.10] | Four diagnostic binaries duplicate CLI helpers — extract into `src-tauri/src/bin/common.rs` |
| [§3.13] | Commented-out `println!` in `video_probe.rs` — replace with `tracing::debug!` |
| [§3.14] | `encode` depends on `commands` (for `AppPaths`) — move `AppPaths` to neutral `paths.rs` |
| [§8.1] | Dependency direction must be respected — `paths.rs` is a leaf, `interpolation.rs` is a leaf, `rdp.rs` is a leaf |
| [§13.2] | Extract shared algorithms only after tests exist and prove behavior preservation |
| [§11.1-11.2] | Interpolation is a hot path — do not add allocation, cloning, or locking inside interpolation loops |

---

## Deliverable Overview

| # | Deliverable | Priority |
|---|---|---|
| 1 | Extract shared RDP logic into `rdp.rs` | MEDIUM |
| 2 | Consolidate interpolation logic into `interpolation.rs` | MEDIUM |
| 3 | Split `render/widgets/common.rs` into cohesive sub-modules | MEDIUM |
| 4 | Consolidate CLI helpers into `src-tauri/src/bin/common.rs` | LOW |
| 5 | Remove commented-out debug code from `video_probe.rs` | LOW |
| 6 | Integrate new composite modules (error migration, tests) | MEDIUM |
| 7 | Move `AppPaths` from `commands/mod.rs` to `paths.rs` | MEDIUM |

---

## Step-by-Step Execution Plan

---

### Step 1: Full Audit of All Target Files

**Duration estimate:** 45 min

Before making any changes, read every file touched by Phase 3 in its entirety.

#### 1a. Read the duplicated RDP code

| File | Function | Lines | Notes |
|------|----------|-------|-------|
| `render/widgets/route.rs` | `simplify_route_samples` + inner `perpendicular_distance` | 431–471 | RDP loop operates on `RouteSample` points |
| `render/widgets/elevation.rs` | `simplify_elevation_samples_segment` + inner `perpendicular_distance` | 807–849 | Identical algorithm, operates on `ElevationSample` points |
| `render/widgets/elevation.rs` | `simplify_elevation_samples` wrapper | 777–803 | Preserves flagged samples, calls segment simplifier per window |

Key observation: Both `perpendicular_distance` functions are byte-identical (same formula: `(dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()`). Both RDP recursions are structurally identical. Only the element type differs (`RouteSample` vs `ElevationSample`).

The RDP algorithm itself can accept `&[(f32, f32)]` tuples — callers convert their domain types to tuples, call the shared function, then map results back.

#### 1b. Read the duplicated interpolation code

| File | Function | Lines | Type | Visibility |
|------|----------|-------|------|------------|
| `activity/interpolate.rs` | `collect_valid_numeric_points` | 17 | `fn(&[f64], &[Option<f64>]) -> Vec<(f64, f64)>` | private |
| `activity/interpolate.rs` | `interpolate_points` | 29 | `fn(&[(f64, f64)], f64) -> Option<f64>` | private |
| `activity/interpolate.rs` | `interpolate_numeric_series_value` | 64 | `pub fn(&[f64], &[Option<f64>], f64) -> Option<f64>` | **public** |
| `activity/interpolate.rs` | `densify_activity` | 225 | `pub fn(&TrimmedActivity, f64, &RenderDataRequirements) -> DenseActivityReport` | **public** |
| `render/widgets/common.rs` | `interpolate_numeric_points` | 87 | `fn(&[(f64, f64)], f64) -> Option<f64>` | private |
| `render/widgets/common.rs` | `interpolate_numeric_series_many` | 115 | private — bulk interpolation over many targets | private |
| `render/widgets/common.rs` | `interpolate_optional_numeric_series` | 267 | `fn(&[f64], &[Option<f64>], f64) -> Option<f64>` | private |
| `render/widgets/common.rs` | `interpolate_distance_progress_at_elapsed` | 191 | already delegates to `crate::activity::interpolate::interpolate_numeric_series_value` at line 202 | — |

Key observation: `common.rs:87` `interpolate_numeric_points` takes `&[(f64, f64)]` (same input type as `activity/interpolate.rs:29` `interpolate_points`). Both use `partition_point` for linear interpolation. The `common.rs:267` `interpolate_optional_numeric_series` mirrors `activity/interpolate.rs:64` `interpolate_numeric_series_value` — same inputs, same outputs, same logic. **However**, `common.rs` comments say these operate in f32 domain but the actual code uses `f64` — the f32 comment is misleading; verify at audit time.

#### 1c. Read `render/widgets/common.rs` in full

Map every function group to a responsibility:

| Section | Lines | Functions | Target module |
|---------|-------|-----------|---------------|
| Constants | 16–24 | `DEFAULT_COLOR`, `DEFAULT_LINE_WIDTH`, etc. | `common.rs` (keep) |
| Opacity & distance | 27–38 | `normalize_opacity`, `distance` | `geometry.rs` |
| Fit/layout | 41–84 | `fit_points_to_widget_with_inset` | `geometry.rs` |
| Interpolation | 87–134 | `interpolate_numeric_points`, `interpolate_numeric_series_many` | delete (delegate to `interpolation.rs`) |
| Progress & frame values | 137–264 | `frame_progress_values`, `custom_export_range_active`, `interpolate_distance_progress_at_elapsed`, `relative_distance_frame_progress_values`, `normalize_optional_progress_window`, `interpolate_optional_numeric_series` | `progress.rs` (rename from frame progress helpers) |
| Progress lookup w/cursor | 302–363 | `point_at_metric_progress_with_cursor`, `point_at_progress_x` | `progress.rs` |
| Polyline drawing | 366–421 | `draw_polyline`, `draw_polyline_with_shadow` | `polyline.rs` |
| Area drawing | 424–440 | `draw_area` | `polyline.rs` |
| Marker drawing | 443–507 | `draw_marker`, `marker_layers_from_points` | `marker.rs` |
| Color & style | 511–606 | `plot_base_color`, `legacy_line_width`, `marker_size_from_weights`, `fallback_marker_points`, `scale_marker_points`, `resolve_style_color`, `normalize_shadow_style`, `shadow_with_screen_offset` | `style.rs` (or keep in `common.rs` if small enough post-split) |
| Static layers | 626–640 | `static_layer_padding`, `draw_static_layer` | `common.rs` (keep) |
| Path/transform | 643–688 | `path_from_points`, `with_widget_transform` | `transform.rs` |
| Coordinate wire-up | 691–773 | `rotate_point_to_canvas`, `widget_render_report`, `format_elevation_label` | `common.rs` (keep — small, cohesive with report system) |

**Decision rule:** If extracting a module would leave fewer than ~40 lines of genuinely related code, keep it in `common.rs`. The target is ~200 lines total in `common.rs`, not an exact split of every tiny group.

#### 1d. Read CLI binary files

List all duplicated helpers and their line numbers (confirmed from codebase audit):

| Helper | `render_preview.rs` | `render_video.rs` | `validate_activity.rs` | `parallel_render.rs` | `benchmark_*.rs` (3 files) |
|--------|---------------------|-------------------|------------------------|---------------------|---------------------------|
| `read_arg` | 14–19 | 9–14 | 6–11 | — | `benchmark_transparent.rs:54-58` (returns `Option`) |
| `read_optional_arg` | 29–33 | 16–20 | — | — | — |
| `repo_root` | 21–27 | 22–28 | — | 7–13 | all 3 benchmarks |
| `read_positional` | — | — | — | — | all 3 benchmarks |
| `resolve_path` | — | — | — | — | all 3 benchmarks |
| `format_mmss` | — | — | — | — | all 3 benchmarks |
| `unix_timestamp` | — | — | — | — | all 3 benchmarks |

**Decision:** The 3 benchmark files (`benchmark_composite.rs`, `benchmark_transparent.rs`, `benchmark_widget_rate.rs`) are longer and contain shared benchmark-specific helpers (`read_positional`, `resolve_path`, `format_mmss`, `unix_timestamp`) plus the standard `repo_root`. The 4 older binaries have `read_arg`, `read_optional_arg`, `repo_root`. Create a single `common.rs` in `src/bin/` with ALL shared helpers.

#### 1e. Read composite modules

- `encode/ffmpeg_composite_profiles.rs` (271 lines) — `composite_profile_template(name) -> Option<CompositeProfile>`, no tests, uses `Option` instead of `CoreResult`
- `encode/video_composite_debug.rs` (251 lines) — `write_composite_timing_summary(input) -> CoreResult<PathBuf>`, already uses `CoreResult` (check: Phase 2 migration may have updated this)

#### 1f. Read AppPaths usage

- **Definition:** `commands/mod.rs:32-48` — `pub struct AppPaths`
- **Methods:** `from_repo_root` (line 52), `from_resource_root` (line 57), `ensure_dirs` (line 95), `bundled_template_path` (line 111), `user_template_path` (line 119)
- **Importers (list all by grepping `AppPaths`):**
  - `encode/video.rs:10` — `use crate::commands::AppPaths`
  - `commands/mod.rs` — self (definition site)
  - `src-tauri/src/lib.rs` — Tauri boundary
  - Any other files — audit at execution time

**Keep in mind for Step 1:**
- Master plan §2.3: Do not skip this audit. Read the actual code, not just grep results.
- Record the precise line numbers at audit time — they may have shifted since this plan was written.
- If any file has changed significantly since this plan was written, pause and reassess.

---

### Step 2: Add RDP Snapshot Tests Before Extraction

**Duration estimate:** 45 min

**File:** `ovrley_core/tests/rdp_tests.rs` (new file)

Per master plan §13.2: "Only extract after tests exist and prove behavior preservation."

#### 2a. Test `perpendicular_distance` from both modules

Since the duplicated functions are private inner functions, expose a minimal test seam OR test through the public simplification functions. **Prefer testing through public APIs** to avoid adding unnecessary `pub(crate)` visibility.

**Tests to add:**

```rust
use ovrley_core::render::widgets::route::simplify_route_samples;  // check: is this pub?
use ovrley_core::render::widgets::elevation::simplify_elevation_samples;  // check: is this pub?

#[test]
fn rdp_straight_line_preserves_endpoints() {
    // 3 collinear points — middle should be removed
}

#[test]
fn rdp_single_point_returns_same() {
    // Degenerate case
}

#[test]
fn rdp_empty_input_returns_empty() {
    // Edge case
}

#[test]
fn rdp_preserves_peaks() {
    // A triangle shape should preserve the peak point at reasonable tolerance
}

#[test]
fn rdp_route_and_elevation_produce_identical_geometry() {
    // Given identical coordinate tuples, both simplifiers should produce identical results
    // This is THE key test — proves the two implementations are equivalent
}
```

**Keep in mind:**
- Check visibility of `simplify_route_samples` and `simplify_elevation_samples` — they may be `pub(crate)` or private. If private, add `pub(crate)` test seams.
- `simplify_elevation_samples` has an additional wrapping layer that preserves flagged samples. Test both the segment-level and the full-function behavior.
- These tests protect the exact RDP behavior before we consolidate. They will continue to pass after consolidation using the shared `rdp.rs` functions.
- If `RouteSample` and `ElevationSample` types are not `pub`, the equivalence test may need to work with raw `(f32, f32)` tuples extracted from both widget builders.

#### 2b. Verify

```bash
cargo test -p ovrley_core rdp_tests
```

---

### Step 3: Consolidate Interpolation Logic

**Duration estimate:** 75 min

#### 3a. Create `ovrley_core/src/interpolation.rs`

Move the authoritative `f64`-based functions from `activity/interpolate.rs`:

```rust
// ovrley_core/src/interpolation.rs
//! Shared interpolation utilities.
//!
//! Owns: linear interpolation over numeric series, optional series,
//!        and general point-list interpolation.
//! Does not own: activity densification (that stays in `activity/interpolate`).
//!
//! Allowed dependencies: std.
//! Forbidden dependencies: config, activity, render, encode.
//!
//! ## Performance
//! The `interpolate_points` function uses `partition_point` (O(log n))
//! and is called per-frame during progress lookup. Avoid allocation
//! inside the interpolation function itself — callers should batch.

use std::cmp::Ordering;

/// Collects (x, y) pairs where y is present.
pub fn collect_valid_numeric_points(x_values: &[f64], y_values: &[Option<f64>]) -> Vec<(f64, f64)> {
    // Move existing implementation from activity/interpolate.rs:17
    // (currently private — make it pub(crate) or pub)
}

/// Linearly interpolates y at `target_x` between two nearest points.
///
/// Uses `partition_point` for O(log n) lookup. Returns `None` if
/// `target_x` is outside the point range (clamped to endpoints).
pub fn interpolate_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    // Move existing implementation from activity/interpolate.rs:29
}

/// Linearly interpolates an optional numeric series at `target_x`.
///
/// Filters out `None` values, then delegates to [`interpolate_points`].
pub fn interpolate_numeric_series_value(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    // Move existing implementation from activity/interpolate.rs:64
}

/// Linearly interpolates an optional numeric series at `target_x`.
///
/// Alias for [`interpolate_numeric_series_value`] — provided for
/// callers that use the "optional" naming convention.
pub fn interpolate_optional_numeric_series(
    x_values: &[f64],
    y_values: &[Option<f64>],
    target_x: f64,
) -> Option<f64> {
    // This is the function currently duplicated in common.rs:267
    // After consolidation, it's just a thin wrapper or identical to interpolate_numeric_series_value
}
```

**Important design decision:** The `collect_valid_numeric_points` function currently lives in `activity/interpolate.rs` and allocates a `Vec`. This is called once per densification series, NOT per frame — so the allocation is acceptable. The per-frame hot path is `interpolate_points` which takes pre-collected points. Keep this split: collect once, interpolate many times.

#### 3b. Update `activity/interpolate.rs`

Replace the moved functions with re-exports or thin delegates:

```rust
// activity/interpolate.rs — keep only activity-specific logic

use crate::interpolation;  // or pub use crate::interpolation::*

// REMOVE: collect_valid_numeric_points (moved to interpolation.rs)
// REMOVE: interpolate_points (moved to interpolation.rs)
// REMOVE: interpolate_numeric_series_value (moved to interpolation.rs)

// RE-EXPORT for backward compatibility (all existing callers in activity/ use these):
pub use crate::interpolation::{
    collect_valid_numeric_points,
    interpolate_points,
    interpolate_numeric_series_value,
};

// KEEP: densify_activity — activity-specific logic that uses the shared functions
pub fn densify_activity(
    trimmed: &TrimmedActivity,
    fps: f64,
    requirements: &RenderDataRequirements,
) -> DenseActivityReport {
    // Uses interpolate_points, collect_valid_numeric_points from interpolation module
    // Implementation unchanged — only imports change
}
```

**Keep in mind:**
- `densify_activity` is the public API of the activity module. It must continue to exist and behave identically.
- The re-exports ensure that any internal `activity/` callers don't break — but audit to see if there are any callers of the moved functions outside `activity/interpolate.rs` itself (e.g., `activity/trim.rs` or `activity/mod.rs`).

#### 3c. Update `render/widgets/common.rs`

Replace the duplicated f64/f32 interpolation helpers with delegation to `crate::interpolation`:

```rust
// Before (common.rs:87):
fn interpolate_numeric_points(points: &[(f64, f64)], target_x: f64) -> Option<f64> {
    // ... private implementation
}

// After:
use crate::interpolation::interpolate_points;

// DELETE: interpolate_numeric_points (use interpolate_points instead)
// DELETE: interpolate_numeric_series_many (use interpolate_numeric_series_value in a loop)

// Before (common.rs:267):
fn interpolate_optional_numeric_series(...) -> Option<f64> {
    // ... private implementation
}

// After:
use crate::interpolation::interpolate_optional_numeric_series;  // or interpolate_numeric_series_value
```

**Critical behavior check:** The `common.rs` function comments say they use f32, but the actual code uses f64. The activity version uses f64. Verify this at audit time — if `common.rs` actually uses f32, there may be subtle precision differences in frame progress calculation. If differences exist, document them and ensure the consolidated version (f64) is a strict improvement.

#### 3d. Wire `interpolation.rs` into `lib.rs`

```rust
// ovrley_core/src/lib.rs
pub mod interpolation;  // NEW — leaf module
```

Re-export is optional. The module is public so tests and other modules can import from `crate::interpolation`.

#### 3e. Update callers

Grep for all callers of the moved functions and update imports:

```bash
rg "interpolate_points|interpolate_numeric_series_value|collect_valid_numeric_points|interpolate_optional_numeric_series|interpolate_numeric_points|interpolate_numeric_series_many" ovrley_core/src/
```

Expected callers:
- `activity/interpolate.rs` — `densify_activity` (already updated in 3b)
- `activity/trim.rs` — may call interpolation helpers
- `render/widgets/common.rs` — `frame_progress_values`, `interpolate_distance_progress_at_elapsed`, etc.
- `render/widgets/elevation.rs` — may call `interpolate_optional_numeric_series`
- Any test files in `ovrley_core/tests/`

#### 3f. Verification

```bash
cargo check -p ovrley_core
cargo test -p ovrley_core
```

**Keep in mind:**
- Master plan §3.7: The `activity/interpolate.rs` f64 versions are authoritative. Consolidate TO them, not the other way.
- Master plan §2.7: Interpolation is a hot path (called per-frame during progress lookup). The `interpolate_points` function is O(log n) with `partition_point` — preserve this. Do not add allocation inside the function.
- Master plan §8.1: `interpolation.rs` is a leaf module — it must not import `config`, `activity`, `render`, or `encode`. It only imports `std`.
- If `common.rs` interpolation was used in a way that depends on f32 precision, this change alters behavior. Audit the calling code carefully. If precision differences are intentional, document as an `Intentional behavior change`.

---

### Step 4: Extract Shared RDP Logic into `rdp.rs`

**Duration estimate:** 60 min

#### 4a. Create `ovrley_core/src/rdp.rs`

```rust
// ovrley_core/src/rdp.rs
//! Shared Ramer-Douglas-Peucker line simplification.
//!
//! Owns: the RDP algorithm and perpendicular distance calculation.
//! Does not own: route or elevation domain types.
//!
//! Allowed dependencies: std.
//! Forbidden dependencies: config, activity, render, encode, commands.
//!
//! ## Performance
//! Called once per widget build (not per-frame). O(n log n) worst case
//! with tolerance-based early termination.

/// Perpendicular distance from `point` to the line segment `start`→`end`.
///
/// Returns Euclidean distance if the segment has zero length.
pub fn perpendicular_distance(
    point: (f32, f32),
    start: (f32, f32),
    end: (f32, f32),
) -> f32 {
    let (x0, y0) = point;
    let (x1, y1) = start;
    let (x2, y2) = end;
    let dx = x2 - x1;
    let dy = y2 - y1;
    if dx == 0.0 && dy == 0.0 {
        return ((x0 - x1).powi(2) + (y0 - y1).powi(2)).sqrt();
    }
    (dy * x0 - dx * y0 + x2 * y1 - y2 * x1).abs() / (dx * dx + dy * dy).sqrt()
}

/// Simplifies a polyline using the Ramer-Douglas-Peucker algorithm.
///
/// Returns a subset of the input points. The first and last points are always
/// preserved. `tolerance` is in the same units as the input coordinates
/// (typically pixels).
pub fn simplify_rdp(points: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let mut max_distance = 0.0f32;
    let mut split_index = 0usize;

    let first = points[0];
    let last = points[points.len() - 1];

    for i in 1..points.len() - 1 {
        let distance = perpendicular_distance(points[i], first, last);
        if distance > max_distance {
            max_distance = distance;
            split_index = i;
        }
    }

    if max_distance <= tolerance {
        return vec![first, last];
    }

    let left = simplify_rdp(&points[..=split_index], tolerance);
    let right = simplify_rdp(&points[split_index..], tolerance);

    // Concat: drop duplicate split point from left
    [&left[..left.len() - 1], &right].concat()
}
```

**Keep in mind:**
- Master plan §3.6 says "Prefer simple free functions over a `Point2D` trait unless generic behavior is proven necessary." Use tuple types `(f32, f32)` — both route and elevation widgets already work with these.
- The algorithm is identical to both current implementations. Do not "improve" the math — preserve exact floating-point behavior (same operations, same order).
- The f32 precision is intentional here — these are screen-space coordinates. Don't change to f64.
- `simplify_rdp` returns `Vec<(f32, f32)>` — callers map these tuples back to their domain types externally.

#### 4b. Wire `rdp.rs` into `lib.rs`

```rust
// ovrley_core/src/lib.rs
pub mod rdp;  // NEW
```

#### 4c. Update `route.rs`

Replace the inline RDP functions with calls to `crate::rdp`:

```rust
// BEFORE (route.rs:431-471):
fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    fn perpendicular_distance(...) { ... }
    // RDP recursion
}

// AFTER:
use crate::rdp::{perpendicular_distance, simplify_rdp};

fn simplify_route_samples(points: &[RouteSample], tolerance: f32) -> Vec<RouteSample> {
    // Extract (x, y) tuples from RouteSample
    let tuples: Vec<(f32, f32)> = points.iter()
        .map(|p| (p.x, p.y))  // verify actual field names
        .collect();
    let simplified = simplify_rdp(&tuples, tolerance);
    // Map back: find original RouteSample for each simplified tuple
    // Careful: if multiple samples share coordinates, prefer the first match
    simplified.iter().map(|(x, y)| {
        points.iter().find(|p| p.x == *x && p.y == *y).unwrap().clone()
    }).collect()
}
```

**IMPORTANT — tuple round-trip concern:** The mapping from `(f32, f32)` back to `RouteSample` requires finding the original sample that produced those coordinates. Since RDP only returns a subset of the original points (never interpolated), exact coordinate matching is safe — but float equality on f32 is fragile. Two approaches:

1. **Store original index** — preferred. Modify `simplify_rdp` to return indices instead of tuples:
   ```rust
   pub fn simplify_rdp_indices(points: &[(f32, f32)], tolerance: f32) -> Vec<usize> { ... }
   ```
   This avoids float equality entirely.

2. **Use the shared function on the domain types directly** — requires a trait or closure, which master plan advises against for a first extraction.

**Recommended approach:** Add a second function `simplify_rdp_indices` that returns `Vec<usize>` — indices into the original slice. Both route.rs and elevation.rs can use this to index into their domain arrays safely:

```rust
// rdp.rs
pub fn simplify_rdp_indices(points: &[(f32, f32)], tolerance: f32) -> Vec<usize> {
    if points.len() <= 2 {
        return (0..points.len()).collect();
    }
    // ... same RDP logic, but track indices instead of tuples
    // Return sorted indices
}
```

Then callers do:
```rust
let tuples: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
let indices = simplify_rdp_indices(&tuples, tolerance);
let result: Vec<RouteSample> = indices.iter().map(|&i| points[i].clone()).collect();
```

#### 4d. Update `elevation.rs`

Same pattern as route.rs, with the additional complexity of the `simplify_elevation_samples` wrapper that preserves flagged samples:

```rust
// BEFORE (elevation.rs:777-849):
fn simplify_elevation_samples(points: &[ElevationSample], tolerance: f32) -> Vec<ElevationSample> {
    // Find preserved indexes
    // For each segment between preserved points:
    //   call simplify_elevation_samples_segment (which has inline RDP)
}

fn simplify_elevation_samples_segment(points: &[ElevationSample], tolerance: f32) -> Vec<ElevationSample> {
    fn perpendicular_distance(...) { ... }
    // RDP recursion
}

// AFTER:
use crate::rdp::simplify_rdp_indices;

fn simplify_elevation_samples(points: &[ElevationSample], tolerance: f32) -> Vec<ElevationSample> {
    // Find preserved indexes (unchanged)
    let preserved = points.iter().enumerate()
        .filter(|(_, p)| p.preserve)  // verify actual field name
        .map(|(i, _)| i)
        .collect::<Vec<usize>>();
    
    if preserved.is_empty() {
        let tuples: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
        let indices = simplify_rdp_indices(&tuples, tolerance);
        return indices.iter().map(|&i| points[i].clone()).collect();
    }
    
    // Per-segment simplification (unchanged structure, just delegates to rdp)
    // ...
}
```

#### 4e. Remove `perpendicular_distance` from both files

After step 4c–4d, both files should have zero RDP algorithm code — only the domain-type mapping and (for elevation) the preserved-sample wrapper.

#### 4f. Verification

```bash
cargo check -p ovrley_core
cargo test -p ovrley_core rdp_tests
cargo test -p ovrley_core  # full suite — especially route/elevation-related tests
```

**Keep in mind:**
- The RDP snapshot tests from Step 2 must pass unchanged — they validate that the extracted algorithm produces identical output.
- The mapping from `(f32, f32)` to domain types is the highest-risk part of this extraction. Verify that no sample has identical coordinates but different metadata (e.g., two `RouteSample`s at the same position with different timestamps). If they do, the index-based approach is required.
- Both route.rs and elevation.rs use `f32` for RDP tolerance (specified in screen pixels). Preserve this.
- Master plan §13.2: This step is the canonical example of shared algorithm extraction. Ensure it's clean.

---

### Step 5: Split `render/widgets/common.rs`

**Duration estimate:** 90 min

#### 5a. Strategy

Split `common.rs` (currently 774 lines) into cohesive sub-modules. The target is ~200 lines in `common.rs` for shared constants and tiny helpers only.

**Rule:** Only extract a module when it genuinely improves cohesion. Do not split mechanically at a line count threshold (master plan §2.8, §14.4).

#### 5b. Extraction order and plan

**5b-i. Create `geometry.rs`** (~60 lines)

Move:
- `distance` (line 36)
- `normalize_opacity` (line 27)
- `fit_points_to_widget_with_inset` (lines 41–84)

```rust
// render/widgets/geometry.rs
//! Point/rect/math and layout-fitting helpers.
//!
//! Allowed dependencies: std.

pub fn normalize_opacity(value: Option<f32>, default: f32) -> f32 { ... }
pub fn distance(left: (f32, f32), right: (f32, f32)) -> f32 { ... }
pub fn fit_points_to_widget_with_inset(...) -> ... { ... }
```

**5b-ii. Create `polyline.rs`** (~90 lines)

Move:
- `draw_polyline` (line 366)
- `draw_polyline_with_shadow` (line 377)
- `draw_area` (line 424)

```rust
// render/widgets/polyline.rs
//! Polyline and area drawing helpers.
//!
//! Allowed dependencies: skia_safe, crate::render::widgets::types.

pub fn draw_polyline(canvas: &Canvas, points: &[(f32, f32)], ...) { ... }
pub fn draw_polyline_with_shadow(canvas: &Canvas, points: &[(f32, f32)], ..., shadow: Option<ShadowStyle>) { ... }
pub fn draw_area(canvas: &Canvas, points: &[(f32, f32)], baseline_y: f32, ...) { ... }
```

**5b-iii. Create `marker.rs`** (~80 lines)

Move:
- `draw_marker` (line 443)
- `marker_layers_from_points` (line 485)

```rust
// render/widgets/marker.rs
//! Marker and dot drawing helpers.
//!
//! Allowed dependencies: skia_safe, crate::render::widgets::types.

pub fn draw_marker(canvas: &Canvas, center: (f32, f32), layers: &[MarkerLayer], ...) { ... }
pub fn marker_layers_from_points(points: &[MarkerPointConfig]) -> Vec<MarkerLayer> { ... }
```

**5b-iv. Create `transform.rs`** (~55 lines)

Move:
- `path_from_points` (line 643)
- `with_widget_transform` (line 670)

```rust
// render/widgets/transform.rs
//! Skia path and transform helpers.
//!
//! Allowed dependencies: skia_safe.

pub fn path_from_points(points: &[(f32, f32)], baseline: Option<f32>) -> SkPath { ... }
pub fn with_widget_transform(canvas: &Canvas, center: (f32, f32), rotation_deg: f32, draw_fn: impl FnOnce(&Canvas)) { ... }
```

**5b-v. Manage the remaining code in `common.rs`**

After extraction, `common.rs` should retain:

| What stays | Lines (approx) | Why |
|------------|----------------|-----|
| Constants | 16–24 | Shared across ALL widget modules — natural home |
| Interpolation delegation | ~10 | Re-exports from `crate::interpolation` for convenience |
| `frame_progress_values` + related | 137–264 | Progress calculation is a distinct concern, but if it's only ~120 lines and tightly integrated with the widget build flow, keep here for now. Consider `progress.rs` only if it grows. |
| `point_at_metric_progress_with_cursor` | 302–363 | Progress lookup with monotonic cursor — coupled to `frame_progress_values` |
| Color & style resolution | 511–606 | ~95 lines. If `<100` lines, keep in common.rs. Only extract `style.rs` if it grows significantly. |
| `static_layer_padding`, `draw_static_layer` | 626–640 | Thin layer — keep |
| `rotate_point_to_canvas`, `widget_render_report`, `format_elevation_label` | 691–773 | Report/format wire-up — keep |

**Post-extraction common.rs target:** ~200–250 lines.

#### 5c. Update imports

After splitting, every file that imported from `common.rs` must be updated. The most impacted callers:

- `route.rs` — imports from `common` for drawing, transforms, markers
- `elevation.rs` — imports from `common` for drawing, transforms, progress
- `value.rs` — imports from `common` for transforms, geometry
- `mod.rs` (render/) — imports from `common` for high-level orchestration

**Upgrade pattern:**

```rust
// Before:
use crate::render::widgets::common::{draw_polyline, draw_marker, path_from_points};

// After:
use crate::render::widgets::polyline::draw_polyline;
use crate::render::widgets::marker::draw_marker;
use crate::render::widgets::transform::path_from_points;
```

#### 5d. Re-export from `common.rs` for convenience (optional)

If many callers import a mix of items from multiple sub-modules, `common.rs` can re-export for backward compatibility:

```rust
// common.rs — re-exports for convenience
pub use super::geometry::*;
pub use super::polyline::*;
pub use super::marker::*;
pub use super::transform::*;
```

This preserves existing `use crate::render::widgets::common::*` imports. Use this pattern **only if** it avoids touching 10+ caller files. If only 3–4 callers need updates, prefer direct imports (clearer dependency graph).

**Recommendation:** Do NOT re-export. Update callers directly. This makes the widget module's dependency structure explicit and is consistent with the master plan's aim for clean boundaries.

#### 5e. Update `widgets/mod.rs`

```rust
// render/widgets/mod.rs
pub mod common;
pub mod types;
pub mod geometry;     // NEW
pub mod marker;       // NEW
pub mod polyline;     // NEW
pub mod transform;    // NEW
pub mod value;
pub mod route;
pub mod elevation;
```

#### 5f. Verification

```bash
cargo check -p ovrley_core
cargo test -p ovrley_core
```

Fix any import errors from callers that referenced the moved functions via `common::`. After all imports are fixed:

```bash
cargo clippy -- -D warnings
```

**Keep in mind:**
- Master plan §2.8: Split by cohesion, not line count. If polyline.rs and area drawing are naturally one concern (they are — both draw filled/outlined paths), keep them together in `polyline.rs`. Don't split `draw_area` into its own file just because it's a different function.
- Master plan §3.8: The target module structure from the master plan includes `geometry.rs`, `marker.rs`, `polyline.rs`, and `transform.rs`. A `fit.rs` for layout/fitting (currently `fit_points_to_widget_with_inset`) can be deferred — geometry is small enough.
- The interpolation functions that were already in `common.rs` should have been removed in Step 3. Verify no leftover duplication.
- After extraction, ensure `common.rs` has a module-level doc comment explaining what remains and what moved where.

---

### Step 6: Move `AppPaths` to Neutral Module

**Duration estimate:** 45 min

#### 6a. Create `ovrley_core/src/paths.rs`

```rust
// ovrley_core/src/paths.rs
//! Application path configuration and resolution.
//!
//! Owns: AppPaths struct, path construction, template path resolution,
//!        directory ensuring.
//! Does not own: runtime configuration (that's `config`), render parameters.
//!
//! Allowed dependencies: std, crate::error.
//! Forbidden dependencies: config, activity, render, encode, commands.
//!
//! This module lives in a neutral location because both `commands` and
//! `encode` need `AppPaths`. Placing it here breaks the circular-ish
//! dependency where `encode` had to import from `commands`.
//!
//! ## Thread Safety
//! AppPaths is `Clone + Debug` and contains only `PathBuf`/`Vec<PathBuf>`.
//! It is safe to share across threads (paths are immutable after construction).

use std::path::{Path, PathBuf};
use crate::error::CoreResult;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub repo_root: PathBuf,
    pub font_dirs: Vec<PathBuf>,
    pub debug_render_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub bundled_templates_dirs: Vec<PathBuf>,
    pub user_templates_dir: PathBuf,
    pub downloads_dir: PathBuf,
}

impl AppPaths {
    pub fn from_repo_root(repo_root: PathBuf) -> Self { ... }
    pub fn from_resource_root(repo_root: PathBuf, resource_root: PathBuf) -> Self { ... }
    fn from_roots(repo_root: PathBuf, resource_root: PathBuf) -> Self { ... }
    pub fn ensure_dirs(&self) -> CoreResult<()> { ... }
    pub fn bundled_template_path(&self, filename: &str) -> Option<PathBuf> { ... }
    pub fn user_template_path(&self, filename: &str) -> Option<PathBuf> { ... }
}
```

**Action:** Copy the entire `AppPaths` struct and `impl` block from `commands/mod.rs:32-123` into this new file. Do not modify the implementation.

#### 6b. Update `commands/mod.rs`

Remove the `AppPaths` definition and `impl` block. Replace with a re-export:

```rust
// commands/mod.rs — after removal
pub use crate::paths::AppPaths;
```

This re-export preserves backward compatibility for any code that already imports `AppPaths` from `commands`. It can be removed in a later phase after all callers are updated.

#### 6c. Update `lib.rs`

```rust
// ovrley_core/src/lib.rs
pub mod paths;  // NEW
```

#### 6d. Update all `AppPaths` importers

Grep for all files that reference `AppPaths` and update imports:

```bash
rg "use crate::commands::AppPaths|use crate::commands::\{.*AppPaths" src-tauri/ovrley_core/src/
```

Expected update pattern:

```rust
// Before:
use crate::commands::AppPaths;

// After (option 1 — direct import):
use crate::paths::AppPaths;

// After (option 2 — keep using re-export, no change needed):
// The pub use re-export in commands/mod.rs means existing imports still compile.
// Update to direct import for cleanliness.
```

Files that import `AppPaths` (audited list):
- `encode/video.rs:10` — `use crate::commands::AppPaths`
- `src-tauri/src/lib.rs` — Tauri boundary
- Any test files referencing `AppPaths`

#### 6e. Verify the dependency graph

After this step:

```
encode  ---> paths (AppPaths)     ✓ (no longer depends on commands)
commands --> paths (AppPaths)     ✓ (re-exports from paths)
```

The `encode -> commands` dependency is eliminated. `encode` now imports `AppPaths` from `paths.rs` — a leaf module with no reverse dependencies.

#### 6f. Verification

```bash
cargo check
cargo test
cargo clippy -- -D warnings
```

**Keep in mind:**
- Master plan §3.14 and §8.1: `encode` must not depend on `commands`. This step fixes that.
- Master plan §4.1: `paths.rs` is in the target architecture layout.
- The re-export in `commands/mod.rs` is a transitional convenience. It should be removed in Phase 6 (final polish) after confirming no external code depends on the old location.
- `AppPaths` itself does not change — only its location. No behavior change.
- `src-tauri/src/lib.rs` constructs `AppPaths` at app startup — ensure that import is updated.

---

### Step 7: Consolidate CLI Helpers

**Duration estimate:** 40 min

#### 7a. Create `src-tauri/src/bin/common.rs`

Extract ALL shared helpers:

```rust
// src-tauri/src/bin/common.rs
//! Shared CLI argument helpers for diagnostic binaries.
//!
//! All binaries use `anyhow` for error handling. Core library code
//! uses typed errors — these helpers are binary-only.

use std::path::PathBuf;
use anyhow::{Context, Result};

pub fn repo_root() -> Result<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // If binaries need the workspace root, resolve accordingly.
    // Current pattern: parent of CARGO_MANIFEST_DIR (src-tauri/)
    // Verify what binaries actually need.
    Ok(dir)
}

pub fn read_arg(flag: &str, args: &[String]) -> Result<String> {
    let flag_prefix = format!("{}=", flag);
    args.iter()
        .find(|a| a.starts_with(&flag_prefix))
        .map(|a| a[flag_prefix.len()..].to_string())
        .with_context(|| format!("Missing required argument: {}", flag))
}

pub fn read_optional_arg(flag: &str, args: &[String]) -> Option<String> {
    let flag_prefix = format!("{}=", flag);
    args.iter()
        .find(|a| a.starts_with(&flag_prefix))
        .map(|a| a[flag_prefix.len()..].to_string())
}

pub fn read_positional(index: usize, args: &[String]) -> Option<String> {
    args.get(index).cloned()
}

pub fn resolve_path(input: &str, repo_root: &PathBuf) -> PathBuf {
    let path = std::path::Path::new(input);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

pub fn format_mmss(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u64;
    let secs = (seconds % 60.0) as u64;
    format!("{:02}:{:02}", mins, secs)
}

pub fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
```

**Note:** The benchmark binaries use `read_arg` returning `Option<String>` while the older binaries return `Result<String, String>`. After Phase 2, the older binaries should use `anyhow::Result` per master plan §3.10. Standardize on `anyhow::Result<String>` for binaries and `Option<String>` for `read_optional_arg`. Update `read_arg` callers that used `Result<String, String>` to use `anyhow::Result`.

#### 7b. Extract into a `bin_common` helper module

Since `src/bin/` files are standalone binaries (each has `fn main()`), they cannot `mod common;` from a peer file directly — Cargo treats each `.rs` file in `src/bin/` as an independent crate root.

**Solution:** Place `common.rs` in `src-tauri/src/bin_common.rs` (or a `src/bin_common/` directory module) and reference it via `#[path]` or by adding a lib target. The cleanest approach for binaries:

```rust
// In each binary file:
#[path = "../bin_common.rs"]
mod common;

use common::{read_arg, read_optional_arg, repo_root};
```

Alternatively, add a small library target to the Tauri crate's `Cargo.toml` that the binaries can depend on — but this is over-engineering for 7 diagnostic binaries.

**Recommended approach:** Use `#[path = "../bin_common.rs"]` — simple, explicit, and mirrors existing patterns in the codebase. The `bin_common.rs` module is NOT part of the main `app` library; it's compiled only into the diagnostic binaries.

#### 7c. `Cargo.toml` verification

The Tauri crate's `Cargo.toml` already has `anyhow` as a dependency (binaries use it). Verify:

```bash
rg "anyhow" src-tauri/Cargo.toml
```

If `anyhow` is not present, add it:

```toml
anyhow = "1"
```

#### 7d. Update each binary

For each of the 7 binary files, replace the inline helper definitions with the `#[path]` include and update call sites as needed:

| Binary file | Lines to remove | Helpers to replace |
|-------------|-----------------|-------------------|
| `render_preview.rs` | 14–33 | `read_arg`, `repo_root`, `read_optional_arg` |
| `render_video.rs` | 9–28 | `read_arg`, `read_optional_arg`, `repo_root` |
| `validate_activity.rs` | 6–11 | `read_arg` |
| `parallel_render.rs` | 7–13 | `repo_root` |
| `benchmark_composite.rs` | 44–125 | `repo_root`, `resolve_path`, `read_positional`, `format_mmss`, `unix_timestamp` |
| `benchmark_transparent.rs` | 31–106 | `repo_root`, `resolve_path`, `read_arg`, `read_positional`, `format_mmss`, `unix_timestamp` |
| `benchmark_widget_rate.rs` | 32–99 | `repo_root`, `resolve_path`, `read_positional`, `format_mmss`, `unix_timestamp` |

#### 7e. Verification

```bash
cargo check  # compiles all binaries
cargo test
```

Manually run one binary to verify CLI arg parsing still works:

```bash
cargo run --bin validate_activity -- --path=debug/activities/Test_GPX-parse-debug.json
```

**Keep in mind:**
- Master plan §3.10: "Diagnostic binaries may use `anyhow`. Core library code should use typed errors."
- The `read_arg` return type varies across binaries — `Result<String, String>` vs `Option<String>`. Standardize on `anyhow::Result<String>` for required args, `Option<String>` for optional args.
- The `repo_root()` in current binaries returns `CARGO_MANIFEST_DIR` (which is `src-tauri/`). In `bin_common.rs`, this may need to resolve to the workspace root depending on how binaries use it. Check each binary's `repo_root()` implementation during audit — some may prepend `parent()`.
- Binaries are not tested in CI — manual verification is required.

---

### Step 8: Remove Commented-Out Debug Code

**Duration estimate:** 15 min

#### 8a. Replace in `video_probe.rs`

**File:** `ovrley_core/src/encode/video_probe.rs`

7 commented-out `println!` statements at lines 159, 167, 174, 181, 189, 194, 200 (line numbers from audit — verify at execution time).

Replace with `log::debug!` or `tracing::debug!`:

```rust
// Before:
// println!("[OVRLEY] Probing video: {}", file_path);

// After:
log::debug!("Probing video: {}", file_path);
```

OR remove entirely if the diagnostic is no longer useful. The creation-time resolution chain is the most diagnostically valuable — keep those with `log::debug!`:

| Line | Message | Action |
|------|---------|--------|
| 159 | `Probing video: {file_path}` | `log::debug!` — useful for debugging import failures |
| 167 | `Found format.tags.creation_time` | `log::debug!` — useful for time resolution debugging |
| 174 | `Found streams[0].tags.creation_time` | `log::debug!` |
| 181 | `Found format.tags.com.apple.quicktime.creationdate` | `log::debug!` |
| 189 | `No creation time found... Using file system modified time` | `log::warn!` — this is a fallback, notable condition |
| 194 | `Fallback file modified time: {rfc3339}` | `log::debug!` |
| 200 | `Final selected creation time: {:?}` | `log::debug!` — summary of resolution |

**Keep in mind:**
- Master plan §10.1: No `println!` in core library logic. Use `log::debug!` / `log::warn!`.
- Master plan §10.4: Replace commented debug code with proper logging.
- The `log` crate is already a dependency of the project (via Tauri and other deps). Verify it's available in `ovrley_core/Cargo.toml`. If not, add `log = "0.4"`.
- The creation-time priority fallback (line 189) is notable — it's the "no metadata available" code path. `log::warn!` is appropriate because it indicates missing video metadata that may affect output.

#### 8b. Check for other commented-out debug statements

```bash
rg "//\s*println!\(" src-tauri/ovrley_core/src/
rg "//\s*eprintln!\(" src-tauri/ovrley_core/src/
```

Convert any others found to `log::debug!` or remove entirely.

#### 8c. Verification

```bash
cargo check -p ovrley_core
cargo clippy -- -D warnings
```

**Keep in mind:**
- Master plan §3.13: This cleanup was identified as LOW PRIORITY but is straightforward.
- Do not touch `tracing::debug!` that already exists — only convert commented-out `println!`.
- If the `log` crate is not in `ovrley_core/Cargo.toml`, add it. Use the same version as the rest of the workspace.

---

### Step 9: Integrate New Composite Modules

**Duration estimate:** 60 min

#### 9a. `ffmpeg_composite_profiles.rs` — Error Migration and Tests

**Current state:**
- `composite_profile_template(name: &str) -> Option<CompositeProfile>` — returns `Option`, not `CoreResult`
- 17 predefined profiles in `BUILTIN_PROFILES`
- No tests
- Codec name aliasing at lines 227–243

**Actions:**

**9a-i.** Change return type from `Option<CompositeProfile>` to `CoreResult<CompositeProfile>`:

```rust
// Before:
pub(crate) fn composite_profile_template(name_or_codec: &str) -> Option<CompositeProfile> {
    let normalized = match name_or_codec {
        "h264_nvenc" => "nvgpu_h264",
        "hevc_nvenc" => "nvgpu_hevc",
        // ...
        other => other,
    };
    BUILTIN_PROFILES.iter().find(|p| p.name == normalized).map(expand_template)
}

// After:
pub(crate) fn composite_profile_template(name_or_codec: &str) -> CoreResult<CompositeProfile> {
    let normalized = match name_or_codec {
        "h264_nvenc" => "nvgpu_h264",
        "hevc_nvenc" => "nvgpu_hevc",
        // ...
        other => other,
    };
    BUILTIN_PROFILES.iter()
        .find(|p| p.name == normalized)
        .map(expand_template)
        .ok_or_else(|| CoreError::Encode(format!(
            "Unknown composite profile: '{}' (normalized: '{}')",
            name_or_codec, normalized
        )))
}
```

**9a-ii.** Update callers of `composite_profile_template` to handle `CoreResult` instead of `Option`. Grep for callers:

```bash
rg "composite_profile_template" src-tauri/ovrley_core/src/
```

Likely caller: `ffmpeg_composite.rs` (or wherever composite ffmpeg settings are built). Update to use `?` operator.

**9a-iii.** Add snapshot tests for profile resolution:

```rust
// ovrley_core/tests/composite_profile_tests.rs (new file)

use ovrley_core::encode::ffmpeg_composite_profiles::composite_profile_template;

#[test]
fn resolves_known_profile_by_name() {
    let profile = composite_profile_template("software_h264").unwrap();
    assert_eq!(profile.codec, "libx264");
}

#[test]
fn resolves_by_codec_alias() {
    let profile = composite_profile_template("h264_nvenc").unwrap();
    assert_eq!(profile.codec, "h264_nvenc");
    // Verify filter chain includes CUDA-specific args
}

#[test]
fn unknown_profile_returns_error() {
    let result = composite_profile_template("nonexistent_codec");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unknown composite profile"));
}

#[test]
fn all_builtin_profiles_resolve() {
    let names = [
        "software_h264", "software_hevc",
        "nvgpu_h264", "nvgpu_hevc",
        "nnvgpu_h264", "nnvgpu_hevc",
        "qsv_h264", "qsv_hevc",
        "qsv_full_h264", "qsv_full_hevc",
        "mac_h264", "mac_hevc",
        "vaapi_h264", "vaapi_hevc",
        "amf_h264", "amf_hevc",
    ];
    for name in names {
        let profile = composite_profile_template(name)
            .unwrap_or_else(|e| panic!("Failed to resolve '{}': {}", name, e));
        assert!(!profile.output_args.is_empty(), "Profile '{}' has no output args", name);
    }
}
```

**Keep in mind:**
- Master plan §17.15: Profile name aliasing duplicates ffmpeg codec logic from `codec_detect.rs`. If the aliasing is complex, consider unifying — but this is a Phase 4+ concern. For Phase 3, only migrate error types and add tests.
- The `expand_template` function is private but uses `&'static str` slices — converting them to owned `String`s. This allocation is acceptable (called once per render, not per-frame).

---

#### 9b. `video_composite_debug.rs` — Error Migration Verification and Tests

**Current state (post-Phase 2):**
- `write_composite_timing_summary` should already return `CoreResult<PathBuf>` (if Phase 2 migrated it)
- No tests exist
- Debug directory path (`debug_render/phase_7/`) is hardcoded

**Actions:**

**9b-i.** Verify error migration is complete. If `write_composite_timing_summary` still returns `Result<T, String>`, migrate to `CoreResult<T>`:

```rust
// Before:
pub fn write_composite_timing_summary(input: CompositeTimingSummaryInput<'_>) -> Result<PathBuf, String> {
    // ...
    let output_dir = paths.debug_render_dir.join("phase_7").join(&video_id);
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Failed to create debug dir: {}", e))?;
    // ...
}

// After:
pub fn write_composite_timing_summary(input: CompositeTimingSummaryInput<'_>) -> CoreResult<PathBuf> {
    // ...
    let output_dir = paths.debug_render_dir.join("phase_7").join(&video_id);
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| CoreError::Io { path: output_dir.clone(), source: e })?;
    // ...
}
```

**9b-ii.** Extract hardcoded debug path constant:

```rust
// In video_composite_debug.rs (or a shared debug constants module)
const COMPOSITE_DEBUG_PHASE: &str = "phase_7";

// Usage:
let output_dir = paths.debug_render_dir.join(COMPOSITE_DEBUG_PHASE).join(&video_id);
```

**9b-iii.** Add tests for debug summary serialization and ID derivation:

```rust
// ovrley_core/tests/composite_debug_tests.rs (new file)
// Note: tests for composite_debug_id require knowing the output filename convention
// These are unit-level tests — no actual file I/O needed
```

**9b-iv.** Consider folding `video_composite_debug.rs` into `video_debug.rs`:

Master plan §5 Phase 3 deliverable 6 asks to "Consider whether `video_composite_debug.rs` should be folded into `video_debug.rs` with shared debug logic."

**Decision:** Keep separate for now. The two modules serve different pipelines (transparent vs composite). The `video_composite_debug.rs` is 251 lines and has composite-specific concepts (`CompositeTimingSummary`, `PerFrameTiming`). Folding them would create a larger module that mixes two pipeline domains. If shared debug logic emerges later (e.g., a common `DebugSummary` trait), refactor then.

#### 9c. Verification

```bash
cargo test -p ovrley_core composite_profile
cargo test -p ovrley_core composite_debug
cargo test -p ovrley_core  # full suite
```

**Keep in mind:**
- Master plan §17.15 and §17.16: These modules were identified as needing error migration and tests.
- If `video_composite_debug.rs` was already migrated to `CoreResult` in Phase 2, skip 9b-i and focus on 9b-ii and 9b-iii.
- The `composite_debug_id` function strips `"video_composited_"` prefix from output filenames — this is brittle. Document the dependency but do not refactor the naming convention in Phase 3.

---

### Step 10: Full Integration Test Run

**Duration estimate:** 30 min

After all steps complete, run the full test suite and document results:

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
```

#### 10a. Verify no remaining duplication

```bash
# RDP: verify perpendicular_distance no longer appears in route.rs or elevation.rs
rg "perpendicular_distance" src-tauri/ovrley_core/src/render/widgets/

# Interpolation: verify common.rs doesn't have its own interpolation functions
rg "fn interpolate_numeric_points|fn interpolate_numeric_series_many" src-tauri/ovrley_core/src/render/widgets/common.rs

# CLI: verify no duplicate read_arg in binary files
rg "fn read_arg\(" src-tauri/src/bin/

# AppPaths: verify encode no longer imports from commands
rg "use crate::commands::AppPaths" src-tauri/ovrley_core/src/encode/
```

#### 10b. Verify dependency graph

```
interpolation (leaf)
rdp (leaf)
paths (leaf)
types (leaf — Phase 2)
error (leaf — Phase 2)
```

All new modules must be leaves — no imports from other core domains.

```bash
# Verify each new module only imports std, error, serde as allowed
rg "^use crate::" src-tauri/ovrley_core/src/interpolation.rs
rg "^use crate::" src-tauri/ovrley_core/src/rdp.rs
rg "^use crate::" src-tauri/ovrley_core/src/paths.rs
```

#### 10c. Verify common.rs size

```bash
wc -l src-tauri/ovrley_core/src/render/widgets/common.rs
```

Target: ~200 lines. If >300 lines, consider additional extraction (but only if cohesion justifies it).

**Keep in mind:**
- If Phase 2 introduced `CoreResult` but some Phase 3 changes temporarily break compatibility, fix incrementally.
- The `#[path = "../bin_common.rs"]` pattern in binaries may trigger clippy warnings — check and address.
- Do not have two parallel error systems. After migration, `ffmpeg_composite_profiles.rs` must return `CoreResult`, not `Option`.

---

### Step 11: Manual Testing Checklist

After completing all steps, manually verify:

- [ ] App starts (Tauri window opens)
- [ ] Route widget displays correctly in preview (no visual regression)
- [ ] Elevation widget displays correctly in preview (no visual regression)
- [ ] Preview render works — overlay displays on video
- [ ] Transparent overlay export works
- [ ] Composite MP4 export works
- [ ] Cancel a render — cancellation works, UI updates correctly
- [ ] Progress UI still updates during render
- [ ] Output paths and filenames unchanged
- [ ] Diagnostic binaries run correctly:
  - [ ] `cargo run --bin render_preview -- --config=... --activity=...`
  - [ ] `cargo run --bin render_video -- --config=... --activity=...`
  - [ ] `cargo run --bin validate_activity -- --path=...`
  - [ ] `cargo run --bin parallel_render -- --config=...`
- [ ] Composite profiles resolve correctly (no "Unknown composite profile" errors for standard profiles)
- [ ] No measurable hot-path regression in preview render

**If any manual test fails, stop and fix before proceeding.** Do not mark Phase 3 complete with broken behavior.

---

## Files Changed in Phase 3

### New files

| File | Contents |
|------|----------|
| `ovrley_core/src/rdp.rs` | Shared RDP simplification (`perpendicular_distance`, `simplify_rdp_indices`) |
| `ovrley_core/src/interpolation.rs` | Consolidated interpolation (`interpolate_points`, `interpolate_numeric_series_value`, `interpolate_optional_numeric_series`, `collect_valid_numeric_points`) |
| `ovrley_core/src/paths.rs` | `AppPaths` struct and methods extracted from `commands/mod.rs` |
| `ovrley_core/src/render/widgets/geometry.rs` | Point/rect/math and layout-fitting helpers |
| `ovrley_core/src/render/widgets/polyline.rs` | Polyline and area drawing |
| `ovrley_core/src/render/widgets/marker.rs` | Marker/dot drawing |
| `ovrley_core/src/render/widgets/transform.rs` | Skia path and coordinate transform helpers |
| `src-tauri/src/bin_common.rs` | Shared CLI helpers for diagnostic binaries |
| `ovrley_core/tests/rdp_tests.rs` | RDP snapshot tests (added before extraction) |
| `ovrley_core/tests/composite_profile_tests.rs` | Composite profile resolution tests |
| `ovrley_core/tests/composite_debug_tests.rs` | Composite debug summary tests |

### Modified files

| File | Change |
|------|--------|
| `ovrley_core/src/lib.rs` | Add `pub mod rdp;`, `pub mod interpolation;`, `pub mod paths;` |
| `ovrley_core/src/activity/interpolate.rs` | Remove duplicated functions; re-export from `interpolation.rs` |
| `ovrley_core/src/render/widgets/route.rs` | Replace inline RDP with calls to `crate::rdp` |
| `ovrley_core/src/render/widgets/elevation.rs` | Replace inline RDP with calls to `crate::rdp` |
| `ovrley_core/src/render/widgets/common.rs` | Reduce to ~200 lines; move groups to sub-modules; delegate interpolation to `interpolation.rs` |
| `ovrley_core/src/render/widgets/mod.rs` | Add `pub mod geometry;`, `pub mod marker;`, `pub mod polyline;`, `pub mod transform;` |
| `ovrley_core/src/commands/mod.rs` | Remove `AppPaths` definition; add `pub use crate::paths::AppPaths;` re-export |
| `ovrley_core/src/encode/video.rs` | Update `AppPaths` import from `crate::commands` to `crate::paths` |
| `ovrley_core/src/encode/video_probe.rs` | Replace commented-out `println!` with `log::debug!`/`log::warn!` |
| `ovrley_core/src/encode/ffmpeg_composite_profiles.rs` | Change `composite_profile_template` return type from `Option` to `CoreResult` |
| `ovrley_core/src/encode/video_composite_debug.rs` | Verify `CoreResult` migration; extract hardcoded path constant |
| `ovrley_core/src/encode/ffmpeg_composite.rs` | Update callers of `composite_profile_template` for `CoreResult` |
| `src-tauri/src/bin/render_preview.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/render_video.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/validate_activity.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/parallel_render.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/benchmark_composite.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/benchmark_transparent.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/bin/benchmark_widget_rate.rs` | Remove inline helpers; add `#[path = "../bin_common.rs"] mod common;` |
| `src-tauri/src/lib.rs` | Update `AppPaths` import (if referenced directly) |
| Any other files importing `AppPaths` from `commands` | Update import path |

---

## Dependency Graph After Phase 3

```
interpolation (leaf)
rdp (leaf)
paths (leaf)
types (leaf — Phase 2)
error (leaf — Phase 2)

config ──→ types, error
activity ──→ config, interpolation, types, error
render ──→ config, activity, types, error
  render::widgets ──→ interpolation, rdp
    geometry (leaf — std only)
    polyline (leaf — Skia + types)
    marker (leaf — Skia + types)
    transform (leaf — Skia)
encode ──→ config, activity, render, paths, types, error
commands ──→ config, activity, render, encode, paths, types, error

Note: encode → commands dependency is eliminated.
      encode → paths dependency replaces encode → commands (for AppPaths).
```

---

## Blockers and Known Risks

### Risk 1: Float Equality in RDP Tuple Round-Trip

The naive approach of mapping simplified `(f32, f32)` tuples back to `RouteSample`/`ElevationSample` by coordinate equality is fragile. Two samples may share identical coordinates but differ in other fields (timestamp, speed).

**Mitigation:** Use the index-based approach (`simplify_rdp_indices` → `Vec<usize>`) to avoid float equality entirely. This is the recommended approach in Step 4c.

### Risk 2: Interpolation Precision Shift

If `common.rs` interpolation actually uses f32 internally (contrary to the code audit showing f64), switching to the authoritative f64 versions could shift frame progress values. Even a 1-frame shift could affect output alignment.

**Mitigation:** During the Step 1 audit, carefully verify whether `common.rs:87` `interpolate_numeric_points` uses f32 or f64. If f32 is confirmed, run both versions side-by-side on a representative activity and compare frame assignments. If differences exist, document as an intentional behavior change with f64 being the improvement.

### Risk 3: Hot-Path Regression from Interpolation Consolidation

`interpolate_points` is called per-frame during progress lookup. Moving the function to a new module doesn't change performance, but changing the signature or adding error handling inside the loop could.

**Mitigation:** Keep the consolidated functions as free functions (no methods, no generics, no error wrapping). The hot-path behavior is identical — only the import path changes.

### Risk 4: Binaries Break After CLI Consolidation

The `#[path = "../bin_common.rs"]` approach may have edge cases with Cargo's binary compilation. Each binary is compiled as a separate crate root, and relative path resolution may behave unexpectedly.

**Mitigation:** Verify `cargo check` compiles all binaries. If `#[path]` causes issues, alternative: create a small `src-tauri/src/bin_common/` directory with `mod.rs` and have binaries reference it as a module. Since binaries are standalone, this still requires the `#[path]` attribute.

### Risk 5: AppPaths Re-Export Creates Interim Confusion

The `pub use crate::paths::AppPaths;` re-export in `commands/mod.rs` means `AppPaths` is importable from both `crate::commands` and `crate::paths`. This is intentional backward compatibility but could confuse future developers.

**Mitigation:** Add a `// TODO: Remove re-export in Phase 6 after all callers use crate::paths` comment above the re-export. Track this in the Phase 6 checklist.

### Risk 6: common.rs Callers Missed

After splitting `common.rs`, callers that imported from `common::` for moved functions will fail to compile. Grep should catch all of these, but some may be in test files or conditional compilation blocks.

**Mitigation:** Run `cargo check` after each sub-module extraction (not just at the end). Fix import errors incrementally. Run `cargo test` to catch test-file import errors.

### Risk 7: Phase 2 Incompleteness

If Phase 2 did not fully migrate all modules to `CoreResult`, the `ffmpeg_composite_profiles.rs` migration (from `Option` to `CoreResult`) may cascade into callers that still use `Result<T, String>`.

**Mitigation:** Before starting Step 9, verify that `ffmpeg_composite.rs` and all composite pipeline callers use `CoreResult`. If not, note the gap and do the minimum migration to keep things compiling — but do not expand Phase 3 scope to finish Phase 2 work.

---

## Summary Checklist Before Marking Phase 3 Complete

1. [ ] Step 1 audit complete — all target files read, line numbers verified
2. [ ] RDP snapshot tests added and passing (Step 2)
3. [ ] `interpolation.rs` created, `activity/interpolate.rs` delegates to it, `common.rs` delegates to it (Step 3)
4. [ ] `rdp.rs` created, `route.rs` and `elevation.rs` delegate to it (Step 4)
5. [ ] `common.rs` split into `geometry.rs`, `polyline.rs`, `marker.rs`, `transform.rs` (Step 5)
6. [ ] `common.rs` reduced to ~200 lines (Step 5)
7. [ ] `paths.rs` created, `AppPaths` moved, `encode` imports from `paths` not `commands` (Step 6)
8. [ ] `bin_common.rs` created, all 7 binaries updated (Step 7)
9. [ ] Commented-out `println!` in `video_probe.rs` replaced with `log::debug!`/`log::warn!` (Step 8)
10. [ ] `ffmpeg_composite_profiles.rs` returns `CoreResult`, tests added (Step 9a)
11. [ ] `video_composite_debug.rs` verified for `CoreResult`, hardcoded path extracted (Step 9b)
12. [ ] `cargo fmt` passes
13. [ ] `cargo test` passes (all tests, including new Phase 3 tests)
14. [ ] `cargo clippy -- -D warnings` passes
15. [ ] No duplicated RDP logic in `route.rs` or `elevation.rs`
16. [ ] No duplicated interpolation logic in `common.rs`
17. [ ] `encode` does not depend on `commands` (no `use crate::commands` in encode/)
18. [ ] All 7 diagnostic binaries compile and run correctly
19. [ ] No commented-out `println!` in `video_probe.rs`
20. [ ] Manual testing checklist (Step 11) complete
21. [ ] No production behavior changed (except intentional improvements documented as such)
22. [ ] No new `#[allow(...)]` suppressions
23. [ ] No circular dependencies introduced (all new modules are leaves)
24. [ ] New public functions in `interpolation.rs` and `rdp.rs` have documentation
25. [ ] `AppPaths` re-export in `commands/mod.rs` has a TODO comment for Phase 6 removal

---

## Cross-Reference: Phase 3 vs Master Plan

| Master Plan Reference | Phase 3 Deliverable | Covered In |
|----------------------|---------------------|------------|
| §3.6 Duplicated RDP | Extract `rdp.rs` | Steps 2, 4 |
| §3.7 Duplicate Interpolation | Consolidate `interpolation.rs` | Steps 1b, 3 |
| §3.8 Large `common.rs` | Split `common.rs` | Steps 1c, 5 |
| §3.10 Bin Boilerplate | Consolidate CLI helpers | Steps 1d, 7 |
| §3.13 Commented-Out Debug | Clean up `video_probe.rs` | Step 8 |
| §3.14 encode→commands Dep | Move `AppPaths` to `paths.rs` | Steps 1f, 6 |
| §17.15 `ffmpeg_composite_profiles` | Error migration + tests | Step 9a |
| §17.16 `video_composite_debug` | Verify errors, extract constant | Step 9b |
| §4.1 Target Core Layout | `rdp.rs`, `interpolation.rs`, `paths.rs` in tree | Steps 3, 4, 6 |
| §8.1 Dependency Direction | `paths`/`interpolation`/`rdp` are leaves | Step 10b |
| §13.2 Shared Algorithms | Tests before extraction, free functions | Steps 2, 4 |
