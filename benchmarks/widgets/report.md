# G-force native / WASM rendering analysis

This report records the original investigation. See [the rendering follow-up](rendering-follow-up.md) for the rebuilt readback results, comparable native-preview timing, lap caching, shared mask shadows, and the remaining heading-tape hotspot.

The main discrepancy is the WASM preview's full-bitmap pixel readback and alpha conversion. Sampling attributes approximately 76% of its time to that operation. Static circle caching works. Changing circle, border, or guide drawing primitives is not the first optimization to make.

The profiling used the existing release executable and WASM artifact before the shared-readback adoption documented below. All reported performance measurements describe that baseline. The original benchmark files are preserved.

## What the original numbers include

The [original summary](../../debug/benchmarks/widgets/1792162620989-28280/summary.md) reports native `frame.draw` at 1.2920 ms/frame and WASM `wasm.render` at 7.8291 ms/frame.

Native `frame.draw` includes g-force frame preparation and painting. Native draws directly into a caller-owned RGBA buffer with straight alpha; there is no subsequent full-image readback. Its separate full-scene restoration takes another 2.0915 ms/frame. Total native frame time was 3.3871 ms, including that restoration. Its destination is the 3840 x 2160 scene.

WASM `wasm.render` includes sampling, frame preparation, clearing, painting, pixel readback/conversion, and geometry serialization. Its destination is a widget-sized bitmap, normally 671 x 500 here. The benchmark's separate `pixels.copy` stage (0.1015 ms/frame) only copies the already converted WASM bytes into JavaScript's ImageData. It does not measure Skia's internal readback/conversion.

The scene uses scale 2, a 232-unit circle (464 output pixels), 35-unit label font (70 output pixels), shadow strength/distance 2 (4 output pixels), 50% circle fill, and zero circle border thickness. With zero circle border, the circle's border-shadow branch is inactive; text shadows remain active.

Relevant code: [native buffer wrapping](../../src-tauri/ovrley_core/src/render/surface.rs), [native frame rendering](../../src-tauri/ovrley_core/src/render/frame.rs), [WASM g-force preview](../../src-tauri/ovrley_widgets/src/widgets/g_force/preview.rs).

## Profile of the existing WASM artifact

The [probe](../../debug/g-force-analysis/probe.mjs) calls the production ABI directly in headless Chromium, using the original activity, widget request, and serialized font resources. Each variant has 200 warmup frames and 600 measured frames at 30 fps, covering activity seconds 406.667 through 426.633. Baseline and no-shadow variants were also sampled using the Chrome DevTools CPU profiler at a requested 100 microsecond interval.

Baseline measurements reproduced the original timing: 8.0815 ms/frame during profiling, and 7.8848 ms/frame in a later unprofiled repeat. Native drawing reproduced at 1.3114 ms/frame.

| Baseline WASM work                       | Approximate sampled ms/frame | Share of profile |
| ---------------------------------------- | ---------------------------: | ---------------: |
| Pixel readback and alpha conversion      |                         6.16 |            75.7% |
| Text painting, including shadows         |                         1.16 |            14.3% |
| Drawing the cached circle image          |                         0.64 |             7.9% |
| Remaining work and profiling/JS overhead |                         0.17 |             2.1% |

These are sampling estimates, not individually instrumented stage timers. The total profile duration includes a small amount of surrounding JavaScript and profiling overhead.

The release WASM has stripped function names. Attribution was checked against disassembly of the exact artifact and the checked-out Skia source. There are 23 imported functions, so Chrome function index N corresponds to numeric function N - 23 in wasm-dis output. Useful mappings:

| Chrome function       | Attribution                                                     |
| --------------------- | --------------------------------------------------------------- |
| 6238                  | Preview render ABI, containing the inlined Rust widget dispatch |
| 562                   | Surface pixel-readback subtree                                  |
| 6377, 1916, 1780, 705 | Device/bitmap/pixmap readback through pixel conversion          |
| 5405                  | Scalar raster-pipeline execution                                |
| 5890                  | Load 8-bit RGBA channels into floats                            |
| 5814                  | Unpremultiply RGB by alpha                                      |
| 5888                  | Round/store float channels as 8-bit RGBA                        |
| 537                   | Text-blob drawing subtree                                       |
| 6410                  | Cached-image drawing subtree                                    |

The unpremultiply stage has a scalar floating-point reciprocal and three channel multiplications; its neighbors load, normalize, round, and store channels through separate indirect stage calls. No WASM SIMD instructions were present in the disassembled artifact.

