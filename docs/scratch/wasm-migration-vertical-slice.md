# WASM widget vertical slice

Migrate speed with `display_type: text` in the actual `OverlayEditor`. Keep the editable document in Zustand; shared Rust code owns sampling, formatting, layout, and drawing. WASM receives original finalized activity samples when the activity changes and `previewSecond` when rendering.

This document records the agreed architecture. The user subsequently approved implementation and verification builds (native, WASM, focused tests, frontend production compilation; no packaging/release).

Prerequisite: migrate native rendering to Skia `0.153.3` and verify existing rendering before starting the widget slice. Both targets use that release's revision `b7f043e0b1e2a850e702245de811f01df1f6b0ff`.

## 1. Ownership

Native Rust and browser WASM are separate runtimes. Sharing Rust implementation is practical; sharing their live objects or memory is not.

| Concern                                                 | Owner                      |
| ------------------------------------------------------- | -------------------------- |
| Editable widget configuration, selection, editor drafts | Frontend                   |
| Playback clock and previewSecond                        | Frontend                   |
| Activity import and finalization                        | Native Rust                |
| Preview activity snapshot and sampling                  | Rust inside WASM           |
| Widget validation, formatting, layout, drawing          | Shared Rust implementation |
| Preview surfaces and widget caches                      | WASM session               |
| Export scheduling, composition, encoding                | Native Rust                |
| Font discovery and byte acquisition                     | Native Rust                |
| Font matching and typography behavior                   | Shared Rust implementation |
| Canvas presentation and eventual interactions           | Frontend                   |

The end-state frontend stores the activity ID, revision, and UI metadata rather than the full activity values. Native Rust retains the finalized activity for export; WASM retains its own copy for preview. JavaScript transports samples without interpreting or retaining them in application state.

During this slice, unmigrated previews still need the existing JavaScript activity data. Remove that copy only after its remaining consumers migrate. Other UI consumers, such as timeline graphs, should receive purpose-specific summaries or reduced series where needed.

Speed text is a useful first slice because it exercises real activity sampling, units, decimals, fonts, icons, alignment, opacity, and shadows. A static label would leave the timing question unanswered; a route widget would bring substantial geometry into the first migration.

The migration boundary should explicitly be speed/text. Other speed presentations retain their existing renderer until migrated. Failure of the WASM renderer must surface an error, not silently switch to SVG.

## 2. Keep the document in Zustand

Rendering in Rust does not require Rust to own the editable document. The document also supports editor controls, persistence, template management, and future interactions.

Moving it into native Rust would introduce asynchronous IPC into slider updates and require either delayed controls or a frontend optimistic copy with reconciliation. Neither helps this slice.

The update flow should be:

1. The editor updates its existing committed configuration or live draft.
2. A hook selects the effective widget configuration and relevant scene settings.
3. The preview controller sends the latest complete widget configuration directly to WASM.
4. Rust validates it and replaces the widget’s prepared rendering state.
5. The controller redraws at the current previewSecond, including while paused.

Use complete widget updates, not individual property mutation commands. This keeps each update self-contained, makes invalidation predictable, and avoids another configuration-editing system inside Rust. Coalesce rapid updates to the latest state per animation frame; do not debounce until the slider stops.

WASM holds a rendering snapshot derived from the document. Export receives an immutable document snapshot through the existing native boundary.

Preserve the canonical stored field names and shape. Do not introduce a frontend “WASM widget config” that flattens, renames, or repairs existing fields. Shared Rust ingress should resolve and validate configuration once; drawing consumes typed state.

One integration detail matters: editor size sliders already use live drafts. Subscribing only to committed Zustand configuration would miss those updates.

## 3. Activity transport and on-demand sampling

Native Rust should continue finalizing activity. Upload that finalized activity snapshot once into the WASM session, replacing it when the activity changes. All preview widgets share that snapshot.

Use a Tauri command returning raw bytes through `tauri::ipc::Response`. A thin JavaScript loader receives an `ArrayBuffer`, copies it into WASM memory, and releases its reference. No additional server, WebSocket, or custom protocol is needed. Transport belongs inside the session loader, not React components.

```text
Native finalization
    -> activity ID + revision + UI metadata -> frontend store
    -> retained activity snapshot
        -> raw-byte IPC by ID/revision
        -> temporary ArrayBuffer
        -> WASM memory and decoded samples

Each preview frame: frontend previewSecond -> WASM directly
```

