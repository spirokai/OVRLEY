---
Status: ready-for-agent
---

# Rust-Owned Batch Rendering Implementation Plan

## Scope and sequencing

Implement [the batch backend refactor specification](./batch-backend-refactor-spec.md) in six phases. Each phase delivers a coherent boundary with targeted validation; later phases build on those boundaries. These are implementation checkpoints on the development branch, not separately released feature variants.

The specification is authoritative for product behavior. In particular, the final decision for the Activity overlay switch is to preserve its existing behavior: off still renders the video, suppressing metric widgets and plots while retaining labels, backdrops, and rasters. Row removal excludes a video.

Read the current code before editing and preserve unrelated working changes. Use surgical patches, JavaScript with JSDoc for exported frontend utilities, presentational components, and strict ingress contracts. Do not add compatibility branches, duplicate synchronization implementations, or a persistent preparation cache. No application implementation is authorized by creating this plan.

| Phase | Deliverable                                                    | Depends on             |
| ----- | -------------------------------------------------------------- | ---------------------- |
| 1     | Canonical contracts and signed synchronization calibration     | Existing specification |
| 2     | Shared Rust execution ownership and cancellation               | Phase 1                |
| 3     | Lightweight source inspection with explicit freshness          | Phases 1–2             |
| 4     | Per-video render plans, transparent padding, and output naming | Phases 1–3             |
| 5     | Backend-owned sequential queue, progress, and results          | Phases 2–4             |
| 6     | Frontend adoption and complete workflow verification           | Phases 1–5             |

New module names below are proposed implementation locations. Adjust their organization when current code supports a smaller coherent boundary, while retaining the specified responsibility and tests.

## Testing policy: crucial and essential coverage only

The user's latest instruction is to add only crucial and essential tests. This policy governs test additions throughout the plan and takes precedence over the specification's broader testing suggestions. Product requirements remain unchanged.

- Reuse and update existing tests first. Add a test only when existing coverage does not protect a significant changed behavior or a realistic regression introduced by this refactor.
- Concentrate new coverage at the approved Rust batch service seam and a small set of frontend workflow integration tests. Do not add a separate suite for every new module, helper, component, or IPC wrapper.
- Prioritize synchronization and video-local timing, stale-source/session rejection, editor preservation, sequential execution, failure continuation, and cancellation/cleanup. These are the essential correctness and ownership risks.
- Validation lists below describe coverage goals, not a requirement to create one test per bullet, requirement, phase, or input variation. One coherent scenario may prove several related contracts. Reuse that evidence in later phases rather than duplicating assertions at every layer.
- Use representative cases for distinct failure mechanisms. Avoid exhaustive combinations of timestamps, FPS values, codecs, filenames, statuses, or cancellation timings unless a specific unresolved risk justifies them. Parameterized tests must serve a real behavioral distinction, not inflate case counts.
- Do not add implementation-mirroring tests, trivial getter/setter or formatting tests, snapshot proliferation, or tests for reversible presentation-only changes. Test observable outcomes rather than internal call sequences, private helpers, or mocked behavior that simply repeats the implementation.
- Add real FFmpeg integration coverage only where existing tests or service tests cannot establish essential output timing or transparency. Keep UI appearance and routine presentation checks in the desktop verification pass.
- Before adding a test, identify the concrete regression it catches and why existing coverage is insufficient. When reporting a phase, briefly justify any new tests. There is no test-count or coverage-percentage target.

## Phase 1: Define contracts and correct synchronization calibration

**Goal:** Establish the request and lifecycle vocabulary, and make interactive synchronization and batch calibration use the same signed automatic-offset calculation.

### Files

- `app/src/store/slices/createVideoImportSlice.js`
- New `app/src/lib/video-sync.js`
- New `app/src/features/render-video/utils/batchRenderRequest.js`
- New `src-tauri/ovrley_core/src/render_jobs/mod.rs` and `contracts.rs`
- `src-tauri/ovrley_core/src/lib.rs`
- Existing timezone-mode and request tests; only missing essential regression cases added

### Work

