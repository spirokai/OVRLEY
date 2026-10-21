# 04 — Atlas plan and atlas frame production

**What to build:** Before encoding, export plans an atlas. A layout-only pass over every export frame gives each widget's union rectangle as its slot. Slots are snapped to even coordinates and sizes, overlapping slots are merged, and the slots are packed into one atlas frame. During export, workers render tiles into their slots at each frame's offset and convert only the atlas to the pipe's pixel format. A single-slot full-frame plan produces exactly today's frames and is used by transparent export and QSV full overlay. Until ticket 05, the composite path keeps using the full-frame plan, so the output is unchanged. This ticket delivers a verified atlas plan and verified atlas frames.

**Blocked by:** 02 — Layout-only bounds; 03 — Export renders through the shared widget engine

**Status:** ready-for-agent

- [ ] Each slot contains the widget's bounds in every planned frame.
- [ ] Overlapping slots never interleave layers, so overlaying slots in order composites layers in layer order; widgets inside a merged slot composite in layer order.
- [ ] Slot positions and sizes are even; the packed atlas contains every slot without overlap and has even dimensions.
- [ ] The full-frame plan is one slot covering the frame, and its frames equal ticket 03's output.
- [ ] Reassembling an atlas frame onto a transparent full frame at the planned positions equals the full-frame plan's frame for the fixture templates.
- [ ] The planning pass and atlas conversion appear as buckets in the composite timing summary, and the planning pass on a 10-minute export finishes within a few seconds.

## Comments

**Frames are drawn straight into the atlas, not composited from tiles (2026-10-21).** Ticket 03 drew every widget into its own tile and copied the tiles into the frame. That dropped transparent 4K export from 45–48 fps to about 35 fps. The timing summary showed the frame workers were the bottleneck: copying a tile cost 0.67 ms single-threaded and 1.42 ms with four workers, about 15 ms of a 25 ms frame. A tile is a whole rectangle, so each copy touches every pixel, transparent ones included. The tiles were premultiplied BGRA and the frame is straight-alpha RGBA, so every pixel also needed a format conversion. The old renderer drew only the shapes.

Making tiles and frame both premultiplied RGBA made each copy about 3× faster. But converting the frame back to straight alpha for the pipe cost as much as that saved: 10.8 ms for a full 4K frame, or 3 ms when only the drawn areas were converted. So the tiles were dropped instead:

- `ExportWidgets::prepare_atlas` draws the static layers (backdrops, rasters, labels) once per export into two shared straight-alpha atlas bases. One is for frames inside the activity coverage; the other holds only the rasters, for frames outside it. Static layers come first in layer order, so in every slot they lie below the dynamic ones.
- Each frame copies its base, and the engine then draws the dynamic widgets (`PreviewWidget::draw`) straight onto it. Each slot is translated to its atlas position and clipped to its frame-visible scene rectangle.
- This is the old renderer's cost profile with the shared engine's drawing, and transparent export is back to its earlier speed. The same path renders planned atlases for tickets 05–07.
- Ticket 03's "unchanged widgets reuse their cached tile" is gone. Dynamic widgets are redrawn every covered frame, as the old renderer did. Static pixels are cached in the bases, and widget instances still keep their prepared geometry and layout between frames.

**Slots keep their pixel phase.** Drawn straight into the atlas, pixels depend on device position: Skia's ordered dither for gradients repeats every 8 pixels. The packer therefore places each slot so that its atlas origin minus its scene origin is a multiple of `PIXEL_PHASE` (8) on both axes. Reassembled atlas frames then equal the full-frame frames exactly.

**Still open.** Composite runs the planning pass only to report it in the timing summary, and still pipes the full-frame plan until ticket 05. Planning a 10-minute 60 fps export takes about 6 s in both debug and release builds, against the "few seconds" target. The release planning-speed test fails its 3 s budget.
