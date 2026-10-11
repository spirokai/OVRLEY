---
name: migrate-widget-to-wasm
description: Migrate an OVRLEY widget to the shared Rust/WASM renderer, including previewSecond sampling, editor responsiveness, react-moveable gestures, intrinsic resizing, and native export parity. Follow the working speed/text vertical slice.
---

# Migrate an OVRLEY widget to WASM

Deliver one complete widget/display-type slice: shared native/WASM rendering, live preview, inspector edits, selection, gestures, commit/cancel, and resource cleanup. Preserve its existing product behavior. Use speed/text as the implementation reference, not as a universal layout or resize policy.

Baseline: `cb0cf79c` (shared rendering/preview) and `15747cc3` (Moveable/resizing/gestures). Read their relevant diffs and current files before editing. All paths below are repository-relative. Current code takes precedence over scratch plans and stale README claims; module initialization now belongs to canvas mount and does **not** gate app startup.

## Ownership and file placement

```text
src-tauri/
  ovrley_widgets/src/             Shared renderer; depends on neither host
    model/                       Canonical types, scene contract, shared validation
    activity.rs                  Original-density sampling and prepared indices
    interpolation.rs             Shared interpolation primitives
    render/{text,icons,svg}.rs    Shared drawing/resource primitives; FontResolver
    preview.rs                   Session: activity, fonts, widget handles, dispatch
    widgets/value/               Working metric family
      config.rs                  Raw serde input contract
      validation.rs              Raw -> validated config
      format.rs                  Sample -> display content
      frame.rs                   One layout for drawing AND interaction bounds
      layout.rs                  Drawing and static-part rules
      preview.rs                 Surface/cache/revision and geometry output
      interaction.rs             Intrinsic resize policy -> canonical field patch
  ovrley_core/src/               Native parsing, export preparation/composition,
                                encoding, filesystem fonts; calls shared renderer
  wasm-bindings/src/widget_abi.rs Thin C ABI: bytes, handles, status/error buffers
  src/widget_preview.rs          Tauri activity snapshots and font byte transport
app/src/features/wasm-widgets/
  hooks/useWasmWidgets.js         React snapshots, canvas-lifetime session cleanup
  hooks/useWasmCanvas.js          Canvas attachment
  hooks/useWasmWidgetInteractions.js  Moveable intent and transaction lifecycle
  utils/previewSession.js        ABI transport, resources, sole preview RAF, pixels
  utils/widgetInteraction.js     Gesture origin/latest intent/resolve/flush/cancel
  utils/widgetGestureGeometry.js Axis lock and opposite-corner geometry
  utils/widgetPresentation.js    Rust rectangles -> DOM, geometry notification
  components/WasmWidgetCanvas.jsx Presentational wrapper + raster canvas
```

Extend `widgets/value/` for another compatible metric. For a different family, add `widgets/<family>/` with the responsibilities above, splitting files where useful. Move domain code out of native ownership; do not copy it into a WASM-only implementation. Both hosts import shared types directly. Native wrappers are justified for actual resources such as `NativeFonts`, not for retaining old module aliases.

`ovrley_core` and the isolated Emscripten `wasm-bindings` workspace both depend on `ovrley_widgets`. Keep filesystem access, Tauri, ffmpeg, export jobs, and native activity densification out of that shared crate. Components remain presentational; orchestration belongs in hooks, non-React scheduling/geometry in utilities.

## 1. Establish the contract and shared renderer