1. Define one batch request contract: inspection identity, materialized shared template, encoder settings, optional shared external activity, captured calibration, output directory, and ordered jobs. Each job carries source identity, inspected metadata, resolved timing where applicable, the existing `skipOverlay` choice, and its planned destination. Backend acceptance will verify those destinations against the naming convention.
2. Define typed accepted-request, item outcome, batch phase, and batch snapshot contracts. Keep inspection state separate from accepted execution state. Define explicit success, mixed-error completion, all-item failure, and cancellation outcomes rather than deriving completion from whether any row is done.
3. Distinguish renderer busy state, current-item progress, aggregate processed work, actual rendered/encoded frame counts, and final output results. Include a monotonic snapshot revision or equivalent ordering guarantee so an older snapshot cannot overwrite a newer event.
4. Move pure timestamp interpretation and synchronization calculations out of the store slice. Preserve the existing external timestamp-source semantics and timezone formatting. Store actions call the shared utility and commit its result; consumers do not recalculate using their own timestamp rules.
5. Calculate raw automatic offsets as signed differences from the activity sync origin. Remove lower and upper clamping. Keep timestamp resolution separate from positive-overlap eligibility so a manual correction can rescue an initially non-overlapping clip.
6. Add calibration construction from the current reference video's effective creation timestamp/source, committed offset, activity, and captured Apply Timezone setting. Use committed offset minus automatic offset; do not use transient preview offset. Explicitly support absent reference video with zero correction and reject an existing reference with an unresolvable baseline.
7. Resolve queued offsets by adding the correction once to each video's automatic offset. For ambiguous timestamps, force the captured timezone interpretation rather than choosing whichever interpretation happens to overlap. Trusted GPS timestamps retain absolute-time handling. Without an external activity, select per-video embedded telemetry mode with local offset zero.
8. Build immutable frontend request inputs from a single synchronous editor/settings snapshot. Materialize shared globals once. Keep request construction independent of interactive import and of reads from the live store during later execution.
9. Preserve the switch's canonical `skipOverlay` field. Define required fields strictly and document optional absence; make the public serialized batch shape directly consumable by the frontend without an additional compatibility mapping.

### Validation

- Extend existing sync coverage with the essential signed-offset/shared-correction regression: automatic -12, committed -9, correction +3; queued automatic 40 becomes 43; the queued reference remains -9. Include captured timezone treatment and committed-versus-preview offset in the same coherent setup where practical.
- Add a baseline-error or corrected-overlap case only if existing tests do not protect those distinct failure mechanisms. Reuse existing timestamp-source and timezone coverage rather than generating a new timestamp matrix.
- Verify request capture and queue membership through the frontend workflow integration coverage in Phase 6. Do not create a second suite of request-object assertions if that higher seam already proves the contract.

### Completion gate

Interactive synchronization and batch calibration share one implementation, preserve signed offsets, and agree on timezone treatment. Public request/outcome types are defined without changing the current batch execution path yet.

## Phase 2: Separate Rust execution from dispatch and own the renderer lifecycle

**Goal:** Establish a service that can execute one render to completion and reserve the renderer for an operation's full lifetime. This is the foundation for the batch runner.

### Files

- `src-tauri/ovrley_core/src/commands/mod.rs`
- New `src-tauri/ovrley_core/src/render_jobs/execution.rs`
- `src-tauri/ovrley_core/src/encode/progress.rs`
- `src-tauri/ovrley_core/src/debug/mod.rs`
- `src-tauri/ovrley_core/src/encode/pipeline/lifecycle.rs`, if lifecycle integration requires it
- `src-tauri/src/lib.rs` and `tauri_commands.rs`
- Existing command and cancellation tests; direct controller callers updated where required

### Work

