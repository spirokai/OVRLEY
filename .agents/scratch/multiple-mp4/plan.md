# Multiple MP4 Clips Implementation Plan

Status: ready-for-agent
Last updated: 2026-08-13
Depends on: `spec.md`

## Delivery Strategy

Implement the feature as a sequence of contract migrations. Each phase must leave one canonical model at its boundary; do not retain singleton video fields as compatibility aliases. Keep the existing single-video composite pipeline intact and build batch orchestration around it.

Do not run a production build without explicit user permission. Verification should use targeted Vitest suites, Tauri-shell Rust tests, and `ovrley_core` Rust tests.

## Phase 1: Canonical Video Entity and Timeline Utilities

### Work

1. Define the validated frontend video entity and telemetry-state contract.
2. Replace singleton imported-video state with `importedVideos` and ID-addressed actions.
3. Keep external activity as a separate global entity; remove the global embedded-telemetry stash lifecycle.
4. Add pure utilities for:
   - Creation/sync-time ordering with picker-order fallback.
   - Exact frame-grid duration and contiguous MP4-only placement.
   - Timeline-sorted display/export order.
   - Active, upcoming, previous, and final clip resolution.
   - Timeline bounds across activity and videos.
   - Automatic offset calculation per clip.
   - Deterministic forward overlap sweeping.
   - Manual non-overlap validation.
   - Global export-range intersections.
   - Effective external or clip-local telemetry context.
5. Add transient drag state keyed by video ID without storing derived active/selected video state.

### Tests

- Replace singleton slice tests with collection behavior tests.
- Add table-driven utility tests for ordering, exact boundaries, gaps, overlap recovery, manual reorder, removal, range intersection, and telemetry resolution.
- Prove activity replacement recalculates all clips and activity clearing rebuilds contiguous placement.
- Prove late telemetry results cannot mutate a removed entity.

### Completion Gate

- No canonical imported-video singleton fields or global stashed video telemetry remain.
- Collection state is valid by construction and derived order/active clip are not duplicated.

## Phase 2: Strict Source-Video Probe Contract

### Work

1. Extend native source-video metadata with exact frame count, average rational frame rate, and nominal rational frame rate.
2. Validate the CFR contract once at probe ingress.
3. Reject missing, zero, malformed, variable, or indeterminate timing data.
4. Preserve coded resolution and rotation metadata while exposing canonical display-oriented dimensions.
5. Add collection compatibility validation against the first successful picker-order clip or the existing collection reference.

### Tests

- Extend video-probe fixtures/tests for CFR, VFR, absent frame count, mismatched rates, rotation, and invalid metadata.
- Test exact rational comparisons rather than floating FPS comparisons.
- Test partial import compatibility outcomes without repairing incompatible metadata.

### Completion Gate

- Every committed video provides exact frame count, exact shared rational FPS, and matching display-oriented resolution.

## Phase 3: Multi-Registration Preview Server

### Work

1. Replace the server's current-video slot with an import-ID-keyed registry.
2. Change registration, removal, clear, and diagnostic commands to collection semantics.
3. Preserve the existing loopback port, worker pool, opaque URLs, range responses, and filesystem-path validation.
4. Ensure registering a new clip never invalidates another clip's URL.
5. Ensure removing one clip invalidates only its URL.

### Tests

- Extend Tauri video-server integration tests for multiple simultaneous registrations and range requests.
- Cover removal isolation, clear-all, unknown/stale IDs, missing source files, HEAD, and byte ranges.

### Completion Gate

- One server can serve every imported video independently and safely.

## Phase 4: Atomic Multi-File Import and Telemetry Queue

### Work

1. Add a multiple-path file-dialog helper while preserving last-directory behavior.
2. Probe/register all selected files behind the existing blocking loader.
3. Reject duplicate canonical paths and incompatible/VFR files per file.
4. Do not expose partial successes while import is running.
5. Commit all successful video entities together after every selected file settles; report failures per file.
6. Apply mode-specific placement after atomic commit:
   - MP4-only: re-sort the complete collection and rebuild contiguously.
   - External activity: calculate only new offsets, keep new clips fixed, and push conflicting existing/later clips forward.
7. Start a sequential, timeline-ordered telemetry extraction queue after commit.
8. Address extraction results by stable video ID and continue after absent/failed telemetry.
9. Make removal race-safe without introducing telemetry-extraction cancellation.

