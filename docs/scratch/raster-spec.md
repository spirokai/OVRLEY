# Raster Widget Specification

## Status

Agreed product and architecture specification. This document defines behavior; implementation sequencing belongs in `raster-plan.md`.

## Purpose

Add a first-class raster widget that places a user-supplied bitmap in an overlay. A raster is static media: it participates in editor geometry and compositing, but it does not vary with activity time.

## Canonical model

Raster widgets live in a new top-level `rasters` collection alongside `backdrops`, `plots`, `values`, and `labels`.

Every raster has exactly these persisted fields:

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

- `id`, `x`, and `y` are required widget infrastructure fields.
- `width`, `height`, `rotation`, `opacity`, and `path` are the raster-specific editable fields requested by the product.
- `path` is nullable only for an editor placeholder that has never received an image.
- There is one canonical raster shape. Runtime resource handles, decoded pixels, preview URLs, and archive bookkeeping are resource state, not alternate widget config shapes.
- Normalized render input requires every field to have the correct type and valid range. Consumers must not repair malformed submitted render config.

## Source semantics

### Standalone templates

- A populated `path` is an absolute filesystem path.
- Relative image paths are invalid.
- Loading a template resolves and validates its raster resources once. A successfully loaded image remains the session's image; the app does not watch, fingerprint, or silently refresh it when the external file changes.

### Projects (`.oly`)

- Each raster owns its own embedded archive asset, keyed by the raster widget ID.
- The embedded copy is the sole source of truth after project load. There is no fallback to the original external path.
- The original encoded bytes and original supported format are preserved. Images are not transcoded for persistence.
- Duplicate rasters own duplicate archive entries even if their bytes are identical. No cross-widget deduplication is required.
- Runtime extraction, object URLs, handles, or staging paths are document-bound resource adaptations and must not leak into persisted config.

## Supported images and limits

Accepted filename extensions are:

- `.png`
- `.jpeg`
- `.jpg`
- `.bmp`
- `.tiff`

Extension checks are not sufficient by themselves: the bytes must decode as a supported image.

Two limits apply to every imported or loaded raster:

- Maximum encoded file size: 5 MiB (5 × 1024 × 1024 bytes).
- Maximum oriented decoded resolution: 25 megapixels (`width × height <= 25,000,000`).

The existing 40 MiB total `.oly` archive limit remains in force and includes all embedded raster bytes.

Expected image-orientation metadata must be applied consistently. Intrinsic dimensions and the 25 MP check use the final oriented dimensions.

## Creation and image selection

- Adding a raster creates the widget immediately without opening a file picker.
- A raster with no image displays a partially opaque white rectangle in the editor.
- The placeholder is editor-only. It must never appear in final Rust preview or export output.
- Image selection uses a native file picker restricted to supported extensions.
- A successful selection loads and validates the resource, then resets `width` and `height` to the exact oriented decoded pixel dimensions. One image pixel initially equals one scene pixel.
- The dimension reset happens on every successful selection or replacement, even when it makes the layout jump or places part of the image outside the canvas.
- An invalid replacement is transactional: the existing image, dimensions, and owned bytes remain unchanged, and the user receives the validation error.
- Selecting/replacing an image participates in normal editor history. Resource lifetime must retain bytes needed by reachable undo/redo states.

## Editor geometry

- The image fills the widget frame.
- Corner resizing preserves the raster's displayed aspect ratio as it existed at drag start.
- Horizontal edge handles change width independently.
- Vertical edge handles change height independently.
- After independent edge resizing, the image stretches to fill the resulting frame; it is not cropped or letterboxed.
- Moving, rotation, opacity, selection, duplication, deletion, undo, and redo follow established widget behavior.

## Layering and rendering

The compositing order in both the JSX editor and Rust renderer is:

1. backdrops
2. rasters
3. every other widget type

Raster ordering within the raster layer follows collection order using the same ordering convention as existing widget collections.

Rust decodes/prepares each raster once for a render job and draws it into the static cached layer after backdrops and before labels or static metric elements. Raster bytes must not be decoded and raster content must not be re-rendered independently on every elapsed-time frame.

Opacity and rotation apply through the established scene geometry conventions. Rendering uses the configured frame dimensions, including intentional non-uniform stretching.

## Validation and failure behavior

