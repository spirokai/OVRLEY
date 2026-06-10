Status: ready-for-agent

# Elevation Profile: Handle Vertical Motion During Stops/Hovers

## Problem Statement

The elevation profile widget assumes `distance_progress → elevation` is a single-valued function. This is true for cycling/running (distance only increases forward, red-light stops produce flat elevation at a flat x), but **false for drones**. A drone can hover, ascend, or descend at the same GPS location for extended periods, producing many consecutive samples with **identical `distance_progress` but changing `elevation`**.

### Evidence from sample data

**`DJI-sample2-parse-debug.json`** (300 samples, 299 s):
- Samples 0–16: `distance_progress = 0` for all 17 samples, `elevation` rises from 14m → 36m, `course_points` stays at `[149.0251, -20.2532]`
- The drone spends 17 seconds climbing vertically at the same ground position.

**`DJI-sample1-parse-debug.json`** (9000 samples, 300 s):
- A plateau from 11.866s to 14.866s: `distance_progress` near-constant while `elevation` rises 861.712m → 866.112m.
- Across the full file: 873 duplicate-distance runs, 811 with altitude change.

### How the bug manifests

With the current Rust-geometry architecture, the bug manifests in the Rust backend only:

| Layer | File | Failure |
|-------|------|---------|
| **Rust geometry** | `reduction.rs:99-107` | Downsampler drops consecutive samples with identical `progress01`. A 17-sample hover-climb collapses to a single point. |
| **Rust marker** | `frame_state.rs:48-54` | Marker `(x, y)` is resolved from `point_at_metric_progress_with_cursor()` — a progress→geometry lookup. Since the collapsed geometry has only one point at the hover position, the marker's y is stuck regardless of actual altitude. |
| **Rust completed profile** | `frame_state.rs:90-94` | Completed profile is built by `progress ≤ currentProgress`. All vertical-segment points share the same progress, so they are either fully included or fully excluded — partial fill of a vertical segment is impossible. |

The `srt-parser.js` parser is **not** the problem — it correctly preserves raw flight samples with duplicate GPS positions and changing altitude. The breakage starts in widget geometry preparation.

### What is NOT affected

- **Route widget**: The route has no altitude axis. Hovering at the same GPS produces a stationary marker — this is correct and must not change.
- **Elevation label value** (Rust only): `frame_state.rs:56-66` reads `dense_activity.series.elevation[frame_index]`, which IS per-frame and interpolated from elapsed time. The numeric label is already correct; only the visual marker position and drawn profile are wrong.
- **JS frontend**: The hooks (`useElevationPreviewGeometry`, `useRoutePreviewGeometry`) now consume Rust geometry via IPC. No JS geometry code needs to change — Rust is the single source of truth.

## Solution Summary

**The route plot stays fully distance-based. The elevation profile keeps distance on the x-axis but drives marker position, label value, and completed-profile construction from elapsed time instead of distance progress.**

| Component | Decision | Rationale |
|-----------|----------|-----------|
| Route plot x-axis | Keep distance-based | Marker must not drift during stops; drone hover at same GPS is correct behavior |
| Elevation x-axis | Keep distance-based | Same reason — cycling compatibility; marker_x stays put during hover |
| Elevation y-axis | Altitude, as before | No change |
| Duplicate x values | **Preserve** when elevation span is meaningful | These represent vertical flight segments |
| Marker position x | From `distanceProgressAtTime` (unchanged) | Stays stable during hover |
| Marker position y | From `elevationAtTime` projected into widget y-scale | Correctly tracks altitude change during hover |
| Completed profile | Build by chronological sample order, not `progress ≤` | Allows partial fill of vertical segments as time advances |
| Elevation label value | Already correct in Rust | No change needed |

### Why not time-based x-axis?

Switching the elevation x-axis to elapsed time would make the marker drift right during drone hovers AND cycling red-light stops. This directly regresses the original design constraint that motivated distance-based progress. The x-axis semantics ("ground-distance cross section") are correct; the bug is in how the y-coordinate and completed profile are resolved.

## Threshold Design: When to Preserve Duplicate-Distance Runs

