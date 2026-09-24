# 02 — Render rasters in preview and export

**What to build:** A selected raster appears in final Rust preview and video export at the same frame, rotation, opacity, and layer position shown in the editor. Unresolved or malformed rasters stop rendering with a clear error before frames are produced.

**Blocked by:** 01 — Create and select a raster in the editor.

**Status:** ready-for-agent

- [ ] Render input requires the canonical raster fields with valid types and ranges, a non-null absolute `path`, and a readable, supported image within the 5 MiB and oriented 25 MP limits. Malformed present values are rejected at render ingress; consumers do not repair them.
- [ ] A missing, unreadable, unsupported, oversized, or undecodable image stops both preview and export before frame production and identifies the affected raster and remedy where possible. A placeholder is never rendered in final output.
- [ ] Both the JSX editor and Rust renderer composite backdrops, then rasters in collection order, then every other widget type.
- [ ] Rust decodes and prepares each raster once per render job and draws it in the shared static cached layer used by preview and export; elapsed-time frames do not decode or redraw the source image independently.
- [ ] Rust uses the configured frame, rotation, and opacity under established scene geometry, including non-uniform stretching. Editor and final render agree for representative images and transformations.
- [ ] New user-visible errors use translation keys in every supported locale.