Raw-byte transport does not require a new binary activity schema. The initial payload may be canonical JSON bytes decoded by Rust inside WASM, avoiding a large JavaScript object graph. Choose a more compact encoding only if loading measurements justify it.

A few MB transferred once contributes to activity loading time, not per-frame playback cost. Measure transfer and decoding on a representative activity; no specific latency has been established. Streaming is deferred unless measurements show a need.

Native activity retention and export-by-ID are the intended ownership direction, not existing behavior: the current export API receives activity JSON from the frontend. Coordinate that transition with remaining frontend consumers rather than implying the speed slice removes all activity transfers.

Per frame, pass the existing canonical activity-relative previewSecond. Video synchronization stays with the frontend clock; WASM must not apply the offset again or invent its own clock.

The existing frontend binary-searches the surrounding sample timestamps and interpolates on demand. These are not necessarily whole seconds. Gap-bridging may scan outward for present samples; some metrics hold their previous value instead of interpolating.

Preserve that on-demand strategy in WASM:

- Keep original sample density. A roughly 5 MB activity at 1 Hz could become roughly 150 MB at 30 Hz; preview must never materialize that expanded dataset.
- Prepare compact lookup information once, such as valid-sample indices, without unnecessarily duplicating series.
- Cache the surrounding sample pair per required series. While playback stays within that interval, only recalculate the interpolation fraction.
- Advance the pair when crossing a sample. Use binary search for backward seeks or large jumps rather than walking through the intervening activity.
- Sample only metrics required by visible widgets, once per metric per frame, sharing the result across widgets.
- Preserve metric-specific interpolation, missing-data, and endpoint policies; supply numeric samples to shared Rust formatting and drawing.
- Do not scan, filter, allocate sample arrays, or rebuild the activity on each frame. Preview activity storage must not scale with playback FPS.

The existing Rust `interpolate_numeric_series_value` helper collects a fresh valid-point vector on every gap-bridging call. Do not use that convenience path per frame. Prepare reusable lookup data once and use allocation-free lookup/interpolation underneath it.

Export currently builds dense frame-aligned data through [activity/interpolate.rs](../../src-tauri/ovrley_core/src/activity/interpolate.rs). Preview must not generate that dense dataset. Share the underlying sampling policy, with export evaluating its frame timestamps and preview evaluating `previewSecond`. Reworking export's data pipeline is outside this slice.

This recommendation is primarily about consistent behavior and ownership. For one scalar, JS interpolation is unlikely to be the performance bottleneck. Avoiding per-frame IPC, repeated data transfers, and duplicate presentation logic is more consequential.

No-activity demonstration values and out-of-range behavior must be explicit preview policies. The existing frontend uses demonstration fallbacks; those should not accidentally become export behavior.

## 4. Standalone widgets retain caching

An independently positioned canvas can still have cached static content.

Each WASM widget instance should own:

- Validated configuration and prepared layout resources.
- Its static drawing cache.
- A reusable pixel buffer.
- Its current content bounds, visual bounds, and anchor relationship.

Rendering restores cached content and draws the dynamic portions. Configuration, font, or raster-scale changes invalidate the affected preparation; changing previewSecond does not automatically rebuild static content.

The current cache split must be preserved precisely. In [value/layout.rs](../../src-tauri/ovrley_core/src/render/widgets/value/layout.rs), a left-aligned icon can be static, whereas a right-aligned unit can be static. Other arrangements depend on changing text width. “Icons and units are always static” would be incorrect.

Shared widget code should expose preparation, static drawing, and dynamic drawing. The hosts then choose cache granularity:

- Export: retain the scene-level base layer and existing composition order.
- Preview: cache within each widget instance.

Rust must also report geometry. The frontend should not continue measuring speed text in SVG merely to size its WASM canvas. Report content width separately from visual bounds, including shadow and stroke extents, so alignment controls work and phase two has a useful interaction contract.

The frontend positions the standalone surface using that geometry. Dragging, resizing, and Moveable integration remain outside this slice.

## 5. Fonts: native discovery, shared policy, separate runtime resources

Font ownership is partly in place. [fonts.rs](../../src-tauri/ovrley_core/src/fonts.rs) already owns native discovery, capabilities, face matching, and variation selection.

