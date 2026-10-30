# Map tile widget — rendering research

Research date: 2026-10-26. Status: research complete; no implementation yet.

Goal: a widget that draws the activity route on top of vector map tiles, with
live preview at 30 fps through the WASM bindings and export through native Skia
with good parity. Typical widget size is ~500×500 px, occasionally up to 1000 px.
Two modes:

- **Top-down** — the map can be a (nearly) static cached image.
- **GPS-follow** — heading-up, optionally pitched, similar to the video-sync
  navigation map (`app/src/features/video-sync/map/NavigationMapController.js`).

## Recommendation (TL;DR)

Rasterize each map tile **once** into a cached Skia image (labels baked in), and
composite the cached tiles **every frame** through a camera matrix. Top-down uses
an affine matrix; follow mode uses a perspective matrix. Route and marker are
drawn per frame on top through the same matrix.

- Adopt an existing CPU tile rasterizer (lead candidate: **ezu**) behind a
  "tile bytes + style → RGBA" interface; fall back to a small own Skia painter
  using `maplibre-expr` only if ezu does not hold up.
- Write only the compositor ourselves (camera, tile cover, draw calls). It is
  small and nothing existing targets CPU Skia.
- View zoom range up to **z18**; downloaded data stays at **z14** (overzoom).
- `MapTileService` is the single owner of tiles **and styles**: the widget reads
  the same OpenFreeMap style JSON as the sync maps, with typed widget overrides
  (layer toggles, colours) on top.
- A shipping competitor (Movaro, see `movaro-map-renderer-analysis.md`) uses the
  same core design: CPU-rasterized tile textures drawn once, own compositor,
  ancestor overzoom, per-tile zoom from on-screen size.

## Current codebase facts

- Both WASM and native render with **CPU raster Skia** (`surfaces::raster`); there
  is no GPU context. WASM is single-threaded with SIMD (SSE4.1 → WASM SIMD), see
  `benchmarks/widgets/skia-simd.md`. WASM pixels are read back as straight-alpha
  RGBA for `ImageData` (`ovrley_widgets/src/render/pixels.rs`).
- Native export renders frames **in parallel threads**
  (`ovrley_core/src/render/engine.rs:206`). Therefore anything frame-dependent
  (heading smoothing, label placement, fades) must be a pure function of activity
  time — no state carried between frames. This also keeps preview scrubbing
  identical to playback.
- `src-tauri/src/map_tile_service.rs` fetches OpenFreeMap MVT `.pbf` tiles, keeps
  a 500 MB HTTP-semantics disk cache, and serves them through the loopback server
  (`video_server.rs`, `/tiles/{z}/{x}/{y}.pbf`, `/styles/{style}`). Zoom is capped
  at 14 and style sources are rewritten to `maxzoom: 14`.
- Styles: `positron`, `bright`, `liberty`, `dark`, `fiord` (OpenMapTiles schema).
  `resolve_style` fetches each once, caches it under
  `openfreemap-styles/{style}.json` (reused while valid, not revalidated), and
  serves it to the sync maps with tile URLs rewritten to the loopback server.
- The video-sync maps use MapLibre GL JS (`maplibre-gl ^6.10`). Navigation map:
  zoom 15.5, pitch 40 (0–60), 15 m lookahead, top padding 45 %.

## OpenFreeMap data limits (verified live)

- TileJSON at `https://tiles.openfreemap.org/planet` declares `maxzoom: 14` for
  the tileset and every layer.
- z15/z16 tile requests return HTTP 200 with **0 bytes**.
- A dense central-Zurich z14 tile (`14/8580/5736`) is **387 KB**.
- z14 is the full-detail level of OpenMapTiles (all buildings, roads, paths,
  POIs). MVT extent is 4096 per tile → ~0.4 m coordinate precision at mid-latitude.
- No lane markings or kerbs exist in the schema; wide roads are plain bands.
- `MapTileService` requests `planet/latest`, but the TileJSON resolves to a dated
  snapshot URL (e.g. `planet/20261018_080001_pt/{z}/{x}/{y}.pbf`), which allows
  pinning a snapshot (see contract notes).

## Why not render vectors per frame

Published CPU rasterization costs per 512 px tile:

| Project                             | Measurement                                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| MapConductor/vectortile (tiny-skia) | z14 Tokyo: 29 ms (compiled filters) – 86 ms desktop; ~100–115 ms Chromium WASM                                |
| ezu                                 | 13–30 ms/tile without labels, 42–105 ms with labels (Apple M1, single thread)                                 |
| RustGIS (#596)                      | 49–69 ms per frame while zooming at 1400×900 on the CPU path; tile pictures were rebuilt on every zoom change |

A pitched follow view covers ~15–25 tiles, so per-frame vector rendering cannot
reach 30 fps in single-threaded WASM. Tiles must be rasterized once and reused.

Cost of reusing a cached tile: perspective `drawImageRect` ≈ one bilinear sample
per covered viewport pixel (250k px at 500², 1M at 1000²). Estimated ~1–2 ms native
and ~3–8 ms in our WASM build at 500² — **not measured yet**; see spike below.

## GPU engines (rejected)

The concern that GPU engines need GPU→CPU readback is valid for WASM (WebGL
`readPixels` into a CPU Skia surface stalls), less so natively (1 MB readback is
cheap). The decisive problems are platform support and parity:

- **maplibre_native** (Rust bindings to MapLibre Native, v0.10.0, 2026-10-02):
  Vulkan/OpenGL/Metal; needs a GPU driver. Windows Vulkan/OpenGL marked "should
  be supported, but currently is not" in CI. No WASM build. Supports arbitrary
  camera (pitch, bearing, f64 zoom), custom `FileSource`. Output RGBA via `image`
  crate, alpha mode undocumented. BSD-2 core. Preview and export would run
  different engines.
- **tileserver-rs** (vinayakkulkarni, MIT, ~59 stars, active): a tile _server_
  binary (Nuxt frontend, PMTiles/MBTiles/PostGIS/COG) whose raster rendering is
  MapLibre Native via its own `mbgl-sys` FFI crate (~200 MB C++ submodule, CMake
  - Ninja + bazelisk). Prebuilt for macOS/Linux only; no Windows binary, no WASM.
    Bearing/pitch only via its HTTP static-image URL. Self-reported ~100 ms per
    raster tile warm and **~3 s per 800×600 static image**, so per-frame snapshots
    are impossible (a 1 h ride ≈ 108k frames). Same category as maplibre_native,
    in a heavier package.
- **maplibre-rs** (wgpu): headless mode is work in progress.
- **galileo**: wgpu only; other backends only mentioned as possibilities.
- **MapLibre GL JS for preview + native for export**: two renderers, parity only
  approximate.

## Off-the-shelf CPU / Rust projects

| Project                                                     | What it is                                                                                                                           | Labels                                                                                 | Usable as "MVT + style → RGBA" library?                |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| **ezu** (reearth, MIT/Apache, 0.13.1, very active, pre-1.0) | Pure-Rust CPU engine; MapLibre styles translated to its own recipes; WASM build (scalar/SIMD/threads); returns PNG, WebP or raw RGBA | Yes: point, polygon, line labels; shared cross-tile collision; deterministic placement | **Yes — lead candidate**                               |
| MapConductor/vectortile (Apache-2.0, 0.1.0, 0 stars)        | tiny-skia MapLibre-style rasterizer; background/fill/line/circle; ~530-line hand-written MVT decoder                                 | No (symbol layers explicitly out of scope)                                             | Yes, but too young; useful as a reference painter      |
| opengeos/RustGIS (Apache-2.0, ~2k stars)                    | Full GIS application (egui desktop, web, CLI); tiny-skia CPU path + optional wgpu                                                    | Partial; road (line) labels missing for vector tiles (#635)                            | No — renderer coupled to its own map model             |
| jwinarske/tessella (BSD-2, 0.0.0, unpublished crates)       | Rust port of MapLibre Native's frontend; emits a renderer-agnostic draw-command stream                                               | Yes (shaping, BiDi, collision, fades)                                                  | No — draws nothing; CPU rasterizer is a test tool only |
| protomaps-leaflet                                           | JS Canvas 2D painter with "paint rules"                                                                                              | Basic                                                                                  | Maintenance mode; conceptual reference only            |

Supporting crate: **maplibre-expr** (reearth/maplibre-expr-rs, MIT/Apache,
0.5.4) — pure-Rust MapLibre expression/filter/property evaluator; claims 563/563
upstream conformance fixtures. Deps: `serde_json`, `libm`, optional ICU collator
(~1.1 MB; disable with `default-features = false`). WASM claimed but Emscripten
not verified.

RustGIS #635 is useful validation: the maintainer plans the same design we chose
(tiles prepared once, ancestor fallback, bounded scheduler, labels as a separate
layer).

### Mipmaps under perspective

Raster Skia's `SkMipmapAccessor` disables mipmapping when the inverse matrix
cannot be decomposed into scales, which is the case for perspective matrices.
Without mitigation, far terrain would shimmer in exported video. Mitigation (as
in MapLibre): choose a lower tile zoom for far tiles so each tile stays near one
texel per pixel. Likely, but confirm in the spike.

## ezu integration (verified from 0.13.1 sources)

### Packaging

- Published on crates.io; no git dependency. `ezu`, `ezu-core`, `ezu-features`,
  `ezu-style`, `ezu-graph`, `ezu-paint`, `ezu-translate` are all at **0.13.1**
  (2026-10-22); dependencies `hokusai` 0.3.0 and `maplibre-expr` 0.5.4 are
  published too.
- Pre-1.0 and changing fast (0.11 → 0.13 within ~2 weeks): pin the exact version.

```toml
ezu = { version = "=0.13.1", default-features = false, features = ["renderer", "translate"] }
```

### Sub-crates and features

Umbrella features (default: `features`, `paint`, `style-json`, `translate`,
`renderer`). We enable `renderer` and `translate`:

| Sub-crate                                             | Role for us                                                                                                                                                                                               |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ezu-core` (always included, plus its `text` feature) | Tile/world coordinates, text shaping (`rustybuzz`)                                                                                                                                                        |
| `ezu-graph` (always included)                         | Evaluates the style recipe per tile                                                                                                                                                                       |
| `ezu-features`                                        | MVT decoding via `geozero`, geometry (`geo`, `i_overlay`)                                                                                                                                                 |
| `ezu-style`                                           | Parses ezu's own recipe JSON                                                                                                                                                                              |
| `ezu-paint`                                           | Rasterizer: **tiny-skia** 0.12, fonts via `fontdb` 0.23 + `rustybuzz` 0.20, label placement                                                                                                               |
| `hokusai`, `maplibre-expr`                            | Brush engine used by the painter; MapLibre expressions                                                                                                                                                    |
| `ezu-translate` (`translate`)                         | Converts MapLibre style JSON into ezu's recipe format. Needed at runtime because the widget reads the style JSON owned by `MapTileService`; the converted recipe is validated by `ezu-style` once at load |

Left out:

- `http`: reqwest/tokio fetching (see below).
- `parallel` (rayon): we can render several tiles at once ourselves; decide in
  the spike.
- `ezu-cli`, `ezu-wasm`.

### Dependency compatibility

- `ezu-paint` requires `image ^0.25` (no default features; `png`, `webp`, `jpeg`).
  Our `=0.25.5` pins (`ovrley_core/Cargo.toml:23`, `ovrley_widgets/Cargo.toml:18`)
  satisfy it. The pin is arbitrary, so relax both to `"0.25"` when adding ezu.
  Cargo merges features, so `webp` decoding gets compiled in (size only).
- tiny-skia, fontdb and rustybuzz are new to our tree; no overlap. Skia stays a
  separate native library; tiles reach Skia as finished RGBA.
- Labels are drawn by ezu's text stack, so label fonts are configured through
  ezu, not our Skia font pipeline.
- WASM fallback only: `ezu-graph` uses `inventory` (startup registration through
  a linker trick), which is uncertain on Emscripten. Irrelevant while tiles are
  rasterized natively.

### Host I/O: keep MapTileService

ezu's `http` feature is a small fetch helper with no persistent cache, used by
the `raster`, `dem`, TileJSON and PMTiles source types
(`ezu-paint/src/host/raster.rs`, `http_client.rs`). We use none of them (our maps
are MVT). Compared with `MapTileService` it has no disk cache, no HTTP
revalidation, no in-flight deduplication, no bounded eviction, and it would not
serve MapLibre GL JS for the sync maps. **No migration; build without `http`.**

ezu's embedding renderer (`ezu/src/renderer/mod.rs`) is designed for "a Rust
service with its own HTTP client and tile cache". The host fetches and binds
bytes:

| Hook                                          | What it gives us                                                                                                 |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `Renderer::sources()`                         | Which sources the style declares, so the host knows what to fetch                                                |
| `source_tile(name, z, x, y)`                  | Maps a z15–18 render tile to the z14 source tile it needs, from the style's maxzoom. **Overzoom is handled.**    |
| `requested_neighbor_offsets(name)`            | Neighbour tiles needed for cross-tile label collision; often none                                                |
| `load_source` + `render_tile_with(SourceSet)` | Decode a tile once, reuse it as its neighbours' neighbour; render on several threads from one shared `&Renderer` |
| `needed_codepoints` / `needed_glyph_ranges`   | Which glyphs the labels need (only relevant for SDF glyph sources)                                               |
| `OutputFormat::Rgba`                          | Straight (un-premultiplied) 8-bit RGBA; premultiply once when wrapping as `SkImage`                              |

Native adapter: `MapTileService::resolve()` → bytes → `load_source` →
`render_tile_with` → RGBA → raster tile cache.

Assets ezu needs that our service does not provide today:

- **Label fonts**: bind bundled TTF/OTF fonts through ezu's `font` source type
  instead of fetching MapLibre SDF glyph PBFs. Works offline, deterministic, no
  service change.
- **POI icon sprites**: fetch and cache through `MapTileService` alongside the
  style, or omit if POI icons are dropped at z17–18.

## Competitor observations

A competitor overlay shows a Mapbox-attributed mini map.

- Screenshot 1: heading-up, no pitch; the user reported labels rotating with the
  map.
- Screenshot 2 (close-up, "Captain Cook Dr"): estimated **z17–17.5** (±0.5;
  screenshot scale unknown). The street label follows the road; the POI "P"
  appears upright.
- Street labels map-aligned plus point labels upright is the MapLibre/Mapbox GL
  default, which suggests a live GL renderer (e.g. the Mapbox mobile SDK). Still
  frames cannot prove it; watching a turn would.
- Likely alternative: Mapbox Static Tiles API (server-rendered 512 px raster
  tiles from a vector style, no bearing parameter), rotated on the client.

Takeaway: labels rotating with the map is accepted in the market.

## Zoom decisions

- Data: always z14 downloads; higher view zooms overzoom the z14 data.
- View zoom: **maximum z18** (user decision after testing the navigation map up
  to z19).
- At z17–18 the OpenFreeMap styles show almost only street labels, which are
  map-aligned anyway → **all labels can be baked into raster tiles** for v1.

Scale at ~47° latitude, 500 px widget:

| View zoom | m/px | Widget shows | Ground speed on screen at 30 km/h | Data precision on screen |
| --------- | ---- | ------------ | --------------------------------- | ------------------------ |
| 15.5      | 1.15 | ~580 m       | 7 px/s                            | sub-pixel                |
| 17        | 0.41 | ~200 m       | 20 px/s                           | ~1 px                    |
| 18        | 0.20 | ~100 m       | 41 px/s                           | ~2 px                    |
| 19        | 0.10 | ~50 m        | 83 px/s                           | ~4 px                    |

Higher zoom conveys speed (ground pixels per second); pitch amplifies it in the
foreground. A speed-dependent zoom is possible if it is a pure function of
activity time.

Tile widths: z14 ≈ 1.67 km, z15 ≈ 0.83 km, z16 ≈ 0.42 km, z17 ≈ 0.21 km,
z18 ≈ 0.10 km.

Pitched footprint (MapLibre fov 36.87°, 500 px, z15.5): at 60° the view spans
~0.35 km wide near → ~1.4 km wide far, ~1.5–2 km deep. At 40° it reaches only
~0.5 km ahead. The 60° maximum keeps the horizon out of view (top ray ≈ 78°).

## Tile volume estimates (30 km route)

- Downloads (z14): ~25–40 tiles top-down, ~40–70 for pitched follow; **~5–20 MB**
  in urban areas, less rural. One-time, disk-cached. Independent of view zoom.
- Reuse: at 30 km/h a z14 tile is crossed in ~3.3 min (~6,000 frames at 30 fps).
  A 1 h ride (~108k frames) is served by fewer than ~100 data tiles.
- Raster tiles: ~4–9 visible at once at z18; a few hundred rendered over a 30 km
  export at z18. Each is cheap: it covers a small area of an already-decoded z14
  tile.
- Export: the camera path is deterministic, so all tiles are prefetched and
  rasterized before encoding (seconds, parallel).
- Preview: prefetch the route corridor after activity load; a cold jump needs
  ~9 (top-down) to ~20 (pitched) tiles, ~100–300 ms natively, with the parent tile
  shown meanwhile.

## Proposed architecture

```
 MapTileService (existing: MVT .pbf + style JSON, disk cache, loopback)
 ├─ prepare     route corridor → needed tiles → needed | downloading | ready | failed
        │ bytes
        ▼
 ovrley_widgets::map  (shared native + WASM code)
 ├─ style       service style JSON + typed widget overrides; translated and
 │              validated once at load (unsupported layer/operator → loud error)
 ├─ raster      tile rasterizer behind "tile bytes + style → RGBA" (ezu first),
 │              overzoom z14 → z15–18, line labels baked         ← expensive, cached
 ├─ camera      pure fn(activity time) → center, bearing, pitch, zoom
 │              (lookahead + windowed bearing smoothing; port of getCourseNavigationCamera)
 ├─ cover       MapLibre-style coveringTiles: per-tile zoom by on-screen size
 └─ compositor  per frame:                                       ← cheap
        canvas.concat_44(projection × pitch × bearing × translate)
        draw cached tile images (bilinear), parent tile while loading
        route (existing route geometry + progress) through the same matrix
        marker in screen space
```

Modes:

| Mode                           | Per-frame work                                                                                         |
| ------------------------------ | ------------------------------------------------------------------------------------------------------ |
| Top-down, full route, north-up | Compose tiles once into a widget-sized `SkImage`; per frame draw it + route progress + marker (< 1 ms) |
| Top-down follow / heading-up   | Compositor with pitch 0 (affine; mipmaps work)                                                         |
| Pitched follow                 | Full compositor; pitch ≤ 60°                                                                           |

Where tiles are rasterized for preview: **natively**, served over a new loopback
endpoint (e.g. `/map-raster/{style}/{z}/{x}/{y}?scale=`) as premultiplied RGBA;
WASM wraps the bytes as an `SkImage` and only composites. Reasons: 30–100 ms per
tile would stall the shared single-threaded WASM frame loop; native is
multi-threaded and faster; preview and export get byte-identical tile textures.
Cost ~1–2 MB per tile over loopback. Every raster cache (WASM and native) is an
LRU bounded by **entry count and bytes** (tiles are 1–4 MB each at 512–1024 px).
Fallback: same rasterizer in a Web Worker WASM instance.

Raster tile resolution: `512 × 2^(cameraZoom − z) × rasterScale` so one texel ≈
one screen pixel at the focus point; integer or half-step zooms keep tiles crisp.
Tile zoom is chosen by rounding **up** from ground resolution, so a tile is never
magnified (Movaro: `z = ceil(log2(cos(lat) · 40075016.69 / (m_per_px · 256)))`).
`PreviewRasterScale` lets preview request smaller tiles. Cover is capped per zoom
level (Movaro: ≤ 64 tiles) as a guard against degenerate views.

### Contract notes (AGENTS.md)

- Styles come from `MapTileService` (shared with the sync maps); the widget
  translates and validates the style once at load and fails loudly on anything
  unsupported. Widget overrides (layer toggles, colours) are typed config.
- **Tile snapshot pinning**: the service resolves the dated snapshot from
  TileJSON and keys cached tiles by it. One export always reads a single snapshot,
  and tiles a prepared project depends on are excluded from LRU eviction.
- **Prepare step**: map tiles are prepared explicitly (needed → downloading →
  ready | failed) from the route corridor before export. A failed fetch surfaces
  in this step, before encoding; export never draws blank map areas.
- Tile data is external input. In preview, a missing tile shows its parent or a
  placeholder (product behavior).

### Widget configuration

- Camera mode: `fixed | follow` plus `rider_up | north_up`; pitch 0–60°.
- Scale as **`range_meters`** (visible ground extent), converted to zoom
  internally and capped at z18; combines with an optional speed-dependent range
  (pure function of activity time).
- Map layers as typed toggles (e.g. roads, paths, water, buildings, labels) and
  colour overrides, applied on top of the selected style.
- Route: separate travelled/remaining colour and width, remaining opacity,
  casing; reuse the existing route widget geometry.

### Non-goals for v1

- Terrain relief. A flat pitched ground needs only `concat_44` +
  `drawImageRect`. Relief would need a DEM mesh (`drawVertices` with texture
  coordinates on a grid) and own depth handling for route, labels and marker.
- Raster layers (satellite imagery, hillshade). They would compose into the same
  cached tile before compositing, with no compositor change; data licensing must
  be checked first.

## Labels

- v1: **bake all labels** into raster tiles. At z17–18 these are street labels,
  which follow their road like MapLibre's.
- Known difference: MapLibre flips a street label that would read upside down;
  baked labels cannot flip, so cross-street names can appear upside down when
  heading roughly south. Cheap fix if needed later: bake each tile's street
  labels in both reading directions and pick per frame by heading.
- The rasterizer interface separates the two label kinds from the start: line
  labels are baked into tiles; point labels/POIs are returned as data
  (anchor, text, rank). v1 may bake both; switching point labels to per-frame
  upright drawing then needs no interface change.
- Upright point labels are drawn per frame at their map anchors, with collision
  decided at bake time using rotation-invariant (circular) boxes so placement
  stays stateless. Requires ezu to bake line labels only and expose point labels
  as data, or stripping point-symbol layers and placing POIs ourselves.
- Fonts: if we ever draw labels ourselves, use bundled fonts through the existing
  serialized-font path (broad-coverage font, prefer `name:latin`).

## Risks at high zoom

- GPS error of ±3–5 m is 15–25 px at z18; route line and marker may sit beside
  the road. 1 Hz sampling puts points ~8 m (~40 px) apart, so interpolation must
  be smooth. Map-matching would be a separate feature.
- Sparse rural areas look empty at z18, weakening the sense of speed.

## Next steps (spikes)

1. **Compositor spike**: 20 synthetic 724 px tiles, pitch 0/40/60, widget at 500
   and 1000 px. Measure ms/frame with `benchmarks/widgets` in WASM and native;
   render a short clip to judge far-field shimmer. Target: < ~8 ms/frame in WASM
   at 500 px. Compare 256 px vs 512 px tiles (LOD granularity, wasted edge area,
   draw calls) and measure raster cache bytes for a pitched view of ~20 tiles.
2. **ezu spike**: translate the cached OpenFreeMap Liberty style JSON at runtime,
   feed tiles from `MapTileService` through the embedding renderer, render natively
   at z14–18 — ms/tile, label quality, overzoom appearance (`source_tile`), bundled
   fonts via the `font` source; check that it can bake line labels only and return
   point labels as data, and that widget overrides (layer toggles, colours) map
   onto the translated recipe. A `wasm32-unknown-emscripten` build is optional
   (native-only rasterization is the plan).
3. **Own-painter baseline**, in the same spike, for one style: minimal Skia painter
   (background/fill/line on OpenMapTiles classes) + `maplibre-expr` with
   `default-features = false`. Movaro ships a hand-coded painter, so this is a
   viable path; choose by output quality, speed and override fit before committing
   to pre-1.0 ezu.

## Sources

- maplibre_native: https://docs.rs/maplibre_native · https://github.com/maplibre/maplibre-native-rs
- maplibre-rs: https://maplibre.org/maplibre-rs/docs/api/maplibre/
- galileo: https://github.com/Maximkaaa/galileo
- MapConductor/vectortile: https://github.com/MapConductor/vectortile
- ezu: https://github.com/reearth/ezu · https://reearth.github.io/ezu/maplibre/compatibility/ · https://crates.io/crates/ezu (0.13.1 sources inspected)
- maplibre-expr-rs: https://github.com/reearth/maplibre-expr-rs
- RustGIS: https://github.com/opengeos/RustGIS · issues #596, #635
- tessella: https://github.com/jwinarske/tessella
- tiny-skia: https://github.com/linebender/tiny-skia
- Skia mipmaps: https://codebrowser.dev/flutter_engine/flutter_engine/third_party/skia/src/core/SkMipmapAccessor.cpp.html
- Mapbox Static Tiles API: https://docs.mapbox.com/api/maps/static-tiles/
- protomaps-leaflet: https://docs.protomaps.com/pmtiles/leaflet
- OpenFreeMap TileJSON: https://tiles.openfreemap.org/planet
