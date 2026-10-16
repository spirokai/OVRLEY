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

M3a status: committed (`d7818199`). Decisions made during implementation:
- Adding files never re-places existing clips. New clips are placed by detected time plus the shared correction of the collection (committed − detected offset of the earliest existing clip whose time can be determined; zero without one) — the same reference-correction idea as batch calibration. Activity changes, timezone mode, and filename-time actions still re-place every clip as before.
- Placements report `determined`; undetermined clips still start at 0 with a warning until M3b's placement prompt.
- Files are probed and registered one at a time; the successful ones are committed in one collection change. Non-videos, probe/validation failures, and paths already in the collection (or repeated in the selection) are reported per file in the import dialog. The dialog also lists incompatible clips when an import makes the collection incompatible in a new way.
- Only the first import into an empty collection sizes the scene and render FPS (from the compatible group).
- Telemetry extraction is a module-level sequential queue in timeline order; it no longer holds the import loader.

M3b status: committed (`bb8c5b29`). Decisions made during implementation:
- `videoEdits[id].placementAnchor` = `null | { videoId, side: 'before'|'after' }`, persisted in v4 (`sources.videos[].placementAnchor`; Rust checks references and cycles). A full sync uses the clip's detected time whenever it can be determined and falls back to the anchor only when it cannot; anchoring moves the clip next to its anchor and clips anchored to it follow. Anchors to videos skipped on project load are dropped (the clip keeps its offset).
- Follow-up steps after an import run one dialog at a time: report → placement per undeterminable clip → transition defaults → overlaps. Each step is checked when reached. A placement prompt offers every clip except clips still waiting for their own placement; with no candidates (first undeterminable clip on an empty timeline) the clip stays at 0 and anchors the next one. Skipping leaves the clip at 0 with its warning.
- The placement answer is committed as part of the import (`changeVideoCollection`), so undo never moves the clip back to 0.
- Transition defaults dialog: due when the collection has more than one video and `dialogShown` is false; OK validates and stores the values, dismissing stores the current (preselected) values; both set `dialogShown`. Editing the defaults in the project settings stays with M8.
- Overlap dialog lists only overlaps that involve clips of this import; the timeline conflict band and panel warnings come with M3c.

M3c status: committed (`48794576`). Decisions made during implementation:
- Per-clip warnings come from one utility (`lib/video-program/video-warnings.js`): incompatible format, overlaps (named on both clips), and the clip's sync warning (outside activity, unknown time). The videos panel and the timeline show the same list.
- Videos panel: one row per clip in timeline order (name, remove, warnings, "Start on timeline" offset input that moves only that clip, details incl. telemetry state); "Remove all videos" stays; session sync controls stay below.
- `removeImportedVideo`: drops the clip and its edits; clips anchored to it inherit its anchor (or lose it) and keep their position; manual sync moves to the first remaining clip; the playhead is clamped. The preview registration is released after the collection change; the last removal resets the background. Pausing playback of a removed playing clip stays with M4.
- Timeline: one video lane holds every clip (clip IDs stay `video:<id>`, so drag/nudge/snapping are unchanged) plus striped overlap bands; clips with warnings get a red outline and list the warnings in their tooltip. Delete/Backspace removes the selected video clip (shortcut `timelineClip.remove`). No timeline context menu yet (keyboard and panel cover removal).

M3d status: committed (`3e658b5a`). Decisions made during implementation:
- The folder batch queue is gone (video folder, folder listing, Clear, per-row removal). `renderTarget: batch` now renders every imported video to its own file in the output folder, inspected by path, in timeline order. The per-video "Activity overlay" toggle stays (`batchSkipOverlay`, keyed by video ID, not persisted). M6 replaces this with output groups of pieces.
- Each job renders at its clip's committed offset. Native contract fixed at its owner: `BatchActivity::ExternalActivity { activity, offsets }` (per source); the calibration reference, batch timezone mode, and calibration-source plumbing (inspection selection, output alias check) are removed, as are `createBatchCalibration`/`resolveBatchVideoTiming`. With an external activity a video outside it is blocked (until M5's rasters-only rule); an unreadable camera timestamp only hides the row's date.
- Project v4 `render` drops `batchVideoFolder` (and `renderTarget` is required). Reading a v3 project with `renderTarget: batch` returns `legacyBatchVideoFolder`; after the project is applied the frontend imports that folder through the normal import path (dedupe, warnings, placement, transition dialog), with a notice at the top of the import report; a missing, unreadable, or empty folder only shows a notice. In-branch v4 files that still contain `render.batchVideoFolder` no longer load.

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