- Inventory the target's canonical config, defaults/inheritance, display types, telemetry columns, formatting, assets, layout, native draw path, and supported gestures. Migrate only the requested variants.
- Put raw config and validation at shared ingress. Require effective values supplied by the existing config/default-resolution pipeline. Malformed present config fails visibly; consumers use validated fields without coercion or fallback repair. Missing external telemetry remains a legitimate domain case.
- Move formatting, layout, drawing, and cache eligibility into the shared family. Make native export call them with its prepared samples and resource resolver. Preview supplies on-demand samples through the same formatting/drawing rules.
- Use one prepared frame for painting and bounds. Keep `content_bounds` tight to visible glyph/icon strokes; exclude line-box whitespace, shadows, and raster padding. Raster bounds include all painted effects and are rounded outward for allocation.
- Reuse existing font identity and serialized Skia typeface transport (`previewFont` -> `PreviewFonts`). Do not substitute browser font measurement. Shared icons come through `render/icons.rs` and `ovrley_widgets/build.rs`; inspect that resource path before adding assets.

Existing native resource-boundary pattern:

```rust
ovrley_widgets::widgets::value::layout::draw_metric_parts(
    canvas, style, parts, scale, &crate::fonts::NativeFonts(font_dirs),
    static_parts, validated,
)?;
```

### Extend the current speed-only seam explicitly

At this baseline, `PreviewSession` stores value `PreviewWidget`s, `WidgetRequest` contains `ValueConfig`, `render`/`resize_widget` sample speed, and the preview constructor rejects anything except speed/text. Widening `isWasmWidget` alone is insufficient.

For another text metric, the canonical config already identifies the telemetry: e.g. `"value": "power"`. Use that existing field (validated as `metric`) to select its activity column and formatting in Rust. Extend the speed-only sampling/formatting and supported-metric check; reuse the value renderer, intrinsic resize policy, ABI, and frontend gesture lifecycle. Resize measurement must use the selected metric too. Do not introduce another identifier, frontend column mapping, per-metric renderer, or per-metric binding.

For a different widget family, extend shared session request/widget dispatch and add the required family rendering/resize policy using canonical config discriminants. Do not infer types from optional fields or manufacture a parallel JS schema. Keep ABI and JS session generic transport, with explicit errors for unsupported variants.

In `activity.rs`, deserialize the required canonical finalized columns once per activity and prepare reusable sampling state. Match that metric's native missing-data/interpolation policy; speed's gap bridging is not universal. Avoid rebuilding dense arrays, scanning the whole activity, or allocating sample vectors every frame. Support backward seeks as well as sequential playback.

## 2. Wire preview and preserve responsiveness

The production path is:

```text
effective widgets + scene + previewSecond
  -> useWasmWidgets -> previewSession snapshot / requestAnimationFrame
  -> resolve latest gesture -> create/update Rust handles -> render(second)
  -> changed revision -> RGBA + geometry -> canvas + wrapper -> Moveable refresh
```

Reuse `OverlayCanvas.jsx`'s binding (inside the existing orchestration):

```js
const wasm = useWasmWidgets({
  widgets, scene: wasmScene, activity, second: previewSecond,
  defaultFont: defaultValueFont,
})
```

- `previewSecond` is the sole preview clock, including resize measurement. Do not round to an index, add an independent timer, or make Tauri render calls per frame.
- JS transfers native `preview_activity_id` snapshots as opaque bytes. Rust deserializes/samples original-density data. `null` activity explicitly clears samples; missing identity on present activity is an error. Keep native release ownership in `createMediaSlice.js`, accounting for both active activity and stashed video telemetry.
- Cache the module promise; keep one disposable session for the canvas lifetime, including zero migrated widgets. Start activity transport and default-font preparation independently when the snapshot arrives. Delete widget handles when removed; retain session resources after the last deletion until unmount.
- On activity replacement, clear old WASM samples/pixels immediately, reject stale async completions by identity/disposal, and render after resources arrive. Surface module/resource/render errors through the existing canvas alert.
- Coalesce snapshots/intent into the existing session RAF. Update config only when changed. Rust owns static caches and presentation revisions; JS copies pixels only on revision change and reallocates `ImageData` only when raster dimensions change. Cache keys must include everything affecting the new widget's output; speed's formatted-parts key is insufficient for a continuously changing graph.
- Keep ABI input allocation/deallocation in `try/finally`; check status (`0` success; create returns `0` on error). Output buffers are borrowed until mutation/destruction: copy pixels promptly and reacquire `HEAPU8` after WASM calls because memory can grow. Browser `ImageData` requires straight-alpha RGBA; Skia surfaces draw premultiplied pixels and read out as `Unpremul`.

