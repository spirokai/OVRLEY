# 02 — Layout-only bounds for every widget family

**What to build:** Every widget family can report, for a given activity sample, the scene rectangle it would draw into, without allocating a surface or drawing. Families with fixed geometry (plots, gauges, backdrops, rasters, heading tape) report constant bounds. Text-based families (values, labels, lap timer, time, gradient) run only their layout step. These bounds are exact enough that export can plan tile slots from them.

**Blocked by:** 01 — Host-independent widget engine

**Status:** ready-for-agent

- [ ] Every family implements layout-only bounds for an activity sample.
- [ ] For every family and a spread of activity samples (including value width changes, every alignment, rotation, shadows and strokes), the rendered tile's non-transparent pixels lie inside the layout-only bounds.
- [ ] Layout-only bounds equal the rendered tile's bounds for the same sample.
- [ ] Computing layout-only bounds allocates no Skia surface (verified by a test or a timing assertion that it is much cheaper than rendering).
