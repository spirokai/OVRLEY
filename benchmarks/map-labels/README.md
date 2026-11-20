# Map label benchmark

Measures what map labels cost in `ovrley_map`: per-frame rendering with labels off and on, in native export and the WASM preview, plus the font and glyph preparation that labels need.

```sh
pnpm benchmark:map-labels --build   # native binary + wasm-map-bindings (needs permission to build)
pnpm benchmark:map-labels --fetch   # downloads the plan's OpenFreeMap tiles to debug/benchmarks/map-labels/tiles
pnpm benchmark:map-labels --run     # both backends; --backend native|wasm, --frames N, --warmup N
node benchmarks/map-labels/summarize.mjs <run-id>
pnpm benchmark:map-labels --diagnose  # WebGL diagnosis, see below
pnpm benchmark:map-labels --verify    # quick WASM check: 4 cases, buildings on/off, frames saved for pixel comparison
```

`native.rs` (`ovrley_map` bin `benchmark_map_labels`) writes the plan that both backends replay and runs the export path. `browser.js` drives the production `wasm-map-bindings` in headless Playwright Chromium on WebGL2. `run.mjs` serves the page cross-origin isolated (5 µs timers) and launches Chromium with ANGLE on Vulkan, so WebGL runs on the hardware GPU; the summary prints the WebGL renderer string to confirm it. The WASM run needs `pnpm exec playwright install chromium` once.

## Cases

Three areas × two sizes × two cameras, each run with labels off then on, with a fresh renderer every time.

- **Areas**: dense urban is a 5.6 km loop through central Prague. Rural is an 8.9 km line through farmland and villages near Žďár nad Sázavou, drawn between waypoints that do not follow roads, which doesn't matter for rendering cost. CJK urban is a 7.6 km loop around Shinjuku, Tokyo, where the style shows names in Latin and Japanese.
- **Sizes**: square 400 × 400 and 1000 × 1000 px rectangular maps, pixel ratio 1, no corner radius.
- **Cameras**: top-down is the widget's fixed camera at zoom offset 0 (zoom 11.6 rural, 13.2 urban, 13.0 Tokyo), with one label placement for every frame. Chase is the widget default: zoom 16, pitch 50, rider-up, 15 m/s, with label placements every 0.3 s and the route drawn by the renderer.
- Style liberty with buildings and paths, MSAA 4: the map widget defaults.

The camera and label placement logic mirrors `ovrley_widgets::widgets::map::camera`. Each case runs 150 warm-up frames and then 900 measured frames at 30 FPS.

**Native** times `GpuMapRenderer::render`, which includes the blocking RGBA readback that export does. `load_view` per frame is timed separately and costs under 0.05 ms once tiles are resident. Preparation follows `ExportRenderer::new`: `MapTileSizer::measure` on every tile, run once cold (glyphs rasterized) and once warm. The benchmark runs it on one thread, while export spreads it over all cores. Every glyph the job draws is then made again on a fresh `GlyphService` and timed per face. After the frames, the benchmark counts any glyphs the service made while drawing.

**WASM** times the `draw`/`draw_with_route` call (CPU and command submission), and the same call plus a 1 px `readPixels` that waits for the GPU. Preparation is what a preview tile worker does: fetch the font files, build `TileDecoder`, and decode each tile. Here it runs on the page thread so it can be timed, and it decodes twice, cold then warm.

## Results

Run `1794050179875-107478`: Ryzen 5 5600X, RTX 4070 (native Vulkan; Chromium 148 ANGLE/Vulkan), Linux. A second run (`1794050007272-106167`) agrees on every mean to within about 0.15 ms. The full tables, including first-upload and renderer-creation times, are in each run's `summary.md`.

### Frame cost (ms, mean / p95)

