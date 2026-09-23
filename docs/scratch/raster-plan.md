# Raster Widget Implementation Plan

## Basis and outcome

Implement the behavior in [`raster-spec.md`](./raster-spec.md) as a first-class static widget across the manifest, template pipeline, editor, project archive, and Rust renderer.

The finished implementation must provide one canonical persisted raster config, one resource-ingress validator/decoder, recoverable document loading, and strict render submission. It must not introduce compatibility aliases, per-consumer coercion, browser-only rendering behavior, or path fallbacks for embedded project assets.

No styling work beyond fitting the existing controls, placeholder, and error surfaces is part of this plan.

## Target architecture

### Durable widget config

The only durable widget shape is:

```json
{
  "id": "stable-widget-id",
  "x": 0,
  "y": 0,
  "width": 640,
  "height": 480,
  "rotation": 0,
  "opacity": 1,
  "path": null
}
```

It lives in `config.rasters`. Do not add `type`, `display_type`, MIME, extension, intrinsic dimensions, preview URL, archive path aliases, load status, or error text to this record. The presentation wrapper produced by `buildConfigWidgets` supplies `type: 'raster'` and `category: 'rasters'` just as it does for the other collections.

### Runtime resource binding

Introduce a document-scoped raster resource registry outside widget config. It binds a raster widget ID and history revision to:

- immutable original encoded bytes;
- supported source format/extension;
- final oriented intrinsic width and height;
- a browser-displayable preview resource;
- a render-stage resource containing the same successfully loaded image;
- a concrete load error when the binding is unresolved.

The registry is resource state, not a second widget model. Store consumers should access it through selector hooks rather than reading the Zustand store directly in leaf components.

The registry must own bytes after a successful load. External files are read only at ingress, not watched or reopened for later editor paints, project saves, or renders in the same session. Object URLs and staged temporary files must be revoked/removed when no reachable current or undo/redo state owns them and when the document closes.

### Source resolver boundary

Use one owner to translate durable `path` semantics into a runtime resource:

- Template: `path` is an absolute external path; ingress reads it once.
- Project: `path` identifies the raster's owned archive entry; ingress reads bytes from `.oly` and never consults an external path.
- Render: a Tauri adapter stages registry-owned bytes to a controlled temporary absolute path, or passes bytes through an explicit prepared-resource API, before calling the standalone core. Any temporary config rewrite happens once at this external boundary and is never persisted.

The standalone core continues to accept absolute raster paths so rendering a standalone template does not depend on Tauri session state.

### Decode owner

Create a Rust image-ingress module in `ovrley_core` and reuse it from Tauri commands and render preparation. It owns, in this order:

1. encoded-size enforcement (5 MiB);
2. extension/declared-format acceptance;
3. byte-signature/decode validation;
4. orientation metadata application;
5. oriented pixel-count enforcement (25,000,000 pixels);
6. production of oriented decoded pixels and metadata.

Prefer a narrowly configured Rust image decoder with only PNG, JPEG, BMP, and TIFF support. If `image` is used, enable only those codecs and use its decoder orientation support; add another metadata dependency only if fixture tests prove one of the required codecs cannot apply orientation correctly. The core must return typed/domain-specific failures that the Tauri layer maps to stable user-facing error codes. Do not duplicate these checks in React or project-file consumers.

For editor preview, return a browser-compatible PNG preview generated from the already-oriented decode because browser support for TIFF/BMP cannot be assumed. Persist and archive the untouched original bytes, not the preview PNG.

## Phase 1: define contracts, manifests, and versions

### 1.1 Standard widget manifest

Update `assets/standard-widgets.json` with one raster definition:

- `type: "raster"`;
- widget name and short-name translation keys;
- category `general`;
- defaults containing only `x`, `y`, `width`, `height`, `rotation`, `opacity`, and `path`;
- initial editor placeholder dimensions selected from existing sensible frame defaults;
- `path: null`.

Export `RASTER_DEFAULTS` from `app/src/lib/widget/standard-widgets.js`. Extend `src-tauri/ovrley_core/src/standard_widgets.rs` only where Rust must consume or validate the raster defaults/metadata; keep `assets/standard-widgets.json` canonical.

### 1.2 Template version

Update `assets/standard-template.json` from file version 2 to 3 and add an empty `rasters` collection to canonical default config/fixtures.

