---
Status: ready-for-agent
---

# Rust-Owned Batch Rendering Refactor

## Problem Statement

OVRLEY currently renders a folder of videos by repeatedly importing each video into the visible editor, reading the resulting application state, and submitting a single render. The preview changes behind the render dialog, template dimensions and render settings change, synchronization state is reset, and embedded telemetry can replace the active activity. Batch cleanup clears the imported video instead of preserving the user's editor session.

Folder inspection already prepares videos without committing them to the editor, but execution repeats metadata probing and telemetry extraction. Queue advancement and completion depend on frontend orchestration and progress listeners. Cancellation during import can still allow the item to start rendering afterward.

Several timing and output defects must be corrected alongside the ownership refactor: automatic offsets are clamped to zero, the current video's manual correction is lost, transparent exports repeat the activity instead of using each video's synchronized passage, and output names do not distinguish composite video from transparent overlay output. Prepared inspection results also need an explicit freshness lifecycle.

The activity-overlay row switch has a useful existing purpose that must be preserved: switching it off still renders the item, while suppressing metric widgets and plots. Removing a row is the action that excludes a video from the batch.

## Solution

Submit batch rendering as one independent job owned by Rust. The editor supplies an immutable template, shared settings, synchronization calibration, and optional external activity. The backend processes queued videos sequentially through the existing rendering pipelines and reports authoritative job and aggregate progress. Execution never imports queued videos into the editor or changes its preview registration.

Every opening of batch configuration starts a fresh, lightweight inspection. Inspection checks the source metadata needed for synchronization, activity overlap, and frame estimates. Submission verifies that sources still match that inspection, and each source is checked again immediately before processing. Large embedded telemetry payloads are extracted only when needed for the actual render.

Use the current video's manual adjustment as a shared session correction: subtract its signed automatic offset from its committed offset, then add that difference to each queued video's signed automatic offset. Capture and apply the same Apply Timezone setting throughout the batch.

Composite and transparent outputs use each video's corrected synchronization window. Composite output contains the source footage with the configured overlay. Transparent output contains the matching overlay in video-local time, including transparent frames outside activity coverage so it aligns with the source footage without an additional time shift.

For a source named `ride.mp4`, composite output is `ride_video.mp4` and transparent output is `ride_overlay.mov`. Existing output files may be overwritten without per-file confirmation.

## User Stories

