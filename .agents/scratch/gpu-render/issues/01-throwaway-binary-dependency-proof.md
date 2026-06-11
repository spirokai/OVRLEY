# 01 - Throwaway Binary + Dependency Proof

Status: ready-for-agent

## Parent

Research plan: `.agents/scratch/gpu-render/gpu-render.md` - Research Phase 0, work items 0A + 0B.

This issue is part of the Phase 0 feasibility packet only. Passing it does not prove the zero-copy render-to-encode pipeline; it only proves the local dependency and process shape needed for the later Phase 0 issues.

## What to build

Create an isolated standalone crate at `src-tauri/gpu-poc/` (its own `Cargo.toml`, no dependency on `ovrley_core`) that proves all three dependency families can compile and link together:

- `ash` - creates a Vulkan instance, selects a physical device (integrated or dedicated), creates a logical device and queue. Prints adapter name, vendor/device IDs, and Vulkan driver/API versions.
- `skia-safe` with Vulkan support - creates a Skia Vulkan `DirectContext` from the Rust-owned Vulkan handles. Skia Vulkan is mandatory for this Phase 0 packet; do not substitute raw Vulkan rendering for the render proof.
- FFmpeg development headers/import libraries - linked by Rust so that libavcodec/libavutil symbols resolve at link time (not just subprocess execution).

The binary must compile with `cargo build` and run without crashing (if a GPU is present) or print a clear "no suitable GPU" message. The FFmpeg linkage must be verified at compile time, not merely as runtime subprocess discovery.

Use ugly, duplicated, research-only code. Use local paths, environment variables, and rough scripts for dependency discovery. Reproducible packaging is for Research Phase 1.

`src-tauri/gpu-poc/` is the canonical POC location for all Phase 0 issues. It may be added as a `src-tauri` workspace member if that makes local commands simpler, but it must remain independent from `ovrley_core`.

## Acceptance criteria

- [ ] Standalone crate exists at `src-tauri/gpu-poc/` with its own `Cargo.toml`, no dependency on `ovrley_core`.
- [ ] The issue notes whether `src-tauri/gpu-poc/` was added to the workspace or intentionally built from its own directory.
- [ ] `cargo build` succeeds inside `src-tauri/gpu-poc/`.
- [ ] Binary prints selected physical device name, vendor/device IDs, Vulkan driver/API versions when a GPU is available.
- [ ] Binary prints a clear message when no suitable GPU is found (does not crash).
- [ ] Skia Vulkan `DirectContext` creation from Rust-owned Vulkan handles succeeds.
- [ ] FFmpeg symbols link (any libavcodec/libavutil call demonstrates linkage succeeded).
- [ ] `ovrley_core` is not listed as a dependency of the POC crate. Nothing in `ovrley_core` is modified.

## Blocked by

None - can start immediately.
