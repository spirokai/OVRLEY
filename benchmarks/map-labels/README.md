# Map label benchmark

Measures what map labels cost in `ovrley_map`: per-frame rendering with labels off and on, as export renders, plus the font and glyph preparation that labels need.

```sh
pnpm benchmark:map-labels --build   # the native binary (needs permission to build)
pnpm benchmark:map-labels --fetch   # downloads the plan's OpenFreeMap tiles to debug/benchmarks/map-labels/tiles
pnpm benchmark:map-labels --run     # --frames N, --warmup N
node benchmarks/map-labels/summarize.mjs <run-id>
```

`native.rs` (`ovrley_map` bin `benchmark_map_labels`) writes the plan and replays it through the export path.

## Cases

Three areas × two sizes × two cameras, each run with labels off then on, with a fresh renderer every time.

- **Areas**: dense urban is a 5.6 km loop through central Prague. Rural is an 8.9 km line through farmland and villages near Žďár nad Sázavou, drawn between waypoints that do not follow roads, which doesn't matter for rendering cost. CJK urban is a 7.6 km loop around Shinjuku, Tokyo, where the style shows names in Latin and Japanese.
- **Sizes**: square 400 × 400 and 1000 × 1000 px rectangular maps, pixel ratio 1, no corner radius.
- **Cameras**: top-down is a north-up camera that fits the route's longer side into 500 units less an 8% margin on each side (zoom 11.6 rural, 13.2 urban, 13.0 Tokyo), with one label placement for every frame. Chase is a follow camera at zoom 16 (the widget's default is 17.5), with the widget's default pitch 50 and rider-up orientation, at 15 m/s, with label placements every 0.3 s and the route drawn by the renderer.
- Style liberty with buildings and paths, MSAA 4, as the map widget's defaults.

The follow camera's padding and the label placement mirror `ovrley_widgets::widgets::map::content`. Each case runs 120 warm-up frames and then 600 measured frames at 30 FPS by default (`--warmup`, `--frames`). The recorded run below used 150 and 900.

**Native** times `GpuMapRenderer::render`, which includes the blocking RGBA readback that export does. `load_view` per frame is timed separately and costs under 0.05 ms once tiles are resident. Preparation follows `ExportRenderer::new`: `MapTileSizer::measure` on every tile, run once cold (glyphs rasterized) and once warm. The benchmark runs it on one thread, while export spreads it over all cores. Every glyph the job draws is then made again on a fresh `GlyphService` and timed per face. After the frames, the benchmark counts any glyphs the service made while drawing.

## Results

Run `1794050179875-107478`: Ryzen 5 5600X, RTX 4070 (Vulkan), Linux. A second run (`1794050007272-106167`) agrees on every mean to within about 0.15 ms. The full tables, including first-upload and renderer-creation times, are in each run's `summary.md`.

### Frame cost (ms, mean / p95)

| Case                    | Native off  | Native on   | Δ    |
| ----------------------- | ----------- | ----------- | ---- |
| rural-400-top-down      | 0.16 / 0.22 | 0.16 / 0.20 | −3%  |
| rural-400-chase         | 0.23 / 0.31 | 0.26 / 0.35 | +14% |
| rural-1000-top-down     | 0.51 / 0.61 | 0.51 / 0.58 | −1%  |
| rural-1000-chase        | 0.65 / 0.75 | 0.89 / 1.28 | +35% |
| urban-400-top-down      | 0.26 / 0.32 | 0.27 / 0.34 | +4%  |
| urban-400-chase         | 0.33 / 0.40 | 0.51 / 0.86 | +57% |
| urban-1000-top-down     | 0.65 / 0.76 | 0.67 / 0.82 | +2%  |
| urban-1000-chase        | 0.91 / 1.05 | 1.65 / 3.03 | +82% |
| urban-cjk-400-top-down  | 0.31 / 0.42 | 0.32 / 0.40 | +0%  |
| urban-cjk-400-chase     | 0.38 / 0.47 | 0.47 / 0.68 | +22% |
| urban-cjk-1000-top-down | 0.88 / 1.09 | 0.88 / 1.09 | +1%  |
| urban-cjk-1000-chase    | 1.15 / 1.37 | 1.54 / 2.50 | +34% |

Chase with labels on: the frames that place labels (100 of 900) against the rest. The difference estimates what one placement costs.

| Case                 | Native placement / other | Placement cost |
| -------------------- | ------------------------ | -------------- |
| rural-400-chase      | 0.30 / 0.25              | 0.05           |
| rural-1000-chase     | 1.29 / 0.83              | 0.46           |
| urban-400-chase      | 0.84 / 0.47              | 0.37           |
| urban-1000-chase     | 3.05 / 1.48              | 1.57           |
| urban-cjk-400-chase  | 0.65 / 0.44              | 0.21           |
| urban-cjk-1000-chase | 2.50 / 1.43              | 1.07           |

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

## Against the budget of the map labels' plan

| Target                                        | Measured                                                                         | Verdict |
| --------------------------------------------- | -------------------------------------------------------------------------------- | ------- |
| Labels add ≤ 1 ms per frame on average        | At most +0.74 ms (Prague 1000 chase), +0.39 Tokyo 1000 chase, about 0 top-down   | ✅      |
| Placement ≤ 3 ms per grid step                | At most 1.57 ms                                                                  | ✅      |
| About 20–30 µs per Latin glyph                | 15–20 µs                                                                         | ✅      |
| About 0.1 ms per CJK glyph at 2×              | 92–96 µs                                                                         | ✅      |
| About 15 ms of glyph work for a European ride | Rasterizing 75–153 glyphs at 16–20 µs: 1–3 ms per job, single-threaded           | ✅      |
| Well under a second for a CJK city ride       | Tokyo: 25–57 ms of rasterization and 1.5–27 ms of layout per job, single-threaded | ✅     |
| Export frames make zero glyph requests        | 0 glyphs made during frames in every case, and no atlas clears                   | ✅      |
| Atlas ≤ 16 MB, label buffer < 1 MB            | Not measured; the atlas size is capped by its texture                            | —       |

These numbers are at pixel ratio 1. A default widget in a 4K export is 1000 px at ratio 2, which shows a quarter of the area with text twice as large.

## Findings

- **Top-down: labels are effectively free.** A fixed camera has a single placement, so frames change by 0–4% (noise) in every area and size.
- **Chase: labels cost 14–82%.** The cost grows with label density and map size. Prague's old town costs more than Shinjuku (+0.74 vs +0.39 ms), even though Tokyo draws more glyphs. The benchmark doesn't show why.
- **The chase cost comes in spikes.** The frame that places labels every 0.3 s costs about twice the others.
- **Glyph work is small, even for CJK.** CJK glyphs take about 6× as long as Latin ones (95 vs 16 µs), so Tokyo jobs spend 25–57 ms rasterizing. That is still small beside tile tessellation. Export never makes a glyph while drawing frames.