1. As a user, I want batch rendering to leave my imported video in the editor, so that I can resume work with the same media afterward.
2. As a user, I want the background preview to remain stable while batch items change, so that internal processing is not visible behind the dialog.
3. As a user, I want my template dimensions and widget configuration preserved, so that batch rendering does not alter my design.
4. As a user, I want my loaded activity preserved, so that another video's telemetry does not replace my editor data.
5. As a user, I want my playhead, timeline viewport, and synchronization landmarks preserved, so that batch rendering does not discard my working context.
6. As a user, I want batch rendering to preserve editor state after success, failure, and cancellation, so that every outcome leaves my session usable.
7. As a user, I want a folder of videos inspected without importing them into the editor, so that I can review the queue before submitting it.
8. As a user, I want each reopening of batch configuration to inspect the folder again, so that eligibility reflects the current files and activity.
9. As a user, I want closing configuration to discard its inspection results, so that a later opening cannot submit an old preparation session.
10. As a user, I want late inspection results ignored after closing or changing folders, so that old asynchronous work cannot repopulate the queue.
11. As a user, I want relevant settings changes to update synchronization, overlap, and estimates, so that the queue reflects my current choices.
12. As a user, I want lightweight inspection while using an external activity, so that the dialog does not wait for unnecessary full telemetry extraction.
13. As a user, I want changed source files detected at submission, so that rendering does not silently use stale timing or metadata.
14. As a user, I want sources checked again before their turn in the queue, so that edits made while earlier items render are detected.
15. As a user, I want changed or unavailable files identified individually, so that I can understand which item requires attention.
16. As a user, I want one batch submission to capture all shared inputs, so that later editor changes cannot alter an accepted render.
17. As a user, I want videos rendered sequentially, so that rendering resource usage remains bounded.
18. As a user, I want execution owned by the desktop backend, so that queue advancement does not depend on a React component remaining mounted.
19. As a user, I want another render prevented while my batch owns the renderer, so that jobs cannot compete during preparation or between items.
20. As a user, I want automatic offsets to retain negative values, so that footage starting before the activity stays correctly synchronized.
21. As a user, I want overlap checked after synchronization is resolved, so that a valid clip is not rejected merely because its start precedes activity time zero.
22. As a user, I want the current video's manual adjustment carried into the batch, so that the alignment I already established is retained.
23. As a user, I want that correction derived from the committed offset and the automatic offset, so that the batch uses my actual adjustment rather than copying one video's absolute position.
24. As a user, I want the same correction applied once to every queued video, so that recordings from the same session receive consistent treatment.
25. As a user, I want a queued reference video to retain its committed offset, so that applying calibration does not adjust it twice.
26. As a user, I want the same Apply Timezone setting used for every queued video, so that ambiguous camera timestamps receive consistent interpretation.
27. As a user, I want an unavailable calibration baseline reported clearly, so that my manual correction is not silently replaced with zero.
28. As a user, I want activity overlap evaluated using the manually corrected offsets, so that my adjustment can bring videos into or out of the eligible interval.
29. As a user, I want an external activity to remain authoritative for all items, so that each video uses the same activity data.
30. As a user, I want each video to use its own embedded telemetry when no external activity is loaded, so that one recording never borrows another recording's data.
31. As a user, I want missing required embedded telemetry reported for its owning item, so that failure does not produce an invented activity or an unrelated overlay.
32. As a user, I want the Activity overlay switch to retain its current function, so that I can render an item without its metric widgets and plots.
33. As a user, I want labels, backdrops, and raster elements retained when that switch is off, so that the remaining template content behaves as it does today.
34. As a user, I want switched-off items included in execution and progress, so that the switch is not mistaken for a video-selection control.
35. As a user, I want removing a row to exclude its video from submission, so that I have an explicit way to omit an item.
36. As a user, I want transparent batch output scoped to each video's synchronization window, so that the whole activity is not exported repeatedly.
37. As a user, I want transparent overlay time zero to correspond to source-video time zero, so that I can place the output directly over the source footage.
38. As a user, I want transparent padding before or after activity coverage, so that negative offsets and partially overlapping clips remain aligned.
39. As a user, I want composite footage and its transparent overlay to sample the same activity instants, so that both export modes agree on synchronization.
40. As a user, I want each composite output to follow its source video's display dimensions and exact frame rate, so that the batch preserves the source characteristics supported by the existing pipeline.
41. As a user, I want transparent outputs to respect my selected FPS and widget update rate, so that their export settings remain consistent across the batch.
42. As a user, I want output names to identify video and overlay exports, so that I can distinguish the two modes without opening the files.
43. As a user, I want original filename stems preserved, so that outputs remain easy to match to their source videos.
44. As a user, I want existing output files overwritten without repeated dialogs, so that rendering a folder remains one operation.
45. As a user, I want output targets prevented from replacing source inputs, so that permitted output overwrites do not destroy my source media.
46. As a user, I want duplicate output destinations rejected before execution, so that two items cannot silently replace each other's results.
47. As a user, I want active-item and total progress together, so that I can see both local progress and the remaining batch work.
48. As a user, I want total progress weighted by planned output frames, so that long and short videos contribute in proportion to their work.
49. As a user, I want excluded and blocked items omitted from the work total, so that progress describes the submitted render work.
50. As a user, I want a distinct preparing state, so that source and telemetry preparation is not presented as importing a video into my editor.
51. As a user, I want cancellation available during preparation and rendering, so that I can stop the batch throughout its lifecycle.
52. As a user, I want cancellation to prevent later jobs from starting, so that stopping the batch does not launch additional renders.
53. As a user, I want cancellation reported complete only after active work has stopped, so that I can safely start another operation afterward.
54. As a user, I want an individual render failure recorded while later eligible items continue, so that one damaged recording does not discard the rest of the batch.
55. As a user, I want successful outputs retained after failure or cancellation, so that completed work is not lost.
56. As a user, I want incomplete active output cleaned up, so that a partial file is not mistaken for a successful result.
57. As a user, I want finished, partially failed, and cancelled batches shown as explicit outcomes, so that the dialog does not remain indefinitely in a rendering state.
58. As a user, I want shared settings remembered as deliberate render preferences, so that per-item source FPS does not overwrite those preferences.
59. As a user, I want malformed render requests to fail clearly, so that invalid required configuration is not repaired into a different export.
60. As a user, I want completed batch results available independently of progress events, so that a missed event does not lose the final outcome.