Not all duplicate-progress runs should be preserved. Cycling barometric noise during a red-light stop typically varies by <0.3m. Preserving every noise sample as a vertical scribble wastes the downsampling budget and creates visual artifacts.

**Threshold**: `0.5m` elevation span across a consecutive duplicate-progress run.

| Scenario | Elevation span | Action | Rationale |
|----------|---------------|--------|-----------|
| Drone hover-climb (DJI-sample2) | 22m | **Keep all points** | Real vertical flight segment |
| Drone hover-climb (DJI-sample1) | 4.4m | **Keep all points** | Real vertical flight segment |
| Cycling red-light stop (barometric noise) | <0.3m | **Collapse to single median** | Just noise; single point suffices |
| Mountain bike stopped on steep slope (GPS drift + pressure change) | >1m | **Keep all points** | Real altitude change, correctly shown |

**Implementation strategy**: The preserve decision must happen **before** candidate selection, not after. Even-spacing alone can produce 0–1 candidates from a short but meaningful hover-climb (e.g., a 3-second plateau in a 300-second activity at low target density), so a post-pass would never see the run.

Two-step approach in `downsample_elevation_points()`:
1. **Pre-scan** the smoothed points to identify consecutive runs sharing the same `progress01` (within `f32::EPSILON`). For each run where `max_elevation - min_elevation ≥ 0.5m`:
   - Mark **every point** in the run with `preserve = true` in the `ReducedElevationPoint` array.
   - Also mark the first and last points of the smoothed input as `preserve` (endpoints, existing behavior).
2. **Evenly-spaced selection** modified to never skip a preserved point: if the candidate at `source_index` is marked `preserve`, include it unconditionally (even if its progress duplicates the previous selection). Non-preserved neighbors at the same progress are still skipped. This replaces the blanket "skip if same progress as last" check.

This guarantees that (a) meaningful vertical runs always contribute at least their preserved points to the output, (b) RDP cannot collapse them (existing `preserved_indexes` windowing), and (c) low target densities don't silently drop short climbs.

## Implementation Plan

**Principle**: Rust backend is the single source of truth for geometry generation. The JS frontend consumes Rust geometry via IPC — no duplicated geometry code in JS.

**Architecture context**: The elevation geometry pipeline lives entirely in Rust (`ovrley_core/src/render/widgets/elevation/`). The JS hook `useElevationPreviewGeometry` calls `buildElevationGeometry()` via Tauri IPC and receives pre-built geometry. Per-frame operations (marker interpolation, completed segment splitting, SVG path materialization) remain local in JS for 30fps performance.

---

### Phase 1 — Rust backend: Full elevation fix

**Four interconnected changes across `reduction.rs`, `prepare.rs`, `frame_state.rs`, `draw.rs`, `types.rs`:**

**A) Carry elapsed-time information into geometry**

- `types.rs` — `WidgetGeometry`: add `elapsed_fractions: Vec<f32>` (chronological ordering 0..1, parallel to `progress_values`) and `elevation_data_range: Option<(f64, f64)>` (min/max elevation in meters for y-projection).
- `reduction.rs` — `ElevationSample`: add `elapsed_fraction: f32` field.
- `prepare.rs` — `build_elevation_source_points()`: compute `elapsed_fraction` from the original `activity.sample_elapsed_seconds` (not sample index), normalized to 0..1 against the source duration for the exact geometry being built. For trimmed windows, that source duration is the trimmed `sample_elapsed_seconds.last()` (0..end-start). This same duration must be stored or otherwise made available so frame-state normalization uses the exact same denominator as geometry; do NOT normalize geometry against sample duration and frame state against `dense_activity.frame_elapsed_seconds.last()`. Store `(min_elevation, max_elevation)` into `geometry.elevation_data_range`.
- The raw-point type changes from `Vec<(f32, f64)>` (progress, elevation) to a struct or triple carrying `(progress01, elevation, elapsed_fraction)`. This flows through smoothing, selection, projection, and simplification into `ElevationSample.elapsed_fraction`.

**B) Preserve vertical segments in downsampler**

