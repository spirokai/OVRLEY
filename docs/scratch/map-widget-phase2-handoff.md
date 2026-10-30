# Handoff: connect the GPU map renderer and execute Phase 2

Status: not started. Date: 2026-10-30. Branch: `rust-map-tiles`.

Read `AGENTS.md` first. Its code rules apply to everything here. Then read Phase 2 of
`docs/scratch/map-tile-widget-plan.md`, which has been revised for the GPU renderer. This document adds the execution
detail, and wins if the two ever disagree.

## Goal

Build the `map` widget family in `ovrley_widgets` and connect it to the GPU renderer in
`src-tauri/ovrley_map_gpu`, so one widget draws a real map, route, marker and attribution to a Skia canvas.

Deliverable: a native gate that renders the real widget to stills and a clip, and a WASM benchmark case
that measures the widget's own per-frame cost.

Not in this work: Phase 3 (export), Phase 4 (preview delivery), Phase 5 (editor, manifest entry,
translations). The widget does not appear in the app when this is done.

## Where things stand

- The renderer works and passed its gate on the Zurich path at z17, pitch 60, 500 and 1000 px. Liberty with
  buildings takes 0.84 ms at 500 px and 2.29 ms at 1000 px without MSAA, and 1.45 ms and 4.77 ms with MSAA 4.
- **All of it is uncommitted**, including the renderer, the gate and the `ovrley_map` changes. Do not commit or
  push unless asked. Check `git status` before starting.
- `ovrley_map` has `MapStyle`, `MapLayers`, `MapStyleSpec`, `mercator`, `MapDataSource` and `VECTOR_SOURCE`,
  all featureless. `ovrley_widgets` does not depend on `ovrley_map` yet.
- `src-tauri/src/bin/map_gpu_gate.rs` is the renderer gate, behind the `map-gpu-gate` feature.
- Reference code: upstream maplibre-rs at `96b50a09`, cloned to `E:\mlrs\src`. If that is gone, clone it again
  to a short path. Deep paths silently truncate the checkout on this machine.

## What changed in Phase 2

The plan was first written for ezu raster tiles composited in Skia. The GPU renderer replaces that path for this
widget: the map arrives as one finished frame per camera, so the widget has no tiles. The plan now reflects this table,
which is kept here so you know what the older docs and the ezu code mean.

| Plan item                                             | Status                                                                   |
| ----------------------------------------------------- | ------------------------------------------------------------------------ |
| Config and validation, in `plots` with `value: "map"` | Keep, except the `labels` toggle (see Decisions)                         |
| `track.rs`, `camera.rs`, bearing smoothing            | Keep as written                                                          |
| `view_zoom` formula, capped at 18                     | Keep                                                                     |
| Route, marker, shared line and marker validation      | Keep, projecting with the renderer's own matrix                          |
| Attribution                                           | Keep, and move the `widgetFonts` change here from Phase 4                |
| Dispatch arms                                         | Keep, with `FrameKey::Map { camera, progress }` and no `tile_generation` |
| No activity or no GPS                                 | Keep                                                                     |
| `cover.rs`                                            | **Drop.** The renderer owns tile selection                               |
| `compositor.rs`                                       | **Drop.** The widget draws one image                                     |
| `z = floor(view_zoom)`, `pixel_ratio` clamped to 1–4  | **Drop.** Those are ezu limits. See Gap 2                                |
| `MapTileProvider`, tile keys, ancestor search         | **Replace** with `MapFrameProvider`                                      |
| Fixed mode composing tiles into one image             | **Replace** by caching the last frame per request                        |
| Gate: 20 synthetic tiles, shimmer and seams           | **Replace** (see Gate). Tile textures no longer exist                    |

Everything else in Phase 2 applies. `ezu` stays in the tree and is not used by this widget. Do not remove it.

## Decisions already made

Build on these. Report a problem with one rather than quietly changing it.

1. **Labels:** the GPU renderer draws none, so the Phase 2 config has no `labels` field. A toggle that does
   nothing would break the code rules. Build `MapLayers` with `labels: false`. Add the field when labels exist.