## Implementation Decisions

### Ownership and execution

- Introduce a framework-independent Rust render execution service and batch runner. The Tauri shell owns IPC registration and event forwarding; the core service owns accepted requests, renderer reservation, preparation, execution, cancellation, output tracking, and final results.
- Separate existing command preparation and thread dispatch from the underlying render operation. The runner awaits operation completion directly; progress events are observational and do not advance the queue.
- Reuse the existing Skia and FFmpeg composite and transparent pipelines. Extend shared timing/frame production where needed to implement video-local transparent exports and transparent boundary padding.
- Single rendering and batch rendering share execution and renderer reservation. Avoid a second implementation of encoding, output validation, or cancellation. Existing single-render presentation can retain its current behavior.
- Reserve the renderer for the entire accepted batch, including preparation and transitions between items. Release it only after the active operation and subprocess cleanup have completed.
- Run blocking metadata, telemetry, dense-activity, and render preparation on appropriate background workers. Cancellation must be observable before any subsequent render launch.
- Batch execution must never call interactive import, register queued sources with the editor's preview server, clear imported editor media, or use the live application store as per-item execution state.
- The frontend hook captures inputs, manages configuration inspection, submits requests, and observes service snapshots. Components remain presentational; pure synchronization and request construction belong in utilities.
- Preserve the editor's imported video or background image, preview registration, activity, template, global defaults, playhead, viewport, manual synchronization state, and undo history. Remembering accepted render preferences is a deliberate separate operation; per-source metadata must not rewrite preferences.

### Inspection and freshness

- A batch configuration opening creates a new inspection session. Closing configuration, switching away from the batch target, or replacing the input folder discards that session. Reopening performs fresh folder enumeration and inspection even when the remembered folder is unchanged.
- Persist folder and render preferences as today. Inspection results, eligibility, preparation handles, and derived sync offsets are transient and must not be restored as authoritative project data.
- Identify inspection generations explicitly and ignore results that arrive after their session ends. Discarding a session must not affect an already accepted backend job.
- Inspect only the metadata necessary for source validity, signed synchronization, overlap, dimensions, frame-rate resolution, and frame estimates. Source timestamp extraction may require embedded metadata; avoid extracting and transferring a full parsed sensor activity solely for folder eligibility or frame totals.
- Keep inspection concurrency bounded. Reuse unchanged source descriptors only inside the active inspection session or its accepted batch. Do not introduce a persistent frontend preparation cache.
- Changes to activity, timezone interpretation, reference-video calibration, export mode, FPS, or widget update rate invalidate the corresponding derived inspection results. Recompute against the current snapshot; source metadata may be reused within the same session when the source identity still matches.
- At submission, verify each selected source against the inspection's canonical path and file identity information, including size and modification time. Detectable source changes reject acceptance and identify items that require fresh inspection. Do not silently update their timing and begin execution.
- Inspect the calibration source as necessary even when the current video is not in the queued folder. A changed calibration source must not silently produce a new session correction.
- Check each queued source again immediately before preparing its render. A change after acceptance becomes an item error, and later eligible items continue under the captured request.
- File identity checks are practical stale-input detection, not a promise of cryptographic byte identity or protection against files modified during active decoding. Whole-file hashing and immutable copies are outside this refactor.
- Extract required embedded telemetry once for its owning job and reuse it through that job's preparation and render. Release large per-item resources when no longer needed; do not eagerly retain every video's full telemetry payload.

### Request and input contracts

- Define one canonical batch submission contract containing the inspection identity, immutable effective template and encoding settings, optional shared external activity, captured synchronization calibration, output directory, and ordered jobs.
- Each job identifies its source, inspected metadata/file identity, resolved signed offset where external activity applies, existing overlay suppression choice, and deterministic output destination. Removed or blocked rows are not submitted as render jobs.
- The effective template resolves global presentation settings once. Per-item render plans derive dimensions, timing, and source FPS locally without modifying that template or editor state.
- Pin validated raster resources and other required referenced resources when accepting the request, so later editor resource changes cannot invalidate queued jobs.
- Shared external activity is submitted once and retained as an immutable batch input. An accepted batch must not re-read current activity or render settings between items.
- Validate user configuration and required request structure once at ingress. Malformed present data fails loudly; optional absence has explicitly defined behavior. Consumers use validated canonical inputs without compatibility aliases, repeated coercion, or defensive repairs.
- External media metadata and activity data remain external-system inputs and may require the existing tolerant parsing rules. Preparation must report unusable source data explicitly and must not fabricate required activity or render state.
- Preserve one owner for source metadata and automatic synchronization rules shared by interactive import and batch inspection. Extract pure synchronization from store actions. The backend consumes the resolved synchronization contract and builds the authoritative render plan; it must not introduce a competing camera-time interpretation.