However, the current byte API supplies bundled fonts only. Browser system fonts currently use CSS local(...), which cannot supply a Skia typeface inside WASM.

The intended flow is:

1. Native Rust discovers fonts and supplies catalog/capability metadata.
2. Native Rust supplies bytes and the required face identity for the selected font, including system fonts.
3. The frontend transports those resources to WASM once.
4. WASM owns its typeface cache and uses shared Rust matching/variation rules.

Native export and WASM should use the same font identity and underlying face data, but they cannot share a live typeface object.

The editor needs capabilities as well as a list—for example, weight ranges and italic availability. Widget rendering should no longer depend on browser FontFace registration or browser text measurement. Existing unmigrated previews can retain those dependencies during migration.

Font acquisition is asynchronous: preparation must prevent an older completed font request from replacing a newer selection. Unavailable font bytes should produce an explicit error.

## 6. Crate boundaries and shared implementation

Keep `src-tauri/wasm-bindings` separate from `ovrley_core`. It owns the browser ABI, WASM memory boundary, and Emscripten integration. Moving it into the native core would mix those concerns with native imports, filesystem access, and encoding. Sharing a Cargo workspace is optional; sharing the rendering implementation is the architectural requirement.

The current diagnostic is deliberately isolated from production rendering. Its wrapper creates a module instance per renderer; production should instead have one WASM module and preview session, with multiple widget handles and canvases.

Extract one platform-independent Rust widget-rendering crate, initially containing only the speed/text dependency chain:

- Canonical configuration types and validation needed by the slice.
- Activity sampling primitives and policies.
- Metric formatting.
- Font selection and text layout.
- Static/dynamic widget drawing and geometry reporting.

Native ovrley_core consumes it for speed/text export. wasm-bindings consumes it through a thin ABI. Filesystem discovery, Tauri, activity import, FFmpeg, and browser lifecycle stay outside that crate.

Upgrade native from Skia `0.97.2` to `0.153.3` before starting the slice. Align both hosts to the same pinned rust-skia revision. This supersedes the earlier proposal to support different Skia versions through target-specific dependencies. Do not add parallel drawing implementations or compatibility wrappers to disguise API differences.

Treat the native Skia upgrade as a separate prerequisite and verify existing native rendering before widget extraction. Matching source and dependency versions still requires representative rendering verification across targets. The upgrade and builds have not been performed as part of this design discussion.

The conceptual session interface is small:

```text
register_font(resource)
replace_activity(snapshot)
create_widget(config, scene, raster_scale) → handle
update_widget(handle, config, scene, raster_scale)
render(previewSecond) → pixel buffers + geometry for mounted widgets
destroy_widget(handle)
dispose()
```

A frontend hook owns synchronization and cleanup. A plain controller owns ABI calls and scheduling. The React component presents the canvas and explicit loading/error states. No widget formatting, interpolation, or layout lives in that component.

Pixel presentation must define channel order and alpha representation explicitly: the diagnostic’s opaque output does not establish correctness for transparent widget canvases.

## 7. Preview in OverlayEditor and implementation scope

The slice must run in the normal `OverlayEditor`, not a new diagnostic screen:

1. Load an activity through the existing application flow.
2. Add or select a speed widget with text presentation through the existing widget editor.
3. Display its standalone WASM canvas at its configured position in the overlay.
4. Change existing styling controls and live slider drafts; updates must appear immediately, including while paused.
5. Scrub or play the existing timeline; `previewSecond` drives the displayed speed.

Other widgets and other speed presentations keep their existing preview paths. The diagnostic under `features/wasm-preview-debug` proves the bindings work but is not the production architecture. The existing preview feature is `app/src/features/widget-preview`.

Drag-and-drop, canvas resize interactions, and react-moveable changes are deferred to the next phase. Existing editor styling controls remain in scope, including sliders that use live drafts. Rust geometry reporting is in scope so those controls do not depend on the old speed/text presentation model.

The implemented slice:

- Extract and reuse speed/text rendering in native export and WASM.
- Connect the existing editor and live drafts to a standalone WASM canvas.
- Drive values from the existing previewSecond.
- Supply Rust-owned geometry and font resources.
- Bypass the old JS speed/text presentation model.
- Preserve export static caching and existing composition behavior.

