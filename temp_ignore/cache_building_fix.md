# Cache Building Fix Plan

## Goal

Bring Rust route/elevation cache preparation down to the same order of magnitude as the old Python renderer, and stop paying repeated prepare costs during multi-second preview batches.

## Problems Confirmed

- The Rust preview batch currently rebuilds render assets once per requested second.
- Full-activity templates precompute route and elevation frame state for every frame in the scene.
- Rust route frame-state building is somewhat slower than Python.
- Rust elevation frame-state building is dramatically slower than Python.
- The main Rust elevation cost comes from repeated per-frame interpolation work and rebuilding temporary filtered vectors inside the hot loop.

## Target Outcome

- Preview batch: prepare render assets once, render many requested seconds from the same prepared assets.
- Full render: route/elevation frame-state precompute cost should be comparable to Python for the same activity/template.
- No change in route/elevation visual behavior or timing bucket names.

## Work Plan

1. Reuse prepared assets across preview seconds

- Change `render_preview` batch flow so `prepare_render_assets(...)` runs once per config/activity pair.
- Render all requested seconds from the same cached route/elevation geometry and label base layer.
- Keep PNG writing per output frame, but do not rebuild widget caches per second.

2. Remove repeated linear interpolation work from frame-state builders

- Stop calling generic interpolation helpers that scan full sample arrays for every frame.
- Precompute compact lookup arrays once before the route/elevation frame-state loops.
- For elevation specifically, stop rebuilding the filtered `(x, y)` interpolation vector on every frame.

3. Match Python’s route/elevation frame-state strategy

- Route:
  - use dense `frame_distance_progress` directly
  - walk route geometry with a monotonic pointer or indexed search instead of rescanning from the start
- Elevation:
  - use dense per-frame elevation values directly when available
  - compute marker X from frame progress
  - compute marker Y from the plotted geometry or a precomputed progress-to-point mapping
- The full-frame loop should be O(frame_count), not O(frame_count \* sample_count).

4. Add a preview/full-render cache policy

- Preview mode:
  - either compute widget frame state on demand for requested frame indices
  - or precompute once only if it is proven cheaper than on-demand
- Full export mode:
  - full precompute is acceptable if it remains near Python timings
- Geometry caches should stay shared in both modes.

5. Add internal timings around sub-steps

- Split Rust prepare timings further while optimizing:
  - route geometry
  - route frame states
  - elevation geometry
  - elevation frame states
- Keep Python bucket names as the public baseline output.
- Extra Rust-only diagnostic timings can remain internal or preview-only until Phase 5 is complete.

6. Verify against Python baseline

- Use the same full-activity template/activity pair that exposed the issue:

```powershell
cargo run --bin render_preview -- --config ..\templates\new_template.json --payload ..\app\debug\2025-04-21_2180810019_Velkonocne_blbnutie_z_Zugu-parse-debug.json --seconds 600,607,615,622,629 --out ..\tmp\Phase4\rust-fix\template_previe_600.png --timing-out ..\tmp\Phase4\rust-fix\template_preview_batch_timing.json
```

- Success criteria:
  - preview batch no longer multiplies prepare cost by number of requested seconds
  - route cache roughly in Python range found in: \tmp\Phase4\python_new_template_full_prepare\prepare_render_assets_timing.json
  - elevation cache reduced from multi-second range to Python-like sub-second range

## Expected First Fix Order

1. Reuse prepared assets in preview batch.
2. Fix Rust elevation frame-state hot loop.
3. Tighten route frame-state lookup strategy.
4. Re-measure full-activity template against Python.