| Case                    | Native off  | Native on   | Δ    | WASM draw off | WASM draw on | Δ    | WASM frame off | WASM frame on | Δ    |
| ----------------------- | ----------- | ----------- | ---- | ------------- | ------------ | ---- | -------------- | ------------- | ---- |
| rural-400-top-down      | 0.16 / 0.22 | 0.16 / 0.20 | −3%  | 0.14 / 0.19   | 0.14 / 0.18  | −1%  | 0.40 / 0.55    | 0.42 / 0.62   | +4%  |
| rural-400-chase         | 0.23 / 0.31 | 0.26 / 0.35 | +14% | 0.48 / 0.62   | 0.53 / 0.72  | +10% | 1.48 / 1.82    | 1.55 / 1.88   | +5%  |
| rural-1000-top-down     | 0.51 / 0.61 | 0.51 / 0.58 | −1%  | 0.37 / 0.47   | 0.38 / 0.47  | +2%  | 1.26 / 1.54    | 1.29 / 1.50   | +2%  |
| rural-1000-chase        | 0.65 / 0.75 | 0.89 / 1.28 | +35% | 1.14 / 1.51   | 1.54 / 2.59  | +36% | 3.21 / 3.79    | 3.69 / 4.78   | +15% |
| urban-400-top-down      | 0.26 / 0.32 | 0.27 / 0.34 | +4%  | 0.62 / 0.70   | 0.64 / 0.79  | +3%  | 2.34 / 2.90    | 2.39 / 2.94   | +2%  |
| urban-400-chase         | 0.33 / 0.40 | 0.51 / 0.86 | +57% | 0.81 / 1.06   | 1.10 / 1.97  | +37% | 3.94 / 4.59    | 4.33 / 5.28   | +10% |
| urban-1000-top-down     | 0.65 / 0.76 | 0.67 / 0.82 | +2%  | 0.90 / 0.97   | 0.93 / 1.01  | +3%  | 3.19 / 3.77    | 3.42 / 4.20   | +7%  |
| urban-1000-chase        | 0.91 / 1.05 | 1.65 / 3.03 | +82% | 1.73 / 1.91   | 2.97 / 6.46  | +72% | 8.06 / 9.04    | 9.52 / 13.14  | +18% |
| urban-cjk-400-top-down  | 0.31 / 0.42 | 0.32 / 0.40 | +0%  | 0.78 / 0.87   | 0.80 / 0.88  | +2%  | 3.81 / 4.42    | 3.83 / 4.46   | +1%  |
| urban-cjk-400-chase     | 0.38 / 0.47 | 0.47 / 0.68 | +22% | 0.95 / 1.34   | 1.11 / 1.58  | +17% | 6.80 / 7.44    | 6.97 / 7.63   | +2%  |
| urban-cjk-1000-top-down | 0.88 / 1.09 | 0.88 / 1.09 | +1%  | 1.48 / 1.63   | 1.50 / 1.66  | +2%  | 7.81 / 8.59    | 7.90 / 8.82   | +1%  |
| urban-cjk-1000-chase    | 1.15 / 1.37 | 1.54 / 2.50 | +34% | 1.94 / 2.22   | 2.81 / 4.88  | +45% | 18.60 / 20.13  | 19.57 / 21.85 | +5%  |

Chase with labels on: the frames that place labels (100 of 900) against the rest. The difference estimates what one placement costs.

| Case                 | Native placement / other | Placement cost | WASM draw placement / other | Placement cost |
| -------------------- | ------------------------ | -------------- | --------------------------- | -------------- |
| rural-400-chase      | 0.30 / 0.25              | 0.05           | 0.63 / 0.51                 | 0.12           |
| rural-1000-chase     | 1.29 / 0.83              | 0.46           | 2.63 / 1.40                 | 1.23           |
| urban-400-chase      | 0.84 / 0.47              | 0.37           | 2.01 / 0.99                 | 1.02           |
| urban-1000-chase     | 3.05 / 1.48              | 1.57           | 6.61 / 2.52                 | 4.09           |
| urban-cjk-400-chase  | 0.65 / 0.44              | 0.21           | 1.60 / 1.05                 | 0.55           |
| urban-cjk-1000-chase | 2.50 / 1.43              | 1.07           | 5.07 / 2.52                 | 2.55           |

