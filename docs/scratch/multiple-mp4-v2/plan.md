# Multiple Video Clips Implementation Plan

Status: needs-info (draft for owner review)
Last updated: 2026-10-15
Depends on: `spec.md`, `notes.md` (this folder). Previous version: `../multiple-mp4/`.

## Delivery Strategy

Ship everything together from the `feature/multi-video` branch: loading multiple clips without previewing and exporting them together has no product value. Inside the branch, work proceeds in the milestones below; each milestone ends with green targeted tests so the branch never sits broken for long.

Commit policy (owner decision): the branch may contain separate commits per feature/milestone for easier review, but **the agent never commits**. At a suitable point (usually a milestone end with green tests) the agent says that it is OK to commit and proposes a commit message. The owner reviews the changes and the message, commits, and then tells the agent to continue.

Rules that apply throughout:

- The canonical model (including deleted parts and transitions) is designed in M2 so later milestones never migrate state or the project format again.
- No singleton aliases or compatibility adapters (see `AGENTS.md`). The M2 migration of singleton consumers is therefore done in one pass.
- Transitions are isolated in M8. Everything before it works with hard cuts, so transition risk cannot block the rest.
- Never run a production build without explicit permission. Validate with `npx vitest run` (from `app/`), `cargo test` in `src-tauri/ovrley_core`, and the Tauri crate tests.

## M1 — Native groundwork

1. Video server: replace `current: Option<CurrentVideo>` with a registry keyed by import ID (`src-tauri/src/video_server.rs`). Register, remove one, clear all, diagnostics for all.
2. Tauri commands: `backend_register_preview_video` / `backend_import_preview_video` add a registration; `backend_remove_preview_video(importId)`, `backend_clear_preview_videos`, `backend_get_preview_videos` (`src-tauri/src/tauri_commands.rs`, `app/src/api/backend.js`). Single-video frontend callers remove the previous registration only after the new state is committed.
3. ~~Probe changes~~ — not needed natively (decided during M1). The probe already returns exact `fpsNum/fpsDen`, `hasAudio`, resolution, and rotation. `nb_frames` is absent in MKV and other containers, so frame count is derived once at frontend ingress in M2 as `round(duration × fpsNum / fpsDen)`. VFR files stay accepted as today (no new rejection).

Tests: video-server integration (multiple registrations, ranges, isolated removal, unknown IDs, clear-all); project-load replacement order.

Status: committed (`17fd29ef`).

## M2 — Canonical model, program derivation, persistence

Delivered as three commits: **M2a** pure utilities (item 4), **M2b** model switch (items 1–3, 5–7, 9), **M2c** project format v4 (item 8).

M2a status: committed (`0286c401`). M2b status: committed (`af7c48a4`). Decisions made during implementation:
- Canonical state is split: `importedVideos` (validated source entities: metadata, preview registration, telemetry, sync warning) and `videoEdits[videoId]` (`offsetSeconds`, `deletedParts`, `transitionIn`). Timeline clips are the derived join (`joinVideoEdits`).
- Undo tracks `videoEdits` (replacing `videoSyncOffsetSeconds`). Collection changes run through `changeVideoCollection`, which rebases history so undo never resurrects removed clips and keeps edits of remaining clips.
- `parsedActivity`/`activitySummary` hold only the external activity. Embedded telemetry stays on each clip; without an external activity widgets use the telemetry of the clip shown at the playhead in clip-local time (`useActivityContext`). The timeline activity lane now shows only an external activity.
- Drag preview is `{ videoId | null, deltaSeconds }`; dragging the activity lane shifts every clip. Snapping includes other clips' edges.
- Preview still uses one `<video>` element: it shows the clip at the playhead (upcoming clip in gaps, last clip after the end) and hands over through the timeline clock (dual-element hand-off is M4).
- Current-video render and project format v3 hold at most one clip until M6/M2c; multi-file import arrives in M3, so this limit is unreachable from the UI.
- Exact `fpsNum/fpsDen` are required at import; the float-FPS fallback in render config is removed.

