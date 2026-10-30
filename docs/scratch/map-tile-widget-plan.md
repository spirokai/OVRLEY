# Map tile widget — implementation plan

Status: plan, not started. Date: 2026-10-30. Builds on `map-tile-widget-research.md`.
ezu 0.13.1 API and all five OpenFreeMap styles were checked against sources and live
responses while writing this.

## Decisions

| Topic                    | Decision                                                                                                   |
| ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| Tile rendering           | ezu, native only. The own-painter fallback is dropped; if the Phase 1 gate fails we stop and reconsider    |
| Preview topology         | Native rasterizes, loopback serves RGBA, WASM composites only                                              |
| Labels                   | All baked into tiles. Bundled Noto Sans; the adapter rewrites `text-field` to Latin names                  |
| Attribution              | Always drawn in the widget, not configurable                                                               |
| Tile pinning             | None. Keep `planet/latest` and the existing HTTP revalidation                                              |
| Map styling              | Style preset and layer toggles (roads, paths, water, buildings, labels). No map colour overrides           |
| Route and marker styling | Same fields and controls as the route widget (see below)                                                   |
| Camera                   | `fixed` (whole route, north-up, pitch 0) or `follow` (`north_up` / `rider_up`, pitch 0–60, `range_meters`) |
| Not in v1                | Speed-dependent range, terrain, raster layers, upright point labels, map matching, corner radius or border |

The map widget uses the route widget's field names and editor controls for its route and marker:

- **Kept:** `completed_line_width`, `completed_line_color`, `completed_line_opacity`,
  `remaining_line_width`, `remaining_line_color`, `remaining_line_opacity`, `simplify_tolerance_px`,
  `show_full_activity`, and all marker fields (`marker_variant`, `marker_variant_diameter`,
  `marker_size`, `marker_color`, `marker_opacity`).
- **Not carried over:**
  - `rotation`: the camera owns the map's orientation.
  - `target_density`: it sets a vertex budget for fitting the whole route into the widget
    (`route/geometry.rs:83`), which has no meaning when the widget shows part of the route. The map
    limits work by only drawing visible vertices.

## Changes to the research design

The code and the ezu sources require these changes:

1. **The rasterizer cannot live in `ovrley_widgets`.** That crate compiles to WASM. Native-only
   code goes behind a feature of a new crate (see Architecture).
2. **Export prepares bytes up front and rasterizes on demand.** Pre-rasterizing a few hundred
   tiles at 1–16 MB each could exceed 1 GB. The fail-before-encoding guarantee only needs the
   download step to run first; rasterization is local and deterministic.
3. **Style translation warnings are logged, not fatal.** The style is external data, and every
   style produces approximations (for example `fill-extrusion` becomes a flat footprint).
   `ConvertError`, `Renderer::new` errors and malformed widget config still fail loudly.
4. **The route cannot reuse `RouteGeometry`.** It fits equirectangular points into a box
   (`route/geometry.rs:189`). The map needs Mercator world coordinates. We reuse the
   simplification and curve helpers and `plot_marker`.
5. **ezu renders whole zooms only, and `pixel_ratio` must be 1–4.** Fractional view zoom
   therefore becomes pixel ratio (see Phase 2). This is also why speed-dependent range is out:
   it would change texture resolution per frame.
6. **The map needs a stacking rule.** Export draws static layers into a base before any dynamic
   layer (`engine.rs:320`), so an opaque map would hide a label placed on it, while the preview
   shows the label on top (see Phase 3).
7. **The rasterizer interface does not split line and point labels yet.** The split can be added
   later by removing point-symbol layers in the adapter and using `render_tile_with_features`,
   with no interface prepared now.

## Architecture

```
MapTileService (app crate)         tile, style and sprite bytes; disk cache
      │ implements ovrley_core::MapDataSource (same pattern as RasterResourceResolver)
      ▼
ovrley_map, feature "raster"       style adapter + ezu → premultiplied RGBA     native only
      │                       │
      │ in process            │ GET /map-raster/{style}/{layers}/{z}/{x}/{y}/{ratio}
      ▼                       ▼
ExportMapTiles (core)         PreviewMapTiles (widgets, filled over the ABI)
      └──────────┬────────────┘
                 ▼ impl MapTileProvider
ovrley_widgets::widgets::map       camera, cover, compositor, route, attribution
```

**`src-tauri/ovrley_map`** is a new workspace member and the owner of the shared vocabulary.

- Without features it has no Skia and no ezu: `MapStyle` (moved from `MAP_STYLES`), `MapLayers`,
  `MapStyleSpec`, `TileKey { spec, z, x, y, ratio_milli }`, Mercator math, `tile_pixels(ratio)`.
  `ovrley_widgets` depends on this, so WASM never compiles ezu.