### Tests

- Cover blocking lifecycle, atomic visibility, partial success, all-failed import, duplicates, compatibility failures, and additions to an existing collection.
- Cover sequential extraction ordering, continued processing after failure, and removal before completion.

### Completion Gate

- Multi-file import is atomic from the editor's perspective, while telemetry remains independently asynchronous.

## Phase 5: Video Toolbar and Multi-Clip Timeline

### Work

1. Replace the obsolete video/video-sync settings panel with a dedicated videos toolbar.
2. Render one presentational row per timeline-sorted clip, including metadata, telemetry state, sync diagnostics, offset input, and removal.
3. Enable offset input only with external activity.
4. Route all row mutations through ID-addressed hook actions.
5. Render multiple clip segments on one timeline video track with stable `video:<id>` identities.
6. Update viewport fitting and total/minimum timeline calculations to consume the collection.
7. Update clip selection, keyboard nudge, snapping, and drag behavior for arbitrary IDs.
8. Allow transient drag overlap and committed reordering; reject invalid releases without rippling neighbors.
9. Keep toolbar order stable during drag and re-sort after a valid commit.
10. On removal of the playing clip, pause first, apply mode-specific reflow, and clamp the playhead.

### Tests

- Add toolbar behavior tests using canonical entities.
- Extend timeline lane, overlay player, clip-drag, geometry, viewport, and keyboard tests to multiple clips.
- Assert behavior and accessibility identities, not visual styling.

### Completion Gate

- Every clip is independently visible/editable and timeline behavior no longer assumes an ID of `video`.

## Phase 6: Dual-Element Preview Playback

### Work

1. Refactor preview source resolution to derive active and upcoming clips from the playhead.
2. Introduce two reusable video elements with explicit active/preload roles.
3. Preload the next timeline clip and hand off at exact frame-grid boundaries.
4. Keep the active video element as playback clock only while the playhead is inside its clip.
5. Use the timeline clock through external-activity gaps while freezing the upcoming clip's first frame.
6. Freeze the last clip's final frame after the final clip.
7. For direct jumps into unloaded clips, commit the playhead immediately, hide stale footage, load/seek the target, and reuse the existing loading presentation.
8. Resolve MP4-only effective telemetry and local time from the active clip without mutating global parsed activity on every frame.
9. Preserve mute state and warning behavior across element role swaps.

### Tests

- Extend playback engine and preview hook tests for boundary handoff, clock ownership, preload swaps, gaps, direct jumps, final freeze, mute, warnings, and removal.
- Use mocked media elements/events; keep one small native-WebView manual verification for actual decoding continuity.

### Completion Gate

- Contiguous clips play with a hard cut and external gaps follow the agreed frozen-frame behavior using constant player resources.

## Phase 7: Canonical Batch Render API

### Work

1. Define one strict batch request containing:
   - Shared effective render configuration and encoding settings.
   - One optional shared external activity.
   - One global export range and concat mode.
   - Timeline-ordered clip jobs with source timing and optional embedded telemetry.
2. Prepare the request in the frontend from one stable export snapshot after all telemetry extraction settles.
3. Intersect the global export range with clips on the shared frame grid and omit zero-overlap clips.
4. Extend the backend render controller from one render lifecycle to one batch lifecycle with batch ID, current job identity/index, total frame work, outputs, and concat phase.
5. Validate all required data once at ingress before starting background work.
6. Execute clip jobs sequentially through the existing single-video composite pipeline.
7. Map external activity time from each clip's committed timeline offset.
8. Map MP4-only telemetry through clip-local time.
9. Add an explicit optional-activity render path that emits empty overlay frames without fabricating parsed activity.
10. Stop on first failure and preserve completed outputs.
11. Route cancellation to the active clip job and delete only incomplete artifacts.

### Tests

- Test frontend request preparation and strict omission of singleton aliases.
- Add Rust batch-controller tests with injected job execution for sequencing, validation, frame-weighted progress, failure, cancellation, and cleanup.
- Extend composite timing tests for shared external activity, independent embedded activity, trimmed range intersections, video beyond activity coverage, and no-activity blank overlays.

### Completion Gate

- One submitted batch renders every intersecting clip portion sequentially with one correlated progress/cancellation lifecycle.

## Phase 8: Optional Concatenation

### Work

