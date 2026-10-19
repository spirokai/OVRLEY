# Multiple Video Clips — Working Notes

Last updated: 2026-10-15
Companion to `spec.md` and `plan.md`. Facts collected while planning, owner decisions, risks, and open questions.

## Owner decisions (2026-10-15 review)

1. **One import button.** The file dialog allows multi-select; users open a folder and select all files. No separate "add folder" button. The folder batch queue goes away.
2. **Outside activity → rasters only.** Frames without activity coverage hide every widget except the image/raster widget (static, keeps logos).
3. **Transitions at deleted parts too.** Deleting 5:00–15:00 of a 20-min file leaves two pieces 10 min apart in real time — effectively different shots — so the join needs a transition selector.
4. **Compatibility as early as possible.** Checked at import, not at export. "I don't want to spend 1h editing and then realize I can't get all together."
5. **Ship together.** Multi-load, multi-preview, and multi-export are one feature. Internal milestones only.
6. **Transition picker popup** with an animated preview of each transition, like the FFmpeg wiki page (https://trac.ffmpeg.org/wiki/Xfade).
7. **Overlay timing must not drift around transitions.** Prefer the `merge.ps1` model (each clip has its own naturally synced overlay; the transition is created from that material). Transitions may be handled last if that reduces risk to everything else.
8. **Transparent export:** overlay only for the time of the loaded files minus deleted parts, no transitions, single file or multiple files.

Earlier decisions from the first planning round that still stand: warn on overlaps (never auto-push), place clips outside the activity, ask the user to place clips whose time cannot be determined, default "no transition" for camera splits with a gap of a couple of seconds.

## Owner decisions (2026-10-15, second review)

9. **Time detection and sync stay exactly as today.** Do not change time guessing/management. The user selects timezone/offset on import as now and it applies to all files; afterwards each video's offset can be changed individually.
10. **Clips outside the activity keep their real signed offset** (not forced to 0), with a warning. Applies to single-video sessions too.
11. **File modified time** stays the last fallback and is used as the start time, as today.
12. **Without activity**, clips are placed by detected real time, keeping gaps.
13. **O1:** incompatible clips are allowed. Warn on import; single-file export disabled while they are on the timeline. Users can delete whole files from the timeline; once no incompatible clip remains, single-file export is allowed again.
14. **O3:** default transition `wipeleft`, ~~3 s~~ → **2 s** (changed in the third review).
15. **O5:** the player must show transitions, so the user can see the output without rendering everything → FFmpeg transition proxies.

## Owner decisions (2026-10-15, third review)

16. **Default transition duration 2 s** (type stays `wipeleft`).
17. **Commit policy:** separate commits per feature/milestone are welcome for review, but the agent never commits. The agent says when it is OK to commit and proposes a message; the owner reviews, commits, and tells the agent to continue.
18. **O10:** the shared offset shifts all clips by the same amount, keeping each clip's individual adjustment.
19. **O12:** opacity cross-fade of the two live overlays during a transition in preview is acceptable for now; check once implemented and change if needed.
20. **O4:** skip gaps during playback.
21. **O6:** one codec for everything — head/tail/body/transition parts all use the same codec and settings as the output; no separate intermediate codec.
22. **O7:** in preview, play the earlier clip, then continue the later clip at the matching position, with a very visible warning while inside the overlap. This behaviour is preview-only. Any overlap blocks single-file export; deleting the overlapping part of a clip resolves it.
23. **O8:** keep undo/redo as it is now; review after all changes are done.
24. **O9:** the rasters-only rule also applies without external activity (coverage = the clip's own embedded telemetry).
25. **O11:** 2 s is the hard default transition duration, kept in the project-wide settings; each transition (join) can have its own type and duration. Interpretation used in the docs: the project settings hold the default transition type (`wipeleft`) and duration (2 s); the camera-split gap threshold stays a fixed 2 s in code. Correct this if the threshold should also be a project setting. → Superseded by decision 29: the threshold is a project setting.

## Owner decisions (2026-10-15, existing projects)

26. **Existing projects are converted automatically and silently on open**, like v1/v2 → v3 today (`migrate_project` in `src-tauri/src/project_file.rs`). v3 → v4: single video → one-clip collection with its saved offset; landmarks get the clip ID; no deleted parts/transitions; default transition settings added. No change in look or behaviour.
27. **Batch-folder projects** (`render.renderTarget: batch`, `batchVideoFolder`, `batchOutputFolder` — persisted today and hydrated in `features/projects/utils/projectHydration.js`): on open, import all videos currently in the folder into the timeline (normal import path: dedupe with the project video, compatibility/overlap warnings, time placement), switch export to separate-file mode, keep the output folder, show a notice. The folder content is read at open time and may differ from when the project was saved. Missing/empty folder → keep the project's own video and say so in the notice.
28. **Backup on first v4 save:** keep the original next to it and tell the user once that older versions cannot open the new file. Name it `<name>.oly.v3.bak`. Do not use `<name>.v3.oly`: `list_project_files` lists every `.oly` file, so the backup would appear in the projects drawer.

## Owner decisions (2026-10-15, transition defaults dialog)

29. **One-time transition defaults dialog.** When an import makes the project contain more than one video, show options for the default transition type (including "No transition"), its duration, and the threshold for treating clips as continuous. Preselected: `wipeleft`, 2 s, threshold 2 s. Each transition remains individually configurable later.
    - Not shown when a single file is imported.
    - Adding videos to a single-video project shows it (if not shown before).
    - Shown only once per project (`dialogShown` persisted in the project): never shown again, so existing changes are not overridden.
    - The threshold is a project setting (resolves O13).
    - Dismissing the dialog applies the preselected values and counts as shown (O14).
    - The defaults stay editable in the project settings; changing them re-applies only to joins still on their default (O15).

## Existing video time chain (must not change)

Checked 2026-10-15. Per clip, at probe time:

1. **GPS** (`timeSource: 'gps'`) — `media/mp4_telemetry/mod.rs` `probe_video_metadata`: sync time from embedded camera telemetry (GoPro GPSU, DJI, Insta360) when present.
2. **Container metadata** (`'ffprobe'`) — `media/video_probe.rs`: `format.tags.creation_time` → `streams[0].tags.creation_time` → `format.tags.com.apple.quicktime.creationdate`. Merged in `media/prepared_video.rs` `merge_ffprobe_metadata` only when no GPS time exists. (The doc comment in `video_probe.rs` lists stream before format; the code checks format first.)
3. **File modified time** (`'file_mtime'`) — `video_probe.rs`, last fallback, converted to UTC RFC 3339.
4. **Filename** (`'filename'`) — never automatic; applied by the user action `setVideoCreationTimeFromFilename` (`YYYYMMDD_HHMMSS`).

Sync (`app/src/lib/video-sync.js`):

- `calculateAutomaticVideoOffset`: offset = video time − activity start. `gps` and `file_mtime` are absolute; `ffprobe`/`filename` are ambiguous and interpreted per `videoSyncTimezoneMode`.
- `resolveVideoSyncState`: with no explicit mode, tries `local` and `utc` and selects the one whose clip overlaps the activity (prefers local). If none overlaps → **offset 0 + warning** today. This last branch is the only thing that changes (decision 10): keep the signed offset of the selected candidate (`matches[0] ?? candidates[0]`) and show a "no activity coverage" warning.
- Errors (`VideoSyncError`: no creation time, missing activity timezone, invalid timestamps) → offset 0 + warning today. With many clips this would stack clips at 0, so these clips go to the user placement prompt instead.

Notes for implementation:

- `file_mtime` is typically the moment recording stopped (camera writes until stop), and some copy tools reset it. Owner decision: keep using it as the start time as today. Clips using it should show their time source clearly in the videos panel (already done for one video via `videoDrawerUtils.js`).
- Per-clip inference (decision 9) means two clips with ambiguous sources can resolve to different timezone interpretations when one overlaps the activity and one does not. Once the user selects a mode it applies to all clips.

## `merge.ps1` technique (copy in this folder; original in gitignored `tmp/`)

## Changes vs. the 2026-08-13 spec

| Topic | Old | New |
| --- | --- | --- |
| Transitions | out of scope, hard cut only | per-join `xfade`, also at deleted parts |
| Overlaps | auto-pushed forward | warning, user resolves, combined export blocked |
| MP4-only placement | contiguous | real-time from the unchanged time chain (gaps visible; needed for default transition rule) |
| Clips outside activity | forced to offset 0 + warning | real signed offset kept + warning; rendered with rasters-only overlay |
| Unknown time | picker order | user placement prompt (only when no time can be determined / sync fails) |
| Compatibility | reject at import incl. CFR | resolution + exact FPS checked at import; incompatible clips allowed with warning, single-file export disabled until removed; audio presence not required |
| Transition preview | n/a | FFmpeg proxies played in the live player |
| Persistence | none | project format v4 (projects exist now) |
| Batch API | new API | extend existing `render_jobs` |
| Combined export | concat whole clip outputs | concat pieces; transitions from rendered head/tail; separate audio pass |
| Folder batch | separate concept | removed; collection is the batch |

## Verified: transition timing

Command (bundled FFmpeg 8.1, `vendor/ffmpeg/bin`):

```sh
ffmpeg -f lavfi -i testsrc2=s=320x180:r=30:d=60 -f lavfi -i sine=f=440:d=60 -shortest -c:v libx264 -c:a aac a60.mp4
ffmpeg -f lavfi -i testsrc2=s=320x180:r=30:d=60 -f lavfi -i sine=f=880:d=60 -shortest -c:v libx264 -c:a aac b60.mp4
ffmpeg -i a60.mp4 -i b60.mp4 -filter_complex \
  "[0:v][1:v]xfade=transition=fade:duration=3:offset=57[v];[0:a][1:a]acrossfade=d=3[a]" \
  -map "[v]" -map "[a]" xf.mp4
```

Result: inputs 60.000 s / 1800 frames each; output **117.000 s / 3510 frames**, audio 117 s. So:

- Combined duration = Σ pieces − Σ transition durations. Hard cuts remove nothing (60 + 60 = 120).
- `xfade` `offset` = (outgoing length − d) in the outgoing input's time; the output loses `d` per transition.
- Overlay sync: every output frame comes from source frames at their own source time. If the overlay is baked per piece (merge.ps1 model), sync is correct by construction; during the transition both overlays blend. If one overlay were instead drawn over the program following the outgoing clip, then after a `d`-second transition the incoming clip's overlay must resume at source time `d`, not `0` — otherwise it lags by `d`. The owner's concern is correct; the chosen design avoids it entirely.

- Layout per clip `i` with transition `d` (same `d` everywhere in the script):
  - original segment: start `d` (or `0` for first), duration `D − d − start` (or "to end" for last), stream copied
  - transition: `xfade` of last `d` s of clip `i` and first `d` s of clip `i+1`, `acrossfade d`, encoded with NVENC matching the source codec
- Requires every clip longer than `2d`.
- Parts are remuxed to MPEG-TS with the Annex-B bitstream filter, then joined with the `concat:` protocol, because MP4 stores SPS/PPS once (avcC/hvcC) and encoder differences break `-c copy` concat.
- The concat list must be UTF-8 without BOM.
- Pitfall: `-ss` + `-c copy` cuts on keyframes, not exact frames. Our version avoids this: we render head/body/tail as separate encodes, so no stream-copy cut ever happens inside a GOP.
- Pitfall: transition parts must use the same codec/profile as the copied parts. In OVRLEY every part is encoded by us with the same settings, so this holds by construction.

## `xfade` catalogue (FFmpeg 8.1)

58 built-in transitions (`custom` excluded): fade, wipeleft, wiperight, wipeup, wipedown, slideleft, slideright, slideup, slidedown, circlecrop, rectcrop, distance, fadeblack, fadewhite, radial, smoothleft, smoothright, smoothup, smoothdown, circleopen, circleclose, vertopen, vertclose, horzopen, horzclose, dissolve, pixelize, diagtl, diagtr, diagbl, diagbr, hlslice, hrslice, vuslice, vdslice, hblur, fadegrays, wipetl, wipetr, wipebl, wipebr, squeezeh, squeezev, zoomin, fadefast, fadeslow, hlwind, hrwind, vuwind, vdwind, coverleft, coverright, coverup, coverdown, revealleft, revealright, revealup, revealdown.

Source: `ffmpeg -h filter=xfade`. Picker previews should be generated by us with the bundled FFmpeg (wiki GIF licensing is unclear).

## Code map (current state on `main` @ 13c3f41f)

Frontend singleton video state:

- `app/src/store/slices/createVideoImportSlice.js` — ~25 flat fields (`importedVideoPath`, `importedVideoDuration`, `importedVideoFps*`, `importedVideoResolution`, creation time/source, preview ID/URL/warnings, `videoSyncOffsetSeconds`, `videoSyncOffsetPreviewSeconds`, `videoSyncWarning`, `videoSyncTimezoneMode`, codec/camera fields).
- `app/src/store/slices/createMediaSlice.js` — activity + embedded telemetry stash (`loadVideoTelemetry`, `clearVideoTelemetry`).
- `app/src/store/slices/createManualVideoSyncSlice.js` — landmark-based manual sync for one video.
- `app/src/store/slices/createBatchRenderSlice.js` — folder batch queue (`batchVideoFolder`, `batchQueue`, `skipOverlay`).
- Consumers: ~46 production files and ~31 test files reference singleton fields.

Frontend preview/timeline/render:

- `features/video-preview/hooks/useVideoImport.js` (single `openSinglePath`), `useVideoPreview.js`, `useVideoPlaybackClock.js`, `utils/videoPreviewSource.js`.
- `features/player/hooks/useTimelineClips.js`, `useClipDrag.js`, `usePlaybackEngine.js`, `useOverlayPlayer.js`, `utils/playerTiming.js`, `utils/timelineGeometry.js`, `utils/timelineViewport.js`.
- Export range is one global `{ type, from, to }` in the template scene (`lib/template/template-constants.js`, `features/overlay-editor/utils/exportRange.js`). `features/render-video/utils/renderConfig.js` converts it into `composite_video_trim_start`, `composite_render_duration`, `composite_sync_offset` for the single video.
- Batch UI: `features/render-video/hooks/useBatchInspection.js` (lists folder files via `backend.listDirectoryVideoFiles`), `BatchRenderQueue.jsx`, `BatchRenderQueueRow.jsx`.
- Projects: `features/projects/utils/projectSnapshot.js`, `projectHydration.js`.

Native:

- `src-tauri/src/video_server.rs` — single `current: Option<CurrentVideo>`; `/video/<id>` range server.
- `src-tauri/src/project_file.rs` — `PROJECT_VERSION = 3`, `sources.video: Option<ProjectSource>`, `sync.videoOffsetSeconds`, manual landmarks with `videoSecond`, `migrate_project` for v1–v3.
- `ovrley_core/src/media/video_probe.rs` — fps num/den from `avg_frame_rate`/`r_frame_rate`, `has_audio`, creation time priority chain. No frame count yet.
- `ovrley_core/src/render_jobs/` — native batch: `contracts.rs` (`BatchRenderRequest`, `BatchActivity::{EmbeddedActivity, ExternalActivity{automatic_offsets, reference}}`, `BatchRenderJob{skip_overlay}`), `planning.rs` (`plan_batch_item`, `plan_single_render`, `derive_composite_render_plan`), `batch.rs`, `batch_state.rs`, `execution.rs`, `submission.rs`, `inspection.rs`.
- `derive_composite_render_plan` errors when the video does not overlap the activity → must change for clips outside the activity.
- `render/mod.rs` `prepare_output_frame` clears frames outside coverage (`pixels.fill(0)`), so rasters disappear today → change to rasters-only.
- `plan_batch_item` with `skip_overlay` clears backdrops/labels/values/course/elevation plots and keeps rasters — reuse for rasters-only.
- `encode/ffmpeg/composite.rs` — input 0 coarse seek + filter-side trim, input 1 RGBA overlay pipe, input 2 audio; supports `trim_start` + `render_duration` already, which is exactly what a piece needs.
- `encode/video_timing.rs` — `ActivityCoverage { blank_leading_frame_count, frame_count, … }`.

## Risks

- **Big-bang M2.** The no-alias rule forces all singleton consumers to migrate at once. Mitigation: utilities and slice first with tests, then consumers feature by feature in one sitting; keep the branch rebased on `main`.
- **Dual decode in WebView.** Two 4K HEVC elements (active + preload) may stutter on WebKitGTK/low-end GPUs. Mitigation: preload only near piece end; verify manually early in M4.
- **Hardware encoders and many short encodes.** Head/body/tail split multiplies encoder sessions (NVENC session limits, startup cost). Sessions are sequential, so limits are not hit, but startup overhead adds up for many short pieces.
- **AAC priming.** Concatenating per-part AAC creates gaps/clicks; solved by the separate audio pass.
- **Frame-grid rounding.** All piece/transition boundaries must be expressed in frames at the session FPS; durations in seconds only for display.
- **Concat compatibility.** Verify mp4 vs TS remux path for every supported composite codec (H.264/H.265 software and hardware) and transparent codecs (ProRes, QT RLE are intra-only).
- **Undo/redo.** Import runs outside editor history (`runWithoutEditorHistory`). Kept as is for now (O8); review which new edits (offsets, deleted parts, transitions, placement) should be undoable after all milestones.
- **Transition proxy cost.** Each transition join needs a background FFmpeg render (≈ `d` seconds of two decodes + one preview encode). Many joins or frequent offset edits cause re-renders; mitigate with a debounced, cancellable, playhead-prioritised queue and content-key cache.
- **Preview vs export overlay in transitions.** Export blends both baked overlays with the transition shape (e.g. wiped for `wipeleft`); preview cross-fades the two live overlays by opacity. Video is exact in both; only the overlay transition shape differs in preview. Accepted for now (O12); re-check after M8.
- **Double encoding of transition regions.** Using the output codec for head/tail (O6) means the `d`-second transition regions are encoded twice (generation loss only there). Check visually with low-bitrate settings and hardware encoders.
- **Mixed time sources in one session.** A camera can produce `gps` time for clips with a GPS lock and `ffprobe` time for clips without one. `ffprobe` times are interpreted by timezone mode, so relative placement between such clips can be off by the timezone offset if the interpretation is wrong. Surface the time source per clip so the user sees it.

## Open questions

Resolved: O1, O2, O3 (`wipeleft`, 2 s), O4 (skip gaps), O5 (FFmpeg proxies), O6 (single codec), O7 (earlier clip first, visible warning, preview only; overlap blocks single-file export until moved/deleted), O8 (undo unchanged, review later), O9 (rasters-only also without activity), O10 (shared shift keeps per-clip adjustments), O11 (2 s default in project settings, per-join override), O12 (opacity cross-fade for now). See owner decisions above.

- **O13 Split gap threshold location:** resolved by decision 29 — project setting, default 2 s, set in the transition defaults dialog.
- **O14 Dismissing the transition defaults dialog:** resolved — dismissing applies the preselected values; the dialog counts as shown (`dialogShown: true`).
- **O15 Changing defaults later in project settings:** resolved — the defaults are editable in the project settings; a change re-applies only to joins still on their default and never touches joins the user changed.

## Manual verification matrix

| Area | Check |
| --- | --- |
| Import | Select a whole folder of camera splits; incompatible file warned at import, single-file export disabled, re-enabled after deleting it |
| Transition defaults dialog | Multi-file first import → shown with `wipeleft`/2 s/2 s; single file → not shown; add to single-video project → shown; add again later → not shown; "No transition" → all joins hard cuts; dismissing applies preselected values and is not shown again; changing defaults in project settings updates only joins still on default; values saved/reloaded with the project |
| Placement | Detected times unchanged vs `main` for the same files; clips before/after activity keep signed offset; undeterminable-time prompt; overlap warning |
| Preview | Hard cut hand-off between splits; gap skipping; deleted-part skipping; jumps; transition proxies play at joins and overlay continues from `d` after them |
| Overlay | Rasters-only outside activity in preview and export |
| Export separate | One file per clip; deleted parts removed; transitions at deleted parts |
| Export combined | Duration = Σ − Σd; overlays synced after each transition; audio continuous |
| Transparent | Combined and separate; no transitions; deleted parts excluded |
| Hardware | NVENC/QSV/AMF/VideoToolbox combined export; cancellation mid-concat |
| Projects | Save/reload full session; open v1/v2/v3 projects (look and behave as before); open a v3 batch-folder project (folder videos imported, separate-file mode, notice); first save creates `.oly.v3.bak` and shows the one-time notice; backup not listed in the projects drawer |
