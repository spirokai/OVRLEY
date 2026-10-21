# Widget atlas: composite export through the shared widget engine

Status: ready-for-agent

## Problem Statement

Composite export runs at well below the speed of the encoder. At 4K the AMF export reaches 44–45 fps, while the same encode without an overlay runs at about 51 fps. At 1080p, export with an overlay runs at roughly 60% of the no-overlay speed. How much is drawn makes no difference: one static label and a busy layout export at the same speed.

The measured cause is not drawing. Every frame, the whole overlay, the full video resolution with mostly transparent pixels, is piped into FFmpeg and blended over the entire frame. The writer thread spends about 96% of the export blocked on that write, and render workers sit idle about 40 ms per frame waiting for FFmpeg to take their buffers.

A second problem sits underneath. The export draws widgets with its own full-frame renderer, while the editor preview draws each widget into its own small surface through the shared widget engine. Two renderers produce the same pixels and are kept in agreement by parity tests.

## Solution

Composite export renders each widget the way the preview does: into its own tile through the shared widget engine. Only those tiles are sent to FFmpeg, packed into one small atlas frame per output frame, and FFmpeg places each tile back at its position on the video.

The user sees the same exported video, colours and layer order, but export with an overlay runs at about the speed of export without one. In benchmarks, piping only the widget area brought 4K from 41 to 49 fps (no-overlay floor 51) and 1080p from 154 to 258 fps (floor 256).

The export's separate renderer is removed. The preview and the export then draw every widget with the same code.

## User Stories

1. As a rider exporting a 4K video with an overlay, I want the export to run about as fast as an export without an overlay, so that long rides don't take much longer to export just because they carry data widgets.
2. As a rider exporting 1080p video, I want the overlay to stop costing about a third of the export speed, so that short clips export almost instantly.
3. As a rider, I want an export with a few small widgets to be faster than one with many large widgets, so that the cost of an overlay follows what is actually drawn.
4. As a rider, I want exported widgets to look exactly like the editor preview, so that what I lay out is what I get.
5. As a rider, I want widgets that overlap, such as a value on a backdrop, to keep their stacking order in the export, so that layouts aren't rearranged.
6. As a rider, I want values whose text changes width, such as speed going from 9 to 10, to never be clipped in the export, so that numbers stay whole.
7. As a rider, I want right- and centre-aligned values to stay anchored where I placed them, so that the layout doesn't jump.
8. As a rider, I want rotated widgets to export with their full rotated extent, so that no corner is cut off.
9. As a rider, I want the parts of a video outside the activity to show only my raster images, as today, so that video coverage behaves the same.
10. As a rider, I want overlay colours unchanged, BT.709 for HD sources, so that this speed-up doesn't shift colours.
11. As a rider using NVIDIA (CUDA), Intel (QSV), AMD (AMF), Linux (VAAPI), macOS (VideoToolbox) or software encoding, I want the speed-up on my encoder, so that it isn't limited to one vendor.
12. As a rider using Intel QSV full overlay, I want rotated videos to keep working while the atlas for that path isn't ready, so that nothing regresses.
13. As a rider exporting a transparent overlay video, I want that export to keep working unchanged, so that editors that composite later still get the full frame.
14. As a rider who cancels an export, I want cancellation and cleanup to behave as today, so that the atlas doesn't leave stray files or processes.
15. As a rider exporting with a widget update rate below the video frame rate, I want the overlay to keep updating at that rate, so that the update-rate setting still means what it says.
16. As a rider with a layout that covers the whole frame, such as a full-screen backdrop, I want the export to still work and be no slower than today, so that unusual layouts are safe.
17. As a developer, I want one widget engine for preview and export, so that a widget change is made once and parity tests no longer guard two implementations.
18. As a developer, I want every widget family to report the bounds it draws into without drawing, so that export planning is exact and cheap.
19. As a developer, I want a test that fails when any family draws outside its reported bounds, so that clipping bugs are caught before release.
20. As a developer, I want the export's FFmpeg graph to describe each tile with crop and overlay positions derived from one atlas plan, so that encoders don't each invent their own layout logic.
21. As a developer, I want the timing summary to show tile rendering, atlas packing and conversion costs, so that the next bottleneck is visible.

## Implementation Decisions

