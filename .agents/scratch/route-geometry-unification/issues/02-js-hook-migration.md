Status: ready-for-agent

# 02 — JS Hook Migration to Consume Rust Route Geometry

## Parent

[Route Geometry Unification PRD](../PRD.md)

## What to build

Rewrite the `useRoutePreviewGeometry` hook to call the new `backend_build_route_geometry` Rust command instead of computing geometry locally in JavaScript. Delete the entire duplicated JS geometry pipeline (`routeGeometry.js`). Keep the cheap per-frame operations (marker interpolation, completed segment splitting, SVG path materialization) in JS — these must run at 30fps with zero IPC latency.

## Acceptance criteria

- [ ] `useRoutePreviewGeometry` calls `buildRouteGeometry()` from `backend.js` and stores the result
- [ ] The hook passes Rust-provided `points` and `progressValues` to local marker interpolation
- [ ] The hook passes Rust-provided `points` and `progressValues` to local `buildRouteFramePreview` for completed segment
- [ ] The hook materializes SVG paths via local `pointsToSvg()`
- [ ] `routeGeometry.js` is deleted entirely
- [ ] `buildExportWindowRouteSamples` is removed from the hook (scoping moves to Rust)
- [ ] `RouteRenderer.jsx` renders correctly with the new hook output (no changes needed to the renderer itself)
- [ ] Canvas-parity test includes route widget comparison
- [ ] `pnpm test` passes
- [ ] `pnpm lint` passes

## Implementation notes

### Hook rewrite

Same pattern as elevation: read config + globalDefaults from store, merge scene for Rust, call IPC, use mock geometry for parity test, keep local per-frame operations.

The route hook is simpler than elevation in some ways:
- No `elapsed fractions` or vertical segment concerns
- No area polygon paths
- Marker + completed segment is the only per-frame work

### Canvas-parity test

Following the elevation pattern:
1. `PreparedRenderAssets` gains `route_geometry_json()` method
2. `write_mock_data` writes `route-geometry.json` alongside `elevation-geometry.json`
3. Playwright script injects both mock geometries
4. Hook checks for `window.__OVRLEY_MOCK_ROUTE_GEOMETRY` before IPC

### Config construction

Same approach as elevation: merge `globalDefaults` into store scene, override export-window timing. Route geometry doesn't depend on `scale` the same way (Mercator projection is scale-independent), but the scene config still needs `start`/`end` for export range trimming.

### Files to modify

| File | Change |
|------|--------|
| `app/src/features/widget-preview/hooks/useRoutePreviewGeometry.js` | Rewrite to call Rust command, keep local interpolation |
| `app/src/features/widget-preview/utils/routeGeometry.js` | **Delete entirely** |
| `app/src/features/widget-preview/components/RouteRenderer.jsx` | Add null guard for geometry (same as ElevationRenderer) |
| `app/src/tests/features/widget-preview/useRoutePreviewGeometry.test.js` | Rewrite with mocked IPC |
| `app/src/tests/features/widget-preview/RouteRenderer.test.jsx` | Update to mock IPC |
| `src-tauri/ovrley_core/src/render/mod.rs` | Add `route_geometry_json()` to `PreparedPreviewAssets` |
| `src-tauri/ovrley_core/tests/common/canvas_parity.rs` | Write `route-geometry.json` in `write_mock_data` |
| `src-tauri/ovrley_core/tests/scripts/canvas_screenshot.mjs` | Inject route mock geometry |

## Blocked by

[#01 — Rust Route Geometry Command + IPC Contract](01-rust-route-geometry-command.md)
