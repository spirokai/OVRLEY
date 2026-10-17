# Widget rendering follow-up

**SIMD follow-up (2026-10-17):** Skia has now been rebuilt with `-msimd128 -msse2`.
Heading tape improved 3.56x, with mixed results elsewhere, including three text-heavy
regressions. See [the SIMD experiment](skia-simd.md) for timings, appearance checks,
artifact hashes and build details. The SIMD result is adopted; temporary scalar
backups were removed. The measurements below
describe the earlier scalar module.

The rebuilt readback helper improved g-force from 7.83 to 2.63 ms in WASM. The next main costs were repeated text shadows and, separately, image sampling/compositing. Lap-log cached glyph layouts but repainted the whole table on each changing timer sample. Heading-tape cached its strip correctly, but scrolling that strip still ran an expensive scalar pixel pipeline.

The requested mask shadows and lap raster cache are implemented for both hosts. The production WASM module and native benchmark were rebuilt, and a full 24-case run completed. No desktop application build was run.

## Comparable timing, without adjusting measured durations

The earlier comparison paired native export's `frame.draw` with WASM's `wasm.render`. They do different work: native draws into a full-scene straight-alpha output buffer and has separate restoration/wrapping costs; WASM samples activity, prepares/draws a widget surface, reads/converts its pixels, and serializes geometry.

The benchmark now also runs [the same preview session natively](preview.rs). Native preview and WASM preview both include sampling, presentation skips, frame preparation, clearing, drawing, RGBA readback, alpha conversion, and geometry serialization. They receive the same serialized font resources, activity, widget request, and timeline. Both exclude setup/warmup, JavaScript pixel copying, and browser canvas upload. WASM includes its browser ABI; native calls the session directly.

No native restoration estimate was subtracted from WASM, and no artificial readback charge was added to native export. The summary compares actual native-preview measurements against actual WASM-preview measurements. Native export draw/total and WASM playback remain in a separate workflow table. Platform font rasterizers still produce small differences in glyph metrics and raster dimensions, typically 1–3 pixels in these cases; this does not claim byte-identical native/browser rendering.

Native preview was also measured against the original post-readback requests, before the shadow/cache changes. Those measurements were added to [run 1792167014911-32756](../../debug/benchmarks/widgets/1792167014911-32756/summary.md). Its existing native-export and WASM samples were preserved.

## Full-widget results

The new run is [1792168448780-12996](../../debug/benchmarks/widgets/1792168448780-12996/summary.md). Both runs use the same 24 cases, resolved inputs, 200 warmup frames, and 1,000 measured frames at 30 FPS. Every WASM redraw count is unchanged. Timings below are actual preview-render averages per playback frame, including frames that skip drawing.

| Widget            | Previous WASM ms | Updated WASM ms | WASM improvement | Updated native preview ms |
| ----------------- | ---------------: | --------------: | ---------------: | ------------------------: |
| lap-log           |          10.7052 |          1.5817 |            6.77x |                    0.6593 |
| current lap       |           4.5366 |          0.7904 |            5.74x |                    0.2078 |
| gradient          |           1.7314 |          0.3182 |            5.44x |                    0.0792 |
| lean angle        |           1.2776 |          0.6689 |            1.91x |                    0.3358 |
| speed arc fill    |           2.0148 |          1.0351 |            1.95x |                    0.4730 |
| speed arc bars    |           1.8868 |          0.9473 |            1.99x |                    0.4131 |
| speed corner fill |           1.9224 |          1.0441 |            1.84x |                    0.4776 |
| speed corner bars |           1.7459 |          0.9855 |            1.77x |                    0.4565 |
| g-force           |           2.6313 |          1.7363 |            1.52x |                    0.8656 |
| heading tape      |          10.1987 |         10.1823 |        unchanged |                    1.8078 |

These are full preview renders, including readback, rather than isolated shadow timings. They are single sequential passes, not statistical confidence intervals. The earlier isolated text benchmark's 3.6x native / 6.1x WASM gains measured label drawing only and should not be substituted for these widget results.