### Glyphs and preparation (ms)

Native, labels on. "Made" counts every glyph the service rasterized during layout. It exceeds the drawn glyphs because layout also makes glyphs for labels it later drops. Label layout is the warm measure with labels on minus with labels off. Rasterize is the cold measure minus the warm one. Both are single-threaded.

| Case                    | Drawn glyphs: other / CJK | µs per glyph: other / CJK | Made | Label layout | Rasterize | Preload atlas | Made during frames | Atlas clears |
| ----------------------- | ------------------------- | ------------------------- | ---- | ------------ | --------- | ------------- | ------------------ | ------------ |
| rural-400-top-down      | 69 / 0                    | 17 / —                    | 75   | 0.7          | 1.4       | 0.15          | 0                  | 0            |
| rural-400-chase         | 105 / 0                   | 20 / —                    | 108  | 1.8          | 1.8       | 0.22          | 0                  | 0            |
| rural-1000-top-down     | 107 / 0                   | 17 / —                    | 125  | 5.5          | 2.1       | 0.23          | 0                  | 0            |
| rural-1000-chase        | 127 / 0                   | 17 / —                    | 129  | 1.1          | 2.6       | 0.26          | 0                  | 0            |
| urban-400-top-down      | 109 / 0                   | 17 / —                    | 123  | ≈0           | 3.4       | 0.23          | 0                  | 0            |
| urban-400-chase         | 135 / 0                   | 16 / —                    | 141  | 8.7          | 1.1       | 0.28          | 0                  | 0            |
| urban-1000-top-down     | 113 / 0                   | 17 / —                    | 128  | 4.0          | 0.9       | 0.25          | 0                  | 0            |
| urban-1000-chase        | 147 / 0                   | 16 / —                    | 153  | 12.6         | 3.7       | 0.30          | 0                  | 0            |
| urban-cjk-400-top-down  | 54 / 102                  | 15 / 95                   | 329  | 1.5          | 25.2      | 0.37          | 0                  | 0            |
| urban-cjk-400-chase     | 118 / 281                 | 16 / 92                   | 574  | 10.2         | 41.8      | 0.93          | 0                  | 0            |
| urban-cjk-1000-top-down | 89 / 170                  | 16 / 96                   | 488  | 11.1         | 36.2      | 0.61          | 0                  | 0            |
| urban-cjk-1000-chase    | 122 / 456                 | 16 / 94                   | 766  | 27.3         | 56.8      | 1.34          | 0                  | 0            |

Rasterize in the European cases (0.9–3.7 ms) is noisy, because it subtracts two passes of 13–180 ms. The per-glyph columns time the glyphs directly. `GlyphService::open` takes about 0.1 ms, and `GpuMapRenderer::new` about 110 ms with labels on or off.

WASM, per decoder. It fetches six font files totalling 5.9 MiB (4.1 MiB is the CJK pack), about 21 ms over localhost. Building the decoder takes 1.6–2.2 ms with fonts and 0.8–1.2 ms without, apart from 2.4 / 3.8 ms in the first case, which includes WASM warm-up.

| Case                    | Tiles | Decode cold off → on | Decode warm on | Rasterize |
| ----------------------- | ----- | -------------------- | -------------- | --------- |
| rural-1000-chase        | 13    | 38.2 → 43.0          | 39.9           | 3.1       |
| urban-1000-chase        | 13    | 228.6 → 254.0        | 245.4          | 8.6       |
| urban-cjk-400-top-down  | 4     | 166.4 → 200.5        | 170.4          | 30.1      |
| urban-cjk-400-chase     | 4     | 125.1 → 194.0        | 140.0          | 54.0      |
| urban-cjk-1000-top-down | 9     | 382.2 → 437.5        | 394.9          | 42.6      |
| urban-cjk-1000-chase    | 11    | 333.5 → 433.6        | 362.6          | 71.0      |