## 8. Essential verification only

The user explicitly wants only essential and crucial tests. Reuse existing coverage; add focused tests only where an important boundary is unprotected.

- Sampling: representative interpolation and missing-sample behavior, backward seeking, and preservation of original sample density.
- Rendering: representative speed/text output and alignment-dependent static caching, using existing native coverage where possible. Verify the Skia upgrade before beginning the slice.
- Integration: focused manual verification in `OverlayEditor` of activity loading, editor changes while paused and playing, font changes, and playback/scrubbing. Include transparent edges, activity replacement, and cleanup in that focused pass.

Avoid exhaustive control-by-control tests, trivial wrapper tests, tests that mirror implementation, and large new snapshot suites.

Measure representative activity transfer/decoding and the full frame path (drawing, pixel copy, canvas presentation). The existing diagnostic is not sufficient evidence for a production frame-rate guarantee. Run only checks appropriate to the change; build execution remains subject to explicit permission.

## Checkpoint — 2026-10-11

Speed/text now renders through a standalone WASM canvas in OverlayEditor. The existing widget editor sends complete effective configuration, including live slider drafts. Other presentations use the existing preview. The migrated canvas is intentionally excluded from Moveable targets and pointer dragging during this phase.

`ovrley_widgets` owns shared contracts, validation, numeric interpolation, formatting, layout, drawing, and preview sessions. The initial checkpoint used Skia 0.153.4; the manual follow-up pins both targets to release 0.153.3. The WASM crate remains a separate host/toolchain.

Each native finalization produces an immutable `preview_activity_id` (a fresh identity also denotes a new revision). The shell retains its original-density snapshot; WASM requests raw bytes once when that identity changes. The media store releases snapshots once neither the active activity nor stashed video telemetry owns them. Legacy previews still retain the existing JS activity payload. Preview/export fonts use native-resolved serialized Skia faces with included bytes and variation coordinates, including system fonts.

Verified at this checkpoint:

- Native shell `cargo check` and frontend production build, including emitted WASM assets.
- 31 existing frontend tests around editor selection, drafts, and backgrounds.
- 15 existing native typography/value tests, the shared sampling/cache tests, and one new native preview-composition seam test.
- Headless Chromium running the actual OverlayEditor, MetricWidgetEditor, and live-edit hooks with native-produced bundled/system font resources. Verified interpolation, paused slider drafts/commit, units/alignment, font replacement, activity replacement/clear, and unmount. IPC was supplied by the verification harness; this does not replace testing imports in the actual Tauri window.
- One activity resource request during playback/editor changes and two font requests when switching between two faces.

User testing checkpoint: run `pnpm dev` from the repository root in a fresh terminal. Machine-specific SDK paths live in the ignored root `.env.local` (see `.env.example`); the dev and WASM scripts load it automatically. The temporary checkpoint launcher is no longer used. Test a newly loaded activity with a speed widget in Text presentation, timeline scrubbing/playback, paused editor changes, font changes, and activity replacement/clear.

Native Skia now uses the published Windows binary for release 0.153.3, shared by `ovrley_core` and `ovrley_widgets`. It does not need Skia source, Ninja, LLVM or libclang. Representative multi-megabyte transfer/frame timings remain to measure after this checkpoint. No 30–60 fps guarantee is claimed from these checks alone.

## CHECKPOINT MANUAL TEST RESULTS

- speed widget text renders correctly in OverlayEditor, including units, alignment, and shadows
- speed widget text updates immediately when changing editor controls, including while paused or during playback
- speed widget value is correctly driven by the previewSecond and interpolated correctly
- the very first speed text widget I add has about 300-500 ms delay before it appears, subsequent are instant. Do we know why? is it the first time the WASM module is loaded?

### COMMENTS

