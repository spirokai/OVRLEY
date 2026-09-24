# 05 — Persist and recover project rasters

**What to build:** A `.oly` project reopens with its raster images intact after the original external files are gone. Each raster owns its original embedded bytes, and damaged individual assets can be replaced without losing unrelated project content.

**Blocked by:** 03 — Edit raster geometry and image history.

**Status:** ready-for-agent

- [ ] New project saves write format version 3, retaining the existing version 2 manual-sync behavior; versions 1 and 2 migrate explicitly with no embedded rasters.
- [ ] Each populated raster owns an archive asset keyed by widget ID. The original supported encoded bytes and format are preserved without transcoding or cross-widget deduplication; project load uses the embedded asset as its sole image source.
- [ ] Saving after replacement overwrites that raster's asset; deletion removes it; duplication creates an independently owned entry. Saving from the reachable project state excludes stale assets while undo and redo continue to restore the correct bytes.
- [ ] Project loading validates embedded images against supported formats, the 5 MiB encoded limit, and the 25 MP oriented limit. A missing or corrupt individual asset preserves the normalized widget and other project content, shows an editor placeholder with a specific error, and can be repaired by choosing a new image.
- [ ] Archive reads reject unsafe paths, duplicate entries, malformed asset mappings, and raster entries that are not regular files in the defined raster namespace. A project raster with no valid owned resource cannot render.
- [ ] The existing 40 MiB total archive limit includes embedded raster bytes and is enforced before destination replacement. Failed validation or archive construction leaves the previous project recoverable and unchanged.
- [ ] Extraction, object URLs, handles, and staging paths remain document-bound resource state outside persisted widget config. Project errors use translation keys in every supported locale.