1. Extract preparation and synchronous operation execution from the two command paths that currently spawn fire-and-forget render threads. Preserve their existing composite and transparent pipeline calls and typed output results.
2. Introduce one renderer reservation owned by the execution service. Acquire it before long-running preparation; keep it through operation cleanup. Both current-video and eventual batch submission must acquire this reservation.
3. Separate session lifecycle from per-item progress reset. Beginning an item may reset its counters, but cannot release the reservation or clear a previously requested session cancellation.
4. Correct cancellation semantics: requesting cancellation publishes a cancelling state, while terminal cancellation is published only after worker and subprocess cleanup. Make finalization happen exactly once.
5. Retain the existing pipeline-owned FFmpeg/writer/frame-worker shutdown and partial-output guards. Define who joins the operation worker and who releases accepted resources. Do not supervise native work through frontend completion events.
6. Move blocking configuration, dense-activity, and render preparation onto the owned worker. Check cancellation before preparation, at cancellable boundaries, and immediately before starting the encoder. If a parser cannot be interrupted mid-call, retain ownership until it returns and prevent the next operation from starting.
7. Route the existing single-render command through this service. Preserve single-render custom-range, output validation, overwrite confirmation, and result-opening behavior. A single request and a batch request may have distinct operation inputs while sharing preparation/execution and reservation.
8. Resolve and retain referenced raster assets at acceptance through the existing resource resolver, using immutable owned references. Required resource failures must occur before dispatch; workers must not re-resolve later editor handles.
9. Update controller call sites in tests and standalone render/benchmark binaries only when signatures change. Avoid retaining an old reservation/cancellation mechanism alongside the new service.

### Validation

- Run existing command and pipeline cancellation tests to preserve single-render behavior and cleanup coverage.
- Add one controlled lifecycle regression proving that cancellation during preparation prevents encoder startup and retains the reservation until cleanup. Reuse its assertions for competing submissions and terminal publication rather than testing those at several layers.
- Verify resource retention within existing accepted-request/service coverage where possible. Add a separate case only if ownership changes introduce an otherwise uncovered resource-lifetime risk.

### Completion gate

One native service owns execution and cancellation; a render operation returns its real outcome after cleanup. Existing single rendering uses that ownership and passes its targeted regressions.

## Phase 3: Add lightweight inspection sessions and source freshness checks

**Goal:** Prepare trustworthy source descriptors without interactive import, full activity extraction for external-activity inspection, or results surviving a closed configuration session.

### Files

- New `src-tauri/ovrley_core/src/media/prepared_video.rs`
- `src-tauri/ovrley_core/src/media/mod.rs` and `source_video_metadata.rs`, if descriptor typing requires changes
- Existing metadata probe ownership in `src-tauri/ovrley_core/src/commands/mod.rs`
- `src-tauri/src/tauri_commands.rs` and `lib.rs`
- `app/src/api/backend.js`
- Public inspection/service coverage for essential freshness regressions; existing probe tests reused

### Work

1. Expose a shared, framework-independent metadata preparation boundary around existing telemetry-first probing and ffprobe salvage. Retain one canonical source metadata shape; keep preview registration a separate interactive-import concern.
2. Define inspected source descriptors containing canonical source identity, required normalized render metadata, timestamp provenance, and a file stamp including size and modification time. Normalize rotation/display dimensions and required timing at source ingress.
3. Record file identity before and after metadata inspection. If detectable changes occur during probing, report the source as changed instead of publishing a descriptor assembled from inconsistent file versions.
4. Introduce backend inspection sessions with opaque identities and descriptors owned by that session. Support creation, bounded per-source inspection, and disposal. The frontend owns which configuration opening is current; the backend owns descriptor validity and lifetime.
5. Inspect the current calibration source even if it is outside the queued folder. Respect the editor's effective timestamp override when calculating calibration, while retaining a stamp for the actual source file.
6. Return only the source information needed for synchronization, overlap, dimensions, and frame estimation. Do not call full parsed-activity extraction for every item when an external activity is authoritative. Metadata-level embedded timestamp reading remains valid.
7. For embedded-activity mode, mark full activity resolution as job preparation. Use source duration for provisional work estimates; do not fabricate eligibility or parsed telemetry from the editor's current video.
8. Add a submission validation operation that checks session ownership, descriptor identities, and current file stamps. A closed/stale session or changed file prevents acceptance and returns affected source identities for reinspection.
9. Define descriptor transfer into accepted jobs: the job retains immutable owned descriptors independently of later session disposal. Check each source again before its turn in execution, treating later changes as item errors.
10. Add typed IPC helpers for inspection and disposal. Ignore late results by session generation in frontend integration later; releasing an inspection must not cancel accepted execution.

### Validation