### Signed offsets and shared session correction

- Automatic offset is the signed difference in seconds between the interpreted video creation time and the activity's synchronization origin. Do not clamp it to zero or to the activity duration.
- Separate timestamp interpretation and automatic-offset calculation from activity-overlap eligibility. A raw automatic offset must remain available even if the uncorrected window is outside the activity, because the shared manual correction may bring it into overlap.
- Capture the current video's effective Apply Timezone setting once. For the existing UI, checked means UTC interpretation and unchecked means local interpretation; the existing nullable unchecked state resolves to local as documented UI optionality.
- Apply that setting consistently to ambiguous camera and filename timestamps throughout inspection and submission. Do not choose a different interpretation per item merely to obtain overlap. Trusted GPS timestamps retain their existing absolute-time semantics.
- When an external activity and current reference video exist, recalculate that video's signed automatic offset under the captured activity, effective timestamp/source, and timezone interpretation. The shared manual correction is its committed sync offset minus that automatic offset.
- Resolve every queued video's effective offset as its own signed automatic offset plus the shared manual correction. Apply the correction exactly once. If the reference video is also queued, its effective offset must equal its committed offset under the same inputs.
- Derive correction from the committed offset, not a transient drag preview, an absolute offset copied to all videos, or synchronization landmarks themselves. Landmark data remains editor-owned and unchanged.
- When no current reference video exists, shared correction is explicitly zero. If a reference video exists but its automatic baseline cannot be determined, report a calibration error rather than substituting zero or inventing a baseline.
- Without an external activity, each source's embedded activity uses that source's local clock with offset zero. Do not apply an activity-file calibration or use the editor's current embedded activity as a shared reference for other sources.
- Test overlap only after timezone interpretation and manual correction. A job is eligible when its corrected video interval has positive intersection with its available activity timeline. Exact boundary touching without positive duration is ineligible.
- Preserve signed offsets in interactive automatic synchronization as well, so the calibration baseline and visible current-video state follow the same corrected rule.

### Activity-overlay switch and queue membership

- Preserve the existing Activity overlay switch. An off item still runs and produces an output file; its render-effective template has metric widgets and plots suppressed.
- Labels, backdrops, and raster elements remain present when the switch is off, matching current behavior. Preserve the existing canonical suppression field rather than introducing an alias with video-selection semantics.
- The switch does not bypass synchronization, overlap checks, required activity, source validation, or output planning. Off items contribute normally to frame totals and completion.
- Removing a row excludes the video from the submitted queue. Preserve separate reporting for excluded rows and items blocked by eligibility checks; do not infer exclusion from the overlay switch.
- Queue membership, ordering, and suppression choices are frozen when the request is accepted.

### Per-video export windows

- Each job has a video-local output clock starting at zero and a duration derived from that source video. At video-local time t, sample activity at the corrected sync offset plus t. The full video window and its intersection with activity coverage are distinct parts of the render plan.
- Composite output preserves the source footage's supported duration and exact rational FPS. Transparent output uses the same video-local window with the selected transparent FPS and widget update rate. Neither mode repeats the entire activity for each queued source.
- Transparent output contains fully transparent leading and trailing frames where the source window lies outside activity coverage. During covered time, render the effective template, including the chosen overlay suppression behavior. Composite output uses equivalent empty overlays outside coverage.
- Keep frame-count rounding and any existing sub-frame duration tolerance in the canonical backend planner. Both modes must use consistent temporal coverage; all planned durations and frame counts account for boundary padding.
- For external activity, the backend retains the full immutable parsed activity for sampling and derives the relevant dense render window per job. Do not mutate activity samples or replace their timeline origin with the clip's origin.
- For embedded telemetry, each job uses only its own parsed activity. Missing or failed required telemetry is an item error, and later eligible items continue. This refactor does not introduce an activity-free rendering mode.
- Each job derives display-oriented output dimensions from its source metadata, preserving current batch import sizing behavior without changing the editor's scene. Composite FPS follows the source; transparent FPS and update rate follow shared export settings.
- Existing single-render custom range behavior remains separate. Batch exports use their per-video windows and must not inherit the editor's full-activity range or current video's custom range as a shared batch window.

