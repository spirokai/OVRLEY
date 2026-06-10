Status: done

# 01 — Rust Geometry Command + IPC Contract

## Parent

[Elevation Geometry Unification PRD](../PRD.md)

## What to build

Add a new Tauri IPC command `backend_build_elevation_geometry` that accepts the full config + activity JSON and returns pre-built elevation geometry. This command wraps the existing Rust geometry pipeline (smoothing, downsampling, projection, RDP simplification) behind a public API that the JS frontend can call.

No JS changes happen in this slice — the existing JS pipeline continues to work. This slice is purely additive: new Rust command, new response struct, new JS API helper. Verification is through `cargo test` and by calling the command from a test script.

## Acceptance criteria

- [x] `ElevationGeometryResponse` struct exists with `#[derive(Serialize)]` and all PRD-specified fields (`points`, `progress_values`, `bbox`, `source_point_count`, `simplification`, `widget_width`, `widget_height`)
- [x] `points` serializes as `[[x,y], ...]` JSON arrays (not tuple objects)
- [x] `build_elevation_source_points` in `prepare.rs` is `pub(crate)`
- [x] `build_elevation_geometry` in `prepare.rs` is `pub(crate)`
- [x] `normalize_elevation_plot` in `normalize.rs` is `pub(crate)`
- [x] New command module `ovrley_core/src/commands/elevation_geometry.rs` exists with `build_elevation_geometry_command` function
- [x] Command parses config JSON into `ValidatedRenderConfig`, extracts `elevation_plot`, errors if missing
- [x] Command parses activity JSON into `ParsedActivity`
- [x] Command calls existing pipeline: `build_elevation_source_points` → `normalize_elevation_plot` → `build_elevation_geometry`
- [x] Command returns `ElevationGeometryResponse` with correct field mapping
- [x] Tauri wrapper `backend_build_elevation_geometry` exists in `tauri_commands.rs`
- [x] Command registered in `lib.rs` `generate_handler![]`
- [x] JS API helper `buildElevationGeometry(config, parsedActivity)` exists in `app/src/api/backend.js`
- [x] `cargo test` passes with new module

## Implementation notes

### Response struct field mapping

```rust
Ok(ElevationGeometryResponse {
    points: geometry.points.into_iter().map(|(x, y)| [x, y]).collect(),
    progress_values: geometry.progress_values,
    bbox: [geometry.bbox.0, geometry.bbox.1, geometry.bbox.2, geometry.bbox.3],
    source_point_count: geometry.source_point_count,
    simplification: geometry.simplification,
    widget_width: normalized.width,
    widget_height: normalized.height,
})
```

### Error handling

- Missing `elevation_plot` in config → `CoreError::Config("Config has no elevation_plot widget")`
- Parse failures → existing `CoreError::Deserialization` from `parse_activity_json` / `parse_config_json`
- Geometry pipeline failures → existing `CoreError` variants from `build_elevation_geometry`

### Files to modify

| File | Change |
|------|--------|
| `ovrley_core/src/commands/elevation_geometry.rs` | **New** — request/response structs + command function |
| `ovrley_core/src/commands/mod.rs` | Add `pub mod elevation_geometry;` |
| `ovrley_core/src/render/widgets/elevation/prepare.rs` | Make `build_elevation_source_points` and `build_elevation_geometry` `pub` |
| `ovrley_core/src/render/widgets/elevation/normalize.rs` | Make `normalize_elevation_plot` `pub` |
| `src-tauri/src/tauri_commands.rs` | Add `backend_build_elevation_geometry` wrapper |
| `src-tauri/src/lib.rs` | Register in `generate_handler![]` |
| `app/src/api/backend.js` | Add `buildElevationGeometry()` helper |

## Blocked by

None — can start immediately.