Artifact SHA-256: `40a8850e7df0534afec70804cea75552e85413c93905f22a9b4f72b9080890d3`. Chromium: 148.0.7778.96. Original benchmark environment: Windows, AMD Ryzen 5 5600X.

Raw evidence: [baseline CPU profile](../../debug/g-force-analysis/baseline.cpuprofile), [no-shadow CPU profile](../../debug/g-force-analysis/no-shadow.cpuprofile), [WASM timings](../../debug/g-force-analysis/wasm-results.json), [native timings](../../debug/g-force-analysis/native.json), [native diagnostic plan](../../debug/g-force-analysis/native-plan.json).

## Why conversion is expensive

[The preview surface](../../src-tauri/ovrley_widgets/src/widgets/value/preview.rs) stores premultiplied RGBA: RGB already includes transparency. At the time of profiling, g-force readback requested unpremultiplied RGBA directly from Skia because browser ImageData expects RGB and transparency separately.

In the checked-out Skia `src/core/SkConvertPixels.cpp`, a conversion involving unpremultiplication cannot use the simple copy path. On this non-NEON target, it also cannot use the specialized swizzle/premultiply path. It falls through to `convert_with_pipeline`. That pipeline processes the whole rectangle, including transparent space and pixels whose alpha is already fully opaque.

Consequently, the baseline converted about 335,500 pixels through that generic float pipeline for every changing marker, even when the displayed label was unchanged. This work was additional to drawing the widget.

The alternative in [read_surface_rgba](../../src-tauri/ovrley_widgets/src/render/pixels.rs) was already used by route and elevation. It asks Skia for premultiplied pixels, permitting a direct copy, then converts with a small integer loop and skips transparent/opaque pixels. All widget surface readbacks now use this helper, including [g-force](../../src-tauri/ovrley_widgets/src/widgets/g_force/preview.rs).

Its post-change WASM speed has not been measured. Integer conversion can differ in channel rounding from the generic float path. The adoption adds independent conversion tests and retains exact drawing comparisons using the canonical transport on both sides; details are recorded below.

## Controlled experiments

| Variant                                     |  WASM render ms/frame | Native draw ms/frame | Normal WASM bitmap |
| ------------------------------------------- | --------------------: | -------------------: | ------------------ |
| Baseline                                    |                8.0815 |               1.3114 | 671 x 500          |
| Shadows disabled                            |                6.1407 |               1.0411 | 639 x 468          |
| Circle fill fully opaque                    |                7.8791 |         Not measured | 671 x 500          |
| Circle fill fully transparent               |                8.0975 |         Not measured | 671 x 500          |
| Label moved 300 logical units farther right |               13.4635 |               1.3493 | 1271 x 500         |
| Scene scale 1                               |                1.9859 |               0.3815 | 340 x 252          |
| Same activity second repeated               | 0.00036; zero redraws |         Not measured | 671 x 500          |
| Baseline repeated without profiling         |                7.8848 |         Not measured | 671 x 500          |

Changing fill opacity barely affects WASM time because the converter still processes the entire rectangle. Moving the label increases mostly empty output space: WASM cost grows sharply while native drawing stays close to baseline. Halving scale cuts pixel area to roughly one quarter and time follows it.

Disabling shadows changes both drawing work and bitmap padding. The sampled text-drawing subtree falls from approximately 1.16 to 0.05 ms/frame; readback falls from 6.16 to 5.41 ms/frame as the rectangle shrinks. The entire 1.94 ms difference must not be attributed solely to shadow drawing.

## What is and is not cached

[GForceCache](../../src-tauri/ovrley_widgets/src/widgets/g_force/layout.rs) prepares a single image containing the circle fill, border, guides, and any circle border shadow. The even-odd ring path, clearing the ring interior, circular clip, and guide-opacity save layer are preparation operations. They are not repeated during playback.

The preview retains its destination surface and pixel vector while dimensions remain equal. In the 600-frame baseline, 595 frames used 671 x 500, two used 671 x 501, and three used 671 x 502. Occasional bounds changes can cause reallocation, but allocation was not a significant sampled cost.

The exact-sample early return works: repeatedly rendering the same second produced zero redraws and about 0.00036 ms/call. For changing telemetry, all 600 measured frames redraw because marker direction/position changes. Comparing only formatted magnitude to skip the entire widget would be incorrect.