## Why the lap widgets were slow

Previously, each changing `current_lap` rendered both its static heading and its changing timer through separate `drop_shadow_only` image filters. In a control run, disabling shadows reduced WASM from 4.67 to 0.31 ms/frame. Hiding the static heading alone left 3.76 ms/frame, so the changing timer's own shadow was also expensive.

For lap-log, [the old layout cache](../../src-tauri/ovrley_widgets/src/widgets/lap_timer/layout.rs) retained header/completed-lap glyphs, but did not retain their finished pixels. Every new current time assembled all rows again, regenerated completed-row delta runs, and repainted every cell and its shadow. Disabling shadows reduced this diagnostic from 11.19 to 0.71 ms/frame. These controls also shrink shadow padding, so they measure more than a pure blur function.

The shared lap renderer now rasterizes the static heading or the log's headers, completed rows/deltas, and current lap number into a bounded cached image. Only the changing time and current delta remain dynamic in the table. Completed rows/deltas rebuild when the completed-lap count or active lap number changes. Showing/hiding the current row is part of that state because it shifts the history rows. Activity replacement and configuration replacement discard the owning layout/cache. Backward seeking rebuilds the appropriate history rather than retaining future completed laps.

The cached image stays inside its widget's existing draw position and is painted before its dynamic cells. It is not moved into a global scene background. Other overlapping widgets/backdrops retain their ordering. Integer cache origins retain fractional glyph placement. Native frame workers share the immutable image through a short cache lock; the cache retains only the current static state, rather than one full bitmap per lap count.

## Shared mask shadows

All former `drop_shadow_only` drawing in production widget code now uses [`MaskFilter::blur(BlurStyle::Normal, sigma, true)`](../../src-tauri/ovrley_widgets/src/render/shadow.rs). This covers shared text, metric icons, heading ticks/labels/indicator, g-force borders, lean-angle borders, and linear/arc/corner gauge shadows. Route/elevation path shadows already used mask blur and retain their implementation. Widgets without an existing shadow do not gain a new effect.

The shadow pass retains source geometry/stroke styling, uses the configured shadow color, multiplies shadow alpha by the original source alpha, and draws at the configured offset. Fills/borders keep their existing drawing and order. Icons convert sigma/offset through their local scale to retain output-pixel sizing. Strength zero creates no blur and executes no shadow drawing. Mask-blurred glyphs can be reused by Skia's glyph cache even when the overall number changes; this does not depend on finished labels remaining constant.

The [archived shadow diagnostic](../text-shadows/README.md) measured an explicit implementation of the former image-filter algorithm against mask blur. Its comparison images and local timing results are retained; the benchmark sources and build wiring have been removed.

## Appearance and correctness

99 focused Rust tests passed across widget/metric preview parity, lap timing, heading, arc/corner/linear gauges, g-force, lean angle, fonts, and value widgets. The existing lap parity test now compares cached preview pixels against direct glyph rendering across lap boundaries, backward seeks, rounded-value skips, and activity replacement, including fractional positions and partial opacity. These comparisons remain exact; no test tolerance was widened for this change.

33 browser before/after cases cover 11 widget styles in normal, shadow-disabled, and fractional/translucent/bordered configurations. All geometry JSON is identical. All 11 shadow-disabled images are pixel-identical, including lap caching. For shadowed cases, maximum premultiplied RGB differences are 1–5 levels out of 255. Maximum alpha differences are 2–28; the largest occurs in heading's shadow falloff. Mean alpha differences over visible pixels are below 1.1 levels. Opaque foreground RGB matches except for one level in one bordered linear-gauge case.

