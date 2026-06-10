Status: done

# 01 — Rust Route Geometry Command + IPC Contract

## Parent

[Route Geometry Unification PRD](../PRD.md)

## What to build

Add a new Tauri IPC command `backend_build_route_geometry` that accepts geometry-specific parameters and activity JSON, and returns pre-built route geometry. This command wraps the existing Rust route pipeline (Mercator projection, LTTB downsampling, RDP simplification, widget fitting) behind a public API that the JS frontend can call.

No JS changes happen in this slice — the existing JS pipeline continues to work. This slice is purely additive: new Rust command, new response struct, new JS API helper.

## Acceptance criteria

- [x] `RouteGeometryResponse` struct exists with `#[derive(Serialize)]` and all PRD-specified fields (`points`, `progressValues`, `bbox`, `sourcePointCount`, `simplification`, `widgetWidth`, `widgetHeight`)
- [x] `points` serializes as `[[x,y], ...]` JSON arrays (not tuple objects)
- [x] `build_route_samples` in `prepare.rs` is `pub(crate)`
- [x] `build_route_geometry` in `prepare.rs` is `pub(crate)`
- [x] `normalize_route_plot` in `normalize.rs` is `pub(crate)`
- [x] New command module `ovrley_core/src/commands/route_geometry.rs` exists with `build_route_geometry_command` function
- [x] Command accepts geometry-specific request (route plot config, scene timing, activity JSON)
- [x] Command errors when no route plot in config
- [x] Command calls existing pipeline: `build_route_samples` → `build_route_geometry`
- [x] Command returns `RouteGeometryResponse` with correct field mapping
- [x] Tauri wrapper `backend_build_route_geometry` exists in `tauri_commands.rs`
- [x] Command registered in `lib.rs` `generate_handler![]`
- [x] JS API helper `buildRouteGeometry(config, parsedActivity)` exists in `app/src/api/backend.js`
- [x] `cargo test` passes with new module
- [ ] `pnpm test` passes (new hook test mocking the IPC call)

## Implementation notes

### Files to modify

| File | Change |
|------|--------|
| `ovrley_core/src/commands/route_geometry.rs` | **New** — request/response structs + command function |
| `ovrley_core/src/commands/mod.rs` | Add `pub mod route_geometry;` |
| `ovrley_core/src/render/widgets/route/prepare.rs` | Make `build_route_samples`, `build_route_geometry` `pub(crate)` |
| `ovrley_core/src/render/widgets/route/normalize.rs` | Make `normalize_route_plot` `pub(crate)` |
| `ovrley_core/src/render/widgets/route/mod.rs` | Make `prepare`, `normalize` modules `pub(crate)` |
| `ovrley_core/src/render/widgets/mod.rs` | Make `route` module `pub(crate)` |
| `src-tauri/src/tauri_commands.rs` | Add `backend_build_route_geometry` wrapper |
| `src-tauri/src/lib.rs` | Register in `generate_handler![]` |
| `app/src/api/backend.js` | Add `buildRouteGeometry()` helper |
| `app/src/tests/features/widget-preview/useRoutePreviewGeometry.test.js` | **New** — test hook with mocked IPC |

### Command function

Follows the elevation pattern: accepts geometry-specific parameters (not full config), parses activity JSON, calls existing pipeline, returns serializable response.

The route command needs: route plot config (dimensions, density, tolerance, line widths, marker size), scene start/end (for export range trimming), show_full_activity flag, and parsed activity JSON.

### Response struct

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGeometryResponse {
    pub points: Vec<[f32; 2]>,
    pub progress_values: Vec<f32>,
    pub bbox: [f32; 4],
    pub source_point_count: usize,
    pub simplification: String,
    pub widget_width: u32,
    pub widget_height: u32,
}
```

## Blocked by

None — can start immediately.