Whole-activity magnitudes are sorted once when the activity sampler is constructed. Per frame, WASM performs two indexed interpolation lookups and a constant-time percentile lookup. Native uses precomputed samples. Magnitude, clipping, formatting, and layout are small compared with the measured pixel work; there is no per-frame scan/sort of the activity.

[Frame preparation](../../src-tauri/ovrley_widgets/src/widgets/g_force/frame.rs) resolves the font and lays out both the value and unit every changing sample. [draw_text_blob](../../src-tauri/ovrley_widgets/src/render/text.rs) also constructs TextPaints, including a drop-shadow filter, for each run on every draw. A persistent label repeats this work when only the marker moved; a rapidly changing label still repeats style setup and shadow filtering.

An independent reproduction of preserve interpolation and one-decimal magnitude formatting found only 43 label transitions across these 600 measured frames, with 13 distinct formatted values. A last-label cache could therefore reuse the prepared/rasterized label for most frames in this fixture. This is configuration- and data-dependent: labels that change every frame receive little benefit. This count is a reproduction from activity data, not an instrumented WASM counter; see [label reuse data](../../debug/g-force-analysis/label-reuse.json).

## Optimization order and visual equivalence

1. Adopt the existing specialized RGBA readback helper across widget surfaces, then measure its cost separately. Adoption is complete; the WASM rebuild and performance measurement remain pending. This retains the existing drawing commands, border shapes, and shadows.
2. Profile text after the readback change. Reuse font, paint, unit-layout, and glyph-measurement resources. For rapidly changing shadowed labels, compare Skia's mask-blur approach described below. A finished-label cache is an optional optimization for labels whose text persists across frames, rather than a general solution. Preserve subpixel placement, opacity, and the existing shadow/fill order.
3. Evaluate avoiding drawing the cached circle each frame, as described in the native compositing chapter below. This is a conditional compositor optimization, outside the readback change.
4. Consider dirty regions or separate static and dynamic output layers if a larger transport change is justified. A changing 24-pixel marker currently forces conversion and transport of the entire widget bitmap. Preserve clipping, shadows, alpha, and overlap behavior when changing this boundary.

The old JSX parity restriction does not explain the dominant cost. Replacing the startup ring, clip, or guide drawing cannot remove the measured per-frame conversion. A new profile after transport optimization should guide drawing changes, with visual parity checks for any replacement shadow algorithm.

## Native compositing: drawing the cached circle each frame

"Circle compositing" and "blending the circle" refer to the same operation here: drawing the cached circle onto the destination, combining overlapping colors and alpha. This chapter consistently calls that operation "drawing the cached circle each frame."

Native draws directly into a caller-owned straight-alpha output buffer, avoiding the separate full-image readback and conversion required by the browser preview. A cached circle image avoids rebuilding shapes, but drawing it still combines source and destination colors and alpha every frame.

For g-force, the current sequence is: restore the prepared static overlay, draw the cached circle image, then draw the moving marker and label. If the circle could be rendered into that prepared static overlay once, the existing restoration copy would already contain its finished pixels. This would avoid drawing the cached circle each frame. The overlay remains transparent where appropriate; this does not bake the circle into video footage.

The user's configured layer order is a required constraint. A static backdrop or raster behind the circle can be combined with it once, in the configured order, including their overlapping transparency. A changing widget behind the circle prevents baking the circle into one global initial background: drawing that changing widget afterward would put it above the circle incorrectly.

A general solution would cache consecutive groups of static content and composite those groups between changing layers in the user's order. Static portions inside otherwise dynamic widgets need the same treatment: the circle is static while its marker and label change. Any grouping must preserve the existing order of those drawing operations, overlaps, clipping, shadows, opacity, and antialiasing. Config changes must invalidate the affected group.

**Unmeasured native opportunity:** Avoiding drawing the cached circle each frame is a broader compositor change with conditional benefits, deferred from the readback work. The measured 0.64 ms cost of drawing the cached circle comes from WASM. The corresponding native cost was not independently profiled and cannot be inferred from that number.

**Separate shadow experiment:** Native drawing fell from 1.3114 to 1.0411 ms/frame when shadows were disabled, a measured improvement of about 0.27 ms/frame. Disabling shadows also changes bitmap padding; this experiment does not isolate the cost of drawing the cached circle. It is not a second measurement of the circle optimization.

Text drawing and possible shadow replacements are considered separately in the following chapter.

## Text drawing and shadow algorithms