- Add representative public-service freshness regressions for a disposed inspection and a source changed before submission. Cover a later source change in the batch lifecycle scenario in Phase 5; do not duplicate it here.
- Verify accepted descriptor ownership and deferred telemetry extraction within those service scenarios where practical. Preview preservation and late frontend results are covered at the workflow seam in Phase 6.
- Run existing metadata probe regressions without creating new tests for unchanged probing, rotation, or FPS normalization.

### Completion gate

Fresh session descriptors can be inspected and validated through IPC, stale descriptors cannot be accepted, and accepted execution resources survive inspection disposal. Frontend workflow adoption is deferred to Phase 6.

## Phase 4: Implement per-video timing, transparent padding, and output planning

**Goal:** Give both export modes one authoritative video-local timing model and produce the specified destinations without editor mutations.

### Files

- New `src-tauri/ovrley_core/src/render_jobs/plan.rs`
- `src-tauri/ovrley_core/src/encode/pipeline/composite_plan.rs`
- `src-tauri/ovrley_core/src/encode/pipeline/transparent.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`
- `src-tauri/ovrley_core/src/encode/pipeline/frames.rs`, only if the shared frame-index contract requires changes
- `src-tauri/ovrley_core/src/activity/mod.rs` and relevant timeline helpers, only where needed for clip-scoped sampling
- `src-tauri/ovrley_core/src/output.rs`
- `src-tauri/src/tauri_commands.rs` and `app/src/api/backend.js`, for configuration-time planning results
- `app/src/features/render-video/utils/renderConfig.js`, to establish shared-template materialization
- Existing timing, pipeline, and output tests; only missing critical regressions added

### Work

1. Build each job's output window from source duration and signed corrected offset. Output time zero is source time zero; activity sampling at output time t uses corrected offset plus t.
2. Separate full output duration from covered activity duration. Determine positive overlap with canonical activity bounds, leading transparent coverage, and trailing transparent coverage. Preserve the full parsed activity and its original time origin for sampling.
3. Reuse exact rational FPS/frame-count utilities. Composite output follows source FPS; transparent output follows the selected layout FPS and widget update rate. Derive total encoded frames and container FPS from one backend owner, including any established sub-frame tolerance.
4. Adapt composite planning to use the shared timing rules while retaining its supported footage duration, source trim semantics, and empty overlay behavior outside activity coverage. Preserve single-render custom-range behavior explicitly.
5. Extend transparent planning and frame production to emit the full source-local overlay window, not merely a dense report of the entire activity or its cropped intersection. Render activity frames only for coverage; emit RGBA-zero frames for out-of-coverage times.
6. Keep blank-frame mapping shared with the existing video frame renderer where possible. Ensure frame prewarming and reused buffers respect leading/trailing blank output and do not leak a static label, raster, or the first activity frame into padding. Avoid constructing synthetic activity samples to represent padding.
7. Apply per-item display dimensions and rendering cadence locally. Clone/derive a job-effective configuration from the captured template; never write the dimensions, timing, or FPS back into editor config or preferences.
8. Apply `skipOverlay` by suppressing metric widgets and plots before preparing that job's assets. Retain labels, backdrops, and raster elements during covered time. Do not skip execution, validation, or required activity because this switch is off.
9. Derive filenames from the original stem by stripping only its final extension: `_video.mp4` for composite, `_overlay.mov` for transparent. Return destinations through configuration-time native planning and reuse those values in frontend review/submission. Backend acceptance verifies them against the captured output context; do not implement a second naming algorithm in the frontend.
10. Validate destination writability, codec/container compatibility, duplicate target paths under filesystem comparison rules, and aliases to any source/calibration file. Preserve permitted existing-output overwrite with no batch confirmation or automatic renaming.
11. Produce immutable planned-job data and exact frame totals using validated inputs. For embedded activity, finalize the activity-dependent coverage when the owning job extracts its telemetry, retaining its video-local output duration.

### Validation

- Extend the existing timing/pipeline seam with the essential transparent-window regression: activity 0–120, 20-second sources at offsets -5 and 110. Assert correct activity sampling, full video-local duration, and alpha-zero padding without duplicating the same timing assertions across helpers and service tests.
- Reuse existing FPS, update-rate, overlap, and single-render range tests. Add a distinct fractional-frame boundary case only if the changed planner is not already protected.
- Extend existing output coverage for required suffixes and dangerous target conflicts; reuse existing overwrite validation. Check routine stem variants during desktop verification unless a specific naming defect warrants a regression test.
- Use an existing FFmpeg fixture where possible. Add a small output integration case only if essential alpha/timing behavior cannot be established by the available pipeline tests. Verify switch element retention in existing renderer coverage or the desktop pass rather than building another component test matrix.