- `reduction.rs` — `smooth_elevation_points()` or a new pre-pass: before even-spaced selection, scan the smoothed `ReducedElevationPoint` array for consecutive runs sharing the same `progress01` (within `f32::EPSILON`). For each run where `max_elevation - min_elevation ≥ 0.5m`, set `preserve = true` on every point in the run. Endpoints of the entire array are also marked `preserve` (existing behavior).
- `reduction.rs` — `select_evenly_spaced_elevation_points()`: replace the blanket dedupe (lines 99–107) with a targeted check: skip a candidate only if its progress equals the previous selection's progress AND the candidate is NOT marked `preserve`. Preserved points are always admitted.
- No change to RDP — `simplify_elevation_samples()` already protects `preserve`-flagged points via `preserved_indexes` windowing.

**C) Drive marker-y from elapsed-time elevation**

- `reduction.rs` — extract y-projection formula into `pub(crate) fn project_single_elevation_y(elevation_m: f64, min: f64, max: f64, height: f32, margin: f32, y_scale: f32) -> f32`. Make `project_elevation_points()` delegate to it.
- `prepare.rs` — pass `&NormalizedElevationPlot` to `build_elevation_frame_states()` (currently only receives geometry).
- `frame_state.rs` — `build_elevation_frame_states()`: keep `marker_x` from `point_at_metric_progress_with_cursor()`. Compute `marker_y` by projecting `elevation_m` (from `dense_activity.series.elevation[frame_index]`) via `project_single_elevation_y()` using `geometry.elevation_data_range` and plot params. Fall back to existing geometry lookup if range is `None`.

**D) Build completed profile by chronological order**

- `types.rs` — `ElevationFrameState`: add `frame_elapsed_fraction: f32`.
- `frame_state.rs` — `build_elevation_frame_states()`: compute `frame_elapsed_fraction` from actual elapsed time, not frame index. Normalize `dense_activity.frame_elapsed_seconds[frame_index]` against the same source duration used when computing `geometry.elapsed_fractions` in Phase 1A (for example, a stored geometry/scoped duration), not against `dense_activity.frame_elapsed_seconds.last()`. This keeps geometry chronology and frame chronology on the same 0..1 basis even though the dense frame timeline intentionally stops before the exact scene end.
- `frame_state.rs` — `build_elevation_completed_points()`: change params from `(points, progress_values, progress01, marker_point)` to `(points, elapsed_fractions, frame_elapsed_fraction, marker_point)`. Filter by `elapsed_fractions[i] ≤ frame_elapsed_fraction`.
- `draw.rs` — `draw_elevation_widget()`: pass `&geometry.elapsed_fractions` and `state.frame_elapsed_fraction` instead of `&geometry.progress_values` and `state.progress01`.
- `frame_state.rs` — `interpolate_elevation_for_progresses()` (the fallback path when `dense_activity.series.elevation` is unavailable): switch from progress-based to elapsed-based interpolation. Pass `dense_activity.frame_elapsed_seconds` as the per-frame target x-axis and `scene.start` as the offset so the lookup target is `scene.start + frame_elapsed_seconds[frame_index]`, matching `activity.sample_elapsed_seconds` as the interpolation x-domain. Without this, the fallback path would drift for custom export windows where `scene.start != 0`.

**E) Unit tests**

New file `src/render/widgets/tests/elevation_reduction_tests.rs`:
- `downsample_preserves_meaningful_vertical_run` — 10 consecutive same-progress samples spanning 5m, multiple points survive.
- `downsample_collapses_noise_vertical_run` — 10 consecutive same-progress samples spanning 0.1m, output is a single point.
- `downsample_mixed_runs` — alternating forward progress + vertical runs, correct points in each.

New file `src/render/widgets/tests/elevation_frame_state_tests.rs`:
- `marker_y_follows_elevation_during_hover` — vertical segment geometry, frame states show changing marker_y.
- `completed_points_fills_vertical_segment_chronologically` — mid-hover frame, completed points include partial vertical segment.

Wire new modules in `tests/mod.rs`. Verify `cargo test` passes.

**Acceptance criteria:**
- [ ] DJI-sample2 geometry: multiple distinct y-values at x ≈ 0 after simplification.
- [ ] DJI-sample2 frame states: `marker_y` changes during first 17s while `marker_x` stays constant.
- [ ] DJI-sample2 completed profile: at mid-hover, area fills partially up the vertical segment.
- [ ] Cycling/red-light activity: marker has no horizontal drift during stops. Any vertical movement at stop positions corresponds only to real altitude changes ≥0.5m (no noise-level scribbles).
- [ ] `cargo test` passes with new + existing tests.
- [ ] Route widget unaffected.