Delivered as two commits: **M4a** program-driven playback with the existing single element (items 2, 3, 4, 5), **M4b** two video elements with preloading and hand-off (item 1).

M4a status: committed (`26478df2`). Decisions made during implementation:
- The video clock ends a piece when the presented frame passes the piece's source end (not only on `ended`), so deleted parts end a piece mid-file. At a piece end playback continues at the next piece's start (gaps and deleted parts skipped, hard cut); after the last piece the element holds the piece's last frame and the timeline clock plays the rest of the activity.
- Playback started inside a gap or deleted part starts at the next piece. Before the first and after the last piece the timeline clock plays the activity as before (gap skipping applies between pieces only, and only with video playback).
- The element re-seeks when the playing piece changes, also within the same video (after a deleted part).
- Scrubbing into a deleted part shows that video's frame with a "Deleted part – not exported" indicator; gaps keep showing the next piece's first frame (dimmed). A red banner shows while the playhead is inside an overlap.
- Removing a clip pauses playback first; the removal clamps the playhead.
- The hand-off reaches the clock through `useEffectEvent`, so the frame loop does not restart on every render.

M4b status: committed (`ed70f861`); native WebView check pending. Decisions made during implementation:
- Two persistent `<video>` elements. A pure assignment (`lib/video-program/preview-slots.js`) keeps the active element on the piece at the playhead (the upcoming piece in a gap) and parks the other one, paused and muted, on the first frame of the piece after it — also the same video after a deleted part. A cut to the parked piece swaps elements; a jump within the shown video seeks; only a jump to an unprepared video reloads an element.
- The existing playback, scrub, drag, clock, and warning logic runs on whichever element is active (its ref switches), so a swap re-syncs the new element and the demoted element is parked again.
- The parked element is always prepared (also while paused); it only decodes its parked frame, so two elements never play at once. Verify on WebKitGTK/WebView2 with two 4K/HEVC clips (plan risk) and fall back to preloading near the piece end if needed.

1. Two reusable video elements with active/preload roles (`useVideoPreview.js`, `useVideoPlaybackClock.js`, `usePlaybackEngine.js`, `videoPreviewSource.js`).
2. Continue at piece end → next piece start, skipping gaps and deleted parts; hard cut at every join (transition windows are added in M8).
3. Direct jumps to unloaded clips; deleted-part indicator while scrubbing. Playback skips gaps; scrubbing into a gap shows the next piece's first frame frozen.
4. Overlaps (preview only): play the earlier clip to its end, then continue the later clip at the matching source position. Show a very visible warning on the player for the whole time the playhead is inside an overlap.
5. Removal of the playing clip: pause, remove, clamp playhead.

Tests: playback engine / preview hooks with mocked media elements. Manual: native WebView (WebKitGTK, WebView2) hand-off and dual 4K/HEVC decode.

## M5 — Rasters-only outside activity coverage

