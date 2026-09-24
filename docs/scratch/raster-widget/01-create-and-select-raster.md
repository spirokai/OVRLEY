# 01 — Create and select a raster in the editor

**What to build:** A user can add a raster immediately, see a partially opaque white placeholder, then choose a supported image and see it in the editor at its oriented intrinsic size. The widget uses the canonical top-level `rasters` collection and remains selectable like other widgets.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] A new raster is created without opening a picker. Its canonical config entry has exactly `id`, `x`, `y`, `width`, `height`, `rotation`, `opacity`, and `path`; `path: null` represents only a never-selected editor placeholder. Runtime image data stays outside widget config.
- [ ] The placeholder is a partially opaque white rectangle in the JSX editor. A raster with no image is never silently omitted from a render request.
- [ ] The native image picker offers `.png`, `.jpeg`, `.jpg`, `.bmp`, and `.tiff`; relative paths and `.tif` are not accepted.
- [ ] Selection validates both the extension and decoded format, rejects encoded files above 5 MiB and oriented images above 25 MP, and applies expected orientation metadata consistently.
- [ ] A successful selection displays the image stretched into a frame whose width and height equal the exact oriented decoded pixel dimensions, at one scene pixel per image pixel, even when that moves part of the image outside the canvas.
- [ ] Failed selection leaves the placeholder intact and identifies the concrete validation problem. New user-visible text has translation keys in every supported locale.