2. **Route over buildings:** the route and marker are drawn over the finished frame, so a route passing behind
   a tall building shows on top of it. This matches the plan's screen-space stroking.
3. **Buildings:** the `buildings` toggle maps to `Buildings::FromStyle` for Liberty and a fixed extrusion for the
   others. Move that table out of the gate, into `ovrley_map_gpu` as the owner, for example
   `Buildings::for_style`. Fiord needs a colour. It is in no gate so far, and the gate's table gives it a colour
   meant for light styles.
4. **MSAA:** the host chooses it, not widget config. Default to 4 in the gate.
5. **Pending frames:** `Pending` draws the preset's flat placeholder colour, then the route and attribution. Do
   not draw a stale frame: its camera differs, so the route would sit off the road.

## The connection

**One camera, one matrix.** This is the core of the work. The widget projects the route on the CPU and the
renderer draws the roads on the GPU. They must use the same matrix, or the route drifts off the road.

`ovrley_widgets` compiles to WASM and must never depend on `ovrley_map_gpu`, because that pulls in `wgpu`. So move
the camera to the shared crate:

- Move `ovrley_map_gpu/src/camera.rs` (`MapCamera`, `ViewState`, `view_projection`, the view region) to featureless
  `ovrley_map`, adding `cgmath`. `ViewPadding` and the region code can follow it, or stay behind through a small
  `TileId` type, whichever keeps one owner. `ovrley_map_gpu` imports them. No second copy, no re-export alias.
- Add `ViewState::project_ground(lon, lat)` or similar, returning clip coordinates in `f64`, for the widget.
- `ovrley_map` must stay free of `wgpu`, Skia and ezu. Check with `cargo check -p ovrley_map`.
- Adding `ovrley_map` and `cgmath` to `ovrley_widgets` changes `src-tauri/wasm-bindings/Cargo.lock`. `cgmath` is
  pure Rust, but it has never been built for `wasm32-unknown-emscripten` here. The WASM build needs approval, so
  this is first tested at the gate.

**The provider.** Define this in `ovrley_widgets::widgets::map`, because it returns a Skia image:

```rust
pub struct MapFrameRequest {
    pub spec: MapStyleSpec,
    pub camera: MapCamera,
    pub width_px: u32,
    pub height_px: u32,
    pub pixel_ratio: f32,
}

pub enum MapFrame {
    Ready(skia_safe::Image),
    Pending,
}

pub trait MapFrameProvider: Send + Sync {
    /// Never blocks on the network. Export's provider never returns `Pending`.
    fn frame(&self, request: &MapFrameRequest) -> MapFrame;
}
```

`spec` is built from the validated config. Adjust names as needed, but keep it non-blocking with `Ready`/`Pending`:
preview delivery is undecided, and both candidates need it.

**Getting the provider to the widget.** `ValidatedWidget::Raster` already carries what its host resolved
(`preview_widget.rs:58`), so add `ValidatedWidget::Map { widget, frames }` with an `Arc<dyn MapFrameProvider>`. For
the preview path, give `PreviewSession` a provider that always returns `Pending` for now. `ovrley_core` only
constructs `PreviewWidget` through `from_validated` (`engine.rs:157`), so it needs no change in this phase.

**The native side.** A provider wrapping `GpuMapRenderer` lives in the gate for now. Do not add `ovrley_map_gpu` to
`ovrley_core` or `ovrley_widgets`, so ordinary app builds still skip `wgpu`. Check this with
`cargo tree -p app -e normal -i wgpu`, which must find nothing without the feature.

## Renderer gaps to close first

The renderer's gate never needed these, and the widget does. Close them before building on the camera.

**Gap 1: top padding.** The plan's follow camera uses 45 % top padding, as `NavigationMapController.js:96` does. I
dropped upstream's edge insets as unused. Restore them from `render/view_state.rs:123` and `render/camera.rs:256`
in upstream, as `top_padding_ratio` on `MapCamera`. This code has never run here, so verify the sign: the camera's
centre must project to `y = 0.725 * height` at a ratio of 0.45, and to `0.5 * height` at 0.