M2c status: committed (`4a5d0bec`). Decisions made during implementation:
- v4 layout: `sources.videos[]` (import order) = `{ id, path, offsetSeconds, deletedParts[{ id, fromSecond, toSecond, transition }], transitionIn }`; a transition is `null` (follows the defaults) or `{ type, durationSeconds }` (override; `none` has duration 0). Top-level `transitionDefaults`. `sync.videoOffsetSeconds` is gone; `sync.manual.videoId` names the clip that owns the landmarks (matches the M2b workspace model).
- Clip IDs are persisted; v3 → v4 generates one UUID for the single video. Deleted-part IDs are unique across the project (join keys).
- Rust validates structure and the transition catalogue (Rust copy, tested against the JS catalogue); the frontend checks deleted parts and landmarks against probed durations.
- Missing videos are recovered one by one (dialog names the file); a skipped clip drops its edits, and its landmarks if it owned them.
- Backup: Rust copies a pre-v4 target to `<name>.oly.v<old version>.bak` before the first overwrite and returns `backupPath`; the frontend shows a one-time dialog. "Save As" elsewhere leaves the original untouched.
- v4 may still change inside the branch (owner, 2026-10-16): `render` (render target, batch folders) stays as in v3 until M3 removes the folder batch and M6 adds output mode; placement source/anchor is added with M3's placement prompt. Batch-folder import on open lands in M3.

1. Replace singleton fields in `createVideoImportSlice.js` with `importedVideos[]` and ID-addressed actions (add batch, remove, set clip offset, set deleted parts, set transition). Session-level sync actions (timezone mode, shared offset, filename timestamps, reset) keep their current semantics and apply to every clip.
2. Move embedded telemetry into the entity (remove global stash in `createMediaSlice.js`).
3. Keep the time chain and timezone inference untouched (`media/video_probe.rs`, `media/mp4_telemetry/mod.rs`, `media/prepared_video.rs`, `lib/video-sync.js`). Only change `resolveVideoSyncState`: a non-overlapping clip keeps its signed offset with a "no activity coverage" warning instead of offset 0. Apply it per clip.
4. Pure utilities in `app/src/lib/video-program/` — **M2a commit**:
   - `frame-grid.js`: `secondsToFrames`, `framesToSeconds`, `fpsEquals` (exact rationals)
   - `transition-catalog.js`: 58 `xfade` names, `DEFAULT_TRANSITION_SETTINGS` (`wipeleft`, 2 s, threshold 2 s, `dialogShown: false`), `HARD_CUT`
   - `video-collection.js`: `sortVideosByTimeline`, `getVideoEndSecond`, `getVideosTimelineBounds`, `deriveCompatibility` (largest resolution+FPS group compatible, ties → earliest import)
   - `program.js`: `buildProgram` (pieces after deleted parts + export range on the frame grid; overlaps reported and resolved for playback; joins keyed `clip:<videoId>` / `deletedPart:<partId>`; stored transition `null` = default; default rule gap ≤ threshold → hard cut, else project default, hard cut if it does not fit; `durationSeconds` = Σ pieces − Σ transitions), `findInvalidTransitions`, `resolveProgramPosition` (before / piece / gap / transition window / after)
   - Detected-time placement and user anchors move to M2b next to timestamp parsing in `lib/video-sync.js`.
5. Manual landmark sync works on a chosen clip (`createManualVideoSyncSlice.js`); the resulting correction is a shared shift like the shared offset (notes O10).
6. Shared offset: changing it shifts every clip by the same delta and keeps each clip's individual adjustment; per-clip edits change only that clip.
7. Migrate every singleton consumer (≈46 production files, ≈31 test files) to the collection or to derived program values.
8. Project format v4 in `src-tauri/src/project_file.rs` (+ `features/projects/utils/projectSnapshot.js`, `projectHydration.js`), with v3 → v4 migration at ingress. Includes project transition defaults (`type` `wipeleft`, `durationSeconds` 2, `splitGapThresholdSeconds` 2, `dialogShown` false) and per-join transition type/duration. Existing-project handling (spec → Persistence):
   - silent in-memory conversion on open (extend the `migrate_project` chain);
   - batch-folder projects: import the folder's videos into the timeline, switch to separate-file export, keep the output folder, show a notice (the folder import itself lands with M3's multi-import path);
   - first save of a converted project keeps `<name>.oly.v3.bak` and tells the user once that older versions cannot open v4.
9. Undo/redo: keep the current behaviour (import outside editor history); review once all milestones are done.

Tests: table-driven utility tests; slice tests; project migration tests.

## M3 — Import and editing UX

Delivered as four commits: **M3a** multi-file import (items 1 import part, 2), **M3b** post-import dialogs and placement anchors (items 3, 3a, 4 dialog), **M3c** videos panel, clip removal and the shared video lane (items 4 timeline/panel part, 5, 6, 7), **M3d** folder batch removal, batch sources from the collection, and v3 batch-folder projects (item 1 batch part, M2c deferral).

