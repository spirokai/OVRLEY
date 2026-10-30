# Handoff: bring the maplibre-rs renderer into OVRLEY

Status: not started. Date: 2026-10-30. Branch: `rust-map-tiles` (ezu spike is commit `bc7c49fe`).
Upstream reference: `maplibre/maplibre-rs` commit `96b50a09`, MIT OR Apache-2.0.

Read `AGENTS.md` first. Its code rules apply to everything here.

## Goal

Build a native, GPU, per-frame map renderer inside our codebase by porting the rendering method of
maplibre-rs, and test it as an alternative to the ezu tile path. Deliverable: a headless renderer
and a gate binary that draw a moving, pitched map of our cached tiles with 3D buildings, as RGBA, with
measured frame times.

The reason for this work is the look in `E:\Downloads\map-buildings.png`: a heading-up, pitched map
with real 3D buildings, which baked flat tiles cannot produce. Exports must stay fast, so the frame
cost has to stay close to what maplibre-rs achieves.

## Scope

In scope:

- Vector tiles: background, fill and line layers from our OpenFreeMap styles.
- 3D buildings from `fill-extrusion` layers.
- Headless rendering with blocking readback, a camera with pitch and bearing, overzoom past z14.
- A gate binary with timings, stills and a clip.

Out of scope for now, do not start these:

- Labels, shields, icons, one-way arrows, all `symbol` layers. Skip them, no SDF or glyph code.
- Dashes, custom caps and joins, patterns. maplibre-rs draws none of these either.
- Route, marker, attribution, widget config, editor, export integration, preview integration.
- Removing ezu. It stays as the comparison. Do not make the new code depend on it.

## The governing rule

**Match the rendering method of maplibre-rs so we stay close to its performance. A minor regression is
acceptable.** Port its approach before improving it. Do not redesign draw submission, merge layers into
single draws, or add batching in this work, even where it looks like an easy win. If you believe a
deviation is needed, list it in the results with its measured cost. The allowed deviations are listed
under "Required deviations".

Only the method is being ported. Its ECS (`tcs`), render graph, schedule, `Kernel`, `Eventually`, HTTP
layer and async procedure calls are not. Replace them with plain structs and direct calls.

## The method to match

Paths are under `maplibre/src/` in upstream.

**Per frame, in one render pass:**

1. Clear colour, depth and stencil (`render/main_pass.rs`).
2. Draw every visible tile's mask first, writing that tile's stencil value
   (`render/render_commands.rs`, `render/shaders/tile_mask.vertex.wgsl`). The value comes from
   `stencil_reference_value_3d` in `coords.rs`: `z * 4 + x parity + 2 * y parity`-style cases. MVT tiles
   carry geometry beyond their extent, and the masks stop neighbours double-drawing it.
3. Draw layers sorted by style layer index (`render/systems/sort_phase_system.rs`,
   `vector/queue_system.rs`). Each item is one layer of one tile, so draws are layers × tiles. This is
   the shape to keep.
4. Each draw (`vector/render_commands.rs`) sets the stencil reference, then binds slices of shared
   buffers: slot 0 vertices, slot 1 the per-tile instance data, slot 2 per-layer metadata, slot 3
   per-vertex feature colour. Then one `draw_indexed`.

**Pipeline state** (`render/resource/tile_pipeline.rs`): triangle list, no culling, stencil compare
`Equal` for layers and `Always` with replace for masks, depth writes off and depth compare `Always`.
Depth is not what orders ground layers; draw order is. Format `Depth24PlusStencil8`.

**Geometry** (`vector/tessellation.rs`, `vector/process_vector.rs`):

- `lyon` tessellation driven by `geozero` MVT processing, tolerance 0.02.
- `ShaderVertex` is `{ position, normal }` in tile units (extent 4096). Fills have normal 0.
- Tessellated once per tile and layer on a worker thread, never per frame.
- Per-vertex colour (`FillShaderFeatureMetadata`) is built at upload (`vector/upload_system.rs`).

**Lines** (`render/shaders/line.vertex.wgsl`, `line.fragment.wgsl`): widths are in screen pixels. The
normal is projected to clip space, then offset by `outset * 2 / viewport * w`. The fragment shader does
analytic edge antialiasing. Width is per layer in `ShaderLayerMetadata`, evaluated at upload.

