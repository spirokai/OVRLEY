# Serial Render Refactor

## Goal

Refactor the backend overlay renderer so video export no longer depends on a prebuilt `self.frames` list or a fully serial render-then-write loop, while keeping the code runnable and measurable after every phase.

Primary objectives:

1. Replace prebuilt frame accumulation plus the serial ffmpeg write loop with on-the-fly frame generation and a bounded render-to-encode queue.
2. Remove or relax forced `gc.collect()` cadence and re-measure.
3. Refactor route reveal state so `Frame.draw()` is side-effect-free, then test threaded rendering safely.

## Non-Goals

- No GPU rewrite in this refactor.
- No template format changes.
- No frontend or Tauri integration changes beyond what is needed to keep backend rendering working.
- No attempt to replace ffmpeg subprocess usage in this workstream.

## Current Baseline

Reference implementation hotspots:

- Serial render/write loop: [backend/scene.py](backend/scene.py#L265)
- Blocking ffmpeg stdin write: [backend/scene.py](backend/scene.py#L274)
- Forced GC cadence: [backend/scene.py](backend/scene.py#L299)
- Prebuilt frame creation: [backend/scene.py](backend/scene.py#L1482), [backend/scene.py](backend/scene.py#L1500), [backend/scene.py](backend/scene.py#L1502)
- Route reveal shared mutable state: [backend/frame.py](backend/frame.py#L302)
- Route/elevation frame state generation coupled to `self.frames`: [backend/scene.py](backend/scene.py#L601), [backend/scene.py](backend/scene.py#L644)

Reference timing snapshot:

- `frame.draw` avg: 47.159 ms
- `ffmpeg.write` avg: 44.2 ms
- `frame.total` avg: 93.791 ms

Source: [backend/debug_render/phase_1/20260415_003334/timing_summary.json](backend/debug_render/phase_1/20260415_003334/timing_summary.json#L1)

## Invariants To Preserve

- Rendered overlays remain visually identical for a fixed GPX file and template, except where a deliberate performance refactor changes internal timing only.
- Cancellation still stops rendering and removes incomplete output.
- Debug output and timing summaries still work.
- The backend remains runnable with the current CLI entry points in [backend/main.py](backend/main.py#L1).

## Test Dataset

Use a single fixed render scenario throughout all phases so timing comparisons are meaningful.

Recommended inputs:

- GPX: [backend/uploads/evening_ride(2).1776101907221.gpx](<backend/uploads/evening_ride(2).1776101907221.gpx>)
- Template: [templates/safa_brian_a_4k_gradient.json](templates/safa_brian_a_4k_gradient.json)

Primary validation commands:

```powershell
pnpm lint:backend
```

```powershell
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Optional demo-frame smoke test:

```powershell
cd backend; uv run main.py demo -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json -second 30
```

## Deliverables

- Streamed frame generation for video export.
- Bounded producer-consumer queue between rendering and ffmpeg writes.
- Removal or configuration of forced GC cadence.
- Side-effect-free route reveal rendering.
- Experimental threaded rendering mode behind a safe opt-in.
- Updated timing artifacts for before/after comparison.

## Phase Plan

### Phase 0: Baseline Lock-In

Purpose:

- Create a reproducible baseline before changing control flow.

Tasks:

- Confirm the reference render command completes successfully on the current branch.
- Save or note one timing summary path to compare against later.
- Verify backend lint passes before starting refactor work.
- Record the current output filename, total frame count, and average timings.

Acceptance criteria:

- `pnpm lint:backend` passes.
- Reference render command completes.
- One timing summary is available for comparison.

Rollback boundary:

- No code changes in this phase.

### Phase 1: Decouple Frame State Generation From `self.frames`

Purpose:

- Make render asset preparation independent from prebuilt `Frame` objects.
- Prepare the codebase for streaming frame generation without breaking existing export.

Tasks:

- Introduce a small utility for total frame count derived from `seconds * fps`.
- Add index-based helpers that derive `second`, `frame_number`, and `activity_index` from a frame index.
- Refactor `build_route_frame_states()` to iterate by frame index instead of `for frame in self.frames`.
- Refactor `build_elevation_frame_states()` to iterate by frame index instead of `for frame in self.frames`.
- Where label text currently depends on a full `Frame` instance, either:
  - Create a temporary frame-like object from index data, or
  - Extract the label formatting logic into a helper that does not require persistent frame storage.
- Keep existing `self.frames` behavior in place for `render_demo()` and `draw_frames()` for now.

Files expected:

- [backend/scene.py](backend/scene.py)
- [backend/frame.py](backend/frame.py) if label formatting must be extracted

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Render completes with no functional regression.
- Timing summary still writes successfully.
- Route and elevation widgets visually match baseline.

Rollback boundary:

- All changes confined to frame-state preparation and helper extraction.

### Phase 2: Make Frame Construction Return A Frame Instead Of Appending

Purpose:

- Prepare a streaming export path while keeping legacy paths runnable.

Tasks:

- Change `build_frame()` so it returns a `Frame` object.
- Introduce a `build_frame_sequence()` or `iter_frames()` generator for export use.
- Keep `build_frames()` as a compatibility wrapper for demo/debug code by appending returned frames to `self.frames`.
- Update any code that assumes `build_frame()` mutates `self.frames` directly.

Files expected:

- [backend/scene.py](backend/scene.py)

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Export still works.
- Demo/debug workflows still work.
- No call site relies on implicit append behavior.

Rollback boundary:

- Frame creation API only; export loop still serial.

### Phase 3: Convert Export To On-The-Fly Sequential Rendering

Purpose:

- Stop prebuilding all frames before export, but do not add concurrency yet.

Tasks:

- Update `render_video()` so it no longer calls `build_frames()` up front.
- Update `export_video()` to consume frames from the generator sequentially.
- Replace `len(self.frames)` references in export with an explicit `total_frames` value.
- Keep progress callbacks, sample frame capture, and cancellation behavior intact.
- Confirm no part of the export path depends on `self.frames` anymore.

Files expected:

- [backend/scene.py](backend/scene.py)

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Export remains correct.
- Memory footprint does not grow with total frame count due to stored `Frame` objects.
- Timing summary still contains `frame.draw`, `ffmpeg.write`, and `frame.total`.

Rollback boundary:

- Streaming exists, but render and ffmpeg write are still sequential on one thread.

### Phase 4: Remove Or Relax Forced GC And Establish A New Baseline

Purpose:

- Measure whether explicit GC is harming latency.

Tasks:

- Remove the unconditional `gc.collect()` call in the export loop, or gate it behind a configurable cadence.
- If configuration is introduced, default it to disabled.
- Re-run the reference render at least twice.
- Compare average and max `frame.total` against Phase 3.
- If memory growth is unacceptable, reintroduce a much lower-frequency configurable collection interval.

Files expected:

- [backend/scene.py](backend/scene.py)
- [backend/constant.py](backend/constant.py) only if adding a constant is cleaner than scene-local config

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Export still completes.
- Memory remains acceptable.
- New timing summary exists for comparison.

Rollback boundary:

- Only GC behavior changed relative to Phase 3.

### Phase 5: Add Bounded Render-To-Encode Queue

Purpose:

- Overlap rendering and ffmpeg writes without changing visual output.

Tasks:

- Introduce a bounded queue of rendered frame payloads.
- Add a dedicated encoder worker thread that owns `p.stdin.write(...)`.
- Keep the producer on the main thread initially.
- Use a sentinel object for clean shutdown.
- Add shared error propagation so worker failures stop the producer immediately.
- Keep ffmpeg stderr monitoring intact.
- Keep `ffmpeg.write` timing inside the encoder worker.
- Keep `frame.draw` timing in the producer.
- Ensure cancellation stops both threads and removes partial output.

Queue requirements:

- Start with a small queue depth such as 4.
- Do not let queue size scale with video length.

Files expected:

- [backend/scene.py](backend/scene.py)

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Export completes successfully.
- Visual output matches baseline.
- `frame.total` improves measurably versus Phase 4.
- No deadlocks, orphaned ffmpeg processes, or incomplete files on cancellation.

Rollback boundary:

- Concurrency limited to render producer plus single encode worker.

### Phase 6: Refactor Route Reveal State To Be Side-Effect-Free

Purpose:

- Remove shared mutable rendering state so `Frame.draw()` becomes deterministic, frame-order independent, and easier to optimize further.
- Prepare the draw path for render-side work now that `qscale` plus encoder-side serialization have moved the bottleneck away from ffmpeg and back to `frame.draw`.

Tasks:

- Remove runtime mutation of `route_cache.reveal_mask` and `route_cache.last_revealed_state_index` from the draw path.
- Replace `update_route_reveal_mask()` with a pure helper that computes the needed reveal mask for a given frame state, or selects it from immutable precomputed data.
- Slim `RouteWidgetCache` so it contains only immutable render assets and frame-state arrays.
- Re-check whether route reveal work can be reduced as part of the purity refactor, for example by selecting immutable prebuilt reveal assets instead of mutating masks incrementally.
- Ensure rendering a frame out of order produces the same output as rendering it in sequence.
- Keep the current fast ProRes settings fixed during this phase so timing changes reflect draw-path work, not encoder variation.

Files expected:

- [backend/frame.py](backend/frame.py)
- [backend/render_assets.py](backend/render_assets.py)
- [backend/scene.py](backend/scene.py) if asset preparation must change

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Additional determinism check:

- Render a few representative frames in non-sequential order in a small ad hoc script or test harness and compare output hashes with sequential renders.

Acceptance criteria:

- Export still matches baseline visually.
- Route reveal output is frame-order independent.
- No shared mutable route rendering state remains in the hot draw path.
- Timing review shows whether the purity refactor reduces `frame.draw` directly, or at minimum leaves it no worse while enabling the next experiment.

Rollback boundary:

- Draw remains single-threaded, but is now pure with respect to route reveal state.

### Phase 7: Add Experimental Threaded Rendering

Purpose:

- Test whether draw-side throughput improves once the draw path is deterministic.
- Treat this as an experiment, not a guaranteed landing phase, because the current bottleneck is render-side draw work and Python/Pillow threading may or may not help in practice.

Tasks:

- Add an opt-in threaded rendering mode behind a flag or scene config.
- Keep the qscale-based ProRes settings fixed while evaluating threaded rendering.
- Use a small `ThreadPoolExecutor` for frame rendering only.
- Keep ffmpeg writes owned by the single encoder worker thread.
- Keep encoder-side serialization and `stdin.write(...)` in the encoder worker thread.
- Keep output ordering stable by buffering completed render results until their write turn arrives.
- Start with 2 workers, then 4 if stable.
- Capture timings for sequential mode and threaded mode using the same test dataset.
- Compare `frame.draw`, `frame.total`, `total_time_taken`, `queue.put_wait`, and `encoder.queue_wait` between modes.
- If threaded rendering does not improve `total_time_taken` or materially reduce producer-side `frame.total`, stop and document the rejection rather than expanding the threading model further.

Files expected:

- [backend/scene.py](backend/scene.py)
- [backend/frame.py](backend/frame.py) only if minor concurrency-safe adjustments are needed

Validation after phase:

```powershell
pnpm lint:backend
cd backend; uv run main.py render -gpx uploads/evening_ride(2).1776101907221.gpx -template ..\templates\safa_brian_a_4k_gradient.json
```

Acceptance criteria:

- Threaded mode is opt-in and stable.
- Output remains visually identical.
- Timing summary shows clearly whether threaded rendering helps or not against the current qscale baseline.
- If no improvement is observed, sequential queue-based export remains the default.

Rollback boundary:

- Threaded mode can be disabled without affecting previous phases.

## Measurement Checklist Per Phase

After each code-changing phase:

1. Run `pnpm lint:backend`.
2. Run the reference render command.
3. Confirm output video exists and is playable.
4. Compare timing summary against the prior phase.
5. Sanity-check route reveal, elevation widget, text overlays, and alpha output.

Metrics to track:

- `frame.draw` average and max
- `ffmpeg.write` average and max
- `frame.total` average and max
- Total render wall-clock time
- Approximate process memory usage at start, midpoint, and end

## Suggested PR Breakdown

PR 1:

- Phase 1 and Phase 2

PR 2:

- Phase 3 and Phase 4

PR 3:

- Phase 5

PR 4:

- Phase 6

PR 5:

- Phase 7

This keeps each reviewable unit small enough to verify independently.

## Exit Criteria

The refactor is complete when:

- Video export does not require a prebuilt `self.frames` list.
- Render and ffmpeg write are decoupled through a bounded queue.
- Forced GC is either removed or justified by measurement.
- Route reveal rendering is deterministic and side-effect-free.
- Threaded rendering has been evaluated with real measurements, with either:
  - A stable default if beneficial, or
  - A documented rejection if not beneficial.