- is trackPreviewActivityLifetime necessary? it seems supsicious. What do we need it for? we already have scaffolding for swapping activities no?
- pin rust-skia to a version with an existing prebuilt Windows binaries for both native and wasm. We will not be compiling everything from scratch
- overlay_widgets need proper folder structure. There will be dozens of widgets, so it needs to be structured by widget type. The shared files can stay in the root - if reasonable number - otherwise they need some folder structure too - please not 1-2 files per folder. The goal is to have a clear structure for the widgets and their shared files, so that it is easy to find and maintain them in the future.
- the one-liner files with orphaned files inside ovrley_core/src and ovrley_core/src/render/widgets ovrley_core/src/normalize etc that contain just pub use XXXX must be cleaned up; they are clutter
- is the widget editor somehow coalesce rapid updates to the latest state per animation frame? Do not make changes, I just want to know the status.
- there is 400 LOC of icon mapping in ovrley_widgets/src/icons.rs including hardcoded icon paths. Why cannot we simply use the icons already declared inside the manifest in assets/. This must be doable in a much more sensibel and maintainable way. Can't we generate all maps, parsed icons, paths etc programatically from the manifest?

## Manual review follow-up — 2026-10-11

The first migrated widget originally triggered `loadWidgetModule()` in `useWasmWidgets`. Loading was then moved to canvas mount, but manual testing still found a 300–500 ms first-widget delay in release. The app bootstrap now initializes the module while the existing “OVRLEY is starting” screen is visible. That screen waits for both drawer preferences and renderer readiness before mounting the editor, and startup project selection also waits for renderer readiness. Initialization failures are shown visibly. There is no separate pre-mount WASM loading screen. The initial window background matches the app background. The cached module promise is reused when widgets create their preview session. The standalone diagnostic keeps its own renderer initialization. Activity snapshot and font loading still start through the preview session. In development, the first `/wasm-bindings` asset request additionally waits for Vite's serialized bindings build. The reported delay has not been profiled into individual costs.

The preview controller already coalesces complete snapshots with one pending `requestAnimationFrame`. Each update replaces `snapshot`, and the scheduled render reads the latest state. This applies to playback and paused editor changes, including live drafts. It does not debounce until a control stops changing. Scheduling was left unchanged.

Native activity retention needs explicit release: swapping a JS activity does not free its entry in `PreviewActivities`. The store-wide `trackPreviewActivityLifetime` subscriber was removed. The existing media actions now release discarded identities after committing their transitions, retaining active and stashed video telemetry. New-project and staged-project replacement use those same owner actions. The unused direct `setParsedActivity` setter was removed.

Both hosts were tried with published Skia `0.153.3` CPU binaries. Native compilation passed, but WASM linking failed with `undefined symbol: emscripten_longjmp`. Both targets now pin release `0.153.3` / `b7f043e0b1e2a850e702245de811f01df1f6b0ff`: native uses the published binary, while WASM uses a locally built archive from the same source with native WASM exceptions. `.cargo/config.toml` selects prebuilt downloads for the release Git pin, and the native host enables `no-compile`. `pnpm wasm:skia:build` alone enables source compilation; ordinary `pnpm wasm:build` reuses the archive with `no-compile`. No exception stripping, missing-symbol suppression, or legacy longjmp shim is used.

The shared crate now has four areas:

```text
ovrley_widgets/src/
  lib.rs, error.rs, activity.rs, interpolation.rs, preview.rs
  model/          scene, domain types, manifests, common validation
  render/         text, icons, SVG geometry
  widgets/value/  config, validation, formatting, layout, frame, preview surface
```

Future widget families belong under `widgets/`. Shared drawing resources stay under `render/`, and session/activity ownership remains outside widget implementations. Seven native forwarding files were deleted; consumers import contracts and utilities from their owning shared modules.

`ovrley_widgets/build.rs` reads `assets/standard-metrics.json` and the time icon in `assets/standard-widgets.json`, deduplicates asset filenames, and generates embedded SVG resources and their cache entries. The hand-maintained icon enums, mapping matches, cache declarations, and include paths are gone. Each asset's SVG and Skia paths are parsed once and cached; drawing reuses that geometry. A missing asset fails compilation, and malformed geometry fails explicitly when first loaded.

Initial cleanup verification: 130 Rust unit tests, 16 formatting/value/preview composition tests, and 64 frontend media lifecycle and project tests passed; all core integration test targets compiled. Focused lint and formatting checks passed. The two new lifecycle tests cover native snapshot retention through stashing/restoration and release on replacement/removal. After pinning release 0.153.3, the one-time WASM archive build, ordinary bindings build, desktop `cargo check`, and all 16 focused native tests pass. Native checks use the published binary without source-tool settings; the ordinary WASM build imports the regenerated archive with `no-compile` and links without the `longjmp` error.