**Tile positioning** (`coords.rs::transform_for_zoom`, `render/tile_view_pattern/`): each visible tile
gets a matrix `view_projection * translate(tile) * scale(tile_scale / EXTENT)`, calculated in `f64` and
downcast to `f32` per tile. It is uploaded each frame as instance data along with zoom factor and
viewport size. Keep this: per-tile `f32` matrices are what keep precision at z17.

**Camera** (`render/view_state.rs`, `render/camera.rs`): MapLibre-style. Field of view
0.6435011087932844 rad, camera height `(height / 2) / tan(fov / 2)`, near `height / 50`, far from the
frustum corner distance × 1.01, then `FLIP_Y * OPENGL_TO_WGPU_MATRIX * perspective * camera`.
`Camera::calc_matrix` applies pitch (x), yaw (y), roll (z). Bearing is **roll**. Visible tiles come from
`create_view_region` with `view_region_bounding_box`.

**Memory** (`vector/resource/buffer_pool.rs`): one large buffer each for vertices, indices, layer
metadata and feature metadata, used as a ring with a `RingIndex`. Evicts oldest. Port this so draws keep
binding slices of shared buffers.

## Required deviations

These are known defects or gaps. Each one is verified from the spike or the source.

| #   | Change                                                                        | Why                                                                                                                                                                                                           |
| --- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `fill.vertex.wgsl`: `z` must be 0, not `-z_index`                             | **Bug.** Upstream lifts fills by their layer number as world height, so buildings and fills visibly slide over roads under perspective. `final_position.z = z_index` can stay                                 |
| 2   | Cap fetched and drawn tile level at z14, render deeper by scaling             | Overzoom is a TODO upstream (`vector/request_system.rs`). OpenFreeMap returns empty tiles past z14. See `data_zoom_level` in the patch                                                                        |
| 3   | Size the buffer pools from config, default about 2 million entries            | Upstream's fixed 10 million entries total about 925 MB with symbols. It lost the GPU device in 3 of 8 runs on this 512 MB iGPU, and 0 of 8 at 2 million. Small sample. Symbol pools go away here, so re-check |
| 4   | Read our styles with `maplibre-expr` (`default-features = false`)             | Upstream evaluates only legacy filters and cannot parse our expression forms. Use `parse_filter` and `parse_property`, with `EvaluationContext::with_zoom` and `with_feature`. It is already in `Cargo.lock`  |
| 5   | Honour `minzoom` and `maxzoom`, and `f64` zoom values                         | Upstream parses them as `u8`, so real styles fail to parse on `15.5`, and never gates layers on them. Liberty's flat `building` layer (`maxzoom: 14`) showed at z17                                           |
| 6   | `line-width` as `interpolate` expressions                                     | `evaluate_at_zoom` only understands stop arrays. Going by the code, it returns 1.0 for 185 of the 198 line widths in our styles. Evaluate at the view zoom through `maplibre-expr`                            |
| 7   | Blending on the fill pipeline, same as lines (`SrcAlpha`, `OneMinusSrcAlpha`) | `shaders/mod.rs:195` has `blend: None` for fills, so `fill-opacity` and alpha colours cannot work. Our styles use `fill-opacity` 27 times. Measure the cost                                                   |
| 8   | MSAA as a parameter, measured at 1 and 4 samples                              | `surface.rs:397` hard-codes headless MSAA off, so **all baselines below are without MSAA** and fill edges are jagged. Line edges have the analytic AA. Report both and recommend one                          |
| 9   | `wgpu` 30, not 22                                                             | Current version, fits our Rust 1.95 (MSRV 1.87). `docs/scratch/maplibre-rs-spike/wgpu30-tiles-and-buildings-bench.rs` is working wgpu 30 code for pipelines, depth, MSAA and readback                         |
| 10  | No `unwrap` or panics on tile or style data                                   | Tile data is external. Log and skip a bad layer. Malformed widget config still fails loudly                                                                                                                   |

Also drop upstream's fallback tile logic only if it gets in the way. Keeping parent fallback is fine.

## 3D buildings

