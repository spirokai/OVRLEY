# Map tile widget — implementation plan

Status: Phases 1, 2 and 3 built. Video exports with a map work and passed the Phase 3 gate on Windows. Phase 4: built, with the route download added after the gate results below, so it has not been run. Remaining gate checks are manual. The app can't add a map until Phase 5.
Updated 2026-10-30.
Builds on `map-tile-widget-research.md`.

**Phases 3–5 run on a different machine.** Timings here came from an AMD integrated GPU and aren't comparable there, so
re-measure a baseline before judging any target. Anything needed has to be downloaded there too, because nothing under
`E:\mlrs` or `debug/` is in git: the maplibre-rs reference, the Native reference tool, the scratch tile cache with its
live styles, and the activity fixtures the gates use, including `Evening_Ride-parse-debug.json`.

**The renderer changed.** The widget now uses the GPU renderer in `ovrley_map::gpu`, which draws
one finished map frame per camera, including real 3D buildings that flat tiles cannot produce. Its gate results
are in the Phase 2 handoff. Phase 1's ezu rasterizer, its fonts and sprites, and the gate binaries are removed, and remain
in git history. Commands below that name `map_gpu_gate` or `map_tile_gate` record how results were produced.

## Decisions

| Topic                    | Decision                                                                                                         |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| Map rendering            | `ovrley_map::gpu` (wgpu), native for export and WebGL2 for preview. Replaces ezu for this widget                 |
| Preview topology         | `ovrley_map::gpu` in its own WASM module on WebGL2, drawn under the Skia widget. Measured in Phase 4             |
| Labels                   | **None for now.** The GPU renderer draws no labels. Needed before release, approach open                         |
| Attribution              | Always drawn in the widget, not configurable                                                                     |
| Tile pinning             | None. Keep `planet/latest` and the existing HTTP revalidation                                                    |
| Map styling              | Style preset and layer toggles (roads, paths, water, buildings). No map colour overrides                         |
| Route and marker styling | Same fields and controls as the route widget (see below)                                                         |
| Route drawing            | A ground layer in `ovrley_map::gpu`: above roads, below buildings, so buildings hide it. The marker stays on top |
| Camera                   | `fixed` (whole route, north-up, pitch 0) or `follow` (`north_up` / `rider_up`, pitch 0–60, `range_meters`)       |
| Not in v1                | Speed-dependent range, terrain, raster layers, upright point labels, map matching, corner radius or border       |
| Export without a GPU     | Fails before encoding. Software adapters are refused, so there is no CPU fallback                                |
| Pool evictions           | Draw only tiles under the camera's trapezoid, size pools per job, and guarantee every tile in view is resident   |

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
ovrley_map::gpu                    tile and route tessellation + wgpu → RGBA frame        native only
      │                       │
      │ in process            │ same code, WebGL2, own WASM module (`src-tauri/wasm-map-bindings`)
      ▼                       ▼
export provider (Phase 3)     map canvas under the widget (Phase 4)
      │ impl MapFrameProvider ▲ view and route line through the ABI
      ▼                       │
ovrley_widgets::widgets::map       camera, route line, frame image, marker, attribution

