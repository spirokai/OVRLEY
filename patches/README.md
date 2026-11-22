# Patched crates

Published crates with a local change, wired in through `[patch.crates-io]` in `Cargo.toml`. Each
copy is the published version exactly, plus the changes marked `OVRLEY:` in its source. When the
upstream crate is bumped, copy the new version here and re-apply the changes.

## i-slint-renderer-skia 1.19.0

- `itemrenderer.rs`, `draw_glyph_run`: glyphs are drawn without hinting. Slint leaves Skia's
  default (normal hinting), and on Linux FreeType then snaps small text to the pixel grid, so the
  0.667–0.741rem labels render with different shapes than on macOS, where Core Text ignores
  hinting. Slint has no setting for it.
- `wgpu_30_surface.rs`, `render_scene`, and `lib.rs`, the wgpu constructors: the window is drawn
  with partial rendering into a persistent scene texture, which is copied whole into each
  swapchain image. Upstream repaints the whole window every frame (its surface reports buffer age
  0), plus a full-window clear, so a moving playhead cost a full repaint. The copy replaces the
  clear. A swapchain that can't be a copy destination keeps the upstream path, and so does
  `OVRLEY_SKIA_FULL_REDRAW=1`, which exists to compare the two. `SLINT_DEBUG_PERFORMANCE` names the
  surface `wgpu-partial` when the scene is used.
- `itemrenderer.rs`, cached layers (`cache-rendering-hint`): a layer's bounds come from the whole
  render target and its origin is a whole pixel from the item. Upstream takes the bounds from the
  current clip, which with partial rendering is the region being repainted, and rounds wherever
  that leaves the layer's origin: the timeline's lanes jumped a pixel and lost their overflow
  whenever something in them changed.
- `wgpu_30_surface/dx12.rs`, `make_dx12_surface`: takes the resource state the texture is in.
  Upstream always tells Skia `PRESENT`, also for a texture `transition_to_color_target` has made a
  render target.

`Cargo.lock`, `Cargo.toml.orig` and the registry metadata files are left out.
