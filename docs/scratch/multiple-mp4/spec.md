# Multiple MP4 Clips Specification

Status: ready-for-agent
Last updated: 2026-08-13

## Problem Statement

OVRLEY currently treats an imported source video as a singleton. Video metadata, preview registration, embedded MP4 telemetry, synchronization, timeline placement, playback-clock ownership, settings, and composite export all assume exactly one video. Importing another video replaces the first one and invalidates its local preview URL.

Action-camera users commonly record one activity as several consecutive MP4 clips. They need to import the recording set together, align every clip independently to one activity, inspect and adjust every clip's synchronization, move freely through the complete recording timeline, and export all footage covered by one export range without repeating the workflow for each file.

The feature must preserve clip independence without creating separate preview servers, synthetic cross-clip telemetry, singleton compatibility aliases, or a multi-input composite renderer.

## Solution

OVRLEY will model imported source videos as one canonical collection of independent video entities. Each entity owns its validated source metadata, local preview registration, timeline offset, synchronization diagnostics, and optional embedded MP4 telemetry. A dedicated video toolbar will display all videos and their data as a list and allow per-video synchronization edits.

All clips occupy one non-overlapping video track. Without an external activity, clips are ordered by their canonical creation/sync timestamp, fall back to picker order when that timestamp is unavailable, and are placed contiguously on an exact shared frame grid. With an external activity, every clip receives an independent activity-relative automatic offset and may leave gaps. The global playhead derives the active or upcoming clip and automatically transfers preview playback between two reusable video elements served by one multi-registration loopback HTTP server.

External activity data is authoritative for the entire timeline. Without an external activity, each clip uses only its own embedded telemetry; telemetry is never stitched into a synthetic activity or borrowed from another clip. Missing embedded telemetry remains a valid video state and produces an empty overlay for that clip during MP4-only export.

Export is submitted once as a batch with shared encoding settings, one global export range, an optional shared external activity, and an ordered list of clip jobs. The backend renders positive-overlap clip portions sequentially through the existing single-video composite pipeline. It either returns the separate outputs or concatenates compatible intermediates with FFmpeg stream copy into one final video.

## User Stories

