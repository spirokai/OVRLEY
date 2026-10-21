# 07 — QSV full overlay exports through the atlas

**What to build:** QSV full overlay export pipes the atlas instead of the single full-frame slot. QSV full overlay keeps the main video in coded orientation, so each slot's tile and position are mapped into coded coordinates for 90°, 180° and 270° sources. The exported video, including the display matrix, looks the same.

**Blocked by:** 05 — CPU-overlay encoders export through the atlas

**Status:** ready-for-agent

- [ ] Slot positions and tile orientation are mapped into coded coordinates for every supported source rotation.
- [ ] The graph overlays each slot with the QSV overlay filter at the mapped position.
- [ ] Argument builder tests cover every rotation with multiple slots.
- [ ] Verified on an Intel machine for upright and rotated sources: exported frames match the full-frame plan within tolerance, and the benchmark shows the speed-up.