### Output naming and overwrite behavior

- Remove only the final source extension to obtain the original video name. Preserve the remaining stem, including dots, spaces, and Unicode characters.
- Composite output naming is original video stem followed by `_video.mp4`. Transparent output naming is original video stem followed by `_overlay.mov`.
- Derive destinations inside the selected output directory. These suffixes are mandatory for batch output, independent of the selected codec within the corresponding container.
- Existing output files may be overwritten. Do not add per-file overwrite prompts, conflict dialogs, automatic numbering, rename suggestions, or a multi-file overwrite management flow.
- Validate destinations before acceptance: selected jobs must have distinct destinations under the target filesystem's path-comparison rules, and no output may alias any batch input or the calibration source. Reject conflicting plans with a clear error instead of changing the naming convention.
- Retain completed outputs on later failure or cancellation. Remove incomplete active output through the existing guarded cleanup mechanism. Preserve strict output-directory and codec validation.

### Progress, failure, and cancellation

- Expose an authoritative backend batch snapshot with batch identity, phase, ordered per-item statuses, active-item identity, planned frame totals, current and encoded work, timing estimates, completed output paths, and terminal result counts.
- Subscribe before relying on progress events and read an initial backend snapshot after acceptance. Provide snapshot retrieval for recovery after a missed event or a view remount; frontend listeners are not completion authorities.
- Report preparing, rendering, cancelling, and explicit terminal outcomes. Successful completion, completion with item errors, and cancellation must be distinguishable. End the batch lifecycle even when every attempted item fails.
- Compute planned totals from the actual per-video output window and effective mode/FPS/update rate. Composite and transparent totals include only submitted eligible jobs, including those whose Activity overlay switch is off.
- Aggregate progress by planned output frames rather than video count. Render plans supply authoritative totals; configuration inspection supplies estimates. Keep planning/frame rounding in one owner.
- Account for item failures as terminal processed work without claiming their output was successfully rendered. Report success/error counts separately and ensure failed jobs do not leave the batch stuck below a terminal outcome. Do not represent a mixed-result batch as successful output for every item.
- ETA is an estimate and may change across different sources or codecs. A zero or absent throughput produces an explicitly unavailable estimate rather than a fabricated value.
- Preserve continue-after-item-error behavior. Invalid shared request structure or calibration prevents acceptance; source/telemetry/render failures after acceptance are item errors and later eligible jobs continue.
- A cancellation request applies to the whole accepted batch and the active operation, including preparation. Check it before render startup and before every subsequent item; per-item starts must not reset the batch cancellation request.
- Distinguish cancellation requested from cancellation complete. Await native worker and FFmpeg shutdown/cleanup before releasing the renderer and reporting the terminal cancelled result.
- Completed jobs retain their successful state and output paths. The active interrupted item and remaining unstarted items have explicit cancelled/unstarted outcomes in the final batch snapshot.
- Backend lifetime remains independent of dialog mounting. The existing modal interaction can remain during rendering; introducing editor interaction while a batch runs is not required to establish backend ownership.

## Testing Decisions