**Gap 2: pixel ratio.** The renderer treats one world unit as one output pixel. Rendering at 2× today shows twice
the area with thin lines, instead of the same map sharper. The plan requires the sharper result. Add `pixel_ratio`
to the config, where `pixel_ratio = scene.scale × preview_raster_scale`:

- Output size is the scene size times `pixel_ratio`. World size and tile scale multiply by it.
- `line_width` multiplies by it when evaluated. The line shader's own `pixel_ratio = 1.0` stays, since it then
  works in device pixels.
- Tile level and style values still use the view zoom. A 2× render must pick the same tiles and the same widths in
  scene units as a 1× render.

Verify by rendering the same camera at 1× and 2×, then downscaling the 2× frame. The two should match closely.

**Gap 3: coverage.** Everything so far is one dense area at z17, pitch 60, on square viewports. Before relying on
the widget, render and look at: pitch 0, zooms 10–18, wide and tall viewports, a rural area and all five styles.

Fixed mode on a long route is the main risk. At low zoom a view can cover many tiles, and the gate fails if the
2 million entry pools evict during a load. If that happens, report the numbers before resizing the pools.
`view_tiles` takes the camera and size, and `load_tiles` and `render` are unchanged, so these checks need no widget.

## Implementation notes

Family layout follows `.agents/skills/add-widget/SKILL.md` step 4. The skill's test steps are overridden: **no new
tests unless asked**. Its step 2 and step 5 belong to Phase 5.

- **Dispatch:** `ValidatedWidget` (`preview_widget.rs:58`), `FrameKey` (`:76`), `WidgetRequest` (`:96`), type routing
  (`:170`), `PreviewWidget` (`:259`) and each of its matches, `preview_instance.rs` (`:22`, `:66`, `:89`, `:234`),
  `preview.rs` resize routing (`:271`), invalidation (`:423`) and the render loop (`:497`). `MapCamera` must be
  `PartialEq` for `FrameKey`.
- **Track:** add an accessor on `RouteActivity` (`route/sampling.rs:52`). It keeps latitude, longitude and progress,
  but `RouteSample.point` is equirectangular, so do not reuse it.
- **Camera:** port `getCourseNavigationCamera` (`mapPreviewGeometry.js:116`), with lookahead 15 m and top padding
  0.45. Its bearing is clockwise from north, the same as `MapCamera`, and the renderer's bearing is checked against
  Native.
- **Route:** project visible vertices with `view_projection`, clip at the near plane before dividing by `w`, and
  stroke in screen space. Output pixels map to scene coordinates through the widget's bounds and `scene.scale`.
  `rdp.rs` has `simplify_rdp_indices_f64`. `route/layout.rs` shows how line widths and scale are applied.
- **Marker:** `PlotMarker::new` and `draw` (`plot_marker.rs:48`, `:92`).
- **Frame reuse:** cache the last `Image` by `MapFrameRequest` inside `PreviewMap`, so a fixed camera asks the provider
  once. Skip the render when `FrameKey` is unchanged, as `PreviewRoute::render` does.
- **Placeholder colour:** per preset, from each style's `background-color`. Read Fiord's from its style.
- **Attribution:** plain text from `map_tile_service.rs:1210`, through `FontResolver`. A font the preview did not
  request fails it, so `widgetFonts` in `previewSession.js` must return this font now. The plan had this in Phase 4.
- **Resize:** size-based. During a `transient` drag, rescale the last frame and ask for nothing. The commit
  recomputes zoom and asks for a new frame.
- **Style zoom:** `GpuMapConfig.style_zoom` is fixed per renderer, which matches the plan's constant zoom per widget.
  A zoom change means a new renderer, so key the gate's provider on spec, zoom, size and ratio.

## Gate

Report each of these. If one fails, stop and report it. Do not start Phase 3.

**A. Native (no approval needed for the gate binary).** Add a `widget` mode to `map_gpu_gate`, or a sibling binary
with the same feature. It builds the real `PreviewMap` through `from_validated`, with the provider wrapping
`GpuMapRenderer`, and draws to a raster Skia surface.

- Stills and a 10-second clip for all five styles at 500 and 1000 px, in `follow` with both orientations at pitch
  0, 40 and 60, and in `fixed`.