- Feature `raster` adds the style adapter, the ezu renderer, the decoded-tile cache and the bundled
  fonts. `ovrley_core` and the app enable it.
- `wasm-bindings` is an isolated workspace, so feature unification cannot pull ezu into WASM.

`TileKey` is stateless and fully describes a texture, so no style-registration call is needed.
The native side keeps a small LRU of `Renderer`s keyed by spec.

## Phase 1 — Tile rasterizer and style adapter

Deliverable: `render_tile(TileKey, data source) → RGBA` for all five styles, with no widget yet.

**Service (`src-tauri/src/map_tile_service.rs`)**

- Split `resolve_style` (`:285`) so the raw style bytes are available without the loopback URL rewrite.
- Add sprite fetching (`ofm.json`, `ofm.png`, `@2x`). All five styles use the same sprite. Store it
  under the styles namespace, because `scan_persistent_entries` deletes unknown files anywhere
  else (`:819`).
- Define `MapDataSource` in `ovrley_core` and implement it on the service. Manage a clone of the
  service in app state, because `lib.rs:159` currently moves it into the server.

**Style adapter (`ovrley_map::style`), translating once per spec**

1. Parse the service's style JSON.
2. Strip the `ne2_shaded` source and `natural_earth` raster layer (z ≤ 6 only).
3. Force the `openmaptiles` source to `maxzoom: 14`.
4. Apply layer toggles by setting `layout.visibility: none`, classified by `source-layer` and type:
   - `roads`: `transportation` (non-path), `aeroway`
   - `paths`: `transportation` layers whose id matches `path|pedestrian`
   - `water`: `water`, `waterway`
   - `buildings`: `building`
   - `labels`: all `symbol` layers
5. Rewrite each `name:latin` / `name:nonlatin` `text-field` to
   `["coalesce", ["get","name:latin"], ["get","name_en"], ["get","name"]]`. Leave `ref` shields alone.
6. Call `ezu_translate::maplibre::convert` with `fonts` mapped for Noto Sans Regular, Bold and Italic.
   Log `Report.warnings`.

**Renderer (`ovrley_map::raster`)**

- Bind fonts (`include_bytes!` from `assets/map-fonts/`, OFL licence, so they stay out of the user's
  font picker) and the sprite, using ezu's persistent banks.
- For each tile: resolve the data tile with `source_tile`, gather neighbours from
  `requested_neighbor_offsets`, reuse `load_source` results through a small LRU, build a `SourceSet`
  with `source_zoom: Some(14)`, then call `render_tile_with` with `OutputFormat::Rgba`.
- Premultiply once. Mark tiles opaque when the background layer is present.
- Limit ezu's own cache with `set_cache_budget`, because we keep our own.

**Dependencies:** `ezu = "=0.13.1"` with `default-features = false` and features
`renderer`, `translate`. Relax both `image = "=0.25.5"` pins to `"0.25"`.

**Gate (a throwaway binary writing PNGs and timings; you judge the images):**

- ms per tile at z14–18 on dense and rural tiles, all five styles. Target: under ~100 ms per tile.
- Labels, one-way arrows and shields look right when overzoomed.
- Texture memory at the real extremes, deciding between `tile_size` 512 (default) and 256.

If this fails, stop here. Nothing else has been built on it yet.

## Phase 2 — Camera, cover, compositor and widget family

Deliverable: the `map` family drawing to a Skia canvas from a `MapTileProvider`, measured in WASM
against synthetic tiles.

**Config (`widgets/map/{config,validation}.rs`)** in the `plots` collection with `value: "map"`.
Every field is required and parsed once into a `ValidatedMapWidget` whose `camera` is an enum, so
consumers never see `pitch` on a fixed camera.

**Track (`track.rs`).** Built once per activity and window, from a new `RouteActivity` accessor:
Mercator points, cumulative distance, and prefix sums of unit headings.

**Camera (`camera.rs`), a pure function of activity second**

- Port `getCourseNavigationCamera` (`mapPreviewGeometry.js:116`): 15 m lookahead, top padding 45 %.
- Add windowed bearing smoothing from the prefix sums. When stationary, search backwards for the
  last valid bearing. This replaces the stateful `lastBearing` in `NavigationMapController`, which
  would break parallel export.

**Zoom and resolution, constant per widget**

- `view_zoom = log2(cos(mean_lat) · 40075016.69 / (512 · range_meters / width))`, capped at 18.
  `width` is in scene units, so a scaled scene looks like a sharper version of the same map.
