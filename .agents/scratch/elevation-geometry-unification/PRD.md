Status: ready-for-agent

# Elevation Geometry Unification

## Problem Statement

The elevation profile geometry pipeline (Savitzky-Golay smoothing, even-spacing downsampling, y_scale projection, RDP simplification) is duplicated nearly line-for-line between Rust (`reduction.rs` + `prepare.rs`) and JavaScript (`elevationGeometry.js`). The two implementations must be kept in sync manually — any bug fix or feature addition (e.g., the vertical-segment preservation in `elevation-fix.md`) must be implemented twice, in two different languages, with no shared test coverage.

This duplication exists because the JS frontend needs real-time geometry for the preview (driven by user-controlled sliders for y_scale, density, simplification tolerance), while the Rust backend needs the same geometry for final video rendering. Today, both sides compute geometry independently from the same raw activity data.

## Solution

**Make Rust the single source of truth for elevation geometry construction.** Expose the existing Rust geometry pipeline as a Tauri IPC command. The JS frontend calls this command when parameters change, receives pre-built geometry, and uses it directly for SVG rendering. Per-frame operations (marker interpolation, completed polyline filtering) remain local in JS — they are cheap linear scans that must execute at 30fps with zero latency.

| Concern | Who handles it | Frequency |
|---------|---------------|-----------|
| Geometry construction (smooth → downsample → project → simplify) | Rust | On parameter change only |
| Per-frame marker position + completed polyline | JS (local) | Every frame (~33ms) |
| Final video rendering | Rust (same geometry) | Per frame during encode |

### Why not build geometry at parse time?

Geometry depends on template parameters (widget dimensions, y_scale, simplify_tolerance_px, target_density, export range, show_full_activity toggle) that are not available during activity parsing. These are user-controlled settings that change at runtime.

### Why not send geometry per frame over IPC?

The preview updates at 30fps. IPC round-trip latency would make the preview unusable. The per-frame operations (marker interpolation, polyline filtering) are trivially cheap on the simplified geometry — they stay in JS.

## Architecture

```
User moves slider
  → JS sends params to Rust via invoke('backend_build_elevation_geometry')
  → Rust runs existing pipeline (fast — thousands of points)
  → Rust returns WidgetGeometry { points, progressValues, bbox, ... }
  → JS receives geometry, stores in state
  → JS renders preview at 30fps using local interpolation (zero latency)

Final video render
  → Rust uses same WidgetGeometry for frame states + Skia drawing
  → Preview and render are mathematically identical (WYSIWYG)
```

## IPC Contract

### Request

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildElevationGeometryRequest {
    pub config_json: String,           // full ValidatedRenderConfig JSON
    pub parsed_activity_json: String,  // full ParsedActivity JSON
}
```

Follows the existing `backend_render` pattern. The command parses both internally, extracts `elevation_plot` + `scene` from the validated config.

### Response

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevationGeometryResponse {
    pub points: Vec<[f32; 2]>,         // [[x,y], ...] projected widget-space
    pub progress_values: Vec<f32>,     // per-point 0..1 distance progress
    pub bbox: [f32; 4],                // [min_x, min_y, max_x, max_y]
    pub source_point_count: usize,     // raw samples before simplification
    pub simplification: String,        // diagnostic label
    pub widget_width: u32,             // scaled widget width
    pub widget_height: u32,            // scaled widget height
}
```

Uses `[f32; 2]` (not tuples) so serde produces `[[x,y], ...]` JSON arrays.

## Implementation Decisions

### What moves to Rust

- Export-range scoping (`buildScopedElevationSeries` logic) — already exists in `build_elevation_source_points`
- Savitzky-Golay smoothing — already exists in `smooth_elevation_points`
- Even-spacing downsampling — already exists in `select_evenly_spaced_elevation_points`
- Min/max normalization + y_scale projection — already exists in `project_elevation_points`
- RDP simplification — already exists in `simplify_elevation_samples`

### What stays in JS

- Marker interpolation (`getPointAtMetricProgress`) — cheap linear scan, needs 30fps
- Completed polyline filtering (`buildElevationCompletedPoints`) — cheap filter, needs 30fps
- Elevation label interpolation (`getInterpolatedSeriesValue`) — cheap linear interpolation
- SVG path materialization (`pointsToSvg`, `areaToSvg`) — string formatting
- Progress computation (`getWindowProgressAtTime`) — time→progress lookup

### What gets deleted

- `elevationGeometry.js` — entire file (~240 lines), the full duplicated reduction pipeline
- `buildScopedElevationSeries()` in `exportRange.js` — scoping moves to Rust
- Elevation geometry constants in `overlayEditorConstants.js` — no longer used in JS

### Visibility changes in Rust

Three existing functions need `pub` visibility (currently private or `pub(crate)`):
- `build_elevation_source_points` in `prepare.rs`
- `build_elevation_geometry` in `prepare.rs`
- `normalize_elevation_plot` in `normalize.rs`

## Impact on elevation-fix.md

This unification is a prerequisite for the elevation fix. Once geometry is built in Rust only:
- The vertical-segment preservation (0.5m threshold) is implemented once in `reduction.rs`
- The `elapsed_fractions` and `elevation_data_range` fields are added to the response struct
- The JS hook consumes these enriched fields for chronological completion filtering
- No duplication of the fix logic

## Testing Decisions

- Rust command: unit test that builds geometry from a known activity + config and verifies point count, progress range, bbox
- JS hook: integration test that mocks the IPC response and verifies SVG rendering
- Manual verification: elevation preview matches final render for cycling and drone activities
- `cargo test` and `pnpm test` must pass

## Out of Scope

- The elevation fix itself (vertical-segment preservation, elapsed fractions) — separate issue
- Changing the route widget — not affected
- Changing the elevation x-axis to time-based — explicitly rejected
- WASM or other shared-language approaches