M5 status: committed (`a8c3833a`). Decisions made during implementation:
- Native: asset preparation also draws a rasters-only layer (none without rasters); frames outside the coverage copy it instead of clearing (`prepare_output_frame`). Applies to composite and transparent exports, single and batch.
- `ActivityCoverage::for_video` accepts a video that does not overlap the activity: no covered frames, window = the whole activity (no frame samples it, dense activity gets an empty frame timeline). The single-render composite plan drops both no-overlap errors; batch jobs need no change.
- Frontend: single-file composite export no longer rejects a clip outside the activity, and batch rows outside the activity are no longer blocked. The clip keeps its "outside the activity" warning in the panel.
- Preview: `hasActivityCoverage` (external activity: inside it; otherwise inside the telemetry of the clip playing at the playhead; with no activity and no clip everything counts as covered) makes non-raster widgets invisible (`invisible`, so they cannot be clicked) in the editor canvas.
- ~~Deferred to M6~~ (done in M6a): a clip without any embedded telemetry in embedded mode renders rasters only. Frames past the end of a clip's own telemetry are rasters-only already.
- No FFmpeg end-to-end test of raster pixels outside coverage yet; covered by the frame-mapping unit test and planning tests. Add with M6's FFmpeg integration tests.

1. Native: frames without activity coverage render rasters only instead of `pixels.fill(0)` (`ovrley_core/src/render/mod.rs` `prepare_output_frame`, `encode/video_timing.rs` coverage incl. trailing frames). Reuse the raster-only widget filtering that `skip_overlay` already does (`render_jobs/planning.rs`).
2. Composite planning: allow clips/pieces with no activity overlap (remove the "does not overlap activity" error path in `derive_composite_render_plan` and the batch equivalent) — coverage becomes empty, overlay is rasters only.
3. Frontend preview: hide non-raster widgets when the playhead has no activity coverage.
4. Same rule without external activity: coverage is the clip's own embedded telemetry; clips/frames without it show rasters only.

Tests: render frame mapping tests, planning tests for zero/partial coverage, preview model tests.

## M6 — Export (hard cuts)

Delivered as three commits: **M6a** native piece jobs (item 2 without concat, export range and deleted parts per piece, activity-free path), **M6b** native combined outputs (items 3, 4, 5, 6: intermediates, concat, audio pass, mux, verification, cleanup), **M6c** frontend (items 1, 7: render request from the program, output mode, dialog).

M6a status: implemented (committed). Decisions made during implementation:
- Batch contract: `outputs[] = { id, pieces[] }`, piece = `{ sourceId, startFrame, endFrame, skipOverlay }` on the source's exact frame grid; offsets stay per source in `activity.offsets`. Output planning takes the same outputs (`BatchOutputSelection`); plans report `outputId`. Until M6b every output must hold exactly one piece (validated loudly).
- A piece plans like a video of its window's duration starting at activity second `offset + trimStart` (composite `trim_start`/`render_duration`, its own coverage). A piece reaching the source's last frame ends at the source duration, so a whole-file piece renders exactly as before.
- One frame-count rule: the frontend now counts source frames like `Fps::frame_count_for_duration` (fraction ≤ 1/10,000 of a frame snaps down, otherwise rounds up) instead of rounding, so program frame indices match the renderer.
- Separate-file export renders each video's own pieces after deleted parts and the export range, without overlap trimming (`collectVideoPieces`); a video outside the export range is blocked in the queue.
- Activity-free rendering: `prepare_render_assets`/`prepare_preview_assets`/`encode_video`/`execute_render` take `Option<&ParsedActivity>`; None is accepted only for a config without value widgets or plots and pairs with `DenseActivityReport::without_activity()`. In embedded mode a source without telemetry gets a rasters-only plan (`ActivityCoverage::without_activity`).