1. As an action-camera user, I want to select several MP4 files in one import operation, so that I can load an entire recording session efficiently.
2. As an action-camera user, I want every successfully imported file to remain an independent clip, so that its metadata, telemetry, and synchronization are not overwritten by another clip.
3. As a user, I want failed files reported individually while valid files still import, so that one corrupt or incompatible file does not discard the whole selection.
4. As a user, I want the existing blocking import loader shown until every selected file has either imported or failed, so that the editor never exposes a partially committed import batch.
5. As a user, I want duplicate source paths rejected, so that the same recording cannot accidentally appear twice in one project.
6. As a user, I want all clips to share one display-oriented resolution and exact rational FPS, so that preview transitions and batch output are deterministic.
7. As a user, I want variable-frame-rate or indeterminate-frame-count files rejected individually, so that clip boundaries remain exact and concatenation remains safe.
8. As a user, I want clips with different coded orientation accepted when their display-oriented resolution matches, so that rotated camera files can participate in the same session.
9. As a user, I want MP4-only clips ordered by the canonical creation/sync time detected from existing metadata pathways, so that recordings normally appear chronologically.
10. As a user, I want picker order preserved for clips whose creation/sync time is unavailable, so that their order remains deterministic.
11. As a user, I want MP4-only clips placed end-to-start without gaps, so that the timeline contains only playable footage.
12. As a user, I want contiguous boundaries based on exact frame counts and the shared rational FPS, so that no floating-point gaps or overlaps appear between clips.
13. As a user, I want an imported external activity to calculate an independent automatic offset for every clip, so that all recordings align to the activity timeline.
14. As a user, I want replacing an external activity to reset and recalculate every clip offset, including manually edited offsets, so that synchronization follows the current activity just as it does in the existing workflow.
15. As a user, I want clearing the external activity to rebuild a contiguous creation-time-ordered MP4-only timeline, so that activity-relative gaps and edits do not leak into clip-only mode.
16. As a user, I want newly added clips to re-sort and rebuild the complete MP4-only timeline, so that chronological contiguous placement remains canonical.
17. As a user, I want newly added clips to synchronize independently without changing existing offsets when an external activity is active, so that existing manual alignment work remains stable until the activity itself is replaced.
18. As a user, I want automatic synchronization conflicts resolved without rejecting otherwise valid clips, so that hypothetical overlapping camera timestamps do not block import.
19. As a user, I want automatic conflicts resolved deterministically by keeping the newly calculated clip position and pushing overlapping existing/later clips forward, so that the track becomes non-overlapping without hidden trimming.
20. As a user, I want activity-wide recalculation to sort raw automatic offsets and sweep clips forward to resolve overlaps, so that repeated synchronization produces the same result.
21. As a user, I want displaced video to remain valid even if it extends beyond the external activity, so that rare timestamp conflicts do not cause data loss.
22. As a user, I want every imported video and its metadata shown in a dedicated toolbar list, so that I can inspect the complete recording set at once.
23. As a user, I want a separate synchronization-offset input for each clip while an external activity exists, so that I can correct individual alignment precisely.
24. As a user, I want timeline dragging and toolbar inputs to update the same clip-specific offset, so that both synchronization interfaces remain consistent.
25. As a user, I want manual offset edits to be rejected when their committed result overlaps another clip, so that canonical playback state always remains unambiguous.
26. As a user, I want invalid input to leave the last valid offset committed and show an error on that clip, so that temporary typing errors do not corrupt the timeline.
27. As a user, I want to drag a clip transiently across other clips and reorder it, so that creation-time order does not permanently constrain manual alignment.
28. As a user, I want an invalid drag drop to snap back, so that transient overlap never becomes canonical state.
29. As a user, I want dragging to snap to activity boundaries and every other clip edge without moving neighboring clips, so that manual alignment remains precise and local.
30. As a user, I want toolbar order to follow committed timeline position but remain stable during a transient drag, so that controls do not jump while I am interacting with them.
31. As a user, I want removing a clip to pause playback first and clamp the playhead afterward, so that playback does not unexpectedly jump into unrelated footage.
32. As a user, I want removing a clip in MP4-only mode to close the gap by rebuilding the contiguous timeline, so that no dead time remains.
33. As a user, I want removing a clip with external activity to preserve every other clip offset, so that unrelated synchronization is not changed.
34. As a user, I want video playback available immediately after metadata import, so that I do not wait for slower embedded telemetry extraction.
35. As a user, I want embedded telemetry extracted sequentially in timeline order, so that background extraction does not overwhelm disk and CPU resources or compete unnecessarily with preview playback.
36. As a user, I want late telemetry results discarded safely after their clip is removed, so that asynchronous extraction cannot resurrect removed data.
37. As a user, I want external activity to override embedded telemetry for every clip without per-clip source toggles, so that one timeline has one authoritative telemetry source.
38. As a user, I want each MP4-only clip to use its own telemetry and local clip time, so that telemetry remains independent and resets correctly at clip boundaries.
39. As a user, I want missing or failed embedded telemetry to remain a valid media state, so that footage without supported telemetry still plays and exports.
40. As a user, I want the playhead to transition automatically between contiguous clips with a continuous hard cut, so that the recording set behaves as one playback track.
41. As a user, I want preview to use constant resources rather than one player per clip, so that large recording sets remain practical.
42. As a user, I want a direct scrub or jump to move the playhead immediately even when the destination clip is not preloaded, so that timeline interaction stays responsive.
43. As a user, I want the previous clip hidden while a directly selected clip loads, so that stale footage is never shown at the wrong timeline position.
44. As a user, I want an external-activity gap to show the first frozen frame of the next upcoming clip while widgets continue advancing, so that preview always anticipates the next available footage.
45. As a user, I want the last frame of the final clip preserved after it ends while external activity continues, so that post-video preview matches existing behavior.
46. As a user, I want one global export range to apply to the entire video track, so that one job can export only the clip portions I selected.
47. As a user, I want clips outside the export range skipped and partially intersected clips trimmed, so that outputs contain exactly the available footage within the chosen range.
48. As a user, I want batch export disabled until every telemetry extraction has settled, so that the submitted job uses a stable snapshot.
49. As a user, I want a settled clip without telemetry exported with empty overlay frames rather than failing, so that all valid footage can be included.
50. As a user, I want external activity submitted once for the batch, so that a large activity payload is not duplicated for every clip.
51. As a user, I want MP4-only clip telemetry submitted with its owning clip job, so that each render uses the correct independent activity.
52. As a user, I want every intersecting clip rendered sequentially in committed timeline order, so that batch behavior is predictable and resource usage remains bounded.
53. As a user, I want batch progress weighted by total output frames and to identify the active clip, so that progress reflects actual work rather than treating short and long clips equally.
54. As a user, I want the first failed clip to stop the batch and identify the failure, so that later work is not attempted after the requested job has failed.
55. As a user, I want completed outputs retained after a failure or cancellation, so that successful rendering work is not lost.
56. As a user, I want cancellation to stop the active render, skip remaining clips, and remove incomplete output, so that cancellation has one predictable lifecycle.
57. As a user, I want separate-file export to produce one file for every intersecting clip portion and open the output folder once, so that the app does not launch many system players.
58. As a user, I want an option to concatenate rendered clip portions directly without timeline gaps, so that I receive one continuous compilation of available footage.
59. As a user, I want concatenation to preserve each clip's original activity-relative telemetry sampling while omitting gaps, so that displayed data remains synchronized even though unavailable footage is removed.
60. As a user, I want a concatenated batch to use shared codec, profile, pixel format, bitrate, and encoder settings, so that all intermediates are compatible by construction.
61. As a user, I want mixed audio presence rejected for concatenation while remaining valid for separate-file export, so that FFmpeg does not fail unpredictably.
62. As a user, I want present audio normalized to one batch-level AAC sample rate and channel layout, so that same-session source variations do not break concatenation.
63. As a user, I want rendered intermediates probed before concatenation, so that actual output resolution, FPS, time base, and stream compatibility are verified at the FFmpeg boundary.
64. As a user, I want concatenation to use stream copy, so that the final step is fast and introduces no additional quality loss.
65. As a user, I want cancellation available during concatenation, so that I retain control through the final phase.
66. As a user, I want a successful concatenated export to remove per-clip intermediates only after the final output is verified, so that temporary files do not remain while failures remain recoverable.
67. As a user, I want a one-clip concatenated job to skip the concat phase and treat that clip output as final, so that unnecessary FFmpeg work is avoided.
68. As a user, I want a successful concatenated export to open the single final video, so that completion behavior matches the current single-video workflow.