Only after Rust supports the slice, extend `app/src/lib/widget/wasm-widget.js:isWasmWidget`. This one predicate must select the canvas and exclude migrated widgets from `useOverlayPreviewModels.js` and `widgetRenderGeometry.js`. Preserve `useWidgetManager.js`'s direct data drafts and add/edit/reset flow: WASM owns font preparation; legacy browser-font readiness must not delay these operations. Do not leave a second React/SVG renderer or JS geometry model active for the migrated slice.

## 3. Connect Moveable to Rust geometry

Register the **wrapper** with `widgetRefCallbacks[widget.id]`; attach its child canvas to the session. Preserve `data-widget-id`, selection mouse handling, and the controller registration on the wrapper. The initially hidden wrapper becomes visible on first presentation; the canvas is pointer-transparent and may overflow for shadows. Generalize the current speed-specific accessible label when adding types.

`WidgetGeometry` supplies raster `{ left, top, width, height }`, tight `content` with the same four keys, and `content_width` (layout width used by existing editor consumers; not a substitute for the tight rectangle). All positions/bounds are in scene pixels. Scene scale is already applied by Rust; Moveable handles viewport zoom through the existing scene/root containers. Do not divide deltas or bounds by zoom again.

Reuse `presentWidgetGeometry`, whose essential placement is:

```js
target.style.left = `${geometry.content.left}px`
target.style.width = `${geometry.content.width}px`
canvas.style.left = `${geometry.left - geometry.content.left}px`
target.dispatchEvent(new Event('wasm-widget-render', { bubbles: true }))
```

It also installs top/height and visibility. `useWasmWidgetInteractions` listens for this event and runs `flushSync(() => moveableRef.current?.updateRect())` for selected widgets. Keep this notification during active gestures and playback: glyph widths can change without config edits. The legacy geometry-signature effect skips active gestures and cannot replace it.

Reuse selection routing in `OverlayEditor.jsx`: any WASM selection uses the WASM handlers. Single speed/text widgets use corner **resize**, not CSS scale; disable `checkResizableError` for this Rust-owned geometry. Groups containing WASM widgets support shared data-draft translation; group scaling stays disabled until a shared resize policy exists. Rotation/edge handles require an actual family policy; adding a family must not accidentally expose legacy handlers through its type flags.

## 4. Preserve the gesture transaction

1. **Begin:** flush pending preview work; capture original canonical data, scene, and rendered content bounds. Begin the existing live-edit transaction. For resize, capture the opposite content corner using `resizeAnchor(bounds, direction)` and initialize Moveable from those bounds.
2. **Update:** replace the pending intent. Drag is original x/y plus cumulative `beforeTranslate` (with the existing Ctrl axis-lock utility). Never add a cumulative delta to the previous draft.
3. **Resolve/render:** the session RAF resolves intent, updates WASM, and presents pixels/bounds in that frame. Inspector draft publication is downstream and does not gate this render. WASM drafts carry canonical data with `layout: null`; retain the existing RAF-coalesced draft store.
4. **Finish:** synchronously flush the last intent before committing one canonical patch. Preserve fractional coordinates/sizes exactly; then clear drafts and end interaction.
5. **Cancel:** discard intent/drafts, restore committed presentation, and stop Moveable on Escape, window blur, selection/config replacement, and unmount. Clean up even when flush/commit fails.

Existing metric resize intent:

```js
current.controller.update({
  type: 'resize',
  geometry: {
    factor: Math.max(width / current.origin.bounds.width, 0.01),
    ...current.anchor, // normalized fixed corner + its original scene point
  },
})
```

