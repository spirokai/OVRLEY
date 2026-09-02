# 04 - Wire Render-to-Encode Zero-Copy + Write Feasibility Doc

Status: ready-for-agent

## Parent

Research plan: `.agents/scratch/gpu-render/gpu-render.md` - Research Phase 0, work item 0E + deliverables.

This is the Phase 0 gate issue. Passing issues #01-#03 is not enough to proceed unless this issue proves that a Skia-rendered Vulkan image can reach the selected encoder without CPU pixel staging.

## What to build

Combine the GPU render path from #02 and the hardware encode path from #03 into a single end-to-end zero-copy flow. Try the simplest viable interop model first:

```text
GPU render target
  -> optional GPU-only layout/format conversion
  -> selected hardware encoder frame
  -> encoded packet
```

Select one most plausible ownership model first, based on what #02 and #03 discovered. Only move to the next model if the current model fails for a concrete reason. Candidate ownership models are:

- **FFmpeg-owned images, Skia renders into them** - can Skia wrap an `AVHWFramesContext`-allocated `VkImage` as a render target?
- **Rust/Skia-owned images, FFmpeg imports them** - can FFmpeg import an externally created `VkImage` for encoding?
- **Separate render image + GPU-only copy/conversion into FFmpeg-owned frame** - still zero-CPU-copy if the transfer is GPU-only.
- **Vulkan external memory imported into another hardware API** (CUDA, D3D11/QSV, AMF).

One ugly working route is enough to justify deeper research. Do not try to make all models elegant. If a candidate is not attempted, document why it was skipped and what evidence would be needed later.

Write the decision document: `docs/gpu-research/primitive-zero-copy-feasibility.md`. It must answer:

> Can this repo produce one required output file from a GPU-rendered image without a CPU pixel upload/readback in the encode path?

## Acceptance criteria

- [ ] One encoded output file is produced from a GPU-rendered frame, with no CPU pixel staging in the encode path.
- [ ] The encoded frame source is the Skia Vulkan render output from #02, not a CPU-seeded debug frame from #03.
- [ ] The output use case is clearly labeled (transparent overlay export, composited final-video export, or both).
- [ ] Optional debug PNG readback of the same frame is produced (for visual comparison only, not as encode input).
- [ ] `docs/gpu-research/primitive-zero-copy-feasibility.md` exists and contains:
  - Which interop model worked (or a clear explanation of why none did).
  - Which candidate models were attempted, skipped, or deferred, with reasons.
  - Which backend and codec were used.
  - Whether CPU staging occurred at any point (and how this was verified - API inspection, memory-flow tracing, validation output, debug instrumentation).
  - A go/no-go recommendation for proceeding to Research Phase 1.
- [ ] The document names any unsupported GPUs, drivers, or extensions discovered during testing.

## Blocked by

- #02 - Primitive GPU render (needs a working Skia Vulkan render path).
- #03 - Primitive hardware encode (needs a working GPU encode path).
