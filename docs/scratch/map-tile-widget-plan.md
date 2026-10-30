# Map tile widget — implementation plan

Status: Phase 1 built. Phase 2 not started, see `map-widget-phase2-handoff.md`. Updated 2026-10-30.
Builds on `map-tile-widget-research.md`.

**The renderer changed.** The widget now uses the GPU renderer in `src-tauri/ovrley_map_gpu`, which draws
one finished map frame per camera, including real 3D buildings that flat tiles cannot produce. Its gate results
are in the Phase 2 handoff. Phase 1's ezu rasterizer stays in the tree as the comparison and is not used by the
widget. Phases 2–5 below are revised for this. Phases 3 and 4 still have open design questions, marked there.

## Decisions

| Topic                    | Decision                                                                                                   |
| ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| Map rendering            | `ovrley_map_gpu` (wgpu), native only, one frame per camera. Replaces ezu for this widget                   |
| Preview topology         | **Open.** The WASM preview cannot run the renderer. Loopback frames or WebGL2, decided at Phase 4          |
| Labels                   | **None for now.** The GPU renderer draws no labels. Needed before release, approach open                   |
| Attribution              | Always drawn in the widget, not configurable                                                               |
| Tile pinning             | None. Keep `planet/latest` and the existing HTTP revalidation                                              |
| Map styling              | Style preset and layer toggles (roads, paths, water, buildings). No map colour overrides                   |
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

The code and the ezu sources required these changes. Items 1, 4 and 6 still apply. Items 2, 3, 5 and 7 describe
the ezu path and no longer drive the widget:

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
MapTileService (app crate)         tile and style bytes; disk cache
      │ implements ovrley_map::MapDataSource
      ▼
ovrley_map_gpu                     tile tessellation + wgpu → RGBA frame        native only
      │                       │
      │ in process            │ preview delivery: open (Phase 4)
      ▼                       ▼
export provider (Phase 3)     preview provider (Phase 4)
      └──────────┬────────────┘
                 ▼ impl MapFrameProvider
ovrley_widgets::widgets::map       camera, frame image, route, marker, attribution