- `z = floor(view_zoom)` at the focus point.
- `pixel_ratio = 2^(view_zoom − z) × scene.scale × preview_raster_scale`, clamped to 1–4.
- Preview textures are smaller than export textures, but ezu's label placement does not change with
  pixel ratio, so the maps match.

**Cover (`cover.rs`).** MapLibre-style covering tiles for the same view geometry (fov 36.87°). Far
tiles use a lower zoom so texels stay near one per pixel. Capped at 64 tiles per zoom level.

**Compositor (`compositor.rs`)**

- Matrix: `projection × pitch × bearing × translate`, through `canvas.concat_44`.
- Draw each tile with bilinear sampling, no AA, and about half a texel of destination overlap, so
  rotated tile edges show no seams.
- Fixed mode composes tiles once into a widget-sized image and redraws it per frame until a tile
  changes.
- A tile lookup returns `Ready(Image)` or `Pending`. On `Pending` the compositor searches ancestors,
  then fills with the preset's flat placeholder colour.
- Export's provider never returns `Pending`.

**Route and marker**

- Project the visible route vertices to screen on the CPU, clip at the near plane, stroke in screen
  space at constant width, and split travelled from remaining by progress.
- Stroking through the matrix would change line width with depth and needs no less clipping.
- Use a coarse spatial index so long activities stay cheap.
- `simplify_tolerance_px` applies at the widget's constant view zoom, using the route widget's
  simplification helper.
- `show_full_activity` selects the route window through `host.plot_window`, as the route widget does.
- Marker drawn in screen space with `plot_marker`, with every variant available.
- Validation of the shared line and marker fields moves into one helper used by both families, so the
  two widgets cannot drift apart.

**Attribution.** Fixed text, bottom-right, drawn through `FontResolver`, shrinking to fit the widget
width. Always drawn, including while tiles load.

**Dispatch.** Add arms per the add-widget skill:

- `preview_widget.rs`: `WidgetRequest::Map`, `PreviewWidget::Map` in every match, and
  `FrameKey::Map { camera, progress, tile_generation }`.
- `preview_instance.rs`: placement and `pixel_config`.
- `preview.rs`: resize routing and the render loop.
- Resize is `size`-based. During a `transient` drag, cached tiles are rescaled and nothing is fetched;
  the commit recomputes zoom.

**No activity or no GPS.** Flat placeholder colour, placeholder route, and attribution, with no
network requests.

**Gate (needs your approval for `pnpm benchmark:widgets:build` and the run):** a `map` benchmark case
with 20 synthetic tiles at pitch 0, 40 and 60, at 500 and 1000 px. Target: under ~8 ms/frame in WASM at
500 px. Render a short clip and judge far-field shimmer and seams.

## Phase 3 — Export pipeline

Deliverable: a video export with a map, failing before encoding if any tile is unavailable.

- **Ingress:** `ValidatedRenderConfig.maps`, `validate_map_widgets` in `normalize/widgets.rs`,
  `requirements.include_widget("map")`, and `ValidatedWidget::Map`.
- **Data source plumbing:** pass `Arc<dyn MapDataSource>` beside `resources` through `submit_single`
  (`execution.rs:127`), batch submission and `plan_single_render`. A map widget with no source (the
  native CLI tools) is a loud config error.
- **Prepare step**, inside `ExportWidgets::prepare` (`engine.rs:88`), which runs before ffmpeg starts:
  1. Walk the camera over every rendered dense frame to get the set of covering tiles, then their
     z14 parents and neighbours.
  2. Fetch through `MapDataSource` with bounded parallelism, checking cancellation, and report progress
     on the existing "Preparing video assets..." item.
  3. Hold all bytes in `Arc`s for the job (5–20 MB for 30 km), so disk eviction cannot affect a running
     export. Any failure fails the job here.
- **`ExportMapTiles`:** a shared provider with an LRU of `skia_safe::Image`s (`Image` is `Send + Sync`)
  bounded by bytes, plus per-key in-flight deduplication. Workers rasterize on a miss. It is built once
  and handed to each worker's widget instance.
- **Stacking** (`engine.rs:247`): order becomes backdrops, maps, rasters, labels, values, and the rest.
  Maps are `Layer::Dynamic` and draw first among dynamic layers. A label or raster whose rectangle
  intersects a map moves out of the covered base and draws after the map each frame. A moved raster
  stays in the uncovered base so it still shows outside activity coverage. Scenes without a map produce
  identical output to today.

**Gate:** export the same project twice and compare frames, then export once offline with an uncached
route and confirm it fails before encoding starts.

## Phase 4 — Preview wiring

Deliverable: live preview with tiles arriving progressively while playing and scrubbing.