ovrley_map (featureless)           shared vocabulary and the camera matrix, used by both sides
```

**`src-tauri/ovrley_map`** owns the shared vocabulary and the renderers.

- Without features it has no Skia, no ezu and no wgpu: `MapStyle`, `MapLayers`, `MapStyleSpec`, `MapDataSource`,
  Mercator math, and, from Phase 2, `MapCamera` and the view matrix. `ovrley_widgets` depends on this, so WASM never
  compiles a renderer. The route and the map must use the same matrix, or the route drifts off the road.

**`ovrley_map::gpu`**, behind the `gpu` feature, holds the renderer. `ovrley_widgets` must never enable it. `wasm-bindings` and
`wasm-map-bindings` are isolated workspaces, so the Skia module never links `wgpu`. `ovrley_core` depends on it for export, so ordinary
app builds include `wgpu`.

## Phase 1 — Tile rasterizer and style adapter

**Built, on commit `bc7c49fe`, then superseded and removed.** The rest of this section records what was built.

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

**Provider.** `MapFrameProvider::frame(request) → Ready(Image) | Pending`, where the request holds spec, camera, viewport
and the route to draw. It never blocks on the network. Export's provider never returns `Pending`.

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
- The last frame is cached per request. The route is in the frame, so a request changes whenever progress does, and a
  fixed camera asks again as progress moves. A frame is only ever drawn for the visible position.
- On `Pending`, fill with the preset's flat placeholder colour, then the marker and attribution. Never draw a stale
  frame: its camera differs. The route is not shown until the frame arrives.

**Route and marker**

- The route is one more ground layer in the renderer, drawn after all other ground layers and before the buildings, so
  buildings hide it exactly as they hide roads. It's tessellated like a road, at constant screen width, with the same
  matrix as the map, so it can't drift off the road.
- The widget builds the simplified line, with travelled distance on each vertex, and sends it with the style and current
  distance. The fragment shader splits travelled from remaining at that distance.
- A stencil paints each sample once, so crossings and section seams don't double the opacity. Edges rely on MSAA.
- The line is cut into sections, each with its own origin and matrix, for `f32` precision and to cull far sections.
- `simplify_tolerance_px` applies at the widget's constant view zoom, using the route widget's
  simplification helper.
- `show_full_activity` selects the route window through `host.plot_window`, as the route widget does.
- Marker drawn over the finished frame with `plot_marker`, with every variant available, so it is always visible.
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

**No activity or no GPS.** Flat placeholder colour, placeholder route drawn in Skia, and attribution, with no
network requests.

**Gate**

- **Native:** the real widget with the real renderer: stills and a clip for all five styles, in `follow` and `fixed`.
  You judge whether the route stays on its road while the camera moves.
- **WASM (needs your approval for `pnpm benchmark:widgets:build` and the run):** a `map` benchmark case with a
  pre-rendered frame at pitch 0, 40 and 60, at 500 and 1000 px. Target: under ~8 ms/frame at 500 px. This measures
  the camera, image blit, marker and attribution. The route is in the frame, so its cost shows up in the renderer's
  frame time at the native gate.

## Phase 3 — Export pipeline

**Built and through its gate on Windows.** A video export can draw a map, and fails before encoding if any tile is
unavailable. Nothing in the app can add a map yet, because the widget reaches the editor in Phase 5.

**Ingress.** `ValidatedRenderConfig.maps`, filled by `validate_map_widgets` in `normalize/widgets.rs`. The data source is
passed beside `resources` through `submit_single`, batch submission and `plan_single_render`. A native tool has none, and
fails with a config error as soon as a map needs tiles.

**Prepare (`render/map_export.rs`, inside `ExportWidgets::prepare`, before FFmpeg starts):**

1. Walk the camera over every rendered dense frame. `ActivityMap::views` in `widgets::map` owns this, since the widget
   owns the camera and viewport.
2. Fetch the style and every tile on 8 threads, checking cancellation and reporting progress on the preparing message.
   Bytes are held for the job, so disk eviction or a lost connection can't affect a running export.
3. Tessellate each tile once to count its geometry, which also rejects tiles that don't decode, then size the pools.
4. Open the renderer. No adapter, a software adapter, or a buffer the device doesn't allow fails here.

**One route line per job.** `MapContent` holds the track, camera, viewport and line. It is built once and handed to every
worker's widget through `ValidatedWidget::Map`, because the renderer recognises a line by `Arc` identity.

**One GPU thread.** Workers send requests and wait, and the thread owns one renderer per map widget, each on its own
device. Tiles load on demand. A worker gets its frame or an error, and an error fails the job through the normal pipeline
shutdown, which also removes the partial output.

**Evictions.** The answer is per-job pools plus drawing fewer tiles. A fixed zoom and pitch limit predicts this badly, since
the same view evicts in a city and not in farmland. VRAM isn't used either: wgpu doesn't report it reliably, and output must
not depend on the machine.

- **Indices fill first, not vertices.** Of the 48 eviction-prone views re-measured after the culling below, 9 still exceed
  the old 2M limit: all on indices, 4 on vertices too, none on the layer table. That fits rural z14 evicting at 1.39M
  vertices, and vertex count not predicting eviction.
- **Most of the tiles were never visible.** Views selected the bounding box of the camera's trapezoid. Testing the
  trapezoid itself takes central Zurich from 168 to 86 tiles (z14, square), 315 to 95 (z14, tall) and 54 to 21 (z15.5,
  tall). `view_tiles` and drawing share the region, so they cannot disagree.
- **Pools are sized per job** by `GpuPoolSizes::for_views`: 1.5× the largest view, at least the old default, and never
  more than the job's whole geometry. Vertices, indices and layers are sized separately.
- **Every tile in view is resident before drawing.** `GpuMapRenderer::load_view` loads the view, and if that evicted a tile
  the view still shows, it empties the pools and reloads only that view. An empty ring holds any view that fits, so a
  failure there means the view really doesn't fit, and the job fails with an error instead of drawing a blank patch.
- **A view at the 512 tile limit fails during the walk**, before encoding, instead of silently losing tiles.

**Stacking (`engine.rs`).** Order is backdrops, maps, rasters, labels, values and the rest. A label or raster whose
rectangle meets a map draws after it every covered frame, and such a raster also stays in the uncovered base, so it still
shows outside activity coverage.

**Style cache.** A cached style is now refreshed after a day, keeping the cached copy if that fails. The handoff's stale
`liberty.json` lacked the `hide_3d` filter, and the old cache never revalidated.

**Different from the plan:**

- `render_data_requirements` uses `include_widget("course")`, since a map reads the same columns. `"map"` has no manifest
  entry until Phase 5, and looking it up would panic. Switch to it there.
- The missing-source check runs in prepare, not at submission, so it is one check that also covers the debug preview path.
- `upload_layer` now reports a layer that can't fit through `TileLoadReport.error`, instead of logging and skipping it.

### Gate results

RTX 4070 on Vulkan, Ryzen 5 5600X, `Evening_Ride(4)-parse-debug.json`. Run with
`map_gpu_gate <cache> <absolute out> export`, about 2.5 minutes, or `export-route` and `gaps-evict` for parts. Building the
gate needs `TAURI_CONFIG={"bundle":{"resources":{"../THIRD_PARTY_NOTICES.txt":null}}}`, because that file is generated by
packaging.

| Check                                       | Result                                                                       |
| ------------------------------------------- | ---------------------------------------------------------------------------- |
| Same project exported twice                 | All frames identical: 300 of 300, then 90 of 90 on the final code            |
| Offline, uncached route                     | Fails before encoding, no output file                                        |
| No GPU adapter (`WGPU_BACKEND=metal`)       | Fails before encoding, no output file                                        |
| Views that evicted in Phase 2               | 0 evictions, no blank patches in the stills, including z14, pitch 60, tall   |
| Pools too small for a view                  | `load_view` fails loudly                                                     |
| Route longer than the pools (z14, pitch 60) | 5–6 pool clears over 600 frames, costing about 1.3 s of load in total        |
| Composite outlasting the activity           | Map disappears, raster over it stays                                         |
| Scene without a map, old and new binary     | All 60 frames identical (1080p, transparent)                                 |
| `ovrley_core`, map tile service, map crates | 242 pass, 1 ignored. The other `ovrley_core` integration suites compile only |
| Binary size                                 | `render_video` 20.3 → 27.4 MB, so `wgpu` and the renderer add about 7 MB     |

**Speed.** With maps up to 1000 px wide the GPU thread spends about 4–12 ms per frame on tile upload, drawing, readback and
wrapping the image, the slow end being z14 at pitch 60 in the city. That caps exports near 85–250 fps, above what the encoders
reached here:

| Scene                                  | With map | Without  |
| -------------------------------------- | -------- | -------- |
| 1080p composite, libx264 near lossless | 38.8 fps | 47.9 fps |
| 4K transparent, qtrle                  | 13.9 fps | 22.4 fps |

In the 4K case that is about 27 ms per frame more, of which the GPU thread is about 4 ms. The rest wasn't measured separately.
Encoding the map's detail in qtrle is the likely cause, along with drawing it into the frame. Both codecs are from existing
tests that need near-lossless frames, and the composite clip shows the map on only 36 of its 60 frames, so these say little
about real exports.

**Hardware export (`export-hardware`).** One 60 s run of `nnvgpu_hevc` (CUDA overlay, NVENC) at quality 23 over a 4K HEVC
video, with the 4K template and a 1000 px map on every frame: **47.9 fps**, 1799 frames in 37.5 s, with no evictions. Worker
time per frame was about 60 ms across about 3 workers: 32 ms drawing all widgets, 23 ms converting to `yuva420p` and 5 ms
copying the base. The GPU thread took about 3.9 ms per frame of that. GPU use stayed around 20 %, encoder 30–57 %, and memory
rose by about 1.2 GB. There is no run without a map to compare with on this path.

**Baseline on this machine:** 0.2–2.4 ms per frame with readback on the renderer's own gate, against 4–8 ms on the AMD
integrated GPU, and 51 of 361 views evicted with the old pools and bounding boxes.

**Not tested:** a real software adapter, so the refusal has only been compiled, device loss during an export, an
integrated GPU with the larger pools (up to about 320 MB for the worst view seen), Linux and macOS, and the WASM build,
which needs approval. `ovrley_widgets` changed, but nothing in `wasm-bindings` calls the changed functions.

## Phase 4 — Preview wiring

Deliverable: live preview with the map following the playhead while playing and scrubbing.

**Decided: the same renderer in its own WASM module, on WebGL2.** The numbers are under Delivery measurements below. An
integrated GPU and a Mac are still unchecked.

### Delivery measurements

`map_preview_bench` (uncommitted, throwaway) opens real WebView2 with the app's browser args on an RTX 4070 and Ryzen 5 5600X.
Every candidate draws the requests the real widget makes, on `Evening_Ride(4)`, Liberty, follow, rider-up, pitch 60, MSAA 4. The
WASM renderer is a patched copy of `ovrley_map::gpu` under `debug/map-preview-spike`, which is not in git. It compiled and ran
with changes only to device setup, tile threads and output: the shaders and draw code are unchanged. Its frames match native
at SSIM 0.999999 or better in all three scenarios.

Playback against the clock at 30 fps, 180 frames, the latest request winning, at 1000 px, stacked over a 4K HEVC video. Ranges
span the 400 m and 2 km scenarios:

| Path                     | Frames shown   | Latency p50 (ms) | Latency p95 (ms) | Main thread per frame (ms) |
| ------------------------ | -------------- | ---------------- | ---------------- | -------------------------- |
| Native frames, Tauri IPC | 88–111 of 180  | 50–60            | 58–76            | about 3, plus the wait     |
| Native frames, HTTP      | 174–180 of 180 | 14–23            | 19–38            | about 3                    |
| WASM on WebGL2           | 180 of 180     | 2.8–4.7          | 3.1–5.4          | 2.5–4.3                    |

No path hurt the video: at most 1 dropped frame in any run. Sending frames through Skia instead of straight to a canvas costs
under 0.5 ms at 1000 px, so a direct path isn't worth having.

As fast as each path can go, in frames per second, waiting for the GPU:

| Path                     | 400 m, 500 px | 400 m, 1000 px | 2 km, 1000 px |
| ------------------------ | ------------- | -------------- | ------------- |
| Native frames, Tauri IPC | 74            | 19             | 18            |
| Native frames, HTTP      | 208           | 81             | 80            |
| WASM on WebGL2           | 556           | 256            | 156           |
| WASM on WebGPU           | 1111          | 59             | 59            |

IPC really used its binary path (`ipc.localhost`, no fallback warning) and moves about 80 MB/s, so it cannot hold 30 fps at
1000 px. WebGPU's 15 ms wait at 1000 px is unexplained; its playback was fine at 2–2.4 ms on the main thread.

Scrubbing from a cold renderer, 30 jumps across the route, at 1000 px:

| Path                | Until on canvas, p50 / p95 (ms)    | Main-thread tasks over 50 ms | Longest tile block (ms) |
| ------------------- | ---------------------------------- | ---------------------------- | ----------------------- |
| Native frames, HTTP | 26 / 59 (400 m), 49 / 108 (2 km)   | 0                            | none, off the page      |
| WASM on WebGL2      | 41 / 136 (400 m), 108 / 425 (2 km) | 9 (400 m), 28 (2 km)         | 93 (400 m), 174 (2 km)  |

The WASM spike decodes tiles on the page's thread, about 5–6 ms per tile and up to 30 ms for one. A worker would take this off
the UI, but has not been built or measured.

**Without a GPU in the webview** (`--disable-gpu` and `--use-angle=d3d11-warp`, with the native renderer still on the RTX, so
not a real GPU-less PC): existing Skia drawing keeps working. WebGPU finds no adapter and fails cleanly. WebGL2 silently runs on
the CPU through Microsoft Basic Render Driver and showed 180 of 180 frames on this 12-thread CPU, but Skia's time in the same
page rose from about 1.1 to 6.6–8 ms at 1000 px. wgpu reports this adapter as `Other`, so the `DeviceType::Cpu` refusal would
not catch it. Export refuses to run there, so the preview would show a map that can't be exported, unless WebGL2 also refuses.
Detecting it means matching the renderer string.

**Keep failures to the map.** `PreviewSession::render` stops at the first widget's error (`preview.rs:521`), and `onError` is
session-wide, so neither may carry map delivery failures. A trap in the widget module ends every widget until restart, which
is a reason to keep WebGL2's renderer in its own module, loaded only when a project has a map.

**Not tested:** an integrated GPU, macOS, Linux, repeated runs (one or two per cell), maps over 1000 px, several maps at once,
a WebGL context loss, and linking `wgpu` into the emscripten module instead of adding a second WASM module of 4.7 MB. HTTP
transfer also spikes by 10–20 ms on some frames, with no cause found.

- **Renderer:** `ovrley_map::gpu` compiles for `wasm32-unknown-unknown`. One draw path, with target-specific output: export
  reads back from a texture, preview presents to a canvas. WASM opens its device asynchronously and refuses software WebGL by
  renderer name, because wgpu reports it as `Other`.
- **`src-tauri/wasm-map-bindings`** (isolated workspace, like `wasm-bindings`): the wasm-bindgen module, `wasm_map_bindings`. `MapLayer` owns
  one canvas's renderer. `TileDecoder` runs in workers. `scripts/build-wasm.mjs` builds it next to the widget module, so dev
  rebuilds, CI artifacts and packaging treat both alike. It loads only when a project has a map, and a crash in it can't end
  other widgets.
- **Widget:** `PreviewMap` still owns the camera, marker and attribution. Once its layer is ready it leaves the frame clear, and
  exposes the view to draw and the route line through the ABI (`ovrley_preview_map_view_*`, `ovrley_preview_map_route_*`,
  `ovrley_preview_map_layer_ready`). Until then it draws the flat placeholder, which also covers no WebGL2, no GPU and a lost
  context. The frame upload ABI and `UploadedMapFrames` are removed.
- **`utils/mapLayers.js`:** no React. Draws in the same animation frame as the widgets, so map and marker can't disagree.
  Replaces a renderer when its style, layers, size or zoom change, and shows the new canvas only once it's ready. Its failures
  stay on the map and never reach `onError` or `PreviewSession::render`.
- **Tiles:** two workers fetch through the existing `/tiles` route, decode and tessellate, and transfer the geometry back, so
  the UI thread only uploads. The latest view replaces the queue. A view draws with what is resident, and missing tiles
  show the style's background until they arrive, so the preview never blocks. Pools start at the default and double if a view
  evicts its own tiles.
- **Stacking:** backdrop 1, map 2, raster 3, course/elevation/diving 4, others 10, matching Phase 3.
- **Route download:** `backend_map_prepare` shares the export's walk and fetch (`download_preview_tiles`), one second apart over the
  whole activity. It runs in the background once an activity and a map exist, after edits settle for 500 ms, and a newer request
  cancels the last. Only fields that change which tiles are needed restart it. It emits `map-prepare` events (needed, downloading,
  ready, failed) for the status Phase 5 shows, and stops at the first failed tile, since the preview still fetches on demand.

**Gate:** in the app, play, scrub across the route, and resize. Confirm no stalls and that video preview
stays smooth during frame loads. Compare the same second in preview and export.

### Gate results

Run in the real app on the RTX 4070, driven over the WebView2 debugging port, with the Phase 5 manifest entry applied temporarily
and removed afterwards. The ride was `Evening_Ride(4)` as a GPX, in the default HUD at a 1000 px map (552 px after the preview's
raster scale).

| Check                                             | Result                                                                                                       |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Playback, 8 s                                     | 16.7 ms median frame and no main-thread task over 50 ms. One frame hit 33 ms, and one run had an 83 ms frame |
| Scrub, 20 jumps                                   | No main-thread task over 50 ms, so tile decoding stays off the UI                                            |
| Marker and attribution over the map               | Correct, drawn by the Skia widget on a clear background                                                      |
| Style and size change                             | Replaces the renderer: Dark at 700×400 drew correctly                                                        |
| Removing the map                                  | Its canvas goes, with no error                                                                               |
| Native renderer after the split                   | `widget-smoke`: 0.59 ms at 500 px and 1.62 ms at 1000 px, no evictions                                       |
| Frontend tests, lint, `wasm_widget_preview_tests` | Pass, without the manifest entry                                                                             |

A context-lost bug showed up in the first run and is fixed: releasing a replaced renderer loses its context on purpose.

**Not checked:**

- The resize gesture: it fails with `map resize size is required`, because size-based resize is the Phase 5
  `widgetGestureGeometry.js` work. The transient path that rescales during a drag has therefore never run.
- A video playing under the map in the app. The Delivery measurements cover it for the spike only.
- The same second in preview and export. The spike matched at the same size, but the wired preview has not been compared.
- Pool growth and a real context loss, which no run triggered.
- An integrated GPU, macOS, Linux, the CI and Flatpak changes, and third-party notices for the WASM module's crates.

### Without a GPU

Run with `--disable-gpu`, so the webview had no GPU while the rest of the PC did. A PC with no GPU has not been tested.

- **The map refuses and shows its placeholder**, with the marker and attribution. The software check in `web.rs` caught Microsoft
  Basic Render Driver.
- **The whole editor runs at about 7 fps there, with or without a map.** A trace shows the time in Chromium's software compositor:
  139 ms per draw with no map and 142 ms with a refused one, over about 200 quads on a 3347×1440 window. The map's cost is too small
  to measure. Only this one template and window size were measured.
- **Later work:** a simplified CPU renderer for PCs without a GPU, for example without buildings, is an option for after this phase.
  Native `DeviceType::Cpu` refusal would then need revisiting too, since export has no map on those PCs either.

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

| Risk                                                              | Handling                                                          |
| ----------------------------------------------------------------- | ----------------------------------------------------------------- |
| Route drifts off the road if map and route use different matrices | One shared camera in `ovrley_map`; judged at the Phase 2 gate     |
| Renderer only tested at z17, pitch 60, one dense area, on Windows | Phase 2 coverage check; Linux and macOS still untested            |
| Pool eviction draws blank patches                                 | Solved in Phase 3: a view is resident before drawing, or fails    |
| No usable GPU, or device loss during an export                    | No GPU fails before encoding. Device loss fails the job, untested |
| Larger pools on an integrated GPU                                 | Up to about 320 MB seen; untested on one                          |
| Preview delivery may not hold 30 fps                              | Measured on an RTX 4070; integrated GPU, macOS and Linux untested |
| Far roads look heavier than Native, buildings heavier at 0.8      | Known visual gaps; line width fix awaits your decision            |
| GPS offset of ±3–5 m is 15–25 px at z18                           | Accepted; map matching is a separate feature                      |

## Open decisions and recommendations

| Decision            | When            | Recommendation                                                                                                                                            |
| ------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Labels              | Before release  | None yet. The idea of ezu-baked, label-only tiles draped on the ground has not been measured                                                              |
| Far road line width | Any time        | Fix it. Lines keep a constant pixel width with distance, as upstream does, while Native narrows them. It is a line vertex shader change, not yet measured |
| MSAA                | Settled for now | 4 by default, chosen by the host rather than widget config. Costs about 1.7–2×, about 5 ms at 1000 px, and fill and building edges are jagged without it  |

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

- Compile checks only while iterating (`cargo check -p ovrley_map`, `-p ovrley_widgets`, `-p ovrley_map::gpu`).
- WASM builds, benchmark builds and `pnpm build` run only with your approval, at the gates above.
- No new tests unless requested. The two with the highest value would be camera math, and
  preview-versus-export pixel parity.

## Noticed, not changed

Preview and export already disagree on stacking for existing widgets: export draws routes over values,
while the preview puts values (z-10) over routes (z-3). The renumbering in Phase 4 preserves that order.
