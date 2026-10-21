# 06 — CUDA and VAAPI export through the atlas

**What to build:** Exports on the CUDA and VAAPI hardware overlay stacks pipe the atlas. Each slot is cropped on the CPU, uploaded to the GPU and overlaid with the hardware overlay filter at its planned position. The exported video looks the same and gets the atlas speed-up.

**Blocked by:** 05 — CPU-overlay encoders export through the atlas

**Status:** ready-for-agent

- [ ] The CUDA and VAAPI graphs crop each slot before upload and chain hardware overlays at the planned positions.
- [ ] Rotated sources (GPU rotation filters) keep their current orientation handling.
- [ ] Argument builder tests cover both stacks with multiple slots.
- [ ] Verified on an NVIDIA machine: exported frames match the full-frame plan within tolerance, and the benchmark shows the speed-up.
- [ ] VAAPI verified on Linux, or explicitly recorded in this ticket's comments as untested hardware.