### TENTATIVE SCOPE OF CLEANUP AFTER FULL MIGRATION TO WASM

Yes for the rendering and geometry code. Less of lib/widget disappears than you might expect: much of it manages the editable document rather than drawing widgets.

Based on current imports, after all widget types and their interactions migrate:

| Area                                                                                                     | Expected outcome                                                                                                                                                |
| -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| features/widget-preview/                                                                                 | Its renderers, preview models, SVG generation, text measurement, sampling, and layout become obsolete. A few outside consumers need untangling first—see below. |
| overlay-editor/hooks/useOverlayPreviewModels.js                                                          | Remove entirely. Rust supplies the preview.                                                                                                                     |
| useDragHandlers, useResizeHandlers, useScaleHandlers, useRotateHandlers                                  | Replace the legacy implementations with the shared WASM interaction lifecycle, extended to support every required gesture.                                      |
| widgetRenderGeometry.js, widgetInteractionGeometry.js, widgetResizeScaling.js, widgetScaleInteraction.js | Their legacy layout/resize calculations become obsolete. Widget-specific geometry and resize rules belong in Rust.                                              |
| widgetDomHelpers.js                                                                                      | Remove direct CSS transform/layout manipulation. Small identity and draft-cleanup helpers may remain or move elsewhere.                                         |
| OverlayCanvas / OverlayEditor                                                                            | Remove the legacy rendering branch, preview-model plumbing, geometry signatures, and handler switching. Keep scene composition and editor orchestration.        |
| lib/widget/wasm-widget.js                                                                                | Remove the migration predicate once every supported widget uses WASM.                                                                                           |

Within lib/widget, mostly keep or simplify:

- Keep: widget-config, editor-state, widget defaults/catalogs, sidebar names/icons, resource identities, and display-variant editing. These support creation, editing, persistence, and undo.
- Simplify widget-draft: remove the separate CSS-layout handling and canvas rules that omit x/y or suppress scale drafts. Keep canonical data patches.
- Split widget-resolver: keep config/default/variant resolution; remove rendering geometry. It currently calls the legacy lean-angle layout.
- Trim standard-metrics and altitude: preview sampling/conversion calculations move to Rust; inspector unit choices and input conversions still have frontend consumers.

Additional cleanup candidates:

- lib/geometryUtils.js: its SVG/path and legacy transform helpers should become unnecessary.
- overlayEditorUtils.js: remove widget sampling/interpolation and preview-value calculations; keep scene/grid helpers.
- exportRange.js: remove route sampling/progress calculations used only by previews; keep export-range controls.
- Browser font preparation: remove widget-rendering readiness gates from add/edit, project/template loading, and scene settings. Keep font catalogs and typography controls.
- Remove corresponding obsolete tests, preview-only styles/assets, and any dependencies left unused.

One current complication: widget-preview also serves non-preview consumers. Inspector controls import gauge geometry and formatting options; video-sync imports speed formatting and browser-font helpers. Those consumers must be moved to the appropriate owner or switched to Rust before deleting the directory. Video-sync also still uses JS interpolation, so lib/interpolation.js does not automatically disappear.

Selection, keyboard shortcuts, viewport zoom/pan, Moveable, draft transactions, undo/redo, playback, and inspector controls remain frontend responsibilities.

Assuming every widget and supported gesture has migrated, here is the complete overview for both folders under app/src/features/overlay-editor/.

