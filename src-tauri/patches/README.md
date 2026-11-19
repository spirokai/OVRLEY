# Patched crates

Published crates with a local change, wired in through `[patch.crates-io]` in `Cargo.toml`. Each
copy is the published version exactly, plus the changes marked `OVRLEY:` in its source. When the
upstream crate is bumped, copy the new version here and re-apply the changes.

## i-slint-renderer-skia 1.19.0

- `itemrenderer.rs`, `draw_glyph_run`: glyphs are drawn without hinting. Slint leaves Skia's
  default (normal hinting), and on Linux FreeType then snaps small text to the pixel grid, so the
  0.667–0.741rem labels render with different shapes than on macOS, where Core Text ignores
  hinting. Slint has no setting for it.

`Cargo.lock`, `Cargo.toml.orig` and the registry metadata files are left out.