Validation has two distinct boundaries.

### Document ingress: recoverable normalization

Template and project loading must migrate, normalize, and correct raster config once in the same manner as other widgets. The rest of the document continues loading when a raster resource is unavailable or invalid.

For each affected raster, the editor:

- retains a normalized widget entry and geometry;
- shows the white placeholder;
- reports a specific actionable resource error, such as missing file, unreadable file, unsupported image, encoded size over 5 MiB, decoded resolution over 25 MP, corrupt embedded asset, or missing embedded asset;
- allows the user to pick a replacement.

Correction at ingress does not authorize render consumers to accept malformed config. A document carrying an unresolved raster resource is editable but not renderable.

### Render submission: strict and blocking

A submitted render configuration is a strict contract. Rendering fails loudly before frame production when:

- a required raster field is absent or has the wrong type or invalid value;
- `path` is absent/null;
- the referenced or staged asset cannot be found or read;
- its format is unsupported or its bytes cannot be decoded;
- it exceeds the 5 MiB encoded limit or 25 MP oriented-resolution limit;
- a project raster has no valid owned embedded resource.

There is no placeholder, silent omission, fallback image, fallback path, or consumer-side config repair during render.

## Project persistence

- Saving a project embeds one original image asset per populated raster, keyed by widget ID.
- Replacing a raster image causes that widget's archive asset to be overwritten on the next save.
- Deleting a raster removes its asset on the next save.
- Duplicating a raster creates independently owned bytes for the new widget ID.
- Saving rebuilds the archive from reachable project state, so stale and unreferenced image entries are not carried forward.
- The archive writer enforces the 40 MiB total limit with image bytes included before replacing the destination.
- Saving remains atomic: validation/archive construction must complete before the destination is replaced, and a failed save leaves the previous `.oly` recoverable and unchanged.
- Archive reads reject unsafe paths, duplicate entries, and malformed mappings. Raster entries must be regular files under the defined raster asset namespace.

## Format versions and migration

### Templates

- Increment the template format from version 2 to version 3.
- Version 3 adds the canonical top-level `rasters` collection.
- Loading version 2 explicitly migrates it by adding an empty `rasters` collection, then runs normal ingress normalization.
- New or updated templates are written as version 3.

### Projects

- Increment the `.oly` project format from version 1 to version 2.
- Version 2 permits and defines per-widget raster asset entries in addition to project JSON and thumbnail data.
- Loading version 1 explicitly migrates it with no embedded rasters.
- New saves write version 2.

Migration is versioned and explicit; it must not be implemented as broad compatibility aliases or fallback parsing.

## User-visible errors

Errors should identify the raster when possible and state the concrete remedy or violated limit. At minimum, distinct messages are required for:

- file missing;
- file unreadable;
- unsupported file type;
- image decode failure;
- file larger than 5 MiB;
- image larger than 25 MP;
- embedded project asset missing/corrupt;
- project would exceed the 40 MiB archive limit;
- raster has no selected image when rendering starts.

All new user-facing strings must be represented by translation keys and translated for every supported locale.

## Acceptance criteria

- A user can add a placeholder raster, select any supported image within both limits, and see it at intrinsic oriented dimensions.
- Replacing the image always resets dimensions; a failed replacement preserves the previous state.
- Corner and edge resizing obey their distinct ratio rules.
- JSX and Rust produce the same layer order and geometry.
- Final preview/export never renders a placeholder and blocks on unresolved rasters.
- Raster decoding/drawing is part of the Rust static cache preparation, not elapsed-frame work.
- Standalone templates round-trip absolute paths using template v3.
- Projects round-trip original image bytes without relying on external files using `.oly` v2.
- A damaged/missing template or project raster does not prevent unrelated document content from opening.
- Malformed submitted render config and unreadable submitted render resources fail loudly.
- The 5 MiB, 25 MP, and total 40 MiB limits are covered at every applicable ingress/save boundary.

## Out of scope

- Animated images or frame-dependent raster content.
- SVG or other vector image formats.
- `.tif` unless it is added as a separate product decision.
- Cropping, fit modes, filters, masks, color correction, or image editing.
- Automatic file watching or external-source refresh.
- Project-wide image deduplication or transcoding.
- Styling beyond fitting the established editor controls and error presentation.