Visual assessment: equivalent appearance in the checked cases, with small shadow falloff/overlap differences; not mathematical pixel identity. Evidence: [lap-log comparison](../../debug/widget-render-analysis/comparison-lap-lap_log-normal.png), [fractional heading comparison](../../debug/widget-render-analysis/comparison-heading-tape-fractional-translucent-border.png), [translucent g-force border comparison](../../debug/widget-render-analysis/comparison-g-force-fractional-translucent-border.png), and [all comparison metrics](../../debug/widget-render-analysis/updated-visual.json).

## Remaining WASM cost: drawing and readback are separate

There is no fixed 2–3x WASM slowdown implied by these results. The current module contains no SIMD instructions. In this pinned Skia source, `SkRasterPipeline_opts.h` falls back to scalar stages on this target; its low-precision vector stages are excluded for the scalar build. Native Skia can select CPU-specific SSE/AVX raster routines. The exact selected native routine was not independently instrumented.

The concrete process discussed here is Skia's per-pixel drawing math: image sampling, color/alpha blending, and applying masks. For heading's smooth scroll, Skia fetches neighboring cached-strip pixels, weights their colors according to fractional position, and blends the result into the layer. Cached pixels still have to undergo that sampling/compositing each frame. This is separate from the Rust readback helper that runs after drawing.

A new CPU-sampling profile of the rebuilt module gives the following approximate shares. These are sampled estimates from diagnostic runs, not separately instrumented stopwatches; their absolute times include profiling overhead and differ slightly from the full-run table.

| Widget            | Readback + conversion | Cached image drawing |  Text drawing | Other |
| ----------------- | --------------------: | -------------------: | ------------: | ----: |
| g-force           |         0.93 ms / 51% |        0.66 ms / 36% | 0.18 ms / 10% |    3% |
| lap-log           |         0.48 ms / 29% |        0.69 ms / 41% | 0.45 ms / 27% |    4% |
| current lap       |         0.14 ms / 17% |         0.05 ms / 6% | 0.59 ms / 72% |    5% |
| heading tape      |          0.33 ms / 3% |        8.27 ms / 78% | 0.02 ms / <1% |   19% |
| gradient          |         0.06 ms / 16% |                    — | 0.27 ms / 78% |    7% |
| lean angle        |         0.24 ms / 35% |        0.18 ms / 27% | 0.12 ms / 18% |   20% |
| speed arc fill    |         0.20 ms / 19% |        0.40 ms / 37% | 0.25 ms / 24% |   20% |
| speed corner fill |         0.21 ms / 19% |        0.50 ms / 45% | 0.20 ms / 18% |   18% |

Readback still copies the entire bitmap and scans its alpha. Partially transparent pixels require three integer divisions. It is now a significant residual cost for g-force and lap-log, but cannot explain heading or most current-lap time. The WASM bridge is not shown as the dominant work in these profiles.

The current raster helper is identifiable in the rebuilt disassembly as function 517 (three channel divisions and the alpha skip); image/text subtrees are classified from their Skia draw call stacks. [Stage estimates](../../debug/widget-render-analysis/updated-profile/stage-estimates.json) and [profiles](../../debug/widget-render-analysis/updated-profile/) preserve that evidence.

## SIMD: build differences and unchanged rendering code

