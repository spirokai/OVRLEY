# 03 — Export renders through the shared widget engine

**What to build:** Composite and transparent export draw every widget through the shared widget engine, one tile per widget, and composite the tiles in the export's layer order into the full frame that is piped today. The export output looks the same as before, but it now comes from the same code as the editor preview. The decision about activity sampling (sampling like the preview, or a sampling interface over the dense report) is made and recorded in this ticket's comments.

**Blocked by:** 01 — Host-independent widget engine

**Status:** ready-for-agent

- [ ] The export builds widget instances from the validated render config in its existing layer order: the static layer, then intrinsic values, then boxed values.
- [ ] Each render worker owns its own widget instances; unchanged widgets reuse their cached tile between frames.
- [ ] Activity sampling decision is implemented and documented; update-rate decimation and activity coverage stay owned by export planning.
- [ ] Outside the activity coverage, only raster widgets appear, as today.
- [ ] Golden test: for the fixture templates (including overlapping widgets, rotation and coverage edges), frames composited from engine tiles match the current export renderer within a small premultiplied-alpha tolerance.
- [ ] Existing composite and transparent export tests pass; export speed is not worse than before on the benchmark clip.

## Comments

**Activity sampling decision (2026-10-21): the export samples like the preview.** Widgets sample the shared `ActivitySampler`, built once per export from the same serialized activity the editor preview receives, at `scene.start + dense.frame_elapsed_seconds[i]` for each rendered dense frame `i`. The dense report no longer feeds widgets; it still defines the frame timeline. Drawing the engine widgets straight into the frame reproduces the export renderer's pixels for every family, so the values agree at export frame times and no second sampling interface is needed. Export planning keeps update-rate decimation (which dense frames are rendered) and activity coverage (outside it only raster tiles are composited).

**Layer order.** Backdrops, rasters and labels, then intrinsic values, boxed values, routes, elevation plots and diving profiles. The export renderer drew every text value's units and icons in its full-frame static layer, below all values; the engine draws each value whole, as the preview does. The two differ only where a text value's unit or icon overlaps another value.

**Golden tolerance.** Tiles are premultiplied, as in the preview. Blending a tile into the frame rounds soft anti-aliased and shadow pixels differently from drawing every primitive into the frame, so the golden test allows a premultiplied difference of 8, and at most 64 pixels per frame up to 64.
