# WASM widget vertical slice

Migrate speed with `display_type: text` in the actual `OverlayEditor`. Keep the editable document in Zustand; shared Rust code owns sampling, formatting, layout, and drawing. WASM receives original finalized activity samples when the activity changes and `previewSecond` when rendering.

This document records the architecture and scope agreed during discussion. Implementation still requires explicit user approval. Builds require separate permission under `AGENTS.md`. Updating this document does not authorize either.

Prerequisite: migrate native rendering to Skia `0.153.4` and verify existing rendering before starting the widget slice.

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

Upgrade native from Skia `0.97.2` to `0.153.4` before starting the slice. Align both hosts to the same pinned rust-skia revision where applicable. This supersedes the earlier proposal to support different Skia versions through target-specific dependencies. Do not add parallel drawing implementations or compatibility wrappers to disguise API differences.

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

After approval, the slice would:

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