| File                                  | Outcome             | Changes                                                                                                                                                                                                                                 |
| ------------------------------------- | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| hooks/useDragHandlers.js              | Replace             | Shared WASM interaction handlers own drag intent, axis locking, and commit/cancel. Remove direct CSS movement.                                                                                                                          |
| hooks/useResizeHandlers.js            | Replace             | Frontend forwards requested geometry; Rust owns intrinsic field changes, constraints, and anchor correction.                                                                                                                            |
| hooks/useScaleHandlers.js             | Replace             | Remove bitmap/CSS scaling. Preserve supported single/group scaling through canonical field updates and shared interaction policies.                                                                                                     |
| hooks/useRotateHandlers.js            | Replace             | Forward rotation intent through the shared lifecycle; Rust supplies resulting drawing and bounds. Rotation support must exist before removal.                                                                                           |
| hooks/useEditorKeyboard.js            | Keep                | Copy/paste, delete, nudge, grid, and zoom remain editor actions. Ensure Escape cooperates with gesture cancellation.                                                                                                                    |
| hooks/useEditorViewport.js            | Keep                | Viewport measurement, scrolling, fit scale, and cursor-anchored zoom remain frontend responsibilities.                                                                                                                                  |
| hooks/useOverlayEditorState.js        | Simplify            | Keep effective config, widget identities, node registration, drafts, and commits. Collapse the separate inspector/canvas draft representations once CSS-layout drafts disappear.                                                        |
| hooks/useOverlayPreviewModels.js      | Remove              | Rust replaces metric/text/lap preview preparation and sampling.                                                                                                                                                                         |
| hooks/useWidgetDraftState.js          | Simplify            | Keep temporary canonical patches, subscriptions, RAF-coalesced publication, and transaction lifecycle. Remove the legacy layout payload.                                                                                                |
| hooks/useWidgetSelection.js           | Keep                | Selection IDs, primary selection, group selection, Moveable targets, and snapping guidelines still use registered wrappers. Their bounds come from Rust.                                                                                |
| utils/createOverlayPointerHandlers.js | Keep                | Canvas clicks, modifier selection, marquee selection, and pointer routing remain frontend behavior. Hit testing uses Rust-positioned wrappers. This file already implements a hook; its placement could be corrected separately.        |
| utils/exportRange.js                  | Split/trim          | Keep time-input parsing, range editing, and duration/window helpers needed by export controls. Move preview route sampling and distance-progress calculations to Rust; remove unused JS implementations.                                |
| utils/overlayEditorHelpers.js         | Trim                | Keep modifier detection, selection rectangles/intersection, selection ordering, and primary-ID helpers. Remove getWidgetSceneOrigin when Rust geometry replaces its callers.                                                            |
| utils/overlayEditorUtils.js           | Split/trim          | Keep scene-size and editor-grid helpers. Widget sampling, time interpolation, altitude correction, and progress calculations move to Rust. Existing video-sync consumers need relocation or migration before deleting shared functions. |
| utils/widgetDomHelpers.js             | Mostly remove       | Delete CSS layout/style application and legacy visual-bound reconstruction. Move any surviving identity or draft-cleanup helpers to their actual owners.                                                                                |
| utils/widgetInteractionGeometry.js    | Remove              | CSS-derived origins, transform construction, and temporary drag/resize/rotation layouts are replaced by session gesture origins and Rust geometry.                                                                                      |
| utils/widgetRenderGeometry.js         | Remove              | Rust supplies render/selection bounds; presentation events refresh Moveable. JS geometry models and signatures disappear.                                                                                                               |
| utils/widgetResizeScaling.js          | Move policy to Rust | Move widget-specific sizing rules, limits, aspect constraints, and intrinsic field scaling. Any remaining UI handle-capability metadata should live separately.                                                                         |
| utils/widgetScaleInteraction.js       | Replace             | Shared interaction lifecycle captures origins and commits patches; Rust handles widget-specific scale results. Preserve group behavior before removing it.                                                                              |

“Replace” means the current implementation becomes obsolete, while the interaction itself remains. The surviving editor code should manage user intent and document state; Rust should determine widget appearance and geometry.

## IMMEDIATE NEXT STEPS TO FULLY FINISH METRIC WIDGET MIGRATION

Yes—for ordinary numeric metrics displayed as text, those are the main changes, plus formatting and resize measurement.

| Change                                                                                                              | Where                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Use config value to select the metric’s activity column and interpolation policy, instead of hardcoded speed        | ovrley_widgets/src/activity.rs and preview.rs                                                           |
| Share each metric’s existing unit conversion, precision, icon, and missing-value formatting between native and WASM | Move remaining relevant logic from ovrley_core/src/render/format.rs into shared widgets/value/format.rs |
| Remove the speed-only restriction; render the selected metric                                                       | widgets/value/preview.rs                                                                                |
| Measure the selected metric during resizing, so its current text determines the fixed-corner correction             | widgets/value/interaction.rs                                                                            |
| Route supported text metrics through WASM                                                                           | Widen app/src/lib/widget/wasm-widget.js:isWasmWidget                                                    |

