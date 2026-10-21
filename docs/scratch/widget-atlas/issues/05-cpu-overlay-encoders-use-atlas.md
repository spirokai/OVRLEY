# 05 — CPU-overlay encoders export through the atlas

**What to build:** Exports with software, NVENC, QSV (non-full), AMF and VideoToolbox encoding pipe the atlas instead of the full frame. The FFmpeg graph splits the atlas, crops each slot and chains overlays at the slot's scene position on the main video. The exported video looks the same, and overlay export runs close to the no-overlay speed.

**Blocked by:** 04 — Atlas plan and atlas frame production

**Status:** ready-for-agent

- [ ] The graph for every CPU-overlay encoder has one crop and one overlay per slot at the planned positions, built only from the atlas plan.
- [ ] Source rotation filters, colour tags (BT.709 for untagged HD) and audio handling are unchanged.
- [ ] End to end: a real FFmpeg export through the atlas decodes to the same frames as the full-frame plan, within tolerance, including frames outside the activity coverage.
- [ ] Cancellation, broken-pipe errors and output cleanup behave as before.
- [ ] Benchmark on the AMD machine: 4K and 1080p overlay exports reach close to the no-overlay floor; the timing summary shows ffmpeg write backpressure well below today's level.
