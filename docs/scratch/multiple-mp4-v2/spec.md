# Multiple Video Clips Specification

Status: needs-info (draft for owner review — see "Open Questions" in `notes.md`)
Last updated: 2026-10-15
Supersedes: `../multiple-mp4/spec.md` (2026-08-13: hard-cut only, auto-pushed overlaps, no persistence)
Reference: `merge.ps1` in this folder (owner's prototype of combined export with transitions)

## Problem Statement

OVRLEY treats an imported source video as a singleton. Video metadata, preview registration, embedded MP4 telemetry, synchronization, timeline placement, playback-clock ownership, project files, and composite export all assume exactly one video. Importing another video replaces the first one. Processing several files today means designing on one file and then running a folder batch that renders each file separately.

Action-camera users commonly record one activity as many clips: files split by the camera at a size limit (one continuous recording) and separate recordings with real-time gaps between them. They need to load all clips at once, see them in their correct place on one timeline (also before or after the activity), preview them continuously, remove unwanted parts, choose transitions between parts that are really different shots, and export everything in one operation either as separate files or as one combined video.

## Solution

Imported source videos become one canonical collection of independent video entities. Every entity owns its validated source metadata, preview registration, timeline placement, sync data, embedded telemetry state, deleted parts, and the transition that leads into it.

A single pure "program" derivation turns the collection, the deleted parts, and the global export range into an ordered list of **pieces** (continuous source ranges that will be output) and **joins** between consecutive pieces (a hard cut or an FFmpeg `xfade` transition). Preview, timeline, validation, and export all consume this one derivation.

- One import button opens a multi-select file dialog. The user can select one file or all files in a folder.
- Compatibility (display resolution and exact frame rate) is checked at import. Incompatible clips are allowed and the user is warned immediately; single-file export stays disabled until the incompatible clips are removed from the timeline.
- Time detection and sync management stay exactly as they are today (GPS telemetry → container metadata → file modified time; filename timestamp and timezone/offset selection by the user). The sync choices made at import apply to all clips; afterwards each clip's offset can be changed individually.
- Clips are placed on one real-time timeline from those times, including clips before or after the activity (their real signed offset is kept). Clips whose time cannot be determined at all are placed by the user ("before/after clip X").
- Overlapping clips produce an immediate warning and remain visible as a conflict until resolved; single-file export is blocked while any overlap exists.
- Preview uses one loopback HTTP server with a registry of clips and two reusable video elements. Playback continues from piece to piece, skipping gaps and deleted parts, and shows transitions using short FFmpeg-rendered proxy clips of each transition window, so the user sees the real output without exporting.
- Export renders every piece through the existing single-source composite pipeline with its own naturally synchronized overlay. Output is either one file per clip or one combined file. Transitions are produced afterwards from already-rendered material, so they never change overlay synchronization.
- Frames without activity coverage keep raster (image/logo) widgets and hide all other widgets.
- Transparent (overlay-only) export covers exactly the loaded clips minus deleted parts, with hard cuts only, as one combined file or one file per clip.

## Terms

| Term | Meaning |
| --- | --- |
| Clip / video entity | One imported source file with its own state. |
| Timeline second | Real-time position on the editor timeline. With external activity, `0` is the activity start. |
| Source second | Position inside one clip file. |
| Deleted part | A clip-local source range marked as excluded from output. |
| Piece | A maximal continuous source range of one clip that survives deleted parts and the global export range. |
| Join | The boundary between two consecutive pieces — either between clips or at a deleted part. Carries a transition. |
| Transition | `none` (hard cut) or one of the 58 built-in FFmpeg `xfade` transitions plus a duration. |
| Program | The ordered pieces and joins derived from the collection. |
| Split gap threshold | Maximum real-time gap (project setting, default 2 s) that is treated as a camera file split (continuous recording) rather than a separate shot. |
| Transition defaults | Project settings: default transition type (`none` or an `xfade` name, default `wipeleft`), default duration (default 2 s), split gap threshold (default 2 s), and whether the defaults dialog has been shown. |

## User Stories

### Import

1. As a user, I want one import button that lets me select one or many video files, so that I can load a single clip or a whole folder without separate UI for each case.
2. As a user, I want selecting more files later to add to the existing collection, so that I can build the session incrementally.
3. As a user, I want duplicate paths rejected, so that the same recording never appears twice.
4. As a user, I want the blocking import loader shown until every selected file has imported or failed, so that the editor never shows a partially committed import.
5. As a user, I want failed files reported individually while valid files still import, so that one broken file does not discard the selection.
6. As a user, I want resolution and frame-rate compatibility checked at import and a warning shown immediately, so that I learn — not after an hour of editing — that the clips cannot be combined into one file.
6a. As a user, I want incompatible clips still imported, with single-file export disabled while they are on the timeline, so that I can keep them for separate-file export or remove them.
6b. As a user, I want to delete whole files from the timeline, and single-file export to become available again once no incompatible clips remain, so that I can fix the session without re-importing.
7. As a user, I want rotated clips accepted when their display-oriented resolution matches, so that camera rotation metadata does not block a session.
8. As a user, I want clips with and without audio to be combinable, so that a muted clip does not block single-file export.

### Placement

9. As a user, I want each clip's time detected exactly as today (GPS telemetry, then container metadata, then file modified time) and the clip placed at that real time, so that gaps between recordings are visible and preserved — with or without an activity.
10. As a user, I want clips recorded before or after the activity loaded and shown before or after it with their real offset (not forced to 0), so that I do not lose footage just because the activity does not cover it.
11. As a user, I want to be asked where to place clips whose time cannot be determined (before/after a chosen clip), so that unplaceable clips still end up in the right order.
11a. As a user, I want the timezone/offset choices I make at import (as today) applied to all imported clips, so that one camera's clock setting is handled once.
12. As a user, I want to be warned on import when clips overlap in time, so that I can fix the conflict before exporting.
13. As a user, I want overlaps highlighted on the timeline and in the videos panel until resolved, so that I can see what needs fixing.
13a. As a user, I want the player to play overlapping clips in a defined way (earlier clip first, then the later clip at the matching position) with a very visible warning while inside the overlap, so that I can inspect the conflict.
13b. As a user, I want deleting the overlapping part of a clip to resolve the overlap, so that I can keep both clips and still export a single file.
14. As a user, I want replacing the external activity to recalculate every automatic clip offset, so that synchronization follows the current activity.
15. As a user, I want to change any single clip's offset afterwards without affecting the others, so that I can fix individual clips.

### Editing

16. As a user, I want every imported clip and its metadata shown in a dedicated videos panel, so that I can inspect the whole session.
17. As a user, I want to adjust each clip's offset by dragging it or typing a value, so that both interfaces edit the same state.
18. As a user, I want to remove a clip, so that unwanted recordings leave the session.
19. As a user, I want to mark any part of a clip (for example 1:30–1:45) as deleted, so that it is neither previewed nor exported.
20. As a user, I want to choose a transition at a deleted part, so that a long removed section (for example 5:00–15:00 of a 20-minute file) can be treated as two different shots.
21. As a user, I want to choose a transition (or none) at every join, so that I control how each pair of pieces connects.
22. As a user, I want joins with a real-time gap up to the split gap threshold to default to no transition, and all other joins to default to the default transition, so that camera file splits stay seamless and real shot changes get a transition.
22a. As a user, I want to be asked once — the first time the project contains more than one video — for the default transition (including "no transition"), its duration, and the threshold for treating clips as continuous, with `wipeleft`, 2 s, and 2 s preselected, so that the whole session starts with sensible joins.
22b. As a user, I do not want to be asked when I import a single file, or again when I add more videos after I have already answered, so that my existing transition choices are never overridden.
23. As a user, I want to browse all available transitions in a popup with an animated preview of each, so that I can choose one visually.
24. As a user, I want the player to show the selected transitions during normal playback, so that I see what the output will look like without rendering everything.
25. As a user, I want transition durations that do not fit the adjacent pieces rejected immediately, so that export never fails on them later.

### Preview

26. As a user, I want playback to continue from one piece into the next, playing the chosen transition at each join, so that the whole session plays like the combined output.
27. As a user, I want playback to skip timeline gaps and deleted parts, so that I watch only output footage.
28. As a user, I want scrubbing or jumping anywhere to respond immediately, even into a clip that is not preloaded, so that the timeline stays responsive.
29. As a user, I want widgets other than raster images hidden while the playhead has no activity coverage, so that preview matches export and logos stay visible.

### Export

30. As a user, I want to export all clips in one operation, so that I never export clips one by one.
31. As a user, I want to choose between one output file per clip and one combined output file, so that I can pick the workflow I need.
32. As a user, I want the global export range to apply to the whole session, so that I can export only a selected part.
33. As a user, I want deleted parts excluded from every export mode, so that removed footage never reappears.
34. As a user, I want each piece rendered with its own correctly synchronized overlay, so that transitions never shift overlay data.
35. As a user, I want frames without activity coverage to keep only raster widgets, so that logos stay on footage recorded outside the activity.
36. As a user, I want a combined export to omit timeline gaps, so that it contains only footage.
37. As a user, I want transparent (overlay-only) export to cover exactly the loaded clips minus deleted parts, with no transitions, as one combined file or one file per clip, so that I can composite it in another editor.
38. As a user, I want progress for the whole export with the current clip identified, so that long exports are understandable.
39. As a user, I want cancellation to stop the active work and clean incomplete output, so that cancelling is predictable.
40. As a user, I want my session (clips, offsets, deleted parts, transitions) saved in the project file, so that I can continue later.

## Implementation Decisions

### Model

- The singleton imported-video contract is replaced at its owner by `importedVideos` (array of entities). No singleton aliases, mirrored fields, compatibility selectors, or remapped consumer objects.
- Entity fields: stable `id`, absolute `path`, `pickerIndex`, validated metadata (`durationSeconds`, `frameCount`, exact `fps {num, den}`, display `resolution`, `rotationDegrees`, `hasAudio`, codec/camera/bit-rate info), creation time exactly as today per clip (`creationTime`, `timeSource`, `detectedCreationTime`, `detectedTimeSource`), preview registration (`importId`, `url`, `warnings`), committed `offsetSeconds` (timeline start), `syncWarning`, `placementAnchor` (`null`, or the clip it was placed before/after when its time cannot be determined), `compatible` (derived from the collection, see below), `deletedParts` (clip-local `{ fromSecond, toSecond, transition }`), `transitionIn` (join from the previous piece of another clip), and embedded telemetry state (`pending` | `ready` | `absent` | `failed` with payload).
- Session-level sync settings keep today's controls and semantics (`videoSyncTimezoneMode`, offset input, filename-timestamp action, reset, manual landmark sync). Changing them applies to all clips: changing the shared offset shifts every clip by the same delta and keeps each clip's individual adjustment. Per-clip offset edits (panel input or timeline drag) change only that clip.
- A transition value is `{ type: 'none' | <xfade name>, durationSeconds, isDefault }`. `isDefault` marks values chosen by the default rule; such values are re-derived when neighbours change. User-chosen values are kept.
- Active clip, upcoming piece, program, overlaps, timeline bounds, and export order are derived. They are never stored.
- External activity remains one global entity and is authoritative for all clips whenever present. Without it, each clip uses only its own embedded telemetry in clip-local time.

### Import and compatibility

- One import action, multi-select file dialog (`mp4`, `mov`, `mkv`). The folder-based batch queue is removed; batch export sources are the imported clips.
- Probe and preview registration are part of the blocking import. Embedded telemetry extraction runs afterwards in a sequential, timeline-ordered queue addressed by clip ID; late results for removed clips are discarded.
- The probe exposes exact rational frame rate, resolution, rotation, and audio presence. Frontend ingress validates them once (duration, `fpsNum/fpsDen` required) and derives `frameCount = round(duration × fpsNum / fpsDen)` and the display-oriented resolution. Missing or malformed required timing fails that file. VFR sources remain accepted as today.
- Compatibility is a property of the whole collection: single-file export requires every clip to share display-oriented resolution and exact rational FPS. Audio presence and source codec are not criteria (all output is re-encoded; missing audio becomes silence in combined output).
- When clips differ, the largest group (ties: the group of the earliest picker-order clip) is the compatible group; all other clips are marked incompatible. The import result warns immediately and lists the incompatible clips with their resolution/FPS.
- Incompatible clips are imported, previewed, edited, and exported in separate-file mode. Single-file export is disabled (with the reason shown) while any incompatible clip is on the timeline. Removing those clips from the timeline re-enables it; compatibility is re-derived on every add/remove.

### Placement

- Time detection is unchanged and runs per clip exactly as today: GPS time from embedded telemetry (`gps`) → container metadata `creation_time` / QuickTime `creationdate` (`ffprobe`) → OS file modified time (`file_mtime`, last fallback, used as the start time as today). A filename timestamp (`filename`) is applied only by explicit user action. See `notes.md` → "Existing video time chain".
- Timezone handling is unchanged: ambiguous sources (`ffprobe`, `filename`) are interpreted per clip with today's inference (try local and UTC, prefer the one that overlaps the activity) until the user selects a mode; the selected mode applies to all clips.
- The only behavioural change: a clip that does not overlap the activity keeps its real signed offset (placed before/after the activity) with a "no activity coverage" warning, instead of being forced to offset 0. This also applies with a single video.
- With external activity the timeline origin is the activity start. Without it, clips are placed by the same detected times relative to the earliest clip (real time, gaps kept).
- Clips whose time cannot be determined at all (no time from the chain, or the sync calculation fails, e.g. invalid timestamps) open a placement prompt listing placed clips; the user chooses "before X" or "after X" and the clip is placed adjacent (zero gap) to X.
- Session-level sync actions (timezone mode, shared offset, filename timestamps, reset, landmark sync) apply to all clips. Activity import/replacement recalculates every detected clip as today; user-placed clips are re-applied relative to their anchor clip while their own time still cannot be determined.
- Overlaps are never auto-resolved. Overlaps are computed on pieces (after deleted parts). They produce an import warning, a timeline conflict band, and a panel warning. Single-file export is disabled while any overlap exists; deleting the overlapping part of a clip or moving a clip resolves it. Separate-file export remains available.
- Overlap playback is a preview-only behaviour: the player plays the earlier-starting clip to its end and then continues the later clip at the corresponding source position, showing a very visible warning on the player for the whole time the playhead is inside the overlap.

### Program derivation

- `buildProgram(videos, exportRange)` is a pure utility on the shared frame grid. It produces ordered pieces `{ videoId, sourceIn, sourceOut, timelineIn, timelineOut }` and joins `{ from, to, kind: 'clip' | 'deletedPart', realGapSeconds, transition }`.
- Default transition rule per join: `realGapSeconds <= splitGapThresholdSeconds` → `none`; otherwise the project default transition (type + duration; type may be `none`). For a deleted-part join, `realGapSeconds` is the deleted length. If the default transition does not fit the adjacent pieces (piece shorter than its transitions), the default is `none` for that join. Every join can override type and duration individually; overridden joins are never changed by the defaults again.
- Transition defaults live in the project settings: `{ type: 'none' | <xfade name>, durationSeconds, splitGapThresholdSeconds, dialogShown }`. Hard defaults for new and migrated projects: `wipeleft`, 2 s, 2 s, `dialogShown: false`.

### Transition defaults dialog

- Shown once per project, as part of an import, when the import makes the collection contain more than one video and `dialogShown` is `false`. That covers both selecting several files at once and adding files to a project that had a single video.
- Not shown when the result is a single video, and never again once `dialogShown` is `true` (also not when clips are removed down to one and added again), so existing transition choices are never overridden.
- Fields: default transition type (list of all `xfade` transitions plus "No transition"), duration (disabled for "No transition"), and continuity threshold. Preselected: `wipeleft`, 2 s, 2 s.
- Confirming stores the values, sets `dialogShown: true`, and derives the default transition of every join that is still on its default. Dismissing the dialog (close/Esc) does the same with the preselected values. The rich picker with animated previews (M8) is reused here once available; until then a plain list.
- Order after an import: per-file failures and compatibility warnings → placement prompt (if needed) → transition defaults dialog (if due) → overlap warning.
- Later changes happen per join, or through the project settings, which expose the same three values. A settings change re-applies only to joins still on their default; joins the user changed are never touched.
- Transition validity: each piece must be at least `transitionIn + transitionOut` long plus one frame. Invalid transitions are surfaced immediately in the panel and block export.
- Global export range boundaries never create joins.

### Timing contract for transitions (verified)

- FFmpeg `xfade`/`acrossfade` overlap the end of the outgoing piece with the start of the incoming piece. Two 60 s clips with a 3 s transition produce a 117 s output (verified with bundled FFmpeg 8.1: 3510 frames at 30 fps, audio 117 s). Combined duration = Σ piece durations − Σ transition durations. Hard cuts remove nothing.
- Every output frame still originates from exactly one or two source frames at their own source time. Overlays are rendered per piece from source time, so they stay synchronized by construction. During a transition both pieces' overlays are blended exactly like the footage.
- Consequence for any alternative "single overlay over the program" design: the incoming clip's overlay would need to resume at source time `d` (not `0`) after a transition of length `d`. This design is not used, which removes that risk.

### Rendering

- Every piece renders through the existing single-source composite pipeline using `trim_start`/`render_duration` (trimmed source range) and its own activity mapping. No multi-input overlay renderer.
- Frames without activity coverage render raster widgets only (today they are cleared). This applies to single, separate, combined, and transparent exports and to preview. Without external activity, coverage means the clip's own embedded telemetry; the same rule applies.
- Pieces adjacent to a transition are split into `head` (first `d_in`), `body`, and `tail` (last `d_out`) renders. Every part — head, body, tail, and transition part — uses the same codec and encoder settings as the final output; there is no separate intermediate codec. Only the `d`-second transition regions are encoded twice.
- Transition parts are built by `xfade` of `tail(A)` and `head(B)` (both already carrying their overlays) and encoded with the final encoder settings.
- Output parts (bodies and transition parts) share codec, profile, pixel format, rate control, resolution, and FPS by construction and are joined with the concat demuxer using stream copy (remuxed to MPEG-TS/Annex-B first, as `merge.ps1` does, for robustness).
- Combined-output audio is built in one separate pass from the original sources (`atrim` per piece, `acrossfade` at transitions, `concat` at hard cuts, `anullsrc` silence for clips without audio), encoded once, and muxed with the stream-copied video. Body renders are video-only in combined mode.
- Separate-file mode produces one output per clip; a clip with deleted parts is assembled from its own pieces and joins (including transitions chosen at its deleted parts). A clip with one piece and no transition is a plain single job.
- Transparent mode forces every join to a hard cut. Combined transparent output is all pieces back to back; separate transparent output is one file per clip from its own pieces.
- The existing native batch service (`render_jobs`) is extended rather than replaced: jobs become pieces grouped into output groups, with one batch lifecycle for progress (weighted by output frames), cancellation, failure, and cleanup. Intermediates are removed after a verified final output and kept after failure.

### Preview

- One loopback HTTP server holds a registry keyed by opaque import ID. Registering or removing one clip never invalidates another clip's URL.
- Two reusable video elements: active and preloading the next piece. The playhead stays in timeline seconds. At a piece end playback continues at the next piece's start, skipping gaps and deleted parts. Joins without a transition are hard cuts.
- Joins with a transition are shown in the player with a **transition proxy**: a short clip rendered in the background by FFmpeg with the real `xfade`/`acrossfade` of the outgoing tail and incoming head, at preview resolution, without overlay, short GOP for exact scrubbing. Proxies are registered in the same preview server, cached by a content key (sources, piece bounds, transition type and duration), rendered in a sequential queue prioritised by distance to the playhead, invalidated on any change to their inputs, and deleted when no longer referenced.
- Transition window timing in preview (matches export): the window is the last `d` seconds of the outgoing piece on the timeline. While the playhead is inside it, the proxy plays; the outgoing overlay is drawn at the playhead time and the incoming overlay at `incoming.timelineIn + (playhead − windowStart)`, cross-faded by transition progress. At the window end the playhead jumps to `incoming.timelineIn + d` and the incoming clip continues from source second `d` — the first `d` seconds of the incoming clip were shown inside the transition, so the overlay is not shifted.
- Until a proxy is ready the join plays as a hard cut with a "rendering transition preview" indicator on the join marker.
- The transition picker popup shows a generic animated preview of every transition type.
- Direct jumps commit the playhead immediately, hide stale footage, and show the existing loading state until the target frame is ready.
- Scrubbing into a gap shows the next piece's first frame frozen; scrubbing into a deleted part shows the frame with a deleted-state indicator.

### Persistence

- Project format version 4: `sources.videos[]` with path, committed offset, placement anchor, deleted parts, and per-join transitions (type + duration, default/overridden); project transition defaults (`type`, `durationSeconds`, `splitGapThresholdSeconds`, `dialogShown`); session-level sync settings (timezone mode etc.) stay global; manual sync landmarks belong to the clip named by `sync.manual.videoId`.
- Existing projects (v1–v3) are converted automatically and silently on open, in memory, following the existing `migrate_project` chain (v1/v2 → v3 → v4). The single video becomes a one-element collection with its saved offset, landmarks get that video's ID, there are no deleted parts or transitions, and default transition settings are added. The project looks and behaves as before.
- v3 projects saved with batch rendering (`render.renderTarget: batch` + `batchVideoFolder`): on open, all videos currently in the saved folder are imported into the timeline through the normal import path (dedupe with the project video, compatibility/overlap warnings, time placement). Export switches to separate-file mode and keeps `batchOutputFolder` as the output folder. A notice tells the user what happened. If the folder is missing or empty, the project keeps its own video and the notice says so. If this results in more than one video, the transition defaults dialog is shown (converted projects start with `dialogShown: false`).
- The file is rewritten as v4 only when the user saves. On the first save of a converted project, the original file is kept next to it as `<name>.oly.v3.bak` (not `.oly`, so the projects drawer does not list it), and the user is told once that older OVRLEY versions cannot open the new file.

## Testing Decisions

- Pure utility tests (table-driven) for placement, overlap detection, program derivation, default transition rule, transition validity, export-range intersection, and frame-grid math including the `Σ − Σd` duration identity.
- Zustand slice tests for atomic multi-import, compatibility reporting, duplicate rejection, placement prompts, overlap warnings, deleted parts, transitions, removal, and race-safe telemetry.
- Import-flow tests for the transition defaults dialog: shown for a multi-file first import, shown when adding to a single-video project, not shown for a single file, not shown again after it was confirmed (including remove-to-one then add), never overriding per-join overrides.
- Player/preview hook tests with mocked media elements for two-element hand-off, gap and deleted-part skipping, transition-window timing (proxy playback, overlay times, jump to `incoming + d`), direct jumps, and removal during playback.
- Tauri video-server integration tests for multiple registrations, independent ranges, isolated removal, clear-all, and stale IDs.
- `ovrley_core` tests for probe fields, rasters-only out-of-activity frames, piece planning (head/body/tail), transition command building, concat manifest escaping, audio-pass graph building, batch lifecycle (progress, cancel, failure, cleanup).
- An FFmpeg integration test that renders tiny synthetic clips and asserts combined duration/frame count and audio length for hard cuts and transitions.
- Project-file tests for v3→v4 migration and strict v4 validation.
- Manual verification: real same-camera multi-file session, clips outside the activity, transitions with overlays, audio continuity at joins, hardware encoders, cancellation of real subprocesses.

## Out of Scope

- Multiple video tracks, multi-angle selection, or compositing overlapping clips.
- Re-implementing transitions in the player with WebGL/CSS (the FFmpeg proxy is used instead).
- Baking overlays into transition proxies (preview cross-fades live overlays instead; possible refinement).
- Changes to the existing time-detection chain or timezone inference.
- Rescaling or frame-rate converting incompatible clips.
- Custom `xfade` expressions (`transition=custom`) and non-FFmpeg transition plugins.
- Audio fades other than `acrossfade` at transition joins.
- Parallel piece rendering.