The canvas already uses that predicate, as do the legacy-preview exclusions. No new canvas branch, binding, or gesture implementation is needed.

However, all metric values includes special cases: gear is textual, GPS uses coordinate pairs and potentially multiple lines, altitude has correction, and some metrics have special formatting or missing-data rules. GPS in particular needs extending the current WASM frame/bounds path, which assumes ordinary value/unit text.

So power, heart rate, cadence, etc. are straightforward extensions. Supporting every metric requires carrying over those existing special behaviors too:

Most special formatting still lives in ovrley_core/src/render/format.rs (src-tauri/ovrley_core/src/render/format.rs).

| Rule                                                                                               | Current owner                                                                                                                  |
| -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Metric selection, gear strings, GPS coordinate pairs, prefixes/suffixes                            | format_metric_presentation_parts                                                                                               |
| Units, precision, temperature, pace, balance, shutter/aperture, distance/ascent totals             | format_validated_standard_metric_parts and its helpers                                                                         |
| Missing value → zero for metrics with a preserve policy; entirely unavailable series stays missing | resolve_metric_display_value                                                                                                   |
| Starting-altitude correction                                                                       | altitude_offset_m and raw_value; offset prepared in render/widgets/mod.rs (src-tauri/ovrley_core/src/render/widgets/mod.rs:49) |
| GPS coordinate formats and placeholders                                                            | format_coordinates, format_coordinate_line, format_coordinate_placeholder                                                      |
| Time-of-day, elapsed time, timezone/date formats                                                   | format_validated_time_parts and time helpers                                                                                   |
| Gradient sign/precision                                                                            | format_validated_gradient                                                                                                      |

Some relevant rules live outside that file:

| File                                                                                                  | Responsibility                                                                                                |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| ovrley_core/src/activity/interpolate.rs (src-tauri/ovrley_core/src/activity/interpolate.rs)           | Native sampling: bridge gaps, preserve gaps, hold values, interpolate coordinates/timestamps.                 |
| ovrley_widgets/src/model/standard_metrics.rs (src-tauri/ovrley_widgets/src/model/standard_metrics.rs) | Reads each metric’s formatter, data source, and interpolation policy from the shared manifest.                |
| ovrley_widgets/src/interpolation.rs (src-tauri/ovrley_widgets/src/interpolation.rs)                   | Already-shared numeric interpolation primitives.                                                              |
| ovrley_widgets/src/widgets/value/layout.rs (src-tauri/ovrley_widgets/src/widgets/value/layout.rs:150) | Already-shared GPS multiline drawing. Its geometry still needs integration into the WASM prepared-frame path. |
| ovrley_widgets/src/widgets/value/format.rs (src-tauri/ovrley_widgets/src/widgets/value/format.rs)     | Already-shared speed formatting and display-content types.                                                    |

For migration, extract the remaining formatting rules into the shared value module, with samples and required activity metadata as inputs. Native export and WASM should supply those inputs through their respective sampling paths.

### Metric text implementation — 2026-10-12

The standard metric `text` slice now uses the shared renderer for every supported
metric, including gear and GPS. Formatting, conversions, totals, affixes, and
coordinate formatting live in `ovrley_widgets/widgets/value/format.rs`; native
export supplies dense samples to that same implementation. Specialized time and
gradient widgets and all other display types remain outside this migration.

The preview sampler prepares canonical numeric columns and bridge/hold points
once per finalized activity. It handles preserve gaps, barometric altitude
selection/correction, coordinate pairs, gear labels, and full-activity totals.
Unavailable preview samples produce placeholders; an out-of-activity playhead
does not reuse the last reading. Numeric hold values now also remain held at
native export trim boundaries instead of being linearly blended there.

GPS drawing and selection now consume the same prepared text positions and
bounds. Rendering and intrinsic resize both sample the selected metric at
`previewSecond`. The existing frontend session, Moveable transactions, and
legacy-renderer exclusions are reused through `isWasmWidget`.

Verification includes all manifest text metrics and display units against native
sampling, formatting, and pixels; the existing speed resize and frontend gesture
suites remain in use. The shared/core unit and focused integration suites pass
(179 tests), as do the focused frontend suites (23 tests), frontend lint, and
diff checks. The user elected to run the build separately; actual WASM compilation
and browser interaction verification remain unverified in this session.
