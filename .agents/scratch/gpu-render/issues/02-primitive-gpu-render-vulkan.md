# 02 - Primitive GPU Render into a Vulkan Image

Status: ready-for-agent

## Parent

Research plan: `.agents/scratch/gpu-render/gpu-render.md` - Research Phase 0, work item 0C.

This issue is part of the Phase 0 feasibility packet only. Passing it proves that the POC can render into a GPU-owned Vulkan image through Skia; it does not prove hardware encode or render-to-encode interop.

## What to build

Add render code to the POC crate at `src-tauri/gpu-poc/` that produces visibly identifiable content into a Vulkan-backed image:

- Clear to a transparent background.
- Draw a few colored rectangles and lines.
- Draw at least one anti-aliased vector shape (e.g. a filled path or stroked curve).
- Render through Skia Vulkan. Skia is mandatory for this issue because the parent plan needs to prove the actual OVRLEY rendering family can target GPU-backed Skia surfaces. Do not replace this with raw Vulkan commands, a compute shader, or a fragment shader.
- Write a debug PNG readback to disk for visual inspection.

The point is to prove a GPU-produced image exists that can later become encoder input. OVRLEY widgets, fonts, labels, routes, and activity data are out of scope for this issue.

## Acceptance criteria

- [ ] GPU rendering produces an image into Vulkan memory (no CPU pixel buffer as the render target).
- [ ] Rendering is performed through a Skia Vulkan `DirectContext` and GPU-backed Skia surface.
- [ ] At least one anti-aliased vector shape is visible in the output.
- [ ] A debug PNG readback is written to disk and is visually correct (no garbage, no unexpected black/white, transparency preserved).
- [ ] Vulkan validation layers report no errors (when enabled).
- [ ] The render proof notes the Skia Vulkan surface/image format and alpha type used.

## Blocked by

- #01 - Throwaway binary + dependency proof (needs buildable Skia Vulkan).
