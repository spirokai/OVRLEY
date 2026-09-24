# 03 — Edit raster geometry and image history

**What to build:** A user can resize, move, rotate, duplicate, delete, and replace a raster with normal editor undo and redo. Failed replacement leaves the previous image unchanged, and every successful replacement starts at the new image's intrinsic size.

**Blocked by:** 02 — Render rasters in preview and export.

**Status:** ready-for-agent

- [ ] Corner resizing preserves the displayed width-to-height ratio captured at drag start. Horizontal edge handles change width independently; vertical edge handles change height independently.
- [ ] The image fills the resulting frame without cropping or letterboxing, including after an independent edge resize. JSX and final Rust preview show the same geometry.
- [ ] Moving, rotation, opacity, selection, duplication, deletion, undo, and redo follow established widget behavior.
- [ ] Every successful selection or replacement resets width and height to the new oriented decoded pixel dimensions, including when the layout jumps or extends outside the canvas.
- [ ] Invalid replacement reports the specific error while preserving the existing image, dimensions, and owned bytes.
- [ ] Undo and redo of image selection or replacement restore the matching bytes and dimensions. Resource lifetime retains bytes for all reachable history states, and a duplicate can subsequently own a different image from its source.
- [ ] New user-visible text uses translation keys in every supported locale.
