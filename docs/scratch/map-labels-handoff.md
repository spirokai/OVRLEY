# Map labels: handoff

Written on 2026-11-01, at commit `88fed7c9` on `rust-map-tiles`. Nothing here is built yet. The line counts are estimates
from reading the code, and no part of this has been prototyped.

This supersedes the "Labels" rows in `map-tile-widget-plan.md`, which still say the approach is open. The ezu idea of baked
label tiles in that plan is dropped.

## Goal

Add road, river, lake and settlement names to the GPU map renderer (`src-tauri/ovrley_map/src/gpu/`). It serves export
natively and the preview over WebGL2 through one draw path, so labels must draw the same in both.

**Out of scope:** POI labels, highway shields and `ref` text, and icons, including the settlement dots.

## Decisions

These are settled, so don't reopen them without asking.

| Decision                                   | Choice                                                                                                                                |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| Text stack                                 | OpenFreeMap's pre-made SDF glyph files, plus two small crates for right-to-left and Arabic. `cosmic-text` was considered and rejected |
| Road and river names                       | Flat on the ground, built once per tile, stored in both reading directions. The shader picks the one that isn't upside down           |
| Settlement and lake point names            | Upright on screen. In chase mode they counter-rotate against the map, in the shader                                                   |
| Latin, Greek, Cyrillic                     | Bundled, in Regular, Italic and Bold                                                                                                  |
| Arabic, Hebrew                             | Bundled in Regular only, using `unicode-bidi` and `ar-reshaper`                                                                       |
| Chinese, Japanese, Korean                  | Later, as a one-time download kept in the map cache without expiry. Not bundled                                                       |
| Devanagari, Thai and other complex scripts | Not supported. They need real shaping, which this design can't do                                                                     |
| Fades                                      | None. Placement must be a pure function of the frame, see below                                                                       |
| Road names and buildings                   | Road names are hidden behind buildings. Upright names draw on top                                                                     |

## Constraints

- **No state between frames.** Export draws frames in parallel workers, and scrubbing must match playback
  (`map-tile-widget-research.md`, line 40). So MapLibre's history-based fading and collision can't be copied.
- **Tile preparation sees one tile.** `prepare_tile` runs on worker threads, and in workers in the preview, with only that
  tile's bytes. Glyph metrics must therefore be available to it without async work, which is why bundled glyphs are simple
  and downloaded ones are not.
- **The style zoom is fixed for a renderer's life** (`GpuMapConfig::style_zoom`). This is what makes one placement per tile
  enough for ground labels.
- **WebGL2 allows 16 vertex attributes.** The line pipeline already uses 15, so give labels their own pipeline.
- **The map WASM module has no Skia** and is deliberately separate from the widget module, so it can't borrow Skia's text.
- **Code rules:** `AGENTS.md`. Styles are external data, so a layer that can't be read is logged and skipped.

## What the styles ask for

All five presets use only Noto Sans Regular, Italic and Bold, from
`https://tiles.openfreemap.org/fonts/{fontstack}/{range}.pbf`.

Relevant source layers: `transportation_name`, `waterway`, `water_name`, `place`.

- **Layout properties in use:** `symbol-placement`, `symbol-spacing`, `text-field`, `text-font`, `text-size`,
  `text-letter-spacing`, `text-max-width`, `text-max-angle`, `text-transform`, `text-anchor`, `text-offset`, `text-justify`,
  `text-rotation-alignment`, `text-pitch-alignment`.
- **Paint properties in use:** `text-color`, `text-halo-color`, `text-halo-width`, `text-halo-blur`, `text-translate`,
  `text-opacity`.
- **`symbol-placement` is a zoom `step`** on the shield layers, but those are out of scope.
- **Skip by layer content, not by name.** Layers whose `text-field` reads `ref` are shields or motorway refs. Ignore
  `icon-*` entirely, including on place layers.
- **`dark` and `fiord` ask for `text-pitch-alignment: viewport`** on road names. Ignore it for the first version, and draw
  those flat too. See the readability gate in Stage 1.

The text field is the same almost everywhere:

```json
[
  "case",
  ["has", "name:nonlatin"],
  ["concat", ["get", "name:latin"], "\n", ["get", "name:nonlatin"]],
  ["coalesce", ["get", "name_en"], ["get", "name"]]
]
```