Replace exact-current-version rejection in `app/src/features/template-manager/utils/templateSnapshot.js` with an explicit migration dispatcher:

- v2 input -> clone, set version 3, add `config.rasters = []`, then normalize;
- v3 input -> normalize directly;
- older, newer, malformed, or missing versions -> fail with a specific unsupported-version error.

Mirror the same v2-to-v3 migration at standalone Rust template ingress in `src-tauri/ovrley_core/src/normalize/raw/mod.rs`. Rust must deserialize the version envelope before deserializing the v3 render body so version migration remains explicit.

Update every bundled template under `templates/` to v3 with `rasters: []`; do not rely on migration for files shipped by the current app.

### 1.3 Project version and archive namespace

Update `src-tauri/src/project_file.rs` from project version 1 to 2. Define one canonical archive path format, for example:

```text
rasters/<widget-id>.<original-supported-extension>
```

The archive path is derived by the project owner from widget ID and validated source format. Frontend callers must not construct it independently.

Implement a versioned project parser:

- v1 -> migrate the project document to v2 and add/normalize an empty raster collection;
- v2 -> validate directly;
- unsupported versions -> reject.

The project JSON written inside the archive uses the v2 document version and records each populated raster's canonical owned archive path in `path`. A null path is valid for an editor placeholder but produces no archive entry.

### 1.4 Contract tests first

Before feature wiring, add tests that pin:

- manifest metadata/defaults and field set;
- canonical collection ordering (`backdrops`, `rasters`, then existing widgets);
- template v2 migration and v3 round-trip;
- project v1 migration and v2 archive path rules;
- unsupported future-version rejection;
- stripping of unknown raster fields at document normalization;
- strict rejection of malformed raster fields at render deserialization/validation.

## Phase 2: image ingress and runtime resource ownership

### 2.1 Core raster asset module

Add a focused module such as `src-tauri/ovrley_core/src/raster_asset.rs` containing:

- shared constants for 5 MiB and 25 MP;
- the supported format enum and canonical extension mapping;
- `inspect/decode bytes` and `load absolute path` entry points;
- oriented intrinsic metadata;
- an immutable decoded image representation suitable for Skia preparation;
- structured error variants for missing, unreadable, unsupported, oversize, over-resolution, and decode/orientation failure.

Check encoded length before allocating/decoding. Read dimensions and orientation before allocating a full pixel buffer when the decoder permits it; reject over-25-MP images at the earliest reliable point. Use checked multiplication for pixel count and decoded buffer dimensions.

### 2.2 Tauri ingestion commands

Add narrow commands and wrappers in `src-tauri/src/` and `app/src/api/backend.js`:

- import an external raster by absolute selected path;
- register an embedded project raster from archive bytes;
- create/return a browser preview URL or PNG payload and intrinsic metadata;
- stage or resolve registered bytes for render;
- release document/history-owned raster resources.

The import command reads and validates before mutating the registry. Return a stable error code plus safe details; translation occurs in the frontend. Do not return a successful partial record.

Avoid routing raster import through the generic `read_selected_file_bytes` API, because doing so would duplicate validation and make browser code responsible for image format semantics.

### 2.3 Registry and history integration

Add a raster resource slice or a feature-owned registry controller with selector hooks. Required operations:

- bind successful external import to widget ID;
- bind successful project archive import to widget ID;
- record unresolved resource errors from recoverable document load;
- clone a binding for a duplicated widget under its new ID;
- remove current ownership when a widget is deleted;
- restore bindings on undo/redo;
- replace a binding transactionally;
- expose preview source, intrinsic dimensions, and error for presentation;
- enumerate current owned assets for project save and render preflight;
- release unreachable resources.

Extend the existing history checkpoint owner so config mutation and resource-binding mutation are one undoable transaction. Do not put byte arrays, preview URLs, error strings, or registry revision identifiers into raster config.

### 2.4 Picker transaction

Add a raster-specific picker filter in `app/src/lib/file-dialog.js` for `png`, `jpeg`, `jpg`, `bmp`, and `tiff`.

Implement selection/replacement in a reusable hook owned by the raster editor feature:

1. open picker;
2. if cancelled, do nothing;
3. ask backend to ingest the selected absolute path;
4. on failure, show the mapped error and leave config/resource state untouched;
5. on success, commit one history transaction that binds the returned resource and sets `path` to the absolute selected path plus exact oriented `width`/`height`.

Do not clamp intrinsic dimensions to the scene.

## Phase 3: frontend config and widget lifecycle

### 3.1 Canonical config utilities

Update `app/src/lib/widget/widget-config.js` so every fixed collection operation includes `rasters`:

- identity assignment;
- flatten/find/update/replace/delete;
- duplicate behavior;
- any ordering or iteration helpers.

Update `app/src/lib/widget/widget-presentation.js` to emit raster presentation records immediately after backdrops. Update grouping/name/icon helpers and tests to recognize `type: 'raster'` and `category: 'rasters'` without aliases.

Update `app/src/lib/widget/display-type-behavior.js` so raster is a framed widget. Add a dedicated capability for raster's mixed resize behavior; do not pretend it has the same all-handles ratio policy as another widget type.

### 3.2 Document normalization

Add `RASTER_KEYS` and `normalizeRaster` to:

- `app/src/lib/template/template-constants.js`;
- `app/src/lib/template/template-normalization.js`;
- `app/src/lib/template/template-state.js` where effective config collections are materialized.

At document ingress, normalize geometry and known keys using the established template rules, ensure IDs once, and remove unknown fields. Keep `path: null` for a placeholder. For populated paths, require a string and defer resource resolution to the resource-ingress orchestrator. A malformed present field follows the project's existing documented correction policy; no downstream component repeats coercion.

Keep resource validity separate from structural normalization: a structurally normalized raster can remain in the editor with an unresolved binding and placeholder.

### 3.3 Create, duplicate, delete, and reset

Extend `app/src/features/widget-editor/utils/widgetUtils.js` and `useWidgetManager.js`:

- add creates defaults and selects the placeholder without opening the picker;
- duplicate clones config under a new ID and copies the immutable owned bytes into a separately owned registry binding;
- delete removes the config and current resource ownership in the same history transaction;
- reset returns geometry/opacity/path to raster defaults and releases current ownership, consistent with the meaning of reset for this widget.

Make asset lifecycle operations the responsibility of the widget manager/container hook, not presentational controls.

### 3.4 Drawer/catalog and icon

Expose Raster in the standard widget catalog/drawer using existing metadata-driven mechanisms. Reuse an icon from the established icon system if a semantically correct one exists; otherwise add one repository-native SVG asset. Do not use generated bitmap artwork.

## Phase 4: raster editor and JSX preview

### 4.1 Editor controls

Add a raster editor branch/component in `app/src/features/widget-editor/` containing only:

- image path/status display;
- Browse/Replace action;
- width;
- height;
- rotation;
- opacity;
- concrete load error and replacement affordance.

The component remains presentational. Picker orchestration, async state, transactions, and error mapping live in hooks.

### 4.2 Preview component

Add `app/src/features/widget-preview/widgets/raster/RasterPreview.jsx` and dispatch it from `WidgetPreview.jsx`.

Behavior:

- resolved binding -> render the backend-generated browser preview stretched to `100% x 100%`;
- unresolved/null binding -> partially opaque white rectangle;
- no crop or letterbox (`width: 100%`, `height: 100%`, stretch semantics);
- apply widget/global opacity through the same geometry/opacity ownership used by framed widgets;
- preview component never reads files or validates config.

Pass resource presentation data explicitly from the overlay container/model hook. Avoid a direct store subscription in `RasterPreview`.

Update the custom memo comparator in `WidgetPreview.jsx` and `OverlayCanvas.jsx` so raster repaint depends on its resource binding but not `previewSecond`.

### 4.3 Layer order

In `app/src/features/overlay-editor/components/OverlayCanvas.jsx`, assign raster a z-index above backdrop and below plots/values/labels. Prefer a named layer-order map in feature data over another chain of category-specific class conditionals, while preserving the existing relative order of all non-raster categories.

Ensure selection outlines/Moveable controls remain above widget content and are not trapped by raster stacking contexts.

### 4.4 Mixed resize policy

`react-moveable` currently applies one `keepRatio` value to every handle. Implement raster behavior explicitly:

- edge handles remain available and free-axis;
- at resize start, inspect the active direction;
- for a corner direction, capture `width / height` from the displayed frame at drag start and constrain updates to that ratio;
- for a horizontal or vertical edge, update only the corresponding axis;
- preserve opposite-anchor translation and rotation behavior through existing resize utilities;
- round/commit once at resize end using existing draft/history behavior.

Do not derive corner ratio from intrinsic image dimensions after the user has stretched the frame; use the current displayed ratio. Add pure geometry tests for all four corners, four edges, rotated widgets, global scale, and minimum size.

## Phase 5: project archive v2 and hydration

### 5.1 Write API

Change the frontend/backend project-save contract from only `projectJson` to a project save request that includes the current raster resource ownership manifest. The manifest identifies widget IDs and backend registry handles; it does not resend or duplicate widget geometry.

In `src-tauri/src/project_file.rs`:

1. parse/migrate and structurally validate project JSON;
2. enumerate `editor.config.rasters`;
3. require exactly one current registered asset for each non-null raster path;
4. derive each safe archive entry from widget ID and validated original extension;
5. rewrite the archive copy of each populated raster `path` to that canonical entry;
6. reject supplied assets with no matching raster and duplicate widget IDs/entries;
7. calculate/enforce project JSON, thumbnail, per-image 5 MiB, and total 40 MiB limits;
8. write `project.json`, optional thumbnail, and each original raster byte stream to a temporary archive;
9. verify/finalize the archive and atomically replace the target.

The save response should return the canonical persisted project representation/bindings needed to refresh the frontend dirty baseline. This avoids comparing runtime external paths with archive paths after save.

Compression may remain deflate, but limits are based on the original uncompressed raster entry sizes and total archive policy defined by the existing project owner. Do not allow compression ratio to bypass the 5 MiB per-image limit.

### 5.2 Read API

Read v2 archives in two passes:

1. inventory raw entries and reject unsafe names, directories where files are required, duplicate names, duplicate `project.json`/thumbnail, unsupported namespaces, and entries over their limits;
2. parse/migrate project JSON, derive the exact expected raster entries from normalized config, and match one-to-one.

For each expected raster entry:

- read at most 5 MiB plus one byte;
- run the shared decoder/limit validator;
- on success, register original bytes and return the binding metadata;
- on missing/corrupt/invalid asset, attach a recoverable raster load issue and continue hydrating the rest of the project.

Unexpected or unreferenced raster entries are structural archive errors and reject the archive. A referenced but damaged raster resource is a recoverable widget issue. This distinction keeps malformed archive structure strict while allowing the rest of a valid project to open when image content is unusable.

### 5.3 Frontend snapshot/hydration

Update:

- `app/src/features/projects/utils/projectSnapshot.js`;
- `app/src/features/projects/utils/projectHydration.js`;
- `app/src/features/projects/utils/projectOperations.js`;
- project lifecycle hooks and command wrappers.

Hydration must commit normalized config and all successful/unresolved raster bindings as one document transaction. It reports per-widget raster issues after the rest of the editor state is available. It must not replace an archive path with an old external source path.

Project dirty-state snapshots should compare canonical durable config plus resource ownership/revision, not object URLs or temporary staging paths. Saving a replacement marks the new binding clean only after atomic save succeeds.

### 5.4 Project tests

Add Rust archive tests for:

- v1 migration;
- v2 round-trip preserving exact original bytes and extension;
- replacement overwriting one widget-owned entry;
- deletion removing stale entries;
- duplication producing two entries;
- missing/corrupt referenced image returning a recoverable issue;
- unreferenced, duplicate, traversal, absolute, and malformed raster entries rejecting the archive;
- per-image 5 MiB and archive 40 MiB limits;
- failure before atomic replace preserving the prior archive.

Add frontend tests for hydration bindings, error presentation, dirty baseline, Save/Save As, replacement, duplication, deletion, and undo/redo.

## Phase 6: strict Rust render contract and static cache

### 6.1 Raw and validated config

Add a raw `RasterConfig` with required, non-optional fields to `src-tauri/ovrley_core/src/normalize/raw/mod.rs`. Add `rasters` to `RenderConfig`. For template-v3 render config, the collection itself is required by the v3 contract; migration supplies it for v2 templates before deserialization. Do not use `serde(default)` to hide a missing v3 collection.