Nothing exists upstream (issue #192 is open), so this is new. Keep it in the same style: tessellate once
per tile on a worker, draw per frame with bound slices.

- **Layer:** `fill-extrusion` in the style. Liberty has `building-3d`: source layer `building`, `minzoom`
  14, filter `["!", ["has", "hide_3d"]]`, height `["get", "render_height"]`, base
  `["get", "render_min_height"]`, colour `hsl(35,8%,85%)`, opacity 0.8. Evaluate through `maplibre-expr`.
- **Geometry:** roof by fill tessellation of each polygon with holes, plus one quad per ring edge.
  Skip walls on edges lying along the tile clip boundary, or tile seams grow fake walls. MapLibre does
  this. Add per-vertex shade from the wall normal.
- **Pass order:** ground layers as above, then buildings. Ground writes no depth, so buildings only
  test against each other. Upstream clears depth to 0.0, and its projection is the ordinary 0..1 form
  and not reversed, so **do not copy the clear value**: clear appropriately for your compare function
  and verify. Depth writes on, stencil test off, culling is your choice.
- **Height units:** tile matrices scale x and y but leave z at 1, so convert metres to the current world
  scale yourself, accounting for Mercator latitude. In this camera, world up appears to be negative z
  (the fill bug lifted toward the camera with `-z_index`). Verify by looking at a frame.
- **Opacity:** draw opaque first. Liberty's 0.8 needs an offscreen composite to avoid seeing inner walls.
  Treat it as optional and report the cost if you do it.
- **Styles without extrusion layers** (`positron`, `bright`, `dark`, `fiord`) have flat buildings only.
  For testing buildings on those, use the `building` source layer with a fixed extrusion config in the
  gate. Do not edit the styles.

## Where it goes

- A new workspace crate, `src-tauri/ovrley_map_gpu`, added to `members` in `src-tauri/Cargo.toml`. It holds
  `tile.rs` (decode and tessellate), `pool.rs`, `pattern.rs` (tile matrices and view region), `camera.rs`,
  `pipelines.rs`, `extrusion.rs`, `style.rs`, `shaders/*.wgsl`, and `lib.rs` holding the renderer. It has no
  features for choosing the renderer: depending on the crate is the opt-in.
- It depends on `ovrley_map` with **no features**, for the shared vocabulary (`MapStyle`, `MapLayers`,
  `MapStyleSpec`, `mercator`), so it never pulls in ezu. Do not add `wgpu` or any GPU code to `ovrley_map`,
  which must stay WASM-safe, and do not make `ovrley_map` depend on this crate.
- `MapDataSource` is in `ovrley_map::raster` behind the ezu feature. Move it, and `MapSprite`, to featureless
  `ovrley_map` so both renderers share it, and update `raster.rs`, `map_tile_service.rs` and the re-export in
  `ovrley_core/src/lib.rs`.
- Do not add the crate as a dependency of `ovrley_core` or `ovrley_widgets`. Only the gate uses it for now, so
  ordinary app builds must not compile `wgpu`. See "Gate" for how.
- Keep the upstream MIT notice in the new crate, as `LICENSE-MIT` beside its manifest.
- Suggested surface, adjust as needed:

```rust
GpuMapRenderer::new(spec, size, sample_count) -> Result<Self, String>
renderer.load_tiles(&tiles, source)           // decode and tessellate on workers
renderer.render(&camera) -> Result<&[u8], String>  // RGBA, blocking readback
```

Use `cgmath` if that keeps ported maths close to the original. `lyon`, `geozero` (already in the lockfile
at 0.15 through ezu, upstream uses 0.13, so expect small API changes) and `bytemuck` are the other
dependencies. `protoc` is not needed: upstream only needed it for glyph protobufs.

## Gate

Add `src-tauri/src/bin/map_gpu_gate.rs`, modelled on `map_tile_gate.rs`, using `MapTileService` so it reads
the same cached tiles. In the `app` crate, make `ovrley_map_gpu` an optional dependency behind a `map-gpu-gate`
feature, and declare the binary with `required-features = ["map-gpu-gate"]`, so `pnpm dev` and normal builds
skip `wgpu`. Build it with `cargo build --release -p app --bin map_gpu_gate --features map-gpu-gate`.

Use the camera path from `render-tests/src/bin/live.rs` in the spike patch so numbers
compare directly: 30 km/h along the listed Zurich points at 30 fps, z17, pitch 60, bearing from a ±30 m
heading.

Report for `positron`, `liberty`, `bright` and `dark`, at 500 and 1000 px, three runs each of 1500 frames,
after tiles have loaded:

- Frame time: median, p95, max, with blocking readback included.
- Without buildings and with buildings. MSAA 1 and MSAA 4.
- Per-tile prepare time. The spike's 1 s per dense tile re-decoded the tile for every layer, so it likely
  overstates this.
- Stills and a 10-second clip to judge. Compare against the reference at the same camera with
  `maplibre-native-reference.mjs`.

**Baselines to stay close to.** Measured on this machine (Ryzen 9 7900X with its 2-core integrated Radeon,
Vulkan) with the spike: no MSAA, no buildings, labels off, patched pools.

| Style      | 500 px  | 1000 px      |
| ---------- | ------- | ------------ |
| `positron` | 0.75 ms | 2.0 ms       |
| `liberty`  | 0.95 ms | 2.45 ms      |
| `bright`   | 1.15 ms | 3.0 ms       |
| `dark`     | 0.75 ms | not measured |

Worst single frames were about 9 ms. Liberty on other backends at 500 px: Vulkan 0.84 ms, DX12 0.88 ms, GL
1.24 ms. Vulkan and DX12 frames were byte-identical.

**Reference to beat with buildings:** MapLibre Native on the same machine with real 3D buildings and no labels:
`liberty` 1.6 ms at 500 px, 3.9 ms at 1000 px. With road and place labels: 4.2 and 10–13 ms.

Pass criteria:

1. Without buildings at the same settings as the baselines, within about 20 % of the table. This is my reading of
   "minor regression", so report the number either way.
2. Liberty with buildings under the MapLibre Native figures above.
3. Foundations stay locked to the ground and roads while the camera moves. The user will judge this in the clip.
   Roads, water and buildings line up with the reference.
4. No device loss across at least 8 runs.

If a pass criterion fails, stop and report. Do not start integration work.

## Spike artefacts

In `docs/scratch/maplibre-rs-spike/`:

- `maplibre-rs-spike.patch`: all spike changes against `96b50a09`, including `live.rs`, the headless driver.
- `wgpu30-tiles-and-buildings-bench.rs`: wgpu 30 synthetic benchmark with extrusions, depth, MSAA and readback.
- `convert_style.py`: lowers OpenFreeMap styles to upstream's subset. Only for reproducing the baselines.
- `maplibre-native-reference.mjs`: reference frames. Needs `npm i @maplibre/maplibre-gl-native` beside it.
- `style_inventory.py`: which style properties the five styles use in z14–18.

## Environment traps

- **Windows path limit.** Cloning or building under the deep temp scratchpad silently truncated the checkout,
  emptied the Git index, and broke MSVC includes. Clone upstream to a short path and use a short
  `target-dir`, such as under `E:\`. `E:\mln-spike-tmp` is leftover spike output and can be deleted.
- **Check empty search results.** A truncated clone makes `git grep` return nothing, and a `head` limit hid the
  filter code once. Both produced false claims about upstream, so confirm a negative with `grep -r`.
- **Do not trust timings until you have looked at the frame.** A blank frame reads as very fast.
- **Timings are distorted by other load.** An unrelated ffmpeg encode raised a 4.2 ms Native frame to 10.6 ms.
- **Git Bash:** multi-line Python in heredocs has failed to parse here. Write scripts to files.

## Corrections to older docs and claims

- `docs/scratch/map-tile-widget-research.md` rejects GPU engines. Its
  CPU-tile recommendation is what the ezu branch implements. This work tests the alternative.
- `docs/scratch/map-tile-widget-plan.md` Phases 2–5 describe the Skia compositor path and do not apply here.
- Upstream's README says text is missing, but SDF text exists since February. Its `res/liberty.json` does not
  parse. The book documents only running demos and lists headless as "TBD".
- The `web` workspace member is not missing upstream. That was the truncated clone.

## Working rules

- Compile checks while iterating: `cargo check -p ovrley_map_gpu`, plus `cargo check -p ovrley_map` with no
  features to confirm it stays free of `wgpu` and ezu.
- Release builds of the gate binary are needed for timings, since debug numbers mean nothing.
- Never run `pnpm build` or WASM builds without approval. No new tests unless asked.
- Do not commit or push unless asked.
- Prefer small, reviewable changes. Keep ezu working throughout.

## Open after this work

If the gate passes, decisions remain on: labels (idea: ezu-baked label-only tiles draped on the ground, not yet
measured), dashes and caps, export integration through a shared GPU thread, preview delivery over loopback or
WebGL2, and running on Linux and macOS, which no spike has done.