- **Loopback endpoint** in `video_server.rs`: `/map-raster/{style}/{layers}/{z}/{x}/{y}/{ratio}`, parsed
  strictly like `parse_tile_path`. It returns raw premultiplied RGBA with `no-store`; size comes from
  `tile_pixels(ratio)`. Add a native LRU of finished textures.
- **Worker capacity:** tile requests block for a fetch plus a raster, and the server has 4 workers
  shared with video range requests (`video_server.rs:40`). Raise `WORKER_COUNT` to 8 and cap the JS side
  at 3 concurrent tiles.
- **WASM ABI** (`widget_abi.rs`):
  - `ovrley_preview_map_tile(session, key, pixels)` uploads a texture and bumps `tile_generation`.
  - `ovrley_preview_map_wanted_ptr/len` returns the keys the last render missed, visible tiles first,
    then a few seconds of lookahead along the camera path. Rust stays the only owner of cover logic.
- **`PreviewMapTiles`** in `PreviewSession`: LRU bounded by count and bytes. The render loop passes it to
  map widgets.
- **`app/src/features/wasm-widgets/utils/mapTileLoader.js`:** a priority queue with no React in it.
  Latest want list wins and duplicates merge. In-flight requests finish, because their textures are
  still useful. Tile errors reach `onError`.
- **`previewSession.js`:** `widgetFonts` returns the attribution font (`:17`), and `render()` reads the
  want list after `_ovrley_preview_render`. A tile arrival calls `schedule()`.
- **Stacking:** `WasmWidgetCanvas.jsx:20` becomes backdrop 1, map 2, raster 3, course/elevation/diving 4,
  others 10, matching Phase 3.
- **Route download:** `backend_map_prepare` shares the export's prepare code. It runs in the background
  after an activity loads and emits needed, downloading, ready or failed, so scrubbing mostly hits the
  disk cache.

**Gate:** in the app, play, scrub across the route, and resize. Confirm no stalls and that video preview
stays smooth during tile loads. Compare the same second in preview and export.

## Phase 5 — Editor and release readiness

Deliverable: the widget available to users.

- `assets/standard-widgets.json`: a `map` entry (collection `plots`, `timeDependent`, GPS requirements,
  all defaults) plus an icon in `assets/widget-icons/`.
- `MapWidgetEditor.jsx` in `WIDGET_EDITOR_MAP`, presentational: style preset, layer toggles, camera mode,
  orientation, pitch, range, then the route widget's line styling (thickness, smoothing, finished and
  remaining colour and opacity, show full activity) and `PlotMarkerSection`. Extract the line styling
  from `RouteMapWidgetEditor.jsx:50` into a shared section rather than copying it. State and edits go in
  hooks and `features/widget-editor/utils/`.
- `overlayEditorUtils.js` and `widgetGestureGeometry.js`: free-aspect, size-based resize, no rotation
  handle.
- Translations in `app/src/i18n/locales/*-translation.json`.
- A visible download or failure status for the route download in the inspector.
- Check packaging: bundled font licence, `scripts/package-portable.mjs`, and binary size on Windows and
  macOS.

**Gate:** `pnpm check`, `pnpm test`, and `cargo test` for `ovrley_core`. Save, reload and undo a project
with a map. Full export on both platforms.

## Risks

| Risk                                                               | Handling                                                        |
| ------------------------------------------------------------------ | --------------------------------------------------------------- |
| ezu is pre-1.0 and changes quickly                                 | Exact version pin; all use stays inside `ovrley_map::raster`    |
| Large textures at high scene scale (up to 2048² = 16 MB)           | Byte-bounded LRUs; Phase 1 decides on `tile_size` 256 if needed |
| Far-field shimmer under perspective (Skia raster disables mipmaps) | Lower-zoom far tiles; judged at the Phase 2 gate                |
| Tile seams when rotated                                            | Destination overlap; judged at the Phase 2 gate                 |
| Non-Latin names with no Latin form show missing glyphs             | Accepted for v1; checked at the Phase 1 gate                    |
| Upside-down street labels when heading south                       | Accepted for v1, per the research doc                           |
| GPS offset of ±3–5 m is 15–25 px at z18                            | Accepted; map matching is a separate feature                    |

## Working rules

- Compile checks only while iterating (`cargo check -p ovrley_map`, `-p ovrley_widgets`).
- WASM builds, benchmark builds and `pnpm build` run only with your approval, at the gates above.
- No new tests unless requested. The two with the highest value would be camera and cover math, and
  preview-versus-export pixel parity.

## Noticed, not changed

Preview and export already disagree on stacking for existing widgets: export draws routes over values,
while the preview puts values (z-10) over routes (z-3). The renumbering in Phase 4 preserves that order.