ovrley_map (featureless)           shared vocabulary and the camera matrix, used by both sides
```

**`src-tauri/ovrley_map`** owns the shared vocabulary.

- Without features it has no Skia, no ezu and no wgpu: `MapStyle`, `MapLayers`, `MapStyleSpec`, `MapDataSource`,
  Mercator math, and, from Phase 2, `MapCamera` and the view matrix. `ovrley_widgets` depends on this, so WASM never
  compiles a renderer. The route and the map must use the same matrix, or the route drifts off the road.
- Feature `raster` adds the ezu comparison path.

**`src-tauri/ovrley_map_gpu`** holds the renderer. `ovrley_widgets` must never depend on it. `wasm-bindings` is an
isolated workspace, so feature unification cannot pull it into WASM. Until Phase 3, only the gate depends on it, so
ordinary app builds skip `wgpu`.

## Phase 1 — Tile rasterizer and style adapter

**Built, on commit `bc7c49fe`, and superseded for the widget.** It stays as the comparison. The rest of this section
records what was built.

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

## Phase 2 — Camera, widget family and renderer connection

Execution details, renderer gaps to close first, and environment notes are in `map-widget-phase2-handoff.md`.

Deliverable: the `map` family drawing to a Skia canvas from a `MapFrameProvider`, with real GPU frames in a native
gate and its own cost measured in WASM.

**Renderer gaps, closed first:** top padding on the camera, a pixel ratio so a scaled scene is the same map sharper,
and coverage nothing has tested yet (pitch 0, zooms 10–18, non-square viewports, rural areas, Fiord).

**Shared camera.** Move `MapCamera` and the view matrix to featureless `ovrley_map`. The widget projects the route with
the same matrix the renderer draws with.

**Provider.** `MapFrameProvider::frame(request) → Ready(Image) | Pending`, where the request holds spec, camera, size and
pixel ratio. It never blocks on the network. Export's provider never returns `Pending`.

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
- `pixel_ratio = scene.scale × preview_raster_scale`. It scales output size and line widths. Tile level and style
  values stay on the view zoom, so preview and export show the same map.

**Map frame**

- The widget draws the provider's image over its bounds. Tile selection belongs to the renderer, so there is no cover
  and no compositor.
- A fixed camera asks the provider once, by caching the last frame per request.
- On `Pending`, fill with the preset's flat placeholder colour. Never draw a stale frame: its camera differs, so the
  route would sit off the road.

**Route and marker**

- Project the visible route vertices to screen on the CPU with the shared matrix, clip at the near plane, stroke in
  screen space at constant width, and split travelled from remaining by progress. The route draws over buildings.
- Stroking through the matrix would change line width with depth and needs no less clipping.
- Use a coarse spatial index so long activities stay cheap.
- `simplify_tolerance_px` applies at the widget's constant view zoom, using the route widget's
  simplification helper.
- `show_full_activity` selects the route window through `host.plot_window`, as the route widget does.
- Marker drawn in screen space with `plot_marker`, with every variant available.
- Validation of the shared line and marker fields moves into one helper used by both families, so the
  two widgets cannot drift apart.

**Attribution.** Fixed text, bottom-right, drawn through `FontResolver`, shrinking to fit the widget
width. Always drawn, including while frames are pending. `widgetFonts` in `previewSession.js` returns this font from
this phase, because a font the preview did not request fails it.

**Dispatch.** Add arms per the add-widget skill:

- `preview_widget.rs`: `WidgetRequest::Map`, `ValidatedWidget::Map` carrying the provider, `PreviewWidget::Map` in every
  match, and `FrameKey::Map { camera, progress }`.
- `preview_instance.rs`: placement and `pixel_config`.
- `preview.rs`: resize routing and the render loop.
- Resize is `size`-based. During a `transient` drag, the last frame is rescaled and nothing is requested;
  the commit recomputes zoom.

**No activity or no GPS.** Flat placeholder colour, placeholder route, and attribution, with no
network requests.

**Gate**

- **Native:** the real widget with the real renderer: stills and a clip for all five styles, in `follow` and `fixed`.
  You judge whether the route stays on its road while the camera moves.
- **WASM (needs your approval for `pnpm benchmark:widgets:build` and the run):** a `map` benchmark case with a
  pre-rendered frame at pitch 0, 40 and 60, at 500 and 1000 px. Target: under ~8 ms/frame at 500 px. This measures
  the image blit, route, marker and attribution, because WASM has no GPU renderer.

## Phase 3 — Export pipeline

Deliverable: a video export with a map, failing before encoding if any tile is unavailable.

**To design first:** how parallel export workers share a GPU. A renderer owns one device and renders one frame at a time,
so the direction is one shared GPU thread that workers send requests to. This phase also makes `wgpu` a dependency of
ordinary app builds, so check binary size and what happens on a machine with no usable GPU. No gate has tested either.

- **Ingress:** `ValidatedRenderConfig.maps`, `validate_map_widgets` in `normalize/widgets.rs`,
  `requirements.include_widget("map")`, and `ValidatedWidget::Map`.
- **Data source plumbing:** pass `Arc<dyn MapDataSource>` beside `resources` through `submit_single`
  (`execution.rs:127`), batch submission and `plan_single_render`. A map widget with no source (the
  native CLI tools) is a loud config error.
- **Prepare step**, inside `ExportWidgets::prepare` (`engine.rs:88`), which runs before ffmpeg starts:
  1. Walk the camera over every rendered dense frame to get the set of visible tiles with `view_tiles`.
  2. Fetch through `MapDataSource` with bounded parallelism, checking cancellation, and report progress
     on the existing "Preparing video assets..." item.
  3. Hold all bytes in `Arc`s for the job (5–20 MB for 30 km), so disk eviction cannot affect a running
     export. Any failure fails the job here.
- **Export provider:** implements `MapFrameProvider` over the shared GPU thread. It never returns `Pending`. Tiles
  tessellate on demand or ahead of the camera, and the pools evict the oldest. A route longer than the pools hold is
  the case to measure. It is built once and handed to each worker's widget instance.
- **Stacking** (`engine.rs:247`): order becomes backdrops, maps, rasters, labels, values, and the rest.
  Maps are `Layer::Dynamic` and draw first among dynamic layers. A label or raster whose rectangle
  intersects a map moves out of the covered base and draws after the map each frame. A moved raster
  stays in the uncovered base so it still shows outside activity coverage. Scenes without a map produce
  identical output to today.

**Gate:** export the same project twice and compare frames, then export once offline with an uncached
route and confirm it fails before encoding starts.

## Phase 4 — Preview wiring

Deliverable: live preview with the map following the playhead while playing and scrubbing.

**To decide first: how frames reach the preview.** WASM cannot run the native renderer. The candidates are native frames
over loopback, or a WebGL2 renderer in the browser, and neither has been measured. The steps below assume loopback and
need rewriting if WebGL2 wins. The widget's `Ready`/`Pending` provider suits either.

- **Loopback endpoint** in `video_server.rs`: a frame route keyed by spec, camera, size and ratio, parsed strictly like
  `parse_tile_path`. It returns raw premultiplied RGBA with `no-store`. A 500 px frame is 1 MB, so 30 fps is about
  30 MB/s, which is the number to check first.
- **Worker capacity:** requests share the server's 4 workers with video range requests (`video_server.rs:40`). Decide
  the count from measurements.
- **WASM ABI** (`widget_abi.rs`):
  - `ovrley_preview_map_frame(session, key, pixels)` uploads a frame. The Phase 2 benchmark introduces it.
  - `ovrley_preview_map_wanted_ptr/len` returns the frames the last render missed, the current one first.
- **`PreviewMapFrames`** in `PreviewSession`, replacing the always-`Pending` provider from Phase 2.
- **`app/src/features/wasm-widgets/utils/mapFrameLoader.js`:** a queue with no React in it. The latest request wins, since
  a frame for a past camera is no use. Errors reach `onError`.
- **`previewSession.js`:** `render()` reads the want list after `_ovrley_preview_render`. A frame's arrival calls
  `schedule()`.
- **Stacking:** `WasmWidgetCanvas.jsx:20` becomes backdrop 1, map 2, raster 3, course/elevation/diving 4,
  others 10, matching Phase 3.
- **Route download:** `backend_map_prepare` shares the export's prepare code. It runs in the background
  after an activity loads and emits needed, downloading, ready or failed, so scrubbing mostly hits the
  disk cache.

**Gate:** in the app, play, scrub across the route, and resize. Confirm no stalls and that video preview
stays smooth during frame loads. Compare the same second in preview and export.

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
- Labels must exist before release, or you decide to ship without them. Add the `labels` toggle when they do.
- Check packaging: `scripts/package-portable.mjs`, and binary size with `wgpu` on Windows and macOS. Check the bundled font
  licence if labels use the bundled fonts.

**Gate:** `pnpm check`, `pnpm test`, and `cargo test` for `ovrley_core`. Save, reload and undo a project
with a map. Full export on both platforms.

## Risks

| Risk                                                              | Handling                                                      |
| ----------------------------------------------------------------- | ------------------------------------------------------------- |
| Route drifts off the road if map and route use different matrices | One shared camera in `ovrley_map`; judged at the Phase 2 gate |
| Renderer only tested at z17, pitch 60, one dense area, on Windows | Phase 2 coverage check; Linux and macOS still untested        |
| Pool eviction on long routes or low zooms                         | Phase 2 and 3 measure it; report before resizing the pools    |
| No usable GPU, or device loss during an export                    | Not designed yet; Phase 3                                     |
| Preview delivery may not hold 30 fps                              | Measured first in Phase 4                                     |
| Far roads look heavier than Native, buildings heavier at 0.8      | Known visual gaps; line width fix awaits your decision        |
| GPS offset of ±3–5 m is 15–25 px at z18                           | Accepted; map matching is a separate feature                  |

## Open decisions and recommendations

| Decision                  | When             | Recommendation                                                                                                                                            |
| ------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Labels                    | Before release   | None yet. The idea of ezu-baked, label-only tiles draped on the ground has not been measured                                                              |
| Preview delivery          | Start of Phase 4 | None yet. Measure loopback first (about 30 MB/s at 500 px and 30 fps), then compare WebGL2                                                                |
| Export with no usable GPU | Phase 3          | None yet. Not designed                                                                                                                                    |
| Far road line width       | Any time         | Fix it. Lines keep a constant pixel width with distance, as upstream does, while Native narrows them. It is a line vertex shader change, not yet measured |
| MSAA                      | Settled for now  | 4 by default, chosen by the host rather than widget config. Costs about 1.7–2×, about 5 ms at 1000 px, and fill and building edges are jagged without it  |

## Known visual gaps against MapLibre Native

Not drawn or not applied in the GPU renderer:

- **Dashes:** 62 `line-dasharray` uses across the five styles draw as solid lines. Rail and tunnels show it most.
- **Patterns:** `fill-pattern` layers are skipped entirely, because a solid fill in their place would be wrong.
- **`fill-translate`:** ignored. `bright`'s `building-top` layer uses it for a raised look.
- **`line-gap-width`:** ignored. One layer uses it, `waterway_tunnel` in `liberty`.
- **`line-blur`:** ignored. Four layers use it, in `dark`'s boundary layers and one other style.
- **Custom caps and joins:** `line-cap` and `line-join` are not applied, as upstream does not apply them.
- **Building opacity:** `liberty`'s 0.8 `fill-extrusion-opacity` is not applied, so buildings look heavier and hide the
  roads behind them. It needs an offscreen composite, whose cost has not been measured.
- **`fill-outline-color`, `fill-antialias`:** ignored, as upstream ignores them.

Dashes, patterns, caps and joins were out of scope for the renderer work, so none of these is a defect in it. Which of
them matter is judged once the widget can be seen in a clip.

## Working rules

- Compile checks only while iterating (`cargo check -p ovrley_map`, `-p ovrley_widgets`, `-p ovrley_map_gpu`).
- WASM builds, benchmark builds and `pnpm build` run only with your approval, at the gates above.
- No new tests unless requested. The two with the highest value would be camera math, and
  preview-versus-export pixel parity.

## Noticed, not changed

Preview and export already disagree on stacking for existing widgets: export draws routes over values,
while the preview puts values (z-10) over routes (z-3). The renumbering in Phase 4 preserves that order.