## Against the PRD budget (`.agents/scratch/map-labels/PRD.md` §7)

| Target                                        | Measured                                                                                                                                                                                                | Verdict                                         |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Labels add ≤ 1 ms per frame on average        | Native: at most +0.74 ms (Prague 1000 chase), +0.39 Tokyo 1000 chase, about 0 top-down. WASM draw: +1.24 ms Prague 1000 chase, +0.87 Tokyo                                                              | ✅ export; ⚠️ preview just over in dense Prague |
| Placement ≤ 3 ms per grid step                | Native at most 1.57 ms. WASM 4.09 ms in Prague 1000 chase, 2.55 ms in Tokyo, ≤ 1.23 ms elsewhere                                                                                                        | ✅ export; ⚠️ preview over in dense Prague      |
| About 20–30 µs per Latin glyph                | 15–20 µs                                                                                                                                                                                                | ✅                                              |
| About 0.1 ms per CJK glyph at 2×              | 92–96 µs                                                                                                                                                                                                | ✅                                              |
| About 15 ms of glyph work for a European ride | Rasterizing 75–153 glyphs at 16–20 µs: 1–3 ms per job, single-threaded                                                                                                                                  | ✅                                              |
| Well under a second for a CJK city ride       | Tokyo: 25–57 ms of rasterization and 1.5–27 ms of layout per job, single-threaded                                                                                                                       | ✅                                              |
| Export frames make zero glyph requests        | 0 glyphs made during frames in every case, and no atlas clears                                                                                                                                          | ✅                                              |
| Preview glyph trip of 5–20 ms per tile        | No glyph trip exists: workers rasterize from the full font files. In-worker rasterization is 30–71 ms per Tokyo job over 4–11 tiles, about 5–13 ms per tile. Each worker first fetches 5.9 MiB of fonts | n/a (design changed)                            |
| Atlas ≤ 16 MB, label buffer < 1 MB            | Not measured; the atlas size is capped by its texture                                                                                                                                                   | —                                               |

These numbers are at pixel ratio 1. A default widget in a 4K export is 1000 px at ratio 2, which shows a quarter of the area with text twice as large.

## Findings

- **Top-down: labels are effectively free.** A fixed camera has a single placement, so frames change by 0–4% (noise) on both backends, in every area and size.
- **Chase: labels cost 14–82% natively and 10–72% in the WASM draw call.** The cost grows with label density and map size. Prague's old town costs more than Shinjuku (+0.74 vs +0.39 ms natively), even though Tokyo draws more glyphs. The benchmark doesn't show why.
- **The chase cost comes in spikes.** The frame that places labels every 0.3 s costs about twice the others. In the WASM preview that is 4.1 ms per placement in dense Prague, over the 3 ms budget. Placement is the place to optimize if the preview matters.
- **WASM GPU-synced frame time is the map, not labels.** Labels add 1–18%. Tokyo 1000 px chase already takes 18.6 ms per synced WebGL frame without labels, against 1.15 ms natively including readback.
- **Glyph work is small, even for CJK.** CJK glyphs take about 6× as long as Latin ones (95 vs 16 µs), so Tokyo jobs spend 25–57 ms rasterizing natively and 30–71 ms in WASM. That is still small beside tile tessellation. Export never makes a glyph while drawing frames.
- **The preview's real font cost is transfer and memory.** Each tile worker (two per renderer) fetches and holds 5.9 MiB of fonts. The PRD's per-tile glyph trip was replaced by that up-front cost, and §7 doesn't budget for it.

## Why WebGL frames are slow (diagnosis)

`--diagnose` runs the production map module with variants and a census of every WebGL call, and captures a Chrome trace (`debug/benchmarks/map-labels/diagnose-*/`). It also runs a raw WebGL2 microbenchmark of the same shape of work. Environment switches:

- `BENCH_WASM_DIR` serves another build of the map module.
- `BENCH_RAW_ONLY` stops after the microbenchmark.
- `BENCH_SNAPSHOT` saves the last frame for pixel comparison.
- `BENCH_ANGLE=gl` and `BENCH_HEADED` change how Chromium is launched.

Findings, Prague 1000 px chase with labels off: the WASM frame is 8.3 ms against 0.9 ms natively.

1. **The time is the GPU executing, not WebGL overhead.** In the trace, 6.7 of the GPU process's 8.6 ms per frame is `ContextVk::finishImpl`, waiting for the GPU. Command decoding takes about 1.9 ms, and a readback with nothing queued takes 0.15 ms.
2. **It scales with indices drawn, not pixels.** MSAA 1 vs 4 changes nothing, and top-down Tokyo (roofs only) is as slow. Without buildings the frame drops to 2.3 ms in Prague (16.1 → 2.4 ms in Tokyo), because buildings hold most of the indices: 2.3 M of them per frame in Prague, 5.0 M in Tokyo. Every geometry costs about 2–3 ns per index on this path, against under 0.3 ns natively.
3. **Chrome/ANGLE on its own is fast.** Raw WebGL2 draws 5 M Uint32 indices with the map's vertex layout in 0.68 ms, as long as the buffers are created `STATIC_DRAW`. Created the way wgpu creates them, `bufferData(size, DYNAMIC_DRAW)` then `bufferSubData`, the same draws take 9.8 ms. ANGLE's Vulkan backend places dynamic buffers in host-visible memory, so the GPU reads vertices and indices over PCIe.
4. **wgpu's GL backend creates every non-mappable buffer `DYNAMIC_DRAW`** (wgpu-hal 30.0.1, `src/gles/device.rs`, citing gfx-rs/wgpu#3371). A build of the map module against a wgpu-hal patched to `STATIC_DRAW` draws pixel-identical frames, and the frame including the GPU wait drops from 8.3 to 2.8 ms (Prague 1000 px chase), 17.8 to 3.5 ms (Tokyo chase) and 7.8 to 2.9 ms (Tokyo top-down).

The 2.8 ms that remains is mostly CPU. The draw call takes about 1.9 ms: WASM plus about 8,000 WebGL calls per frame, because wgpu's GL backend re-points about 14 vertex attributes for each of about 550 draws. The GPU process then spends about 1.9 ms decoding those calls. Raw WebGL issuing the same 547 draws with the same re-pointing takes 1.3 ms in total.

**Fix (no fork).** `GpuMapRenderer::for_canvas` (`ovrley_map/src/gpu/web.rs`, `hint_static_buffers`) overrides `bufferData` on its own canvas's WebGL2 context, after the surface creates the context and before the device creates any buffer, so `DYNAMIC_DRAW` becomes `STATIC_DRAW`. `--verify`, labels on, 300 frames, comparing git HEAD built into a scratch directory (`BENCH_WASM_DIR`) with the fixed build:

| Case                    | Frame before | Frame after | GPU wait before → after |
| ----------------------- | ------------ | ----------- | ----------------------- |
| urban-1000-chase        | 9.94 ms      | 4.13 ms     | 6.76 → 1.04 ms          |
| urban-cjk-1000-chase    | 18.95 ms     | 4.22 ms     | 16.20 → 1.60 ms         |
| urban-cjk-1000-top-down | 8.14 ms      | 2.63 ms     | 6.54 → 1.11 ms          |
| rural-400-top-down      | 0.41 ms      | 0.40 ms     | 0.27 → 0.26 ms          |

The last frame of each case is byte-identical before and after. Buildings now add at most 0.6 ms to the frame. This was measured only in headless Chromium on ANGLE/Vulkan with a discrete GPU. The app's own webviews (WebKitGTK, WebView2 on Direct3D 11, WKWebView on Metal) were not measured.