Rust `interaction.rs` validates that request, scales the **original** intrinsic fields, applies product size limits, prepares layout at the **current** `second`, and returns a canonical patch. For speed: font/icon sizes and icon offsets change; no persisted widget scale is introduced. Anchor correction follows the newly measured content:

```rust
x: config.x + request.point[0] - (bounds.left + bounds.width() * request.fixed[0]),
y: config.y + request.point[1] - (bounds.top + bounds.height() * request.fixed[1]),
```

Keep resolving resize when `previewSecond` changes even without a new pointer event. A reflowing/framed widget may need requested width/height and a different Rust policy: extend the intent/dispatch at their owners instead of forcing speed's uniform factor model onto it. Mixed-group drag remains the existing batched draft path and commits all translations together.

## Common mistakes

| Bad pattern | Correct seam |
| --- | --- |
| `data.font_size \|\| 32`, coerced config, unknown kind rendered as speed | Validate canonical input once; report malformed/unsupported input |
| WASM depends on `ovrley_core`; duplicated native/WASM layout | Move shared domain code into `ovrley_widgets`; native supplies resources |
| JS interpolates/formats values or uses DOM text bounds | Rust samples, formats, and prepares the frame |
| Per-pointer React state -> effect -> second RAF -> render | Replace session intent; resolve in its existing RAF |
| `draft.x += beforeTranslate[0]` | `origin.x + beforeTranslate[0]` |
| `target.style.transform = event.transform` / bitmap scaling | Rust updates intrinsic fields and redraws; presenter installs bounds |
| Selection uses shadow-expanded raster box | Wrapper uses tight content; raster overflows |
| Mouseup commits last published inspector draft or rounds values | Flush controller; commit its exact patch once |
| Session effect depends on widget count; resource chain starts on first add | Session lives with canvas; prewarm activity/default font independently |
| Catch WASM failure and silently use legacy preview | Visible error; fix the owning contract/resource boundary |

## Verification and completion

Extend meaningful seam tests, using these references:

- `src-tauri/ovrley_core/tests/wasm_widget_preview_tests.rs`: transported fonts + preview pixels match native composition; unchanged presentation reuses revision; clear/replace activity; malformed config fails; every opposite corner stays fixed across alignment, scale, icon offsets, shadows, and changing playhead. Compare live and committed geometry.
- `app/src/tests/features/wasm-widgets/{widgetInteraction.test.js,useWasmWidgetInteractions.test.jsx}`: latest intent wins, original-data math, mouseup before RAF, cancel/dispose, and batched mixed-group translation.
- `app/src/tests/features/overlay-editor/useWidgetDraftState.test.jsx` and widget-manager tests: coalesced inspector drafts and immediate WASM edits without legacy font gates.

Run targeted Vitest tests from `app/`; run shared Rust tests with `cargo test --manifest-path src-tauri/Cargo.toml -p ovrley_widgets` and parity tests with `cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml --test wasm_widget_preview_tests`. Add the migrated family's native tests and frontend lint as appropriate. Native seam tests do not establish actual browser/WASM behavior: exercise selection, playback/scrubbing, inspector edits, drag/resize at multiple viewport zooms, mixed groups, undo/redo, cancellation, replacement, and delete-last/add-first in the editor when available.

**Never run builds without user permission**, including `pnpm wasm:build`; dev/Vite asset requests can trigger builds too. Reuse an already running app when available. Report any unverified browser/build behavior. Toolchain ownership is in `scripts/build-wasm.mjs`, `scripts/vite-wasm.mjs`, `.env.example`, and `vendor/skia-wasm/manifest.json`; reuse the pinned archive and existing watcher. An ordinary widget migration does not require rebuilding Skia or changing toolchain pins.

Done means the requested slice has one shared rendering owner, one preview clock, correct live/committed geometry, responsive resource/gesture lifecycles, and evidence of native parity and editor behavior. Report changes and remaining verification limits without migrating unrelated widgets.