Places use `"\n"` and road names use `" "`.

**Missing glyphs rule:** if the text holds characters with no bundled glyph, fall back to `name:latin` alone. With no
`name:latin` either, skip the label. Never draw empty boxes. Don't fall back to `name_en`, because when there's no English
name it repeats the native one.

### Latin name coverage

Named features with a `name:latin`, from one z14 tile per location. These are indicative only:

| Location       | Places        | Roads           | Lakes and ponds | Rivers      |
| -------------- | ------------- | --------------- | --------------- | ----------- |
| Tokyo, Shibuya | 232/232       | 116/186 (62 %)  | 10/13           | 3/12        |
| Nagano, rural  | 1/1           | no named roads  | 1/2             | 0/5         |
| Beijing        | 17/109 (16 %) | 90/137 (66 %)   | 4/4             | 10/12       |
| Shanghai       | 76/87 (87 %)  | 224/236 (95 %)  | 2/3             | 1/1         |
| Lanzhou        | 22/40 (55 %)  | 33/79 (42 %)    | 0/3             | 1/1         |
| Tehran         | 48/64 (75 %)  | 144/1119 (13 %) | not counted     | not counted |
| Riyadh         | 7/7           | 371/410 (90 %)  | not counted     | not counted |
| Cairo          | 28/28         | 584/584         | not counted     | not counted |
| Tel Aviv       | 22/22         | 191/191         | not counted     | not counted |

So without Chinese and Japanese glyphs, a ride through a smaller Chinese town or rural Japan can lose half or more of its
road and river names, and no error will say so. Nearly every named feature in those places has `name:nonlatin`.

## Glyph data

Measured for Noto Sans Regular, uncompressed:

| Range file                                | Holds                               | KB           |
| ----------------------------------------- | ----------------------------------- | ------------ |
| `0-255`                                   | Latin                               | 77           |
| `256-511`                                 | Central European, Turkish           | 127          |
| `7680-7935`                               | Vietnamese and other extended Latin | 131          |
| `8192-8447`                               | Punctuation                         | 34           |
| `768-1023`                                | Greek                               | 75           |
| `1024-1279`                               | Cyrillic                            | 125          |
| `1280-1535`                               | Armenian, Hebrew                    | 78           |
| `1536-1791`                               | Arabic base                         | 96           |
| `65024-65279`                             | Arabic joined forms                 | 78           |
| `64256-64511`, and two more up to `65023` | Persian and Urdu forms, ligatures   | 470 in total |

**The bundle:**

- Latin, three weights: about 1.1 MB.
- Greek and Cyrillic, three weights: about 0.6 MB.
- Hebrew and Arabic, Regular only: about 250 KB.
- Persian and Urdu forms: the first of the three files is probably enough if ligatures are skipped. Measure that file, and
  check that `ar-reshaper` can skip ligatures, before deciding.

This all goes into the preview's map WASM module, which only loads when a project has a map.

**Format:** a protobuf `glyphs { stacks { name, range, glyphs { id, bitmap, width, height, left, top, advance } } }`, with a
one-byte SDF per pixel. These are upstream's conventions, so confirm them against real files: 24 px base size, and a 3 px
border around each bitmap. `geozero` already brings in `prost`, or the format is small enough to read by hand.

**Licence:** Noto Sans is under the SIL Open Font License. Add a `NOTICE.md` beside the files, as
`gpu/patterns/NOTICE.md` does. I haven't read the licence text of OpenFreeMap's built glyph files themselves, so check it.

**`.gitattributes`:** add `*.pbf binary`, as was done for `*.rgba`. The repo normalises line endings by default, which would
corrupt these files if git took them for text.

## Stage 1: glyphs, road names and river names (about 700-800 lines)

The point of this stage is to answer the readability question below before building the rest.

1. **Glyph module:** parse the bundled files once, and build one fixed `R8Unorm` atlas. Workers read metrics only. Put lookup
   behind one small interface, so that downloaded glyphs can be added later without touching placement.
2. **Style:** add `VectorPaint::Text` in `style.rs`, and let `is_drawable` accept `symbol` when `MapLayers.labels` is on.
   Evaluate size and letter spacing at the view zoom, as line width is. Colours stay per feature, as elsewhere.