1. Add concat preflight to the batch validator:
   - All included clips have audio or none have audio.
   - Shared video settings are fixed by the batch.
2. Normalize present audio to one explicit AAC sample rate and channel layout for every intermediate.
3. Probe completed intermediates before concat and strictly compare dimensions, exact rational FPS, time base, codec stream compatibility, and audio shape.
4. Build a safely escaped FFmpeg concat-demuxer manifest in a temporary location.
5. Concatenate with stream copy and no artificial timeline gaps.
6. Support cancellation during concat and remove incomplete final output/manifest.
7. Retain intermediates on failure/cancellation.
8. Verify successful final output before removing intermediates.
9. Bypass concat for a one-job batch and write that job directly to the final combined-output target.

### Tests

- Test mixed-audio rejection, normalized audio args, actual-output incompatibility, manifest escaping, stream-copy invocation, cancellation, cleanup, retention, and one-job bypass.
- Use FFmpeg integration coverage only where command-builder and fake-process tests cannot prove the contract.

### Completion Gate

- Concatenated export produces one verified stream-copy result, and every failure/cancellation path leaves recoverable completed work.

## Phase 9: Render UI Integration and Singleton Removal Audit

### Work

1. Update the render dialog to describe the global range, number of included clips, separate/concatenated mode, compatibility status, and aggregate progress.
2. Disable export while any imported clip's telemetry extraction remains queued or active.
3. Open the output folder once for successful separate-file batches.
4. Open the final video for successful concatenated batches.
5. Present the failing clip identity and retained outputs after failure/cancellation.
6. Remove obsolete singleton selectors, props, tests, command names, and comments.
7. Search the frontend and backend for remaining singleton imported-video assumptions and migrate each owner directly to collection or ID-addressed behavior.
8. Update architecture documentation where it describes singleton preview registration, video state, or render lifecycle.

### Tests

- Extend render dialog/workflow tests for availability gating, batch summaries, progress correlation, output behavior, failure, cancellation, and retained outputs.
- Run all targeted frontend video/player/render suites and all Rust video-server/probe/composite/batch suites.
- Run frontend lint and formatting checks only after implementation stabilizes.

### Completion Gate

- No production consumer depends on the old singleton imported-video contract.
- Documentation and tests describe the collection and batch behavior consistently.

## Verification Matrix

| Area | Primary automated seam | Required behavior |
| --- | --- | --- |
| Video state | Zustand slice tests | Canonical entities, activity modes, placement, validation, telemetry ownership |
| Timeline | Pure utility and player-hook tests | Exact boundaries, ordering, active/upcoming resolution, drag/reorder, gaps |
| Toolbar | Hook/component tests | Full list, ID-addressed edits, errors, removal |
| Preview HTTP | Tauri integration tests | Many registrations, independent URLs/ranges, isolated removal |
| Probe | `ovrley_core` integration tests | CFR/frame-count contract, exact FPS, display resolution |
| Playback | Video-preview/player hook tests | Two-element handoff, clocks, jumps, frozen frames |
| Batch request | Frontend render tests | Shared config/activity, ordered clip jobs, global range |
| Batch runtime | Rust controller/pipeline tests | Sequential work, progress, failure, cancellation, outputs |
| Concatenation | FFmpeg builder/integration tests | Compatibility, normalized audio, stream copy, cleanup |
| Native behavior | Manual WebView/FFmpeg pass | Real hard cuts, HEVC, same-camera session, audio, output opening |

## Final Acceptance Checklist

- Multiple compatible MP4 files import together and remain independent entities.
- Incompatible, duplicate, VFR, and failed files are reported per file without discarding valid imports.
- MP4-only placement is creation-time ordered, picker-stable, contiguous, and frame-exact.
- External activity independently synchronizes clips and deterministic recovery prevents committed overlap.
- The videos toolbar and timeline can edit/reorder every clip without singleton state.
- Preview uses one server and two reusable elements, including hard cuts and agreed gap freezes.
- External and embedded telemetry follow the agreed precedence and independence rules.
- One global export range produces sequential separate outputs or one optional stream-copy concatenation.
- Missing embedded telemetry produces empty overlays rather than a failed clip.
- Progress, cancellation, failure retention, success cleanup, and output-opening behavior match the specification.
- Targeted frontend and Rust tests pass.
- No production build was run without explicit permission.