Without shadows, the sampled g-force text-drawing subtree took approximately 0.05 ms/frame. Reusing resolved fonts, Paints, the unit's TextBlob/layout, and glyph measurements can avoid repeated setup and allocations while a number changes. Skia already caches ordinary glyph images. Large savings from changing the basic unshadowed drawing calls are not supported by this profile; measurements after readback adoption should guide further work.

The current text-shadow implementation is native Skia `image_filters::drop_shadow_only` in [TextPaints](../../src-tauri/ovrley_widgets/src/render/text.rs), on both native and WASM. In this Skia revision, it builds an image-filter graph: blur the input image, apply the shadow color using the blurred alpha, then translate it. Our renderer draws that shadow, then any text border, then the text fill. The number and unit are separate runs, each with its own filter; `draw_text_blob` recreates TextPaints/filter objects on every call.

A cheaper candidate is native Skia `MaskFilter::blur(BlurStyle::Normal, sigma, ...)`, drawing shadow-colored text at the shadow offset. It operates on coverage masks and can use Skia's glyph-strike cache for blurred glyph images. Repeated digits can then reuse work even as the overall number changes every frame. Route and elevation already use mask blur for path shadows. Local Skia sources confirm that the scaler applies mask filters when generating glyph images and that the strike caches those images.

This candidate has not been benchmarked for our text or proven equivalent to the current shadow. Per-glyph blurred masks can combine differently where neighboring glyph shadows overlap; blur kernels, opacity, antialiasing, fractional offsets, and transforms also need comparison. It is a promising general investigation for changing shadowed labels. Finished-label caching remains useful only where the displayed text persists long enough to reuse it.

## Shared RGBA readback adoption

All eleven widget families that render Skia surfaces now use the same `read_surface_rgba` helper: generic text/time, gradient, lean angle, g-force, heading tape, lap timer, gauges, label, backdrop, route, and elevation. Route and elevation already used it; nine families were changed. The helper's production conversion algorithm is unchanged. Geometry, drawing order, borders, shadows, and native export drawing are unchanged.

Raster resources keep their one-time direct straight-alpha decode. They do not read back a newly rendered widget surface per frame. Decoding through premultiplied pixels first would discard low-alpha source color precision, so this resource boundary should not be changed just to resemble the rendered-surface path.

Validation passed: 24 existing preview/metric tests and two new readback tests. The new differential test covers all 256 alpha values and every valid premultiplied channel value, comparing against Skia's independent float conversion. Alpha is identical, RGB differs by at most one level out of 255, and transparent/opaque pixels match exactly. The tests also verify that repeated readback does not modify the drawing surface and that reused output buffers discard previous colors and alpha.

The existing drawing-parity tests still compare output bytes exactly. Their reference surfaces now cross the same canonical readback boundary as widget surfaces; conversion accuracy is independently checked by the differential test. The initial run against the old float-conversion references exposed the expected rounding differences, rather than a change in drawing geometry.

No new WASM artifact or application build was produced. A WASM rebuild is required before benchmarking the new source; rerunning the probe against the existing artifact still measures the baseline converter.

To rerun the diagnostic measurements from the repository root using existing binaries:

```powershell
node debug/g-force-analysis/probe.mjs
& 'src-tauri/target/release/benchmark_widgets.exe' --run 'debug/g-force-analysis/native-plan.json'
```

Neither command builds the application or WASM module.

## Rolling chart: line and area fill costs

The rolling chart redraws every frame, because its window slides with the playhead. Cases use the ride fixture at 60 s with the manifest defaults: 600 x 150 plot at scene scale 2, 30 s window, halo marker, legend, area fill with gradient at 60% opacity. Only the series list and the fill switches vary. Values are ms/frame.

| Case                   | Native preview | WASM preview | WASM playback |
| ---------------------- | -------------: | -----------: | ------------: |
| 1 line, no fill        |           0.32 |         0.37 |          0.48 |
| 1 line, flat fill      |           0.64 |         1.03 |          1.15 |
| 1 line, gradient fill  |           0.87 |         1.50 |          1.62 |
| 5 lines, no fill       |           0.88 |         0.97 |          1.08 |
| 5 lines, flat fill     |           1.65 |         3.04 |          3.15 |
| 5 lines, gradient fill |           2.60 |         4.78 |          4.89 |

