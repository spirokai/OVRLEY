# 03 - Primitive Hardware Encode from GPU-Resident Frames

Status: ready-for-agent

## Parent

Research plan: `.agents/scratch/gpu-render/gpu-render.md` - Research Phase 0, work item 0D.

This issue is part of the Phase 0 feasibility packet only. Passing it proves that at least one in-process hardware encode path can accept GPU-resident frames. It does not prove the final zero-copy project gate until issue #04 proves that a Skia-rendered Vulkan image reaches that encoder without CPU pixel staging.

## What to build

Add encode code to the POC crate at `src-tauri/gpu-poc/` to prove at least one hardware encoder backend can accept GPU-resident frames in-process:

- Use direct FFmpeg/libavcodec APIs (not the `ffmpeg` subprocess, not stdin rawvideo).
- Create or import the required hardware device/frame context for the selected backend.
- For this isolated encode proof only, a debug CPU-initialized frame is acceptable as a temporary source if the chosen FFmpeg API requires CPU writes to seed test data. That path must be documented as an encode API proof, not a zero-copy proof. The acceptance criteria still require that the submitted encoder frame is GPU-resident and that the code explains exactly where any temporary CPU initialization occurred.
- Document which backend is used: Vulkan, QSV, CUDA/NVENC, AMF, D3D11VA, VAAPI, or another hardware path.
- Document which output use case the backend supports: transparent overlay export, composited final-video export, or both.
- Write/mux a one-frame or few-frame output file.
- Record the exact requirements: `AVPixelFormat`, hardware frame format, software format, codec/profile, alpha bits (if transparent), pixel format expectations, frame pool requirements, and packet drain sequence.

The encode path must not go through rawvideo stdin. Debug readback is allowed only as a side artifact. If this issue uses any CPU-seeded test pixels, it must explicitly state that issue #04 remains responsible for proving no CPU pixel upload/readback occurs in the render-to-encode path.

## Acceptance criteria

- [ ] At least one hardware encoder backend encodes a GPU-resident frame in-process via libavcodec.
- [ ] An output video file (e.g. `.mov`) is written and playable.
- [ ] The code does not use FFmpeg subprocess, stdin, or rawvideo piping.
- [ ] The backend choice, codec, and pixel format are documented in code comments.
- [ ] The output use case (transparent overlay, composited final-video, or both) is clearly labeled.
- [ ] Any CPU-seeded debug initialization is called out explicitly and is not described as satisfying the zero-copy project gate.
- [ ] The issue result states what issue #04 must still prove for true Skia render-to-encode zero-copy.
- [ ] No crash, deadlock, or resource leak during encode and mux finalization.

## Blocked by

- #01 - Throwaway binary + dependency proof (needs linkable FFmpeg dev headers and discovered runtime).