## Implementation Decisions

- The singleton imported-video contract will be replaced at its owner by one canonical collection of video entities. Singleton aliases, mirrored fields, compatibility selectors, and remapped consumer objects are prohibited.
- Each video entity owns a stable internal ID, absolute source path, picker-order identity, validated source metadata, exact frame count, canonical creation/sync timestamp, preview registration, timeline start, sync warning/timezone interpretation, codec/camera information, and embedded telemetry extraction state and payload.
- The collection does not store a canonical selected video. The toolbar renders the full collection. Any expanded or focused row is local presentation state.
- The active video, upcoming video, effective telemetry, timeline bounds, and display/export order are derived values. They are not duplicated in store state.
- Transient drag state identifies one video and a proposed start time. It may overlap other clips but never mutates committed offsets until validation succeeds.
- External activity remains one global media entity and is authoritative whenever present. Embedded telemetry remains attached to its video and is not moved into a global stash.
- MP4-only effective telemetry is resolved from the video under the playhead and uses video-local time. Route, history, and other whole-activity widgets therefore describe only the active clip.
- Missing, failed, or pending embedded telemetry is documented optional external-data absence. It must not be repaired with another clip's telemetry or a synthetic activity.
- MP4-only placement sorts clips with canonical creation/sync timestamps first and uses picker order as the stable fallback/tie-breaker. Placement is contiguous on cumulative exact frame counts divided by the shared rational FPS.
- External-activity placement calculates every automatic offset independently through the existing creation-time and timezone interpretation rules.
- Activity import or replacement resets every manual value and recalculates the entire set. It then sorts by raw calculated offset, uses current timeline order as a tie-breaker, and sweeps forward to eliminate overlaps.
- Adding clips while external activity exists calculates offsets only for the new clips. A new clip retains its calculated position; conflicting existing/later clips are pushed forward, cascading as required.
- Removing external activity discards activity-relative placement and rebuilds contiguous MP4-only placement.
- Manual offsets are available only with external activity. Manual toolbar and drag commits may reorder clips but must leave the committed track non-overlapping. They never push neighboring clips.
- Committed display and batch order is ascending timeline start. Creation time is initialization data, not a second mutable order.
- Source-video ingress is CFR-only. Probe metadata will expose exact frame count and both average and nominal rational frame rates. A required timing contract that is missing, malformed, variable, or indeterminate fails that file's import.
- The first successfully validated clip in picker order establishes the session's exact rational FPS and display-oriented resolution. Existing collections establish the reference for later imports. Incompatible new files fail individually.
- Rotation and coded dimensions may differ when normalized display-oriented dimensions match.
- Multi-file import is one blocking frontend operation. No partially imported clips enter the visible collection. Once all files settle, successful entities are committed atomically and per-file failures are reported.
- Preview registration and metadata probing are part of blocking import. Embedded telemetry extraction is not; it begins after committed import through a sequential, timeline-ordered queue.
- Async telemetry completion addresses entities by stable ID and verifies continued existence before committing. Removal does not require cancelling native extraction.
- One Tauri-managed loopback HTTP server owns a registry keyed by opaque import ID. Registering or removing one video affects only that registration. Range requests continue to use the existing `/video/<id>` boundary.
- Preview uses two reusable video elements: one active and one preloading the next timeline clip. It does not allocate one element or one server per imported video.
- At a contiguous boundary, the preloaded element takes over with a hard cut and video-clock ownership follows it. During external-activity gaps, the timeline clock advances while the next clip's first frame remains frozen. After the final clip, its final frame remains frozen.
- A direct jump to an unloaded clip immediately commits the playhead, hides stale footage, and shows the existing loading presentation until the requested frame is ready.
- Removing the currently playing clip pauses first, removes its preview registration and state, reapplies the mode-specific placement rule, and clamps the playhead.
- The existing video/video-sync settings panel is replaced by a dedicated videos toolbar that displays every imported clip and its data. Components remain presentational; collection behavior and row actions live in hooks and pure utilities.
- The timeline renders multiple video clips on one video track plus the activity track. Clip identities include the video ID, and snapping includes activity bounds and all other clip edges.
- Batch export has one canonical IPC/request shape containing shared render settings, one optional shared external activity, one global export range, concat mode, and ordered per-video jobs. Each job includes source identity/timing and optional clip-owned telemetry.
- The frontend resolves the export-range intersection for each clip on the shared frame grid and submits only positive-overlap jobs in committed timeline order.
- The backend owns batch execution, progress, cancellation, output tracking, concat lifecycle, and cleanup. It reuses the existing single-video composite pipeline once per job rather than creating a multi-input renderer.
- A job with no resolved activity follows an explicit valid no-activity path that emits empty overlay frames and composites/encodes the source portion. It does not construct a malformed or fabricated parsed activity.
- Shared batch encoder configuration fixes codec, profile, pixel format, bitrate, and related encoding behavior for every intermediate.
- Separate-file mode permits differing source audio presence. Concatenated mode requires all included sources to have audio or none to have audio.
- When audio is present for concatenation, every intermediate explicitly uses the same AAC sample rate and channel layout.
- Before concatenation, the backend probes actual intermediates and requires matching display dimensions, exact rational FPS, time base, video stream compatibility, and normalized audio stream shape.
- Concatenation uses the FFmpeg concat demuxer with stream copy and removes timeline gaps by joining rendered portions directly.
- Render progress is aggregated over the total output-frame count and includes current job index/count and video identity. Concatenation is a distinct final phase.
- The first render failure terminates the batch. Cancellation terminates the active render or concat process. Incomplete active/final output and temporary concat manifests are removed.
- Completed clip outputs remain after failure or cancellation. After verified concatenation success, intermediates are removed. A single included clip bypasses concat and is produced directly as the final combined output.
- Separate-file completion opens the output folder once. Concatenated completion opens the final video.
- No project persistence format is introduced by this feature; imported media remains session state as it is today.