### Completion gate

Native plans express each video's full local window and exact mode-specific work. Transparent exports and composite exports agree on sync; required naming and overwrite behavior are proven independently of queue execution.

## Phase 5: Execute the native batch and publish authoritative progress

**Goal:** Submit a queue once and have the Rust service own every transition through preparation, rendering, error continuation, cancellation, and final results.

### Files

- New `src-tauri/ovrley_core/src/render_jobs/batch.rs`
- `src-tauri/ovrley_core/src/render_jobs/contracts.rs`, `execution.rs`, and `plan.rs`
- `src-tauri/ovrley_core/src/encode/progress.rs` and `debug/mod.rs`
- `src-tauri/src/tauri_commands.rs`, `lib.rs`, and `progress_sink.rs`
- `app/src/api/backend.js`
- New `src-tauri/ovrley_core/tests/batch_render_tests.rs`; relevant command/cancellation tests

### Work

1. Implement acceptance as one service operation: validate shared configuration and activity, verify the live inspection and sources, validate destinations, retain required resources, and acquire the renderer reservation before publishing the accepted batch identity. Any pre-acceptance failure releases ownership without writing output.
2. Freeze the ordered jobs, source descriptors, sync correction/timezone context, template, output plan, and encoder settings. The backend receives shared external activity once and retains it immutably for all items.
3. Dispatch one supervised batch worker. For each item, check cancellation and source freshness, resolve its activity, finalize its render plan, prepare assets, execute the operation directly, await cleanup, and record its outcome before advancing.
4. Extract embedded activity only for its owning item when no external activity is loaded. Reuse it for that item's planning and rendering, then release it. Missing or invalid required activity becomes an item error; do not borrow another video's telemetry.
5. Continue after source, telemetry, preparation, or encoding item errors. Keep completed outputs. Shared contract/calibration failures reject acceptance; cancellation terminates advancement rather than being treated as an ordinary item error.
6. Expose current-item and frame-weighted aggregate progress from the service. Use planned frame weights for processed work; retain actual rendered/encoded counts separately so a failed item is never presented as having encoded its planned output.
7. Handle corrected totals as native plans settle, including embedded-activity preparation. Publish total planning/work consistently; terminal outcome and result counts remain authoritative even if some frames were never produced.
8. Publish explicit preparing, rendering, cancelling, and terminal snapshots. Record batch identity, revision, active item, per-item errors, successful output paths, cancelled/unstarted items, and final counts. Keep the terminal snapshot available for an initial read or reconnect.
9. Route cancellation to the session token and active native work. An item start must never clear the token. Mark cancellation complete and release the renderer only after active workers and FFmpeg have stopped and partial-output cleanup has finished.
10. Emit events through the Tauri sink without introducing queue advancement logic there. Add submit, snapshot, and cancel IPC functions returning structured errors and the canonical serialized types. Frontend state is a view of these snapshots.

### Validation

- Use a compact set of public-service scenarios with controlled execution: sequential mixed success/failure with later continuation, changed-source or missing-telemetry item failure, and cancellation preventing later launches while cleanup completes. Cover aggregate progress, output retention, off-switch inclusion, and terminal results in those same scenarios.
- Add an all-item-failure case only where it exercises terminal handling not established by the mixed-result scenario. Reuse Phase 2's preparation cancellation coverage and existing pipeline cancellation tests; do not create a case for every cancellation instant.
- Verify snapshot recovery/order at the frontend integration seam in Phase 6. Add a separate native event or serialization test only if an actual contract risk is not covered by the public service and workflow tests.

### Completion gate

A native API can run the complete specified batch without any frontend import or event-driven sequencing. The approved Rust batch service seam proves execution, freshness, progress, failures, and cancellation.

## Phase 6: Replace frontend orchestration and verify the complete workflow

**Goal:** Connect the existing dialog to fresh inspection and the native job, preserve the editor, and remove the superseded frontend execution path.

