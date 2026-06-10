Status: ready-for-agent

# Route Geometry Unification

## Problem Statement

The route geometry pipeline (Mercator projection, LTTB downsampling, RDP simplification, widget fitting) is duplicated nearly identically between JavaScript (`routeGeometry.js`, 321 lines) and Rust (`route/simplify.rs` + `route/prepare.rs`, 396 lines). Every algorithm change — LTTB bucket selection, RDP tolerance behavior, Mercator approximation — must be ported manually between two languages with no shared test coverage.

The elevation geometry unification (issue #01–#02) solved the same problem for the elevation widget by exposing the Rust geometry pipeline via IPC and having the JS hook consume it. The route widget should follow the same architecture.

## Solution

**Make Rust the single source of truth for route geometry construction.** Expose the existing Rust route pipeline (Mercator projection, LTTB downsampling, RDP simplification, widget fitting) as a Tauri IPC command. The JS frontend calls this command when parameters change, receives pre-built geometry, and uses it directly for SVG rendering. Per-frame operations (marker interpolation, completed segment splitting) remain local in JS — they are cheap and must execute at 30fps with zero IPC latency.

### Architecture

```
User moves slider
  → JS sends params to Rust via invoke('backend_build_route_geometry')
  → Rust runs existing pipeline (fast — hundreds of points)
  → Rust returns RouteGeometryResponse { points, progressValues, bbox, ... }
  → JS receives geometry, stores in state
  → JS renders preview at 30fps using local interpolation (zero latency)

Final video render
  → Rust uses same RouteGeometryResponse for frame states + Skia drawing
  → Preview and render are mathematically identical (WYSIWYG)
```

### What changes from elevation

| Concern            | Elevation                      | Route                           |
| ------------------ | ------------------------------ | ------------------------------- |
| Downsampling       | Even-spacing                   | LTTB (triangle-area-maximizing) |
| Projection         | y_scale normalization          | Mercator equirectangular        |
| Inset calculation  | None                           | Line width + marker size based  |
| Output shape       | points + progress + area paths | points + progress only          |
| Canvas-parity test | Already exists                 | Needs to be added               |

## User Stories

1. As a template author, I want the route preview to use the same geometry as the final render, so that what I see during editing matches the exported video exactly.
2. As a developer, I want the route geometry pipeline to exist in one language only, so that algorithm changes don't require manual porting.
3. As a user, I want the route preview to update responsively when I adjust density or simplification sliders, so that I can tune the route appearance in real time.
4. As a user, I want the route to respect export range trimming, so that only the exported portion of the route is displayed when a custom range is active.
5. As a developer, I want the canvas-parity test to cover the route widget, so that Rust and JS rendering divergence is caught automatically.

## Implementation Decisions

### New Rust command: `backend_build_route_geometry`

A new IPC command following the same pattern as `backend_build_elevation_geometry`. The command accepts geometry-specific parameters (not a full render config) and returns pre-built route geometry.

**Request structure:**

- Route plot config (widget dimensions, target_density, simplify_tolerance_px, line widths, marker size)
- Scene start/end (for export range trimming)
- Scene scale (for dimension scaling)
- show_full_activity flag
- Parsed activity JSON

**Response structure:**

- `points: Vec<[f32; 2]>` — projected widget-space coordinates
- `progress_values: Vec<f32>` — per-point distance progress (0..1)
- `bbox: [f32; 4]` — bounding box
- `source_point_count: usize` — raw sample count before reduction
- `simplification: String` — diagnostic label
- `widget_width: u32`, `widget_height: u32` — scaled dimensions

### JS hook: `useRoutePreviewGeometry`

Rewritten to call the Rust command via IPC, with a mock geometry fallback for canvas-parity tests. Per-frame operations remain local:

- `getPointAtMetricProgress` for marker placement
- `buildRouteFramePreview` for completed segment splitting
- `pointsToSvg` for SVG path materialization

### Visibility changes in Rust

Three existing functions need `pub(crate)` visibility:

- `build_route_samples` in `prepare.rs`
- `build_route_geometry` in `prepare.rs`
- `normalize_route_plot` in `normalize.rs`

The `RouteSample` type and `simplify.rs` / `downsample.rs` functions are already `pub(crate)` within the route module.

### Canvas-parity test integration

Following the elevation pattern:

1. `PreparedRenderAssets` gains `route_geometry_json()` method
2. `write_mock_data` writes `route-geometry.json`
3. Playwright script injects it as `window.__OVRLEY_MOCK_ROUTE_GEOMETRY`
4. Hook checks for mock before IPC

### Config construction in hook

Same approach as elevation: merge `globalDefaults` into the store scene, override export-window timing. The route hook needs fewer overrides since route geometry doesn't depend on `scale` the same way (Mercator projection is scale-independent, and widget fitting handles dimensions directly).

## Testing Decisions

### Rust tests

- **Integration test** (`route_geometry_tests.rs`): Tests the command function end-to-end — parses config + activity, builds geometry, verifies point count, progress range, bbox, and that points are within widget bounds.
- **Unit tests** (existing `rdp_route_tests.rs`): Already test RDP simplification. No changes needed.

### JS tests

- **Hook test** (`useRoutePreviewGeometry.test.js`): Mocks `buildRouteGeometry` IPC call, verifies output shape (markerPoint, remainingSvgPoints, completedSvgPoints), verifies null during loading.
- **Renderer test** (`RouteRenderer.test.jsx`): Updated to mock IPC, verifies SVG renders correctly.

### Canvas-parity test

- Existing `canvas_parity_tests.rs` extended to also compare route widget rendering between Skia and Playwright SVG.

## Out of Scope

- Changing the route plot to time-based x-axis — explicitly rejected.
- Changing the Mercator projection algorithm — the equirectangular approximation is sufficient for activity routes.
- Adding new route visualization modes (e.g. 3D perspective).
- Changing the LTTB algorithm — it already preserves shape better than uniform sampling.

## Further Notes

- The route geometry pipeline is simpler than elevation in some ways (no elapsed fractions, no vertical segments, no area polygons) but more complex in others (Mercator projection, LTTB downsampling, inset calculation based on line widths and marker size).
- The elevation unification established the exact pattern to follow: Rust command → IPC → hook with mock fallback → parity test. The route implementation should mirror this pattern precisely.
- The `routeGeometry.js` file will be deleted entirely after migration, same as `elevationGeometry.js`.
- The `buildExportWindowRouteSamples` function in `exportRange.js` can be removed from the hook (scoping moves to Rust), same as `buildScopedElevationSeries` was removed for elevation.