M6b status: implemented (committed). Decisions made during implementation:
- Destination is part of the contract: `destination = { kind: 'sourceNamed', directory }` (separate files: each output holds pieces of one source, named `{stem}_{video|overlay}.{mp4|mov}`, replaced if present) or `{ kind: 'file', path, overwrite }` (exactly one combined output; must not alias an input). It replaces `outputDirectory` in the request and in output planning (`BatchOutputSelection.destination`); planning reports the summed frames and window durations.
- Outputs hold any number of pieces. One-piece outputs render straight to the target with their own audio, exactly as before. Multi-piece outputs must join by stream copy: one display resolution, and for composite one source FPS (and one rotation with QSV-full, which keeps the display matrix). Violations are rejected at submission.
- Multi-piece outputs render each piece to `part-NNNN.{mp4|mov}` in a hidden `.ovrley-parts-{target stem}-{n}` directory next to the target. Composite parts are video-only, remuxed to MPEG-TS with `h264/hevc_mp4toannexb`, listed by relative name in an escaped concat manifest and joined with `-c:v copy` (`-tag:v hvc1` for HEVC, `-display_rotation` restored for QSV-full). Transparent parts (intra-only ProRes/QT RLE) join as MOV by stream copy, without audio.
- Audio is encoded once from the sources in the join command: per piece a coarse seek, `atrim`, `asetpts`, `aresample=48000`, stereo `aformat`, `apad` to the part's exact video duration (frames ÷ FPS); `anullsrc` for sources without audio; `concat v=0:a=1`; AAC 192k. No audio track when no source has audio.
- The joined output is verified with ffprobe (video packet count equals the planned frames; one audio stream exactly when audio was encoded) before the parts are removed. Parts are removed after a verified output or a cancellation and kept after a failure; the item's failure message names the kept directory.
- Progress: an item's planned frames are the sum of its pieces; each piece pipeline reports item progress offset by the frames before it (ETA adds the later pieces at the observed rate), then "Joining output pieces...". Every FFmpeg join step polls cancellation and kills/reaps the process.
- Frontend changed only the wire shape (`createSeparateDestination`); separate-file export with deleted parts now renders through native joining. Output mode and combined export UI remain M6c.

1. Render request from the program: shared template/encoding, optional shared external activity, output mode (`separate` | `combined`), output groups of pieces (`renderRequest.js`, `renderConfig.js`, `useRenderWorkflow.js`, `useRenderExecution.js`).
2. Extend `render_jobs` (contracts, submission, planning, batch, batch_state, execution) from per-file jobs to per-piece jobs grouped into outputs. Remove folder-source assumptions.
3. Concat step: concat demuxer with stream copy, TS/Annex-B remux, escaped manifest, verify output (probe frames/duration) before deleting intermediates.
4. Combined audio pass (`atrim`/`concat`/`anullsrc`), mux with `-c:v copy`.
5. Transparent mode: hard cuts only, combined or separate.
6. Progress weighted by output frames, current clip identity, cancellation during render/concat/audio, cleanup and retention rules.
7. Render dialog: output mode selector, included clips summary, disabled reasons (overlap, incompatibility, pending telemetry, invalid transitions).

Tests: request preparation; Rust batch lifecycle with injected executor; command builders; FFmpeg integration with tiny synthetic clips asserting durations/frame counts/audio length.

## M7 — Deleted parts