- **One widget engine.** The preview widget families in the shared widgets crate become independent of the host. They take a font resolver and an image source through interfaces, not the preview's concrete font and image stores, and they expose typed bounds next to the existing serialized geometry. The editor preview keeps its behaviour; the export becomes a second host of the same engine.
- **Layout-only bounds.** Every family can report, for a given activity sample, the scene rectangle it would draw into, without allocating a surface or drawing. Families with fixed geometry (plots, gauges, backdrops, rasters, the heading tape) report constant bounds. Text-based families run only their layout step.
- **Export host.** The export builds its widgets from the validated render config, in the export's existing layer order (the static layer first, then intrinsic values, then boxed values). Each render worker owns its own set of widget instances, because instances hold Skia surfaces and caches. The export renders at raster scale 1, without the preview's raster scale or drag handling.
- **Activity sampling.** The export currently samples its precomputed per-frame dense activity report, while the preview samples the activity sampler at a time in seconds. Ticket 03 decides between two options. If the values agree at export frame times, which the parity tests suggest, the export samples like the preview and the dense report stops feeding widgets. Otherwise the widgets get a small sampling interface that both sources implement. Update-rate decimation and activity coverage stay owned by export planning either way.
- **Atlas plan.** Before encoding, a planning pass runs layout only for every export frame and takes each widget's union rectangle as its slot. Slots are snapped to even coordinates and sizes, because yuva420p chroma covers 2×2 blocks. Overlapping slots merge into one, and widgets inside a merged slot are composited in layer order with Skia. Slots are then packed into one atlas frame. The plan is immutable for the export and is the single source for both frame production and the FFmpeg graph.
- **Frame production.** Per output frame, each worker renders its widgets' tiles and copies each into its slot at the widget's offset that frame. It converts the atlas, not the full frame, to the pipe's pixel format, which stays yuva420p in the source colour. Slots of widgets hidden by coverage are cleared, so outside the activity only raster tiles show.
- **Single path.** A full frame is an atlas plan with one slot covering the frame. Transparent export uses that plan, since it must output the full frame. So does QSV full overlay until its coded-orientation handling is done. There is no separate full-frame pipeline.
- **FFmpeg graph.** The overlay input is the atlas. For CPU overlay stacks (software, NVENC, QSV, AMF, VideoToolbox), the graph splits the atlas, crops each slot, and chains overlays at the slot's scene position on the main video. For CUDA and VAAPI, each slot is cropped before it is uploaded to the GPU and overlaid with the hardware overlay filter. QSV full overlay maps slot positions into coded coordinates for rotated sources. Graph building consumes only the atlas plan.
- **Removal.** Once the export renders through the engine and the atlas graphs are in place, the export's own drawing path and its full-frame static layer are deleted.

## Testing Decisions

- Tests check external behaviour: pixels, rectangles and FFmpeg output, never internal call sequences.
- **Widget engine seam (shared widgets crate):** for every family and a set of activity samples, the drawn pixels lie within the reported layout-only bounds, and the reported bounds match the bounds of the rendered tile. Prior art: the existing preview/native parity tests.
- **Export frame seam:** frames composited from tiles through the engine match the current export renderer's frames for the fixture templates, within a small tolerance for premultiplied-alpha rounding. Coverage cases (before and after the activity) and overlapping widgets are covered. This is the golden reference until the old renderer is removed. After that, it becomes a comparison between the atlas output and the single-slot full-frame output.
- **Atlas plan:** slot unions contain every frame's bounds; merged slots don't overlap; slots are even-aligned; the packed atlas contains every slot without overlap; the full-frame plan is one slot.
- **FFmpeg argument builders:** each encoder's graph has one crop and one overlay per slot at the planned positions, and CUDA and VAAPI crop before upload. Prior art: the composite FFmpeg argument tests.
- **End to end:** a real FFmpeg composite export using the atlas decodes to the same frames as the single-slot full-frame export, within tolerance. Prior art: the composite pipeline tests that decode output frames.
- **Performance:** the overlay pipeline benchmark script and the composite timing summary, run on AMD, Intel and NVIDIA machines.

## Out of Scope

- Sending static layers (backdrops, labels, rasters) once as a looped still image instead of in every atlas frame.
- Loopback TCP or other pipe transports.
- Running FFmpeg in-process, GPU-side Skia rendering, and GPU transition compositing.
- Changes to the transparent export's output format.
- Changes to the editor preview's behaviour or UI.

## Further Notes

- Benchmarks behind this spec, on a Ryzen 9 7900X with a Radeon iGPU and AMF HEVC:
  - **4K:** today's full-frame RGBA path ran at 26.7 fps; full-frame yuva420p, which shipped, at 35.0; cropped to the widget box at 49.4; no-overlay floor 51.1.
  - **1080p:** full frame ran at 154 fps; cropped at 258; floor 256.
- The yuva420p pipe and the BT.709 handling for untagged HD sources are already committed and stay in place. The atlas is converted the same way.
- A layout whose slots cover most of the frame gets no gain but no loss either. The atlas then degrades toward the single full-frame slot.