M3a status: implemented (awaiting owner commit). Decisions made during implementation:
- Adding files never re-places existing clips. New clips are placed by detected time plus the shared correction of the collection (committed − detected offset of the earliest existing clip whose time can be determined; zero without one) — the same reference-correction idea as batch calibration. Activity changes, timezone mode, and filename-time actions still re-place every clip as before.
- Placements report `determined`; undetermined clips still start at 0 with a warning until M3b's placement prompt.
- Files are probed and registered one at a time; the successful ones are committed in one collection change. Non-videos, probe/validation failures, and paths already in the collection (or repeated in the selection) are reported per file in the import dialog. The dialog also lists incompatible clips when an import makes the collection incompatible in a new way.
- Only the first import into an empty collection sizes the scene and render FPS (from the compatible group).
- Telemetry extraction is a module-level sequential queue in timeline order; it no longer holds the import loader.

1. Single import button → multi-select dialog (`lib/file-dialog`, `useVideoImport.js`). Remove the folder batch queue (`createBatchRenderSlice.js` folder state, `useBatchInspection.js` folder listing, `BatchRenderQueue*`) and source batch jobs from the collection.
2. Blocking import: probe + register all, atomic commit, per-file failure report. Compatibility warning shown immediately in the import result (incompatible clips listed with resolution/FPS; they are still imported).
3. Placement prompt for clips whose time cannot be determined ("before/after X").
3a. Transition defaults dialog (spec → "Transition defaults dialog"): shown once per project when an import makes the collection contain more than one video; transition type (all `xfade` names + "No transition"), duration, continuity threshold; preselected `wipeleft`, 2 s, 2 s. Needs the transition catalogue as data in this milestone (plain list; the animated picker comes in M8).
4. Overlap warning dialog after placement; conflict band on the timeline; per-clip warnings. Single-file export blocked while any overlap exists (separate-file export still allowed); deleting the overlapping part (M7) or moving a clip resolves it.
5. Videos panel: one row per clip (metadata, time source, telemetry state, per-clip offset input, compatibility/overlap/no-coverage warnings, remove). Session-level sync controls (timezone, shared offset, filename timestamps) stay where they are today and apply to all clips.
6. Remove whole clips from the timeline (panel action, timeline context/keyboard delete). Compatibility and overlaps are re-derived; single-file export re-enables when no incompatible/overlapping clips remain.
7. Timeline: many clips on the video lane (`useTimelineClips.js`, `useClipDrag.js`, `timelineGeometry.js`, `timelineViewport.js`, `playerTiming.js`), snapping to activity bounds and clip edges, IDs `video:<id>`.

Tests: import hook, panel, timeline lane/drag/geometry tests.

## M4 — Sequenced preview playback

1. Two reusable video elements with active/preload roles (`useVideoPreview.js`, `useVideoPlaybackClock.js`, `usePlaybackEngine.js`, `videoPreviewSource.js`).
2. Continue at piece end → next piece start, skipping gaps and deleted parts; hard cut at every join (transition windows are added in M8).
3. Direct jumps to unloaded clips; deleted-part indicator while scrubbing. Playback skips gaps; scrubbing into a gap shows the next piece's first frame frozen.
4. Overlaps (preview only): play the earlier clip to its end, then continue the later clip at the matching source position. Show a very visible warning on the player for the whole time the playhead is inside an overlap.
5. Removal of the playing clip: pause, remove, clamp playhead.

Tests: playback engine / preview hooks with mocked media elements. Manual: native WebView (WebKitGTK, WebView2) hand-off and dual 4K/HEVC decode.

## M5 — Rasters-only outside activity coverage

1. Native: frames without activity coverage render rasters only instead of `pixels.fill(0)` (`ovrley_core/src/render/mod.rs` `prepare_output_frame`, `encode/video_timing.rs` coverage incl. trailing frames). Reuse the raster-only widget filtering that `skip_overlay` already does (`render_jobs/planning.rs`).
2. Composite planning: allow clips/pieces with no activity overlap (remove the "does not overlap activity" error path in `derive_composite_render_plan` and the batch equivalent) — coverage becomes empty, overlay is rasters only.
3. Frontend preview: hide non-raster widgets when the playhead has no activity coverage.
4. Same rule without external activity: coverage is the clip's own embedded telemetry; clips/frames without it show rasters only.