---

### Phase 2 — JS frontend: Update hooks to consume new geometry shape

Since Rust is the single source of truth for geometry, the JS changes are minimal — only the hooks that consume the Rust response need updating to handle the new fields.

**Files to modify:**

`useElevationPreviewGeometry.js`:
- The hook already calls `buildElevationGeometry()` via IPC and receives the Rust response.
- After Phase 1, the Rust response will include `elapsedFractions` and `dataRange` fields.
- Update the hook to use `elapsedFractions` instead of `progressValues` for completed profile construction.
- Update `buildElevationCompletedPoints()` call to pass `elapsedFractions` and `frameElapsedFraction`.
- The marker-y computation and elevation label already use elapsed-time interpolation in Rust — no change needed.

`svgPreviewUtils.js` — `buildElevationCompletedPoints()`:
- Change signature: accept `elapsedFractions` + `frameElapsedFraction` instead of `progressValues` + `progress01`.
- Filter by `elapsedFractions[i] ≤ frameElapsedFraction`.

**No changes needed to:**
- `elevationGeometry.js` — this file should be deleted (geometry is now in Rust).
- `exportRange.js` — `buildScopedElevationSeries()` is no longer used by the geometry hook.
- Any geometry computation code — all in Rust now.

**Acceptance criteria:**
- [ ] DJI-sample2 preview: marker y changes during first 17s, marker x stays constant.
- [ ] DJI-sample2 preview: completed area fills upward chronologically during hover.
- [ ] Elevation label shows changing altitude during hover.
- [ ] Cycling preview: marker and label unchanged from current behavior.
- [ ] `pnpm test` passes, `pnpm lint` passes.

---

### Phase 3 — Integration verification

Manual verification with real sample data:

1. **DJI-sample2.SRT** (300 samples, Format B):
   - Elevation marker climbs vertically at x=0 during first 17s.
   - Completed area grows upward during climb, full vertical segment filled by 17s.
   - Route marker stays stationary during climb.
   - After 17s, both widgets behave normally for forward flight.

2. **DJI-sample1.SRT** (9000 samples, Format A):
   - Multiple hover-climb/descent segments visible at correct x positions.
   - Marker tracks altitude correctly during each hover.
   - No regression in forward-flight segments.

3. **Cycling .FIT with red-light stops**:
   - Marker stays stable horizontally during stops (no drift).
   - No noise-level vertical scribbles at stop positions (<0.5m barometric noise collapsed).
   - Overall profile appearance unchanged.

4. **Running .GPX**:
   - Normal behavior, no regressions.

## Regression Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Barometric noise during cycling stops creates visible vertical scribbles | Low | 0.5m threshold collapses noise; only spans ≥0.5m produce vertical segments |
| `elevation_data_range` mismatch between geometry projection and marker projection | Low | Use exact same projection formula (`project_single_elevation_y`) in both places; add test |
| `elapsed_fraction` mismatch between geometry and frame state | Low | Both are normalized against the exact same scoped/source duration. Geometry stores or exposes that duration; frame state reuses it instead of `dense_activity.frame_elapsed_seconds.last()`. |
| Route widget accidentally picks up elapsed-fraction logic | Very low | Route widget has its own `progress_values` and `frame_state.rs`; no shared code changes |
| Custom export ranges break because trim operates on elapsed but geometry was built from full activity | Medium | `show_full_activity` and `custom_export_range_active()` already handle this split; the elapsed_fraction is computed on the trimmed source (not full activity) when trimming is active. Verify in Phase 3. |

## Non-Goals

- Changing the route plot to time-based — explicitly rejected.
- Changing the elevation x-axis to time-based — explicitly rejected.
- Adding a drone-specific detection mode — the fix is general and applies to any activity with duplicate-distance runs.
- Changing the parser (`srt-parser.js`) — it is correct as-is.
- Duplicating geometry logic in JS — Rust is the single source of truth; JS hooks consume geometry via IPC.