## Testing Decisions

- Good tests assert observable contracts and state transitions at the highest available seam. Tests must not bind to private helper names, React implementation structure, internal FFmpeg argument ordering beyond contractual pairs, or exact visual styling.
- The primary frontend state seam is the Zustand media/video slice behavior. It will cover atomic multi-import commit, per-file failure, duplicate rejection, compatibility enforcement, mode-specific placement, activity replacement, overlap recovery, manual validation, removal, derived ordering, telemetry ownership, and race-safe late results.
- Pure timeline tests will cover exact frame-grid placement, creation-time/picker ordering, active/upcoming clip resolution, external gaps, range intersection, deterministic overlap sweeping, manual reorder validation, bounds, and snapping.
- Player hook tests will cover two-element source ownership, continuous boundary handoff, timeline/video clock transitions, direct jumps to unloaded clips, gap-first-frame behavior, final-frame behavior, removal during playback, and playhead clamping.
- Timeline integration tests will cover multiple clip rendering, stable IDs, committed versus transient order, drag-through overlap, invalid-drop rollback, snapping, and toolbar/timeline action consistency.
- Toolbar hook/component tests will use canonical video entities to cover list ordering, metadata/telemetry states, per-video offset edits, validation errors, removal, and disabled MP4-only sync inputs. Styling is not part of the assertions.
- Import hook tests will cover multiple picker paths, one blocking lifecycle, atomic successful commit after all probes settle, partial success, incompatible files, sequential telemetry queueing after commit, and continued queue progress after a telemetry failure.
- Tauri video-server integration tests are the primary native preview seam. They will register multiple files, serve independent byte ranges concurrently, preserve old URLs after later registrations, invalidate only removed IDs, clear the registry, and reject unknown IDs.
- Video-probe integration tests will cover exact frame count, average/nominal rational rates, CFR acceptance, VFR/indeterminate rejection inputs, display-oriented resolution, and rotated coded dimensions.
- Render-config/request tests will cover one shared batch contract, optional external activity, optional per-job telemetry, global range intersection, timeline ordering, homogeneous settings, and no singleton compatibility payload.
- Rust batch-render tests will use injected/fake job execution where possible to cover strict request validation, sequential dispatch, frame-weighted aggregate progress, stop-on-first-failure, cancellation during render and concat, output retention, incomplete-output cleanup, and one-job concat bypass.
- Existing composite-pipeline tests remain the seam proving that one clip job maps video-local time to the correct activity-relative frames. New cases will cover shared external activity, clip-local embedded telemetry, and explicit no-activity empty overlays.
- FFmpeg concat tests will cover compatibility preflight, actual intermediate probe validation, mixed-audio rejection, normalized audio settings, concat-demuxer manifest escaping, stream-copy invocation, gap omission, success cleanup, and failure retention.
- Render workflow tests will cover one batch lifecycle, progress event correlation by batch ID, completion behavior for separate versus concatenated modes, cancellation, and preventing export while telemetry extraction remains unsettled.
- Prior art includes existing video-import/MP4 telemetry Zustand tests, player timing and clip-drag tests, video-preview hook tests, render-config/workflow tests, Tauri video-server integration tests, Rust video-probe tests, FFmpeg composite tests, and composite-pipeline tests.
- A manual verification pass remains appropriate for continuous hard cuts in the native WebView, HEVC decoder behavior, a real same-camera multi-file session, activity-aligned gaps, audio continuity, cancellation of real FFmpeg subprocesses, and opening produced outputs. Manual checks supplement rather than replace automated behavior tests.