Tests: render frame mapping tests, planning tests for zero/partial coverage, preview model tests.

## M6 — Export (hard cuts)

1. Render request from the program: shared template/encoding, optional shared external activity, output mode (`separate` | `combined`), output groups of pieces (`renderRequest.js`, `renderConfig.js`, `useRenderWorkflow.js`, `useRenderExecution.js`).
2. Extend `render_jobs` (contracts, submission, planning, batch, batch_state, execution) from per-file jobs to per-piece jobs grouped into outputs. Remove folder-source assumptions.
3. Concat step: concat demuxer with stream copy, TS/Annex-B remux, escaped manifest, verify output (probe frames/duration) before deleting intermediates.
4. Combined audio pass (`atrim`/`concat`/`anullsrc`), mux with `-c:v copy`.
5. Transparent mode: hard cuts only, combined or separate.
6. Progress weighted by output frames, current clip identity, cancellation during render/concat/audio, cleanup and retention rules.
7. Render dialog: output mode selector, included clips summary, disabled reasons (overlap, incompatibility, pending telemetry, invalid transitions).

Tests: request preparation; Rust batch lifecycle with injected executor; command builders; FFmpeg integration with tiny synthetic clips asserting durations/frame counts/audio length.

## M7 — Deleted parts

1. UI: mark range at playhead (in/out) or by dragging on a clip; edit/remove ranges; shaded on timeline; list in videos panel.
2. Join at each deleted part with transition selector (default rule uses deleted length as gap).
3. Data already flows through `buildProgram` (M2), preview skipping (M4), and piece rendering (M6).

Tests: slice/utility tests for edits and program effects; timeline interaction tests.

## M8 — Transitions

1. Transition catalogue (58 `xfade` names) already exists as data from M3. Project transition defaults (type, duration, split gap threshold) are editable in the project settings; changes re-derive only joins still on their default. Every join can override type and duration.
2. Picker popup with an animated preview per transition (like https://trac.ffmpeg.org/wiki/Xfade). Previews generated by a script using the bundled FFmpeg from neutral sample frames (do not copy the wiki GIFs — licensing), committed as small animated WebP assets.
3. Per-join selection in the videos panel and on the timeline join marker; immediate validation of durations.
4. Transition proxies: native command that renders `xfade(tail(A), head(B))` + `acrossfade` from the sources at preview resolution (short GOP), registered in the preview server; frontend queue (priority by distance to playhead), content-key cache, invalidation, cleanup.
5. Player transition window: play the proxy over the last `d` s of the outgoing piece, draw outgoing overlay at playhead time and incoming overlay at `incoming.timelineIn + elapsed` cross-faded by progress, then jump to `incoming.timelineIn + d`. Hard cut + "rendering" indicator while the proxy is not ready.
6. Render: head/body/tail split for pieces next to transitions; head/tail and the `xfade` transition parts use the same codec and encoder settings as every other part of the output (no separate intermediate codec — only the `d`-second transition regions are encoded twice); `acrossfade` in the audio pass.

Tests: catalogue/validation tests; proxy key/invalidation tests; transition-window timing tests (overlay times, jump target); command builder tests; FFmpeg integration asserting `Σ − Σd` duration for video and audio.

## M9 — Audit, docs, manual pass

1. Search for leftover singleton assumptions (`importedVideo*`, `videoSyncOffset*`, `composite_video_path` single-source assumptions, `batchVideoFolder`).
2. Update `docs/ARCHITECTURE.md` (preview registry, video model, render lifecycle) and README features.
3. Lint/format frontend; full targeted test runs.
4. Manual verification matrix (see `notes.md`).

## Final Acceptance Checklist

- One import button loads one or many files; incompatible clips are imported with an immediate warning, and single-file export re-enables once they are removed.
- Time detection and timezone/offset handling are unchanged; clips are placed by detected time, including before/after the activity with their real offset; undeterminable clips are placed by the user.
- Overlaps warn immediately and block combined export until resolved.
- Preview plays the whole program, skipping gaps and deleted parts, and shows transitions via FFmpeg proxies.
- Deleted parts and transitions are editable per join; defaults follow the split gap rule.
- Separate and combined exports (composite and transparent) render everything in one operation.
- Overlays stay synchronized in every mode; combined duration equals `Σ pieces − Σ transitions`.
- Frames without activity coverage show raster widgets only.
- Projects save and reload the full session; v3 projects migrate.
- Targeted frontend and Rust tests pass; no production build was run without permission.