3. **Layout along a line:** in `tile.rs` or a new `labels.rs`, place anchors along each line every `symbol-spacing`. Keep only
   anchors inside the tile, so each exists once. Reject curves sharper than `text-max-angle`, and reject labels longer than
   their line. Emit one quad per glyph on the ground, in both reading directions.
4. **Flip:** the vertex shader compares each label's reading direction on screen with the screen's x axis, and collapses the
   quads of the wrong direction. This keeps chase mode free of per-frame CPU work.
5. **Ground collision:** one pass per tile, in map space at the style zoom, by priority. Ground labels then never pop as the
   camera moves.
6. **Pipeline:** a text pipeline and shaders, with SDF edge and halo. Use `StencilUse::Ignore`, because the tile stencil would
   cut labels at tile seams.
7. **Draw order:** in `record_main_pass`, draw after the extrusions and before the rounded-corner mask. The ground fills set z
   to a layer number and write no depth, but buildings do write it. So flat labels need their real clip depth and a depth test
   with no writes, to hide behind buildings. A flat label therefore draws over the route.
8. **Data path:** see the checklist below.

**Readability gate:** at pitch 60, flat text on roads crossing the view is squashed to about half height, and shrinks with
distance. Line width now narrows by depth only, which avoids that squash, but flat text can't. Look at stills and a chase-mode
clip, at z14-17 and at pitch 0, 40 and 60, before starting Stage 2. If it reads badly, the fallback is screen-facing road names
with per-frame geometry, which is a much larger job. Ask before going there.

## Stage 2: upright names, per-frame collision and the toggle (about 500-700 lines)

1. **Upright labels:** `place` and `water_name` point features. Implement `text-max-width` wrapping, `text-anchor`,
   `text-offset`, `text-justify` and `text-transform`. Each vertex holds its anchor in tile units and a pixel offset, so the
   shader billboards it. Decide whether they draw at a constant pixel size, or narrow with depth as lines do.
2. **Per-frame collision:** upright labels only. They're few, so a greedy pass in screen space by priority should be cheap.
   Visibility is written once per frame. Labels will pop rather than fade. Use a stable priority so that placement doesn't
   flicker while the camera is steady.
3. **Water line labels:** `water_name_line_label` draws like rivers.
4. **Right-to-left and Arabic:** one conversion on the text before layout: `ar-reshaper`, then `unicode-bidi`. Add about
   50-100 lines. Neither crate is in `Cargo.lock` yet. `wasm-map-bindings` has its own workspace and lock file, so check both.
   `ar-reshaper` is small and less proven (1.5.0, last updated January 2024, about 33K downloads). If it disappoints, the
   joining table is small enough to write. Have someone who reads Arabic or Persian check real frames, because wrong joining
   can look plausible to everyone else.
5. **Editor toggle:** mirror `show_buildings` (see the checklist).

## Later: Chinese, Japanese and Korean

Not in the first two stages. Build it when there are riders there.

- **Characters are scattered.** A 5x5 block of z14 tiles, about 12 km across, uses few characters but touches nearly every file:
  844 characters in 81 files in Tokyo, 179 in 57 in rural Nagano, and 1,684 in 84 across five areas. So fetching files only as
  needed saves little.
- **Sizes, extrapolated from samples, for one font:** Chinese and Japanese, about 84 files, 16 MB on disk and 10 MB compressed.
  Korean, about 44 files, 8 MB and 4 MB. Japanese adds only kana, because it shares its kanji files with Chinese. One font is
  enough, because these scripts have no true italic.
- **Fetch the whole set** on the first need, not range by range.
- **Store it in `openfreemap-styles`**, beside the styles, with no expiry. `scan_persistent_entries` skips that folder, and
  deletes files it doesn't recognise anywhere else. The set is capped at about 24 MB. Write each file with `promote_style_asset`
  so an interrupted download can't leave a broken one.
- **Glyphs must exist before layout,** so this touches the tile service, export preparation and the preview's tile workers,
  and needs an atlas that can grow. Estimated at 300-400 lines.
- **Consider a "clear map cache" setting** at that point. `MapTileService::clear()` exists but nothing calls it.
- **Not yet confirmed:** that the served CJK glyphs draw correctly. Only their existence and size have been checked.

## Checklist of code to touch

**Adding a third geometry pool** (`BufferPool<LabelVertex>`):