Add `src-tauri/ovrley_core/src/normalize/raster.rs` with `ValidatedRaster` and strict validation:

- non-empty ID;
- finite x/y/rotation;
- finite positive width/height;
- opacity in the canonical accepted range;
- non-empty absolute path for submitted render config;
- no nullable render path.

Add rasters to `ValidatedRenderConfig` and `PreparedRenderAssets`. Preserve collection order.

Structural validation and resource loading remain separate named steps, but both happen before any frame is rendered. Error paths should include `rasters[index]` and widget ID when available.

### 6.2 Prepare once

During render preparation:

- load bytes from each validated absolute path;
- run the shared 5 MiB/format/decode/orientation/25 MP pipeline;
- convert the oriented decoded result to a Skia image once;
- keep that image in prepared render assets;
- fail the render job immediately if any raster cannot be prepared.

No draw function may reopen the path or decode bytes. No unresolved raster enters `PreparedRenderAssets`.

### 6.3 Static-layer composition

Add a raster draw module under `src-tauri/ovrley_core/src/render/widgets/`. Draw each prepared raster in collection order with:

- scene/global scale applied consistently with framed widgets;
- its frame width/height as the destination rectangle;
- center/origin rotation matching editor geometry;
- raster opacity combined with scene/global opacity according to existing conventions;
- stretch sampling, with no crop/fit mode.

Change `src-tauri/ovrley_core/src/render/static_layer.rs` composition to:

1. backdrops;
2. prepared rasters;
3. labels and existing static metric parts.

Include raster geometry, opacity, and immutable resource identity/content revision in the static cache key. Prefer a render-job-owned static image cache over a process-global cache retaining large raster images indefinitely. If the existing global static cache remains, bound/evict it and ensure it cannot return stale pixels for a replaced file with the same path.

The final static composited image is reused for each elapsed frame. Do not add per-frame raster draw calls after the cached layer is painted.

### 6.4 Render preflight in the app

Before invoking Rust preview/export, the frontend render workflow asks the document resource owner to resolve every config raster. Null path or unresolved binding produces a blocking localized error before job startup. The Tauri boundary stages the exact registry-owned bytes for the job and supplies a strict absolute-path render config to core.

Core validation remains authoritative and repeats no frontend coercion: direct callers and standalone template rendering must receive the same loud failures.

### 6.5 Render tests

Add Rust tests for:

- every supported format;
- extension/decoded-format mismatch policy;
- missing/unreadable/corrupt file;
- 5 MiB boundary;
- 25 MP boundary and multiplication overflow;
- oriented dimension swap fixtures;
- required fields, absolute path, finite geometry, positive dimensions, and opacity;
- backdrop/raster/other-widget pixel compositing order;
- raster collection order;
- rotation, opacity, scaling, and intentional non-uniform stretch;
- preparation decoding once and static-layer reuse across multiple frame times;
- replacement content invalidating the cache even when logical path is unchanged;
- no editor placeholder in Rust output.

Use tiny committed fixtures for each codec and generate large/limit fixtures in test setup where practical to avoid bloating the repository.

## Phase 7: errors, translations, and cleanup

### 7.1 Stable error mapping

Define stable raster error codes at the Tauri boundary. Map codes to i18n keys in one frontend utility. Error details may include filename, actual byte size, dimensions/pixel count, and limits, but React components must not parse Rust prose.

Cover at least:

- missing file;
- unreadable file;
- unsupported format;
- decode failure;
- over 5 MiB;
- over 25 MP;
- missing/corrupt embedded asset;
- unresolved placeholder at render;
- project over 40 MiB.

### 7.2 Locales

Add raster widget name/short name, picker labels, placeholder/status text, Browse/Replace actions, and all error messages to `app/src/i18n/locales/en-translation.json`, then translate them contextually in every other locale. Update locale parity tests. Error translations must preserve interpolated values and units.

### 7.3 Resource cleanup

Exercise document replacement, project close, new project, template load, undo-history truncation, widget deletion, failed replacement, cancelled picker, render completion/cancellation, and app shutdown. Every owned object URL, decoded preview, registry entry, and staged temporary render file must have one explicit owner and cleanup path.

## File-level change map

Expected existing files to modify include:

- `assets/standard-widgets.json`
- `assets/standard-template.json`
- all current `templates/*.json`
- `app/src/lib/widget/standard-widgets.js`
- `app/src/lib/widget/widget-config.js`
- `app/src/lib/widget/widget-presentation.js`
- `app/src/lib/widget/display-type-behavior.js`
- `app/src/lib/template/template-constants.js`
- `app/src/lib/template/template-normalization.js`
- `app/src/lib/template/template-state.js`
- `app/src/lib/file-dialog.js`
- `app/src/api/backend.js`
- `app/src/features/widget-editor/hooks/useWidgetManager.js`
- `app/src/features/widget-editor/utils/widgetUtils.js`
- widget editor dispatch/section files under `app/src/features/widget-editor/components/`
- `app/src/features/widget-drawer/components/WidgetDrawer.jsx` if manifest discovery alone is insufficient
- `app/src/features/widget-preview/WidgetPreview.jsx`
- `app/src/features/overlay-editor/components/OverlayCanvas.jsx`
- `app/src/features/overlay-editor/components/OverlayMoveable.jsx`
- `app/src/features/overlay-editor/hooks/useResizeHandlers.js`
- `app/src/features/overlay-editor/hooks/useOverlayEditorState.js` or its resource/model owner
- project snapshot/hydration/operation files under `app/src/features/projects/`
- render config/workflow files under `app/src/features/render-video/`
- all locale JSON files and locale parity tests
- `src-tauri/Cargo.toml` and lockfile if the shell requires registry support
- `src-tauri/src/lib.rs`
- `src-tauri/src/project_file.rs`
- `src-tauri/ovrley_core/Cargo.toml` and lockfile for decoder support
- `src-tauri/ovrley_core/src/lib.rs`
- `src-tauri/ovrley_core/src/standard_widgets.rs`
- `src-tauri/ovrley_core/src/normalize/raw/mod.rs`
- `src-tauri/ovrley_core/src/normalize/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/mod.rs`
- `src-tauri/ovrley_core/src/render/widgets/types.rs`
- `src-tauri/ovrley_core/src/render/static_layer.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`

Expected focused new files include:

- frontend raster editor component and hook;
- frontend raster preview component;
- frontend resource/error utilities or a store slice plus selector hook;
- core raster asset loader/decoder;
- core raster validator;
- core raster draw/preparation module;
- targeted frontend and Rust test files/fixtures.

Use actual ownership discovered during implementation; do not create parallel modules when an existing owner already has the responsibility.

## Verification sequence

Run narrow tests after each phase, then the full supported suites. Do not run a production build without explicit user permission.

1. Manifest/template normalization tests.
2. Raster resource decoder and validation unit tests.
3. Widget config, presentation, editor geometry, preview dispatch, and picker transaction tests.
4. Project archive and hydration tests.
5. Rust render/static-cache tests.
6. Full frontend suite from `app/`: `npx vitest run`.
7. Frontend lint from repository root: `pnpm lint`.
8. Core tests: `cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml`.
9. Tauri project archive tests: `cargo test --manifest-path src-tauri/Cargo.toml project_file` (or the narrow package/test selector matching the final module layout).
10. Manual editor parity pass with PNG, JPEG, BMP, TIFF, missing files, corrupt files, replacement, undo/redo, save/reopen, and preview/export.

Do not run `pnpm build`, `pnpm tauri build`, or the root production build wrapper unless the user separately authorizes it.

## Completion gates

The work is complete only when all of these are true:

- The manifest and every current durable file use the new versions and canonical `rasters` collection.
- Old supported versions migrate through explicit version branches.
- Adding a raster yields a selected placeholder without opening a picker.
- Successful import owns original bytes and resets dimensions exactly; failed replacement changes nothing.
- Corner and edge handles satisfy their different resize contracts.
- JSX and Rust layer order agree.
- Rust decodes rasters during preparation and reuses the static layer across elapsed frames.
- Template missing-resource and project damaged-resource cases open the rest of the document with actionable raster errors.
- Every unresolved or malformed raster blocks final preview/export loudly.
- `.oly` v2 round-trips exact original bytes per widget and accounts for them under 40 MiB.
- External source changes after successful ingress do not alter the session image.
- All user-facing strings exist in every supported locale.
- Focused tests, full frontend tests, lint, and Rust tests pass; no production build is run without permission.