### Files

- `app/src/features/render-video/hooks/useBatchRenderWorkflow.js`
- `app/src/store/slices/createBatchRenderSlice.js`
- `app/src/features/render-video/hooks/useRenderDialogState.js`, `useRenderVideoDialogState.js`, and `useRenderVideoDerivedState.js`
- `app/src/features/render-video/hooks/useRenderWorkflow.js`
- `app/src/features/render-video/utils/batchRenderRequest.js` and `renderProgress.js`
- `app/src/features/render-video/components/BatchRenderQueue.jsx`, `RenderVideoDialog.jsx`, and `RenderProgressPanel.jsx`
- `app/src/hooks/useAppStoreSelectors.js`, if centralized job selectors are needed
- `app/src/features/projects/hooks/useProjectDocumentState.js`, if its busy-operation guard needs the shared reservation state
- Existing render workflow/dialog tests; project lifecycle tests if the guard changes; locale files for changed preparation/outcome messages

### Work

1. Replace the frontend import/render loop with a hook that manages configuration inspection, constructs one request, submits it, and observes the accepted native snapshot. Remove `useVideoImport` usage, `loadVideoPath`, `clearImportedVideo`, per-item submission, and `waitForRenderCompletion` from batch execution.
2. On every batch configuration opening, enumerate and inspect the remembered folder in a new session. On close, target switch, folder replacement, or unmount, dispose the configuration session and discard late results by generation. Rescanning the same folder must still be fresh.
3. Track user queue choices separately from derived eligibility. Preserve the existing overlay toggle and row removal. Recompute offsets/overlap/frame estimates when relevant activity, calibration, or export inputs change; do not let old inspection results authorize Start.
4. Replace defensive slice repairs with explicit validated queue/session contracts. Use stable source/item identities issued or validated at the owner boundary. Do not generate a new identity because required state is malformed or normalize broken required input into an empty queue.
5. Capture one editor/settings snapshot at submission and build the immutable request from it. Handle a stale-source/session rejection by showing the affected rows and requiring fresh inspection before another submission; do not silently resubmit changed timing.
6. Establish event listening before submission and read the accepted backend snapshot afterward. Correlate updates by batch identity and revision. Ensure a late initial snapshot cannot undo more recent progress. Keep execution snapshots observable outside the dialog component's lifetime without moving execution itself into frontend state.
7. Populate active-item and aggregate progress from backend state. Remove frontend aggregation as an execution authority and remove heuristics that require a done row before recognizing completion. Show preparing, cancelling, completed, partially failed/all-failed, and cancelled outcomes with actionable item errors.
8. Preserve Activity overlay labels and exact suppression semantics. Do not treat off rows as excluded or subtract them from frame totals. Keep row removal as exclusion and require no batch overwrite prompts.
9. Remember accepted shared render preferences deliberately. Per-item FPS, dimensions, sync, telemetry, and activity windows must never overwrite editor state or those preferences. Preserve existing project version/folder persistence and keep inspection/job resources out of durable project state.
10. Apply shared busy-state guards to current rendering and conflicting project operations during native preparation and between batch items. Preserve the existing modal interaction; the service remains independent of component mounting without adding concurrent editor interaction as a feature.
11. Update changed UI text across the existing locale set. Components receive derived status and actions from hooks, and pure formatting remains in utilities.
12. Remove obsolete batch-only helpers and mocks once the new path is integrated. Keep shared interactive/project video preparation utilities wherever they still have real consumers. Do not leave an alternate batch path behind a flag or fallback.

### Validation

- Update existing batch workflow tests instead of adding parallel suites. Use one coherent submission/result scenario to assert native submission, editor-state preservation, captured correction/timezone, off-switch inclusion, and row-removal exclusion. Treat intentionally remembered render preferences separately from preserved editor state.
- Add one close/reopen or late-inspection regression covering stale preparation and Start readiness, plus a result/snapshot-order regression only if existing workflow coverage does not protect it.
- Reuse the public Rust service evidence for execution, failure continuation, cancellation, and output planning. Frontend tests should cover the frontend's own responsibility, not re-test native behavior through mocks or repeat every terminal status across component, hook, and store suites.
- Run focused Vitest suites for synchronization, request construction, batch workflow, dialog state/components, and existing single-render behavior. Run relevant Rust service/planning/commands/cancellation suites and `pnpm lint`.
- After targeted checks pass, run the broader frontend and core test suites once for integration coverage. Repeat or broaden checks only for new failures, changes, or unresolved concerns. Tests may compile Rust; do not run `pnpm build`, `pnpm tauri build`, or a Vite production build without explicit user permission.
- Complete the desktop scenarios below using the existing development workflow when available. Record any verification that remains unavailable rather than claiming it passed.

