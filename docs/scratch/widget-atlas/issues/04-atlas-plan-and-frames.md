# 04 — Atlas plan and atlas frame production

**What to build:** Before encoding, export plans an atlas. A layout-only pass over every export frame gives each widget's union rectangle as its slot. Slots are snapped to even coordinates and sizes, overlapping slots are merged, and the slots are packed into one atlas frame. During export, workers render tiles into their slots at each frame's offset and convert only the atlas to the pipe's pixel format. A single-slot full-frame plan produces exactly today's frames and is used by transparent export and QSV full overlay. Until ticket 05, the composite path keeps using the full-frame plan, so the output is unchanged. This ticket delivers a verified atlas plan and verified atlas frames.

**Blocked by:** 02 — Layout-only bounds; 03 — Export renders through the shared widget engine

**Status:** ready-for-agent

- [ ] Each slot contains the widget's bounds in every planned frame.
- [ ] Merged slots never overlap; widgets inside a merged slot composite in layer order.
- [ ] Slot positions and sizes are even; the packed atlas contains every slot without overlap and has even dimensions.
- [ ] The full-frame plan is one slot covering the frame, and its frames equal ticket 03's output.
- [ ] Reassembling an atlas frame onto a transparent full frame at the planned positions equals the full-frame plan's frame for the fixture templates.
- [ ] The planning pass and atlas conversion appear as buckets in the composite timing summary, and the planning pass on a 10-minute export finishes within a few seconds.
