# 05 — CPU-overlay encoders export through the atlas

**What to build:** Exports with software, NVENC, QSV (non-full), AMF and VideoToolbox encoding pipe the atlas instead of the full frame. The FFmpeg graph splits the atlas, crops each slot and chains overlays at the slot's scene position on the main video. The exported video looks the same, and overlay export runs close to the no-overlay speed.

**Blocked by:** 04 — Atlas plan and atlas frame production

**Status:** ready-for-agent

- [ ] The graph for every CPU-overlay encoder has one crop and one overlay per slot at the planned positions, built only from the atlas plan.
- [ ] Source rotation filters, colour tags (BT.709 for untagged HD) and audio handling are unchanged.
- [ ] End to end: a real FFmpeg export through the atlas decodes to the same frames as the full-frame plan, within tolerance, including frames outside the activity coverage.
- [ ] Cancellation, broken-pipe errors and output cleanup behave as before.
- [ ] Benchmark on the AMD machine: 4K and 1080p overlay exports reach close to the no-overlay floor; the timing summary shows ffmpeg write backpressure well below today's level.

## Comments

**Implemented (2026-10-21), tests and benchmark not yet run.**

- Composite encoding is now built after the atlas is planned. Job execution passes the render plan and source metadata to `encode_video` (`VideoEncodingRequest::Composite`). `encode_video` plans the atlas when the requested stack overlays slots (`CompositeFilterStackKind::overlays_atlas_slots`: software, NVENC, QSV, AMF and VideoToolbox), and then calls `CompositeEncoding::new` with that plan. CUDA, VAAPI and QSV full overlay keep the full-frame plan and refuse any other plan until tickets 06 and 07. A rotated CUDA request decides by the requested stack, so it falls back to NVENC with the full-frame plan and does not get the speed-up yet.
- `build_composite_ffmpeg_settings` takes the atlas plan. The overlay pipe's `-s` is the atlas size. The CPU templates use `{atlas_slot_crops}` and `{atlas_slot_overlays}`: `split=N`, one `crop=w:h:x:y[slotN]` per slot, and one `overlay=X:Y:eof_action=repeat:shortest=1` per slot, chained from `[base]`. A plan with one slot skips the split. A plan with no slots overlays its 2×2 transparent atlas at 0:0, so the pipe is still read and still ends the graph.
- End-to-end test: `encode::pipeline::atlas_export_tests` exports the 1080p test template over `test-1080p.mp4` through the planned atlas and through the full-frame plan, with the activity ending 0.4 s before the export ends. It then compares the decoded frames.

**Overlapping slots are merged only when merging is cheaper (2026-10-21).** In the first AMD benchmark, a rotated raster's bounding box overlapped three bottom slots, and they merged into one 3814×1178 slot. The atlas grew from 3.5 to 6.3 MP and the blended area roughly doubled, which took 4K AMF from 43.7 to 40.8 fps. FFmpeg's side was the bottleneck: `ffmpeg.write` took about the whole frame time. Compositing over is associative, so overlapping slots don't have to share one: overlaid in layer order, they produce the same image up to 8-bit rounding in the overlap. `merge_overlapping` now merges two overlapping slots in two cases. It merges when their union is no larger than the two slots together. It also always merges when their layers interleave, because no overlay order could composite those. Slots stay ordered by their first layer, so the FFmpeg chain overlays every overlap in layer order. The reassembly test now composites slots in order. It is exact where one slot covers a pixel and allows 2 premultiplied levels where slots overlap.