- Use an actual activity if one is available. Otherwise use the Zurich path from the existing gate.
- **The test that matters:** the route stays on its road while the camera moves. The user judges this in the clip.
  Also look at stills at far distance, where a matrix mismatch shows most.
- Report the widget's own cost separately from `frame()`, including RGBA to `Image`.

**B. WASM (needs approval for `pnpm benchmark:widgets:build` and the run).** Add a `map` case to
`benchmarks/widgets/browser.js`, at pitch 0, 40 and 60, at 500 and 1000 px. Target: under about 8 ms/frame at 500 px.

WASM has no GPU renderer, so this measures only the image blit, route, marker and attribution. Feed it a pre-rendered
frame through a minimal upload ABI such as `ovrley_preview_map_frame(session, key, pixels)`, not a test-only branch
in production code. That upload is what loopback delivery would use later. Add the case to `benchmarks/widgets/native.rs`
and `preview.rs` as well if the run requires it.

**C. Isolation.**

- `cargo check -p ovrley_map`, `-p ovrley_widgets`, `-p ovrley_map_gpu`, and `-p app --bins --lib`
- `cargo tree -p app -e normal -i wgpu` finds nothing without the feature
- `cargo tree -p ovrley_map_gpu -i ezu` finds nothing
- The WASM build succeeds, which is covered by the benchmark build

Also re-run the renderer gate's `timings` mode once after closing the gaps, to confirm no regression. A single run
of 300 frames takes about a minute: `map_gpu_gate <cache> <out> timings 1 300`. Give the user a time estimate before
any long run.

## Environment

- Use a short target directory: `CARGO_TARGET_DIR=E:/mlrs/t`. Build the gate with
  `cargo build --release -p app --bin map_gpu_gate --features map-gpu-gate` from `src-tauri/`.
- **Stale style cache:** the app's cached `liberty.json` lacks the `hide_3d` filter, and the cache never revalidates.
  `E:\mlrs\cache` is a scratch copy with live styles. Pass `E:/mlrs/cache` as `<cache_dir>` and leave the app's cache
  alone. The revalidation fix belongs to Phase 3 or 4; report it, do not fix it here.
- **Native reference:** `E:\mlrs\ref` has `maplibre-native-reference.mjs` with its dependency installed. The argument
  order is in the script, and `reference_cameras.txt` from `stills` mode has one line per camera. Native has no
  padding argument, so compare cameras with top padding 0.
- **`Cargo.lock`:** `gpu-allocator` is hand-pointed at `windows 0.62.2`, because `wgpu-hal` will not compile with the
  0.61 that Tauri locks. A re-resolve that reverts it shows up as `windows_core` errors in `wgpu-hal`.
- **Git Bash:** multi-line heredocs have failed here. Write scripts to files.
- **Timings:** look at the frame before trusting a number. Spikes up to about 60 ms appeared in the matrix, mostly at
  1000 px, with no confirmed cause. Other PC use is the likely source. Report medians and do not chase these.

## Working rules

- Compile checks while iterating. Release builds only for timings.
- Never run `pnpm build`. WASM builds and benchmark builds need approval, at the gate above.
- No new tests unless asked. Do not commit or push unless asked.
- Prefer small, reviewable changes. Keep ezu and `map_tile_gate` working throughout.

## Known visual gaps, not for this phase

- Lines stay a constant pixel width with distance, as upstream does, so far roads look heavier than Native. The
  fix is in the line vertex shader, and it needs a user decision because the governing rule of the renderer work
  was to match upstream.
- Liberty's 0.8 building opacity is not applied, so buildings look heavier than in Native.
- Dashes, patterns, `fill-translate`, `line-gap-width` and `line-blur` are not drawn.
- `z_index` is written as clip z, so layers could clip near the bottom of the frame on very small viewports at steep
  pitch, around 75° at 200 px. That comes from the maths and has not been tested. Pitch stays at 60 or below here.

## Open after this work

Labels (needed before release), export through a shared GPU thread, preview delivery over loopback or WebGL2, the
stale style cache, and running on Linux and macOS, which no gate has done.