- Cost grows linearly with the number of lines: about 0.14 ms per line without fill (native preview), on top of about 0.18 ms fixed per-frame work.
- The area fill dominates. At 5 lines in WASM, filling at all adds about 2.1 ms (blending five overlapping semi-transparent regions), and the gradient adds about 1.7 ms on top. The gradient is computed per covered pixel; its shader is already built once per series.
- Without fill, WASM is 1.1–1.2x native, in line with other plots. The 1.6–1.9x WASM gap appears only with fill.
- Disabling anti-aliasing on the fill made no measurable difference and was reverted.
- A cheaper gradient can at best reach flat-fill cost (5-line WASM preview 4.8 → 3.0 ms). A candidate is stepped solid bands (16–32) clipped to horizontal strips; it trades visible stepping against extra path redraws, so band count needs measuring. Not implemented.

Runs: no fill `1792933629235-6388`, `1792933638604-12228`; flat fill `1792934211672-19312`, `1792934223069-19520`; gradient fill `1792934240624-21548`, `1792934252775-23452`. Cases `rolling-chart-{1-line,5-lines}{,-no-area,-flat-area}` in `browser.js`.

## Export: premultiplied frame buffer (reverted)

Export draws every widget into a straight-alpha (`RGBA8888` `Unpremul`) frame. Skia converts each touched destination pixel to premultiplied, blends, and converts back. Blitting the 662 x 349 splits tile costs 0.77 ms into a warm straight-alpha 4K frame against 0.20 ms into a premultiplied one; a tile-sized premultiplied surface also costs 0.21 ms, so frame size is not the factor. A preceding base restore adds about 0.2 ms of cold-cache cost on either format.

The experiment drew export into a premultiplied buffer and converted once downstream: the `yuva420p` converter read premultiplied input (chroma directly, luma through the shared unpremultiply table), and RGBA pipes (transparent codecs, QSV full overlay, untagged composite sources) ran one table pass over the atlas. Every consumer needs straight alpha: FFmpeg `rgba` input, ProRes 4444 and the other transparent codecs, and the hardware overlays (`overlay_cuda`, `overlay_vaapi`, `overlay_qsv` have no premultiplied mode).

Native benchmark on the production overlay atlas, ms/frame:

| Widget (atlas)            | Draw straight | Draw premultiplied | `yuva420p` convert straight / premultiplied | Unpremultiply pass |
| ------------------------- | ------------: | -----------------: | ------------------------------------------: | -----------------: |
| splits-run (668 x 426)    |         0.890 |              0.268 |                               0.758 / 0.760 |              0.150 |
| speed-text (350 x 140)    |         0.273 |              0.104 |                               0.127 / 0.134 |              0.027 |
| heading-tape (1200 x 200) |         2.253 |              1.641 |                               0.559 / 0.513 |              0.106 |

A 4K `prores_ks` export of a full template drew 18.4 ms/frame straight and 4.4 ms premultiplied. Its first run was slower overall because the dev profile compiled `ovrley_widgets`, which holds the unpremultiply pass, without optimization (59.6 ms/frame; 6.73 ms vs 0.15 ms release on the splits atlas). `ovrley_widgets` now builds at `opt-level = 3` in dev, like `ovrley_core`.

Outcome: end-to-end export performance in production builds was unchanged, and text looked slightly less sharp, since 8-bit premultiplied storage quantizes the colour of low-alpha antialiased and shadow pixels. The change is reverted. It remains a performance option for low-end machines, where rendering rather than encoding limits export: `prores_ks` and other software encoders render on a single worker, so drawing time there is wall time.

Export also keeps the composited splits frame and blits it while its content holds (export draw 1.03 → 0.97 ms/frame); the blit into the straight-alpha frame is the remaining cost.

## Export: two render workers for `prores_ks` (reverted)

`prores_ks` reserves no cores per frame worker, so it renders on one worker. With one worker the 4K transparent export was balanced: rendering took 20.5 ms/frame and the writer waited 16.3 ms/frame for frames, for 48.0 fps. Reserving 6 cores per worker ran two workers on 12 logical cores (worker time 1.95x wall). The writer then stopped waiting for frames (0.08 ms), but FFmpeg writes blocked for 21.1 ms/frame and workers waited 21.7 ms/frame for free buffers. Throughput fell to 46.2 fps, because the second worker competed with the encoder's CPU threads. `prores_ks` at 4K tops out around 46–48 fps on this machine, which is also why faster drawing (the premultiplied buffer above) left its end-to-end speed unchanged. The profile is back to a single worker. `prores_videotoolbox` encodes on the media engine, leaving FFmpeg only the atlas `xstack` and pixel conversion on the CPU, so it now reserves 4 cores per worker (2–4 workers on Apple silicon); this still needs measuring on macOS.