## Out of Scope

- Multiple video tracks, overlapping clips, multi-angle selection, compositing two source videos at once, or defining overlap priority.
- Crossfades, dissolves, audio fades, or any transition other than a hard cut.
- User-controlled MP4-only spacing or synchronization offsets; MP4-only placement is contiguous.
- A persistent manual/automatic synchronization mode. Activity import or replacement always resets and recalculates all offsets.
- Per-clip telemetry-source toggles while external activity exists.
- Stitching embedded telemetry into one synthetic activity or creating cross-clip route/history continuity.
- Reordering through a separate explicit reorder UI; committed timeline offsets determine order.
- Automatically pushing neighboring clips during manual toolbar or drag edits.
- Supporting mixed resolution, mixed FPS, variable-frame-rate, or indeterminate-frame-count collections.
- Rescaling or frame-rate converting incompatible source clips during import or concatenation.
- Synthesizing silence for missing audio or supporting mixed audio presence in concatenated mode.
- Re-encoding during final concatenation.
- Selecting an arbitrary subset of imported videos independently of the global export range.
- Parallel clip rendering or parallel embedded telemetry extraction.
- Waiting for telemetry extraction inside an already submitted export job.
- Persisting imported video collections as a project format.
- Reworking unrelated template, widget, or activity-import semantics.

## Further Notes

- The expected source set is normally produced by one action camera during one recording session. Strict compatibility checks protect the runtime but should remain simple and explain individual failures clearly.
- External activity coverage and video-track validity are independent. Automatic conflict recovery may place a clip beyond activity coverage; existing out-of-range widget behavior applies there.
- Export is disabled until every imported clip's telemetry extraction has settled, even though missing or failed telemetry is a valid final state.
- The existing no-activity-time overlay behavior is analogous to the required empty-overlay job path, but the batch ingress must explicitly support optional activity instead of passing malformed activity data.
- This specification intentionally replaces singleton ownership rather than wrapping it in array-to-singleton adapters. Consumers must operate on the collection, address a video by ID, or derive the clip at a timeline second.

