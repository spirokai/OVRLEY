# 04 — Save and recover standalone templates

**What to build:** A raster survives a standalone template save and reopen using its absolute image path. Older templates migrate explicitly, and a template with a broken raster resource still opens so the user can replace that image.

**Blocked by:** 02 — Render rasters in preview and export.

**Status:** ready-for-agent

- [ ] New and updated templates write format version 3 with the canonical `rasters` collection; version 2 explicitly migrates to an empty `rasters` collection before normal ingress normalization.
- [ ] Populated standalone raster paths round-trip as absolute filesystem paths. A relative or malformed present path is not treated as a valid image source.
- [ ] Template loading resolves and validates each raster resource once, including format, 5 MiB encoded size, orientation, and 25 MP decoded resolution. A successfully loaded image remains the session image even if its external file later changes.
- [ ] A missing, unreadable, invalid, or oversized raster retains a normalized widget and geometry, shows the white editor placeholder, reports a specific actionable error, and allows replacement while unrelated template content loads.
- [ ] A document with an unresolved raster remains editable but blocks final preview and export until corrected; strict render input is not repaired by consumers.
- [ ] Template migration and recovery paths are covered by focused verification. New user-visible text uses translation keys in every supported locale.