- `tile.rs`: `PreparedTile.labels`, `LayerKind` (a new kind means updating `Pipelines::layer`).
- `pool.rs`: nothing should change, because it's generic.
- `sizing.rs`: `GpuPoolSizes` gains `labels`, in `uniform`, `for_views` and `measure`.
- `mod.rs`: `validate_config`, `check_pool_sizes`, `assemble`, `pool_bytes`, `evicted_layers`, `load_view` (it clears the pools),
  `load_tiles` (upload and evicted tiles), `queue_items`, `record_main_pass`.
- `web.rs`: `encode_prepared`, `decode_prepared` (the header has two counts today), `upload_tile`, `grow_pools`. Layer kinds 3
  and up already mean fill patterns, so choose values with care.

**Toggle:**

- `ovrley_widgets/src/widgets/map/config.rs`: a new required `show_labels` beside `show_buildings`. Malformed config must fail.
- `validation.rs`, around line 147: `labels: false` is hard-coded, and its comment must go.
- `preview.rs`, line 255, and `RendererConfig` in `wasm-map-bindings/src/lib.rs`: pass it through. They must stay in step.
- `assets/standard-widgets.json`, line 494: the default.
- `MapWidgetEditor.jsx`, around line 176: the control, with its key in all the locale files under `app/src/i18n/locales/`.
- `app/src/features/wasm-widgets/utils/mapDownload.js`: check whether it needs the field.
- `ovrley_core/tests/map_frame_copy_tests.rs`, line 22: a config fixture that will need the new field.

`MapLayers.labels` and `hides_layer` already exist, and already hide every `symbol` layer when it's off.

## Open questions

- **Label collisions across tile edges.** Workers see one tile, so two neighbours can each place a label at their shared edge.
  Keeping anchors inside the tile stops the same anchor being placed twice, not two nearby labels overlapping. A pass over
  whichever tiles are resident would make placement depend on what's loaded, which breaks the pure-function rule. Start by
  accepting it and look at stills. Find a deterministic rule only if it shows.
- **The same road name repeated across tiles.** `symbol-spacing` restarts in each tile, so spacing isn't even across seams.
- **Parent tile fallback in the preview.** A parent standing in for missing tiles has its own labels, which swap when the
  children arrive. Export always has every tile, so this is preview only.
- **Curved Arabic.** Each glyph is a separate quad, so connected letters can show small breaks on tight bends.

## How to verify

- **Look at it first.** Parse errors in a style layer only log a warning, so a broken layer vanishes silently. During the
  style work, two bugs of this kind produced believable timings until a geometry or pixel check caught them.
- **Known traps in `maplibre-expr`:**
  - A bare array is not an expression: `parse_property` rejects `[2, 2]`. See `zoom_numbers` in `style.rs`.
  - A colour must go through `typecheck` with `Type::Color`, or it evaluates to a string, and `evaluate_color` then drops the
    feature. See `color_expr`.
- **Benchmark:** the earlier benchmark wasn't kept. Write a throwaway one, and don't commit it. The method that worked:
  - Draw one camera path with and without the feature, in one process, alternating runs, at 1000 px and MSAA 4.
  - Time 3 runs of 300 frames after 30 warm-up frames, with cached tiles.
  - Run an identical pair as a noise control. It showed about 0.02 ms, with occasional spikes of 0.1 ms.
  - Fail when the view holds none of the feature's geometry, and diff the stills.
  - Across builds, alternate two binaries, and confirm that they really differ before trusting a result.
- **Today's frame times on the RTX 4070** at 1000 px and MSAA 4: about 0.8-1.1 ms at z16 in Zurich, and 1.8 ms at z15. Also
  report vertices, GPU buffer sizes and tile preparation time, because those have been the real costs so far.
- **Preview check:** `WGPU_BACKEND=gl` runs the shaders natively over OpenGL, and `cargo check -p ovrley_map --features gpu
  --target wasm32-unknown-unknown` compiles the preview path. Neither replaces running the preview.
- **`cargo check -p app` fails here** because `tauri.conf.json` names a `THIRD_PARTY_NOTICES.txt` that doesn't exist on this
  machine. That's unrelated to this work.

## Working rules

- No `pnpm build` without the user's permission. WASM builds and benchmark builds also need approval, unless asked for.
- Commit only when asked. No Claude attribution in commits.
- No new tests unless requested.