- The user approved the proposed test boundaries: primary execution coverage through the Rust batch service, plus focused frontend integration coverage for editor preservation, fresh inspection, shared sync adjustment, and queue controls.
- Good tests assert externally observable requests, outputs, timing, and lifecycle transitions. Avoid assertions tied to private helper names, component structure, thread count, or incidental FFmpeg argument order.
- Use the framework-independent Rust service as the primary seam for strict request validation, renderer reservation, sequential execution, source freshness, frame-weighted progress, output paths, failure continuation, cancellation, resource retention, and terminal snapshots. Use injected job execution/source inspection to exercise races deterministically rather than starting FFmpeg for every lifecycle test.
- Prefer existing command/controller tests and cancellation tests as prior art. Keep a small command-boundary test proving that the Tauri-facing submission and progress contract delegates to the same service behavior.
- Frontend workflow tests must assert that submission and inspection never replace editor media, preview identity, activity, scene dimensions, playhead, viewport, sync landmarks, or undo history. Cover successful, failed, and cancelled batches through the public workflow/store seam rather than mocking interactive import as the expected execution mechanism.
- Freshness tests cover reopening an unchanged remembered folder, closing during inspection, changing folders, late results, context changes during inspection, source changes before acceptance, a reference source outside the queue, and source changes before a later item's turn. Verify that old preparation cannot be submitted.
- Shared synchronization tests cover signed automatic offsets, both timezone interpretations, forced common interpretation, committed-versus-preview offsets, positive and negative manual corrections, a queued reference video, missing baseline, absent reference video, and overlap that changes after correction. Interactive import and batch inspection must agree at the shared public sync seam.
- Queue/dialog integration tests cover the Activity overlay switch retaining the item, metric/plot suppression with labels/backdrops/rasters preserved, row removal excluding the item, off items contributing to totals, blocked items being omitted, and terminal UI after all-item failure or partial failure.
- Timing and render-plan tests cover composite and transparent jobs with different offsets/durations, negative starts, trailing overrun, exact-boundary non-overlap, fractional FPS, update-rate frame decimation, and video-local transparent padding. Verify that transparent output uses the video's window rather than the full external activity.
- Output tests cover original stems with multiple dots, spaces, and Unicode; both mandatory suffixes; permitted overwrite of existing output; duplicate destinations; source/output aliasing; and preservation of completed outputs after later errors.
- Extend existing composite pipeline, transparent renderer, and render-config tests only where needed to prove the new timing behavior. Use limited real FFmpeg integration checks for alpha output, output duration/FPS, metadata, and overlay/source alignment that cannot be established by service tests alone.
- Manual desktop verification uses a same-session recording set with an external activity and known manual correction, including a video that starts before the activity. Check the stable background, original video remaining usable after completion/cancellation, both export modes, switch behavior, overwrite behavior, and cancellation during actual preparation and encoding.
- Do not run a production or frontend build without the user's explicit permission. Implementation validation uses appropriate targeted tests and lint checks, with build-based verification requiring separate authorization.

## Out of Scope

- Multiple imported videos in the editor, a multi-clip timeline, preview-server multi-registration, or changing preview playback architecture.
- Parallel video rendering, multi-input compositing, concatenation, or changes to the rendering algorithms unrelated to the required per-video timing and padding.
- Per-video manual synchronization controls, independent timezone choices, multiple camera-session calibrations, clock-drift estimation, or scaling the correction over time.
- Changing the Activity overlay switch into a render-inclusion switch or introducing an activity-free export mode.
- Multi-file overwrite management, confirmation dialogs, automatic renaming/numbering, backups, or a new output collision-resolution UI.
- Persistent inspection caches, whole-file hashing/copying, resumable batches after application restart, headless operation after the desktop process exits, or durable job history.
- New project persistence formats or compatibility adapters between old and new batch data shapes.
- A custom batch export-range UI, edits to an already accepted batch, or enabling concurrent editor interaction during rendering.
- Unrelated frontend cleanup, general synchronization feature redesign, or broad renderer optimization.

## Further Notes

- The reviewed baseline is the batch-rendering PR merge plus the two polishing commits ending at `b525b15e`. Implementation should re-check the current code before editing because this remains a development feature.
- The final user correction supersedes the earlier proposed selection-switch behavior: preserve Activity overlay semantics, and use row removal to exclude videos.
- Existing output overwrite is explicitly permitted. No additional overwrite approval workflow is requested.
- Shared correction assumes the queued videos come from one session and need the same additive manual adjustment. It is derived once under the same timezone setting used for every item, not guessed independently from each item's overlap.
- Example: a reference video's automatic offset is -12 seconds and its committed offset is -9 seconds. The correction is +3 seconds. A queued video's automatic offset of 40 seconds becomes 43 seconds; the reference video remains at -9 seconds if it is queued.
- Transparent boundary behavior is specified as a full video-local overlay with padding to preserve direct alignment. For activity coverage from 0 to 120 seconds, a 20-second video with offset -5 seconds has a 20-second overlay: its first 5 seconds are transparent, followed by sampling activity seconds 0 through 15. A 20-second video with offset 110 seconds renders activity coverage for its first 10 seconds and transparent padding for the remaining 10 seconds. These windows replace repeated full-activity transparent exports.
- Architecture and required behavior are specified here; module boundaries may be refined during implementation while preserving one canonical contract and the approved test seams. No application code is changed as part of creating this specification.