WebAssembly supports 128-bit SIMD operations that browsers compile into hardware vector instructions. Emscripten enables this with `-msimd128` and can translate supported existing SSE/NEON intrinsics. See [V8's SIMD explanation](https://v8.dev/features/simd) and [Emscripten's SIMD documentation](https://emscripten.org/docs/porting/simd).

The scalar WASM binary measured in this report contains no SIMD instructions. This is a concrete difference from native Skia's available CPU vector routines, but the fraction of the remaining slowdown caused by that difference was not isolated in these measurements. Improving a drawing algorithm benefits both hosts; explaining their speed ratio requires comparing their compiled implementations and runtime costs.

A SIMD experiment must rebuild the C++ Skia library with SIMD enabled and ensure that this Skia revision selects a suitable vector implementation. The final WASM module must support those instructions too. Changing only Rust flags or relinking the existing Skia archive does not accelerate that archive. Verify SIMD use in the actual drawing hot paths, then repeat the same benchmark and appearance checks.

**If Skia uses the vector routines, our widget rendering code can stay unchanged.** Calls such as `draw_text_blob`, `draw_image`, and path drawing keep the same API and rendering strategy; Skia's internal implementation performs the work differently. No switch to different widget drawing primitives is required for this experiment.

The custom Rust alpha-conversion loop is outside Skia. Rebuilding Skia with SIMD does not vectorize that loop; its optimization remains separate. The subsequent [SIMD rebuild and comparison](skia-simd.md) measures the result without changing that loop's Rust target-feature flags.

## Heading and other next opportunities

Heading is the largest remaining problem. Its 360-degree strip is prepared once, including labels/ticks and their shadows. Disabling all shadows changed the baseline control only from 10.40 to 9.99 ms/frame. Removing the indicator did not help; it also expands the tape body, so that control is not an isolated indicator-cost subtraction. The updated profile still attributes about 78% to drawing the cached strip. The bilinear sampling stage alone accounts for roughly two-thirds of heading's render time. Layer restoration/fading and other drawing account for most of the remainder; readback is only about 3%.

Potential fixes should preserve fractional scrolling, wrapping gutters, fade, group opacity and overlap. An optimized horizontal strip sampler/compositor or a Skia build with an appropriate vector path deserves a measured comparison. Merely changing text shadows or switching to nearest-pixel sampling does not establish an equivalent solution. Enabling a Rust SIMD flag alone would not change the precompiled C++ Skia library or guarantee that Skia selects a vector raster pipeline.

Other opportunities remain measurable rather than hypothetical blanket rewrites:

- G-force/lap-log: avoid converting/copying unchanged large bitmap regions, or use a browser boundary that can consume premultiplied pixels. These are transport changes with transparency/overlap constraints.
- Lap-log: the large static table is now cached but still composited into and read from the widget surface every changing frame. Caching does not make those pixel passes free.
- Arc/corner/linear bars: preview equality currently includes continuous `fill01`, although bar drawing depends on an integer bar count. All bar cases redrew 970/1,000 frames in this fixture. A presentation key based on visible bar count plus displayed value could skip identical frames. This has not been implemented here.
- Arc/corner/lean angle: cache invariant clip/segment geometry and profile the remaining dynamic fill paths. Existing static tracks/units are already cached; their startup shadow preparation was not the main playback cost.
- Gradient/current lap: after the shadow replacement, text mask painting still dominates. Reuse resources where useful, but a finished-label cache will not help timers that change every frame.

The conditional native circle/background compositing proposal remains a separate deferred topic in the [original report](report.md#native-compositing-drawing-the-cached-circle-each-frame). No global static-layer reorder was introduced.

## Elevation over video: browser composition

The widget benchmarks above do not include video playback or completed GPU presentation. Rust/WASM Skia renders CPU pixels; the browser then uploads and composites the canvases over the HTML video. An opaque elevation area still leaves transparent canvas pixels around the profile and labels.

During the reported playback problem, Windows Task Manager's **3D** engine rose to about 50%, versus roughly 25% before adding elevation. Removing elevation during playback returned usage to baseline. Removing the video's `filter: opacity(1)` reduced reported CPU to 11% and GPU to 19–20%, but restored the washed-out appearance of translucent widgets at smaller editor zoom levels. Making the video 99.99% opaque also fixed the appearance and also made performance unacceptable. Both workarounds have been removed. A software-backed widget canvas (`willReadFrequently: true`) did not fix the appearance and was reverted too.

This evidence points to the browser/video composition path, rather than an expensive GPU operation inside elevation's Skia drawing. Chromium's Windows compositor [rejects translucent video underlays](https://chromium.googlesource.com/chromium/src/+/main/components/viz/service/display/dc_layer_overlay.cc). The observations fit an underlay-related problem, but no trace of the actual WebView2 presentation or driver blending has been collected to prove the exact fault.

`rust-rewrite` and the code immediately before elevation's migration used the same overall DOM structure: video behind a `widget-layer` div, with the entire scene under a shared scale transform. Neither explicitly isolated or promoted the widget container. Elevation itself was SVG. The actual GPU layers selected by those older browser sessions cannot be established from the JSX alone.

A separate Chromium test with canvas/SVG/DOM opacity swatches confirms that ordinary 2D canvases are individually accelerated. Promoting the parent or adding `isolation: isolate` leaves those canvas layers in place. Software-backed canvases paint into the common page layer instead. Screen captures in this test do not reproduce the reported washout; they cannot establish the correctness of the actual WebView2 hardware-underlay presentation.

The live comparison in the actual WebView2 showed **all four rows washing out**: HTML, SVG, accelerated canvas, and software-backed canvas, each with identical 50%-transparent color swatches. The fault therefore affects ordinary browser content too. It is not specific to Skia, WASM readback, or widget caching. The temporary panel was removed afterward.

A layout trial sized the background/video directly to the visible preview and scaled only the widget/interaction scene. At 40% scale, Chromium reported one 1536×864 video layer without a scale transform, instead of a 3840×2160 video layer transformed down to the same visible size. Canvas, SVG, and DOM swatch colors and on-screen bounds agreed at scales 0.4, 0.75, and 1. 108 focused frontend tests passed. In the actual WebView2, however, widgets still washed out below 100% and GPU usage still spiked above 30%. The layout trial was therefore reverted. Browser geometry and screenshot checks alone did not reproduce the actual presentation fault.

The user reports that launching with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--disable_direct_composition_video_overlays=1` appears to resolve the problem. The underscore spelling is present in the installed WebView2 155.0.4283.45 binary; the old hyphenated `disable-direct-composition-video-overlays` spelling is absent. Chromium [accepts driver-workaround names as command-line switches](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/gpu/config/gpu_driver_bug_list.cc), and its [overlay support check](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/gl/direct_composition_support.cc) returns false for this workaround. Microsoft [documents the environment-variable startup mechanism](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags#setting-browser-flags-in-your-local-device-environment). This targets video overlays; it does not request disabling GPU rendering or hardware decoding. The user owns the GPU comparison; no quantified performance result is claimed here.

The workaround is now configured permanently for the main window through `additionalBrowserArgs` in `src-tauri/tauri.conf.json`. That property is Windows-only. The argument string preserves the installed Wry 0.53.5 defaults for disabled Edge UI features and media autoplay, then adds the overlay workaround. Both development and packaged Windows builds receive it without an environment variable. macOS uses WKWebView and Linux uses WebKitGTK; neither has Windows DirectComposition, so this flag has no effect there. Browser arguments require a full application restart and are embedded in the native application configuration. The removed CSS filter stays removed. Configuration was checked without running a build or GPU comparison.

Diagnostic source, layer records, and geometry/pixel samples are in [debug/canvas-compositing](../../debug/canvas-compositing/). These are browser presentation experiments, separate from the CPU widget timing comparisons above.

## Reproduction and artifacts

The full run was executed with `node benchmarks/widgets/run.mjs --run` after explicitly rebuilding native benchmark and production WASM. Future runs record artifact SHA-256 hashes and modification times in `environment.artifacts`; rerunning without a build still uses the existing binaries. The measured updated WASM SHA-256 is `9e25e06b60721654bc9c8f3a647afed4c983e9aef6be6642f9b427339a006e12`.

Local diagnostic scripts and data are under `debug/widget-render-analysis/`: `probe.mjs` for original controls, `probe.mjs --updated` for rebuilt CPU profiles, and `visual.mjs baseline|updated` for comparison images. The baseline module hash and rebuilt hash are saved in [artifacts.json](../../debug/widget-render-analysis/artifacts.json). None of these diagnostic commands builds automatically.