M7 status: implemented (committed). M6c (frontend output mode/combined export dialog) is still open. Decisions made during implementation:
- Store: `createDeletedPartsSlice` owns `addDeletedPart`, `setDeletedPartRange`, `removeDeletedPart`, `setJoinTransition` (writes `transitionIn` for clip joins and the part's `transition` for deleted-part joins; `null` returns to the defaults) and the transient `deletedPartMark`. Parts stay in `videoEdits` (undoable, saved in project v4). Bounds snap to the clip's frame grid; a part must cover at least one frame inside the clip.
- A new or moved part merges with every part it overlaps or touches; the merged part keeps the identity and transition of the earliest part it absorbs, so the project never holds overlapping or touching parts.
- Marking: Shift+I starts a part at the playhead and Shift+O ends it (either order), Shift+X cancels; toolbar buttons do the same. The mark applies to the selected clip when it covers the playhead, else to the clip the preview shows, else to the earliest clip under the playhead; an end beyond the marked clip deletes up to its edge. Alt+drag on a clip draws a part over the dragged source range (a plain drag still moves the clip).
- Timeline: parts are shaded inside their clip; the pending start is a line on its clip. Videos panel: "Deleted parts" per clip with From/To (`M:SS.cc`, invalid edits ignored), restore, "Go to part", and the join transition (Default with the effective default, No transition, or an `xfade` type with its own duration). A part at a clip edge creates no join, so its selector is disabled.
- Transitions chosen at deleted parts are stored and drive the program; preview/export rendering of transitions remains M8 (export is hard cuts until then).

1. UI: mark range at playhead (in/out) or by dragging on a clip; edit/remove ranges; shaded on timeline; list in videos panel.
2. Join at each deleted part with transition selector (default rule uses deleted length as gap).
3. Data already flows through `buildProgram` (M2), preview skipping (M4), and piece rendering (M6).

Tests: slice/utility tests for edits and program effects; timeline interaction tests.

## M8 — Transitions

Delivered as three commits: **M8a** transition editing (items 1, 2, 3), **M8b** native transition rendering (item 6 and the `acrossfade` audio pass), **M8c** transition proxies and the player transition window (items 4, 5). M6c (render request from the program, output mode, dialog) follows M8b so it can send transitions.

M8a status: implemented (committed). Decisions made during implementation:
- Project defaults are edited in a "Default transitions" section of the videos panel (type, duration, continuity threshold), stored at once through `setTransitionDefaults` (not undoable, like the import dialog). Joins still on their default follow; overridden joins keep their choice. The import-time defaults dialog uses the same picker.
- Picker (`features/transitions`): a popover with a filter, "Default (…)" for joins, "No transition", and one animated WebP per `xfade` transition. `scripts/generate-transition-previews.mjs` renders them with the bundled FFmpeg from two generated neutral frames (blue grid → orange box, 120×68, 15 fps, 0.6 s + 1 s transition + 0.6 s, looping) into `assets/transition-previews/` (58 files, ~4 KB each). A test fails when the catalogue and the assets differ.
- One join control model (`buildJoinTransitionModel`, `useJoinTransitionModels`) serves the panel ("Transition into this video" per clip that follows another piece; each deleted part) and the timeline: a diamond marker at the end of each join's outgoing piece opens the control, and the transition window (the last `d` seconds of that piece) is shaded. Typed durations must be above 0 (shown inline); a transition that leaves a joined piece without an untouched frame (`findInvalidTransitions`) is flagged on the control and the marker turns red.

M8b status: implemented (awaiting owner commit). Decisions made during implementation:
- Contract: every batch output carries `transitions[]`, one per join between consecutive pieces: `{ type, durationSeconds }`, `type` `none` (duration 0) or a catalogue `xfade` name. The Rust catalogue lives once in `ovrley_core::encode::ffmpeg::transitions` (project validation uses it; a test mirrors the frontend catalogue). Planning reports `plannedFrames` (rendered work: every piece frame once) and `outputFrames` (pieces minus transitions); `outputDurationSeconds` is the joined duration.
- Validation at submission: the count matches the joins; a transition spans `round(d × fps)` ≥ 1 output frames; every piece keeps at least one frame outside its incoming and outgoing transitions; transparent outputs take hard cuts only.
- A piece next to a transition renders as head (first `d_in` frames), body and tail (last `d_out` frames), each through the ordinary piece pipeline at its own activity second. The output is body, `xfade(tail, head)`, body, …: only the transition regions are encoded twice.
- Transition parts decode the tail and head with `-noautorotate` and encode `xfade=…:offset=0` with the output's encoder, profile output arguments and rate control. Hardware-decode input arguments are left out (`xfade` needs system-memory frames); VAAPI and AMF get their device and `format=nv12,hwupload`; NVENC, QSV (including QSV-full), VideoToolbox and the software encoders take system frames. Transition parts are remuxed to MPEG-TS and joined like every other part.
- Audio: per piece as before, joined pairwise with `acrossfade=d=…:c1=tri:c2=tri` at transitions and `concat` at hard cuts, so audio and video both last Σ pieces − Σ transitions.
- The frontend sends hard cuts between the pieces of a separate-file output for now; M6c builds outputs (separate and combined) from the program, including its join transitions.

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