The desktop scenarios are manual acceptance checks, not instructions to generate an automated test for every table row.

### Completion gate

The dialog uses one native batch lifecycle; the editor remains stable and usable afterward. Freshness, synchronization, overlay-switch behavior, output naming/overwrite, transparent timing, progress, failure continuation, and cancellation all satisfy the specification. The old batch import loop is removed.

## Final desktop acceptance scenarios

| Scenario                                               | Expected result                                                                                          |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| External activity with several same-session recordings | One submission renders items sequentially; background preview and editor state stay stable               |
| Reference automatic offset -12, committed offset -9    | Every queued automatic offset receives extra +3 compensation; a queued reference remains -9              |
| Apply Timezone toggled before submission               | All ambiguous queued timestamps use the same captured interpretation                                     |
| Source begins before activity or ends after it         | Both modes preserve video-local alignment; transparent output pads uncovered time with alpha-zero frames |
| Two different source windows in transparent mode       | Each overlay contains its source-local passage, not a repeated full-activity export                      |
| Activity overlay switch off                            | Item still renders; metrics/plots disappear while static template content remains during covered time    |
| Row removed                                            | That source is absent from jobs and frame totals                                                         |
| Existing `ride_video.mp4` or `ride_overlay.mov`        | Target is overwritten without per-file confirmation                                                      |
| Configuration closed, source changed, then reopened    | Fresh inspection reflects the changed source                                                             |
| Source changed after inspection but before submission  | Submission rejects the stale inspection and identifies the source                                        |
| Later queued source changed during an earlier render   | Changed item errors; subsequent eligible items continue                                                  |
| Cancellation during preparation or encoding            | No later render starts; renderer becomes available after native cleanup                                  |
| One item fails, or every item fails                    | Terminal results show the errors; completed outputs survive; dialog does not remain stuck rendering      |
| Batch complete, failed, or cancelled                   | Original imported media, activity, timeline context, and sync work remain available                      |

## Requirements traceability

| Requirement                                               | Implementation phases | Principal verification                                                        |
| --------------------------------------------------------- | --------------------- | ----------------------------------------------------------------------------- |
| Rust owns execution; editor does not import batch sources | 2, 5, 6               | Service sequencing and frontend editor-state preservation                     |
| Fresh minimal inspection, no stale reopen resources       | 3, 6                  | Session disposal/reopen, file stamps, context invalidation                    |
| A: permitted existing-output overwrite                    | 4, 6                  | Output target tests and one desktop overwrite scenario                        |
| B: signed automatic offsets                               | 1, 4                  | Shared sync regression and negative-window render checks                      |
| C: shared manual correction and Apply Timezone            | 1, 3, 6               | Calibration example, forced common interpretation, reference-source freshness |
| D: preserve Activity overlay switch                       | 1, 4, 6               | Off item rendered and counted; correct template elements retained             |
| E: transparent output follows each video's window         | 4, 5, 6               | Timing/alpha integration and multiple-window desktop scenario                 |
| F: `_video.mp4` and `_overlay.mov` output names           | 4, 6                  | Stem/container destination tests                                              |
| Reliable failure, progress, and cancellation              | 2, 5, 6               | Native lifecycle tests and terminal frontend outcomes                         |

## Documentation and review boundaries

- Keep the spec and this plan as the implementation references. The plan may be updated when module organization changes; product decisions remain governed by the spec.
- Report each completed phase through its delivered behavior and completion gate, including any remaining limitation. Do not measure completion by file count or assume that passing isolated checks finishes later phases.
- Only documentation is changed when creating this plan. Application code, tests, build configuration, and dependencies remain untouched until implementation is requested.
