# Manual Video Sync — Detailed Implementation Plan

## Implementation Principles

- Implement one canonical manual-sync model. Do not introduce alternate landmark shapes, offset aliases, or UI-specific copies of durable data.
- Validate versioned project data once in the Rust project-file boundary. Frontend consumers receive canonical version-2 data.
- Treat parsed activity as external telemetry: missing channels and gaps are supported states, but malformed project/config data fails loudly.
- Keep components presentational. Put store orchestration and reusable interaction state in hooks; put detection, matching, scoring, graph geometry, and formatting in pure utilities.
- Keep the existing `videoSyncOffsetSeconds` as the only applied offset.
- Persist only landmarks and physical sensitivity thresholds. Everything else is derived.
- Do not add Rust matching IPC, a Web Worker, or a charting dependency unless profiling demonstrates a need.
- Do not run a production build while implementing this plan unless the user explicitly authorizes it.

## Intended Feature Structure

Create the feature with this approximate structure. Exact file splitting may be adjusted when a file would otherwise contain only trivial forwarding code. Do not fragment the code into tiny or large files that mix unrelated concerns. We aim at MAX 200-250 LOC per file.

```text
app/src/features/video-sync/
  index.js
  components/
    VideoSyncDrawerContent.jsx
    VideoSyncLandmarkList.jsx
    VideoSyncCandidateList.jsx
    VideoSyncMarkControls.jsx
    VideoSyncCanvasDiagnostics.jsx
    VideoSyncTimelineGraph.jsx
    VideoSyncTimelineLandmarks.jsx
  data/
    videoSyncConstants.js
  hooks/
    useVideoSyncWorkspace.js
    useVideoSyncCalculation.js
    useVideoSyncTimeline.js
    useVideoSyncLandmarkDrag.js
    useVideoSyncDiagnostics.js
  utils/
    activitySyncInput.js
    detectStops.js
    detectTurns.js
    intervalConsensus.js
    matchScore.js
    graphGeometry.js
    landmarkTiming.js
```

Add a dedicated Zustand slice:

```text
app/src/store/slices/createManualVideoSyncSlice.js
```

Place tests under the existing test tree, grouped by utility, state, project persistence, timeline interaction, and component behavior.

---

## Phase 1 — Canonical Contracts, Store State, and Project Version 2

### Objective

Establish the durable and runtime contracts before algorithms or UI consume them. At the end of this phase, landmarks and sensitivity settings can be manipulated through strict store actions and round-trip through a version-2 project. Existing version-1 projects load through one explicit migration.

### Dependencies

None. This is the foundation for every later phase.

### 1.1 Define constants and domain values

Create `videoSyncConstants.js` containing named, exported values for:

- landmark types: `stop`, `turn`, and `location`;
- maximum five landmarks;
- maximum one location landmark;
- default/range near-stop threshold: 5 km/h, 1–10 km/h;
- default/range turn threshold: 90 degrees, 90–360 degrees;
- stop entry dwell: 2 seconds;
- stop exit dwell: 2 seconds;
- stop exit hysteresis: 2 km/h;
- turn maximum duration: 10 seconds;
- heading smoothing window: 1 second;
- user timing tolerance: 2 seconds;
- significant-gap base duration and cadence multiplier;
- candidate merge tolerance, initially 0.5 seconds;
- maximum five candidates.

Keep behavioral defaults centralized. Components must not repeat numeric literals.

### 1.2 Add the manual-sync store slice

Add the slice to `app/src/store/useStore.js` through the existing store composition.

Durable state:

```text
manualVideoSync: {
  landmarks,
  speedThresholdKmh,
  turnThresholdDegrees
}
```

Runtime-derived state should be separate from the durable object:

```text
manualVideoSyncDetection
manualVideoSyncCandidates
manualVideoSyncCandidateStatus
manualVideoSyncError
manualVideoSyncInputRevision
manualVideoSyncHasSearched
```

Use an explicit candidate status union such as `idle | calculating | fresh | stale | error`. Do not encode status through combinations of nulls and booleans.

Store actions should express domain operations rather than expose unrestricted object mutation:

- add a typed landmark at a validated video-local second;
- move a landmark to a validated video-local second;
- remove one landmark;
- clear all landmarks;
- commit each physical sensitivity threshold;
- begin, complete, fail, and invalidate a calculation using an input revision;
- clear video-owned manual-sync state;
- clear activity-derived manual-sync state;
- hydrate validated durable state.

Generate stable landmark IDs at creation through the runtime's UUID facility. Failure to generate a required ID must fail rather than falling back to an index or timestamp.

Enforce limits and finite ranges in the owning actions. UI disablement is not the contract boundary.

### 1.3 Define the landmark discriminated contract

Use strict variants:

```text
stop/turn: { id, type, videoSecond }
location:  { id, type: "location", videoSecond, activitySecond }
```

For a location, `activitySecond` is a required key whose value is either a finite activity second or `null`. Current UI creation always sets it to `null`.

Do not allow stop/turn landmarks to carry `activitySecond`, and do not accept missing `activitySecond` for a version-2 location landmark.

### 1.4 Extend the project archive schema

Update both project-version constants:

- `app/src/features/projects/utils/projectSnapshot.js`;
- `src-tauri/src/project_file.rs`.

Add required version-2 manual-sync data under `sync`, for example:

```json
{
  "sync": {
    "videoOffsetSeconds": 0,
    "videoTimezoneMode": null,
    "manual": {
      "landmarks": [],
      "speedThresholdKmh": 5,
      "turnThresholdDegrees": 90
    }
  }
}
```

Use the final field names consistently in Rust, snapshots, hydration, store state projection, tests, and documentation.

At Rust ingress:

1. Read the envelope sufficiently to identify `format` and `version`.
2. Strictly deserialize version 1 into its original v1 DTO.
3. Strictly deserialize version 2 into its v2 DTO.
4. Convert either DTO exactly once into the canonical v2 `ProjectDocument` returned to the frontend.
5. For v1 only, create empty landmarks and the documented default thresholds.
6. Reject unknown versions, unknown fields, missing v2 fields, duplicate landmark IDs, invalid types, non-finite numbers, out-of-range thresholds, excess landmarks, multiple locations, and invalid location variants.
7. Continue writing only canonical v2 documents.

Migration occurs in memory. Reading a v1 archive does not rewrite it; its next normal save writes v2.

Video duration is unavailable during archive JSON validation, so validate `videoSecond >= 0` in Rust, then validate the upper bound once staged video metadata is available during project preparation/hydration. A saved landmark outside the referenced video's duration is invalid project/source pairing and must abort project activation with an explicit error.

### 1.5 Integrate snapshot, hydration, and dirty tracking

Update:

- `createProjectContentSnapshot` to include only durable manual-sync state;
- `createProjectDirtyState` to include it in the saved baseline;
- `applyNewProjectState` to install documented defaults;
- `applyProjectOwnedState` to hydrate validated v2 manual state;
- `applyPreparedProjectState` ordering so media stages first and saved landmarks hydrate afterward;
- `useProjectDocumentState` selectors so landmark/sensitivity changes update project status.

Do not persist candidates, detected events, calculation status, errors, or graph geometry.

### Phase 1 tests

- Store action tests for valid creation/move/delete/clear and all limits.
- Store tests proving malformed action arguments fail instead of being coerced.
- Frontend snapshot and hydration tests for canonical v2 state.
- Dirty-state tests for landmarks and sensitivity changes.
- Rust project-file tests for strict v2 read/write.
- Rust migration tests for a valid v1 archive.
- Rust rejection tests for malformed variants, missing required keys, duplicate IDs, limits, and invalid thresholds.
- Project hydration test for landmark time beyond loaded video duration.

### Phase 1 exit criteria

- A v2 project round-trips durable manual-sync state.
- A v1 project loads as canonical v2 in memory and is not rewritten on read.
- Invalid v2 manual-sync data cannot reach frontend consumers.
- No derived calculation state appears in a project snapshot.

---

## Phase 2 — Timestamp-Based Activity Event Detection

### Objective

Produce deterministic stop events, turn events, the turning graph series, and explicit metric availability from canonical activity telemetry. This phase has no UI dependency.

### Dependencies

Phase 1 constants and domain types only.

### 2.1 Build one activity-to-detector ingress

Create `activitySyncInput.js` to project `parsedActivity` once into a detector input containing:

- canonical elapsed seconds;
- speed in the canonical internal unit;
- heading in degrees;
- explicit speed/heading availability;
- contiguous valid segments separated at significant telemetry gaps.

The existing activity parser remains responsible for source-format normalization. This feature ingress chooses usable channels and segments external gaps; downstream detectors must not repeat type coercion or availability checks.

Define a significant gap as greater than:

```text
max(3 seconds, 3 × median valid sample interval)
```

Do not bridge a significant gap or infer events inside one.

### 2.2 Implement near-stop detection

Implement a time-domain state machine:

1. Establish movement only after speed remains above `threshold + hysteresis` for the configured dwell.
2. Detect a possible stop at the first downward threshold crossing.
3. Confirm it only if the low-speed state lasts for the full entry dwell.
4. Timestamp the stop at the interpolated initial crossing, not confirmation time.
5. Remain stopped until speed stays above the exit threshold for the exit dwell.
6. Do not emit a stop when a segment begins below threshold without prior established movement.
7. Reset state at significant gaps.

Return typed stop events containing at least the point time and supporting low-speed interval. Keep calculations in canonical internal units; convert the user threshold from km/h once at detector ingress.

### 2.3 Implement turn detection and the shared turning series

Implement elapsed-time circular smoothing over the configured one-second window, then unwrap heading within each valid segment.

Derive a signed heading-change-rate series for both detection and graph presentation. This must be one shared computation; do not derive a second visually similar series in graph code.

Detect coherent signed changes that reach the configured minimum angle within at most ten seconds:

- reset accumulation across significant gaps;
- stop or restart an event when a meaningful direction reversal breaks coherence;
- suppress accumulation while the near-stop state is active;
- merge overlapping or immediately adjacent qualifying windows;
- retain start, end, signed change, and representative time.

All window traversal must use timestamps, not fixed sample counts.

### 2.4 Expose a detector result contract

Return one canonical result:

```text
{
  availability: { speed, heading, course },
  stops,
  turns,
  graphSeries: { speed, turning }
}
```

Missing external channels produce empty corresponding event arrays with explicit unavailable flags. They are not errors and are not repaired.

### Phase 2 tests

- Equivalent synthetic motion sampled at 1 Hz, irregular cadence, and 40 Hz yields equivalent events within the declared time tolerance.
- Near-stop speed drift that never reaches zero still produces one event.
- Threshold chatter produces no duplicate stop events.
- Starting stationary produces no stop.
- Downward crossing interpolation uses timestamps correctly.
- Heading 359→1 degrees produces a small change, not a spike.
- A 90-degree sharp turn qualifies at the default; a 180-degree/two-minute bend does not.
- Signed left/right turns retain direction.
- Heading changes during near-stop state do not produce turns.
- Significant gaps split state and cannot create synthetic events.
- Missing speed or heading yields explicit availability without throwing.

### Phase 2 exit criteria

- Detection is deterministic across supported sample rates.
- Both detectors are linear in input samples.
- The graph and turn detector consume the same turning derivation.
- External gaps or missing metrics never become fabricated events.

---

## Phase 3 — Interval-Consensus Candidate Engine and Likelihood Score

### Objective

Given video landmarks and detected activity events, return at most five deterministic, globally aligned candidate offsets with absolute match scores and diagnostics.

### Dependencies

Phases 1 and 2. No React or store dependency.

### 3.1 Generate typed offset support

For every compatible landmark/event pair, derive supported offset intervals:

- stop support centers on `stop.time - landmark.videoSecond` and extends by ±2 seconds;
- turn support runs from `turn.start - landmark.videoSecond` through `turn.end - landmark.videoSecond`, expanded by ±2 seconds.

Represent support with the landmark ID, event ID, type, interval bounds, and the residual function needed for scoring.

### 3.2 Discover consensus hypotheses

Use a deterministic sweep over interval endpoints to find offset regions supported by at least two distinct landmarks. Do not generate a Cartesian product of event assignments.

For each region:

1. refine a one-dimensional offset within the region to maximize the joint likelihood;
2. assign compatible events one-to-one;
3. require both matches when exactly two landmarks are eligible;
4. allow partial matches for three to five landmarks;
5. reject candidates with fewer than two matched landmarks.

Because landmarks and events are time-ordered, implement one-to-one assignment as an ordered dynamic program per type rather than a general combinatorial search. It must support skipping unmatched landmarks/events while preventing event reuse.

### 3.3 Compute the absolute score

For each chosen assignment:

- stop residual is distance to the stop point;
- turn residual is zero inside the detected interval and distance to its nearest edge outside it;
- only assignments inside the ±2-second supported interval count as matched.

Calculate:

```text
chiSquare = sum((residual / 2)^2)
timingLikelihood = exp(-chiSquare / (2 * matchedCount))
coverage = matchedCount / eligibleCount
matchScore = round(100 * coverage * timingLikelihood)
```

Return score inputs in development/test diagnostics so failures are inspectable, but expose only the presentation model needed by production components.

### 3.4 Merge and order ordinary candidates

- Sort primarily by descending match score.
- Use matched count, total residual, and numeric offset as deterministic tie-breakers.
- Merge candidates within the named candidate merge tolerance, retaining the stronger representative.
- Keep no more than five after resolved-location processing.
- Do not scale scores relative to the best candidate.

### 3.5 Implement dormant resolved-location behavior

Resolve map behavior only when a location landmark has a finite `activitySecond`:

- derive and pin `activitySecond - videoSecond` as the unscored map-only candidate;
- constrain combined candidates to ±2 seconds of that map offset;
- permit strong ordinary stop/turn candidates outside that range only as explicit `map-conflict` variants;
- mark those variants as excluding the map landmark;
- count the pinned map result toward the five-card limit.

An unresolved location with `activitySecond: null` must take no matching branch and must not affect eligibility, coverage, scoring, or candidate count.

### Phase 3 tests

- One stop plus one turn selects their most precise shared alignment.
- Turn marks anywhere inside the interval have zero turn residual.
- Type mismatches cannot contribute support.
- One activity event cannot satisfy two video landmarks.
- Exactly two eligible landmarks require two matches.
- Larger sets allow partial consensus and apply the coverage penalty.
- Score values match the formula and remain stable when unrelated candidates are added.
- Equal-score candidates use deterministic tie-breakers.
- Near-identical hypotheses merge.
- No-match and maximum-five behavior.
- Unresolved location is inert.
- Resolved map-only, map-compatible, and map-conflict variants.
- Long activities with many events do not trigger combinatorial growth.

### Phase 3 exit criteria

- The engine is a pure deterministic function.
- Candidate generation is bounded by landmarks × compatible events plus ordered assignment work.
- Every card variant has explicit evidence and map classification.
- Sparse two-landmark matching behaves correctly without claiming probabilistic confidence.

---

## Phase 4 — Calculation Orchestration and Derived-State Lifecycle

### Objective

Connect the pure detector/matcher to the store with explicit calculation, freshness, error, and media lifecycle behavior. This phase can be tested without rendering the final UI.

### Dependencies

Phases 1–3.

### 4.1 Implement the calculation hook

Create `useVideoSyncCalculation` to select canonical inputs and orchestrate:

1. detector input construction;
2. event detection;
3. candidate matching;
4. revision-checked result commit.

Set `calculating` before scheduling work so React can render the pending state. Capture the current input revision and discard the result if activity, landmarks, or sensitivities change before completion.

Do not put algorithms inside Zustand actions. The slice owns transitions; the hook owns orchestration.

### 4.2 Implement eligibility once

Derive explicit eligibility from:

- landmark types;
- detector metric availability;
- count of usable stop/turn landmarks;
- resolved map availability.

Rules:

- Current UI requires two usable stop/turn landmarks.
- Unsupported video observations remain stored and visible but do not silently count or incur score penalties.
- A future resolved location may enable map-only sync by itself.

Pass explicit eligibility and explanation strings to components. Components must not recalculate availability from raw activity fields.

### 4.3 Implement candidate freshness

- First search occurs only through Landmark Sync.
- Adding, removing, clearing, or committing a dragged landmark increments the input revision and marks existing candidates stale without removing them.
- Stale candidates remain visible but cannot be applied.
- Committing a sensitivity recalculates events; if a search has run before and current inputs are eligible, automatically rerun matching.
- Moving only the playhead does not invalidate or recalculate anything.
- Replacing an underlying source clears candidates rather than retaining cross-source stale results.

### 4.4 Integrate media lifecycle at owners

Update video import/clear transitions to call the manual-sync slice's video reset:

- clear landmarks;
- clear detected events and candidates;
- retain documented sensitivity settings unless a new project is being initialized.

Update activity activate/clear/restore transitions to call the activity reset:

- preserve landmarks and sensitivities;
- clear detection and candidates;
- recalculate detection after a replacement activity becomes canonical.

Do not watch arbitrary store fields from a component to repair lifecycle state after the fact. Invoke domain resets from the owning media transitions.

### 4.5 Add atomic candidate application

Add an owner-level action that validates the candidate offset before committing an atomic update:

```text
offsetDelta = candidateOffset - currentOffset
videoSyncOffsetSeconds = candidateOffset
selectedSecond = clamp(selectedSecond + offsetDelta, new timeline bounds)
```

Use this action only for candidate application. Existing manual entry and lane dragging retain their current playhead behavior.

Candidate application:

- immediately changes the canonical offset;
- compensates the playhead regardless of whether it currently lies inside the video;
- does not rewrite landmarks;
- does not invalidate candidates;
- updates applied-candidate highlighting by comparing against the canonical offset.

### Phase 4 tests

- Calculation status transitions and input-revision result rejection.
- First search never starts from a sensitivity edit alone.
- Existing search reruns after sensitivity commit.
- Landmark edits retain cards but make them stale and disabled.
- Missing metrics produce the correct eligibility explanation.
- Unsupported landmark types do not count or reduce coverage.
- Video replacement clears landmarks; activity replacement preserves them.
- Candidate application validates before mutation and updates offset/playhead atomically.
- Candidate application compensates an inside-video and outside-video playhead.

### Phase 4 exit criteria

- Derived state can never commit against outdated inputs.
- No stale candidate can change the video offset.
- Media replacement behavior is owned by media transitions, not component effects.
- Candidate application touches one canonical offset and one compensated playhead.

---

## Phase 5 — Toolbar Registration, Dedicated Workspace Mode, and Drawer UI

### Objective

Make manual video sync an explicit toolbar tool with a dedicated workspace mode and a complete drawer backed by the established contracts.

### Dependencies

Phases 1 and 4. Algorithm phases may initially be exercised through test fixtures, but the phase is complete only when real calculations are wired.

### 5.1 Register the toolbar tool

- Add one canonical `VIDEO_SYNC_TOOL` value to `createLayoutSlice.js`.
- Add it to the `VerticalToolbar` definitions with the required clock/circular-arrow icon.
- Include it in drawer preference validation/persistence wherever tool IDs are enumerated.
- Export the feature's drawer and workspace hook from `video-sync/index.js`.

Derive `videoSyncMode` from the actual drawer visibility plus active tool. Do not use `renderDrawerContent`, because drawer content remains mounted briefly for the close transition; workspace mode should end when the drawer ceases to be visible.

### 5.2 Compose the feature in `App.jsx`

- Add `useVideoSyncWorkspace` alongside other shell-level orchestration hooks.
- Render `VideoSyncDrawerContent` for `VIDEO_SYNC_TOOL`.
- Pass an explicit `videoSyncMode` and prepared diagnostic model into `OverlayEditor`.
- Pass `videoSyncMode` and timeline feature inputs into `OverlayPlayer`.

Keep `App.jsx` as composition only. Do not move algorithms or raw store selection into it.

### 5.3 Build the drawer sections

Section 1:

- copy the existing video-sync control composition from `VideoDrawerContent` into the new drawer;
- preserve the same owner hook/actions;
- do not extract a new shared component merely to remove duplication.

Section 2:

- render sorted presentational landmark cards;
- implement clear, delete, and card-to-scrub callbacks in the container hook;
- keep colors and icon selection in data/view-model preparation;
- expose disabled explanations when total or location-specific limits are reached.

Section 3:

- use existing Radix/shadcn slider primitives;
- maintain transient thumb values locally;
- commit physical values only on slider commit;
- present higher sensitivity consistently as more detection even though the turn threshold runs in the opposite numeric direction;
- render calculation, stale, error, no-match, and fresh candidate states explicitly;
- disable candidate application when stale or calculating;
- show match evidence and map variant labels from the candidate view model.

### 5.4 Build preview mark controls

Render Mark Stop, Mark Turn, and Mark Location through `VideoSyncMarkControls` in sync mode.

- All remain visible.
- Enable only when current timeline time resolves inside the video and the corresponding limit permits creation.
- Convert timeline time to video-local time once in the workspace hook.
- Location creation stores `activitySecond: null`.
- Creating a mark makes prior candidates stale.

### Phase 5 tests

- Toolbar selection and drawer visibility activate/deactivate sync mode correctly, including close transitions.
- Drawer preference accepts the new canonical tool.
- Project widgets are not changed by entering/leaving the tool.
- Slider transient movement does not commit; slider commit does.
- Candidate state variants and disabled behavior.
- Landmark sorting, card scrubbing, limits, clear, and deletion.
- All three mark buttons, including enabled location creation.
- Mark buttons disabled outside video range.

### Phase 5 exit criteria

- The complete non-timeline workflow is reachable through one toolbar tool.
- Entering the tool produces a dedicated mode without mutating project widgets.
- Every drawer state is explicit and test-covered.
- Location tagging can be exercised even though course-point selection is absent.

---

## Phase 6 — Timeline Graph and Landmark Interaction

### Objective

Extend the existing player timeline with synchronized graph/event geometry and landmark handles while retaining one viewport and one time-to-pixel transform.

### Dependencies

Phases 2, 4, and 5.

### 6.1 Add the timeline integration hook

Create `useVideoSyncTimeline` and invoke it from `useOverlayPlayer` only when sync mode is active. Give it the already-owned player inputs:

- viewport start/end;
- measured width;
- timeline bounds;
- canonical and preview video offsets;
- parsed detector result;
- landmarks;
- scrub and landmark mutation callbacks.

It returns render-ready graph paths, event bands, landmark line/handle models, and pointer props. `TimelineSurface` must receive presentation data, not raw store state.

Do not create a second viewport hook or separate horizontal scroll state.

### 6.2 Build fixed vertical scale preparation

When activity changes, calculate:

- a fixed speed scale starting at zero;
- a fixed symmetric signed-turning scale around zero;
- documented robust percentile clipping values used only for graph display.

Cache these scales with the activity/detector result. Zooming, panning, playhead movement, and landmark movement must not alter them.

### 6.3 Build viewport-aware SVG paths

In `graphGeometry.js`:

1. binary-search elapsed timestamps to select the visible source range;
2. map time to the existing timeline x-coordinate;
3. if samples exceed horizontal pixel capacity, bucket by x pixel and retain ordered local minima/maxima;
4. otherwise retain all visible points;
5. generate one SVG path string for speed and one for signed turning;
6. break paths at missing-data segments.

Throttle geometry replacement to at most once per animation frame during wheel zoom or pan. Recalculate only path geometry; never rerun event detection on viewport changes.

### 6.4 Extend `TimelineSurface`

Insert `VideoSyncTimelineGraph` between the ruler and lanes only in sync mode.

Render:

- speed path in stop red;
- signed turning path in turn green;
- stop bands at event time ±2 seconds;
- turn bands over the detected interval expanded by ±2 seconds.

Extend the existing absolute overlay layer with video landmark lines that span ruler, graph, and lanes. Preserve current playhead/export-marker stacking and pointer behavior.

### 6.5 Implement landmark positioning and dragging

For every landmark:

```text
timelineSecond = displayedVideoOffset + videoSecond
```

Use the preview video offset during an active video-lane drag so landmarks visually remain attached to the video. Do not commit landmark changes during video movement.

For in-view landmark handles:

- use pointer capture;
- convert pointer x to timeline time using the player viewport metrics;
- subtract the committed video offset to obtain video-local time;
- clamp to `[0, videoDuration]`;
- maintain transient drag preview in the hook;
- commit one store update at drag end;
- mark candidates stale only on commit.

If an active captured drag crosses a viewport edge, follow the existing timeline drag conventions for continued pointer handling. The noninteractive rule applies to idle offscreen indicators, not to a drag already in progress.

For idle offscreen landmarks:

- clamp an indicator to the appropriate edge;
- make it pointer-inert and nondraggable;
- do not scrub on click;
- leave card-based scrubbing as the navigation path.

### Phase 6 tests

- Time-to-x alignment with ruler, lanes, and playhead across zoom/pan.
- Fixed vertical scales across multiple viewport changes.
- Min/max decimation preserves a one-sample stop dip and sharp turn extrema.
- Missing-data path breaks.
- Output point count bounded by viewport width.
- No graph calculation on playhead-only changes.
- Bands use detected intervals and tolerance expansion.
- Landmark lines follow committed and preview video offsets.
- Drag preview/commit, video-bound clamping, and candidate invalidation timing.
- Offscreen indicators are rendered at edges and remain pointer-inert.

### Phase 6 exit criteria

- Graph, bands, landmarks, ruler, lanes, and playhead share exact horizontal geometry.
- Zoom/pan changes horizontal detail but never vertical scale.
- A 10,000-point representative activity remains within the agreed UI frame budget.
- Landmark drag updates one video-local time and never changes the video offset.

---

## Phase 7 — Dedicated Canvas Diagnostics

### Objective

Replace normal widget presentation with fixed, non-project diagnostics while sync mode is active, without changing template or project widget state.

### Dependencies

Phases 2, 4, and 5. Timeline implementation is not required.

### 7.1 Add an explicit canvas mode

Pass a mode or explicit `videoSyncMode` through `OverlayEditor` to `OverlayCanvas`.

In sync mode:

- force the imported video as the effective canvas background without changing the user's stored background preference;
- suppress project widget rendering;
- suppress Moveable handles, widget selection badges, and widget-edit gestures;
- render the mark controls and diagnostic layer;
- retain the same scene/video sizing and playback element.

Do not filter project widgets out of the store. Choose what to render at the container boundary.

### 7.2 Create the diagnostic model hook

`useVideoSyncDiagnostics` receives canonical activity and current timeline time and returns explicit render state:

- formatted current speed or unavailable;
- static projected course path or unavailable;
- current route marker or unavailable;
- mark-button state from the workspace controller.

Use the repository's existing activity interpolation utilities. Do not repeat numeric defensiveness for already canonical fields.

### 7.3 Render dedicated speed and route diagnostics

Create fixed presentational components rather than constructing fake project widgets.

- Place speed and route on the right side of the actual video preview.
- Use an opaque white route.
- Use a small bright-red current-position marker.
- Sample activity at timeline time while video playback resolves through the current offset.
- Show an explicit unavailable state for a missing/gapped speed or course value.

Reuse existing pure geometry/interpolation utilities where their contracts fit. Do not pass synthetic widget config through editor/template normalization merely to reuse a configurable widget component.

### Phase 7 tests

- Normal mode still renders project widgets and editing affordances.
- Sync mode hides them without mutating configuration or selection state.
- Sync mode forces video presentation without persisting a background-mode change.
- Speed diagnostic updates from timeline activity time.
- Route marker updates from timeline activity time.
- Applying a candidate changes the activity state shown over the compensated video frame.
- Missing speed/course produces explicit unavailable presentation.
- Diagnostic components never appear in project snapshots or render configuration.

### Phase 7 exit criteria

- Sync diagnostics are visually and architecturally separate from template widgets.
- The same video frame can be compared against different activity times by applying offsets.
- Entering/leaving sync mode has no durable editor-widget side effects.

---

## Phase 8 — Integration Hardening, Accessibility, Localization, and Performance

### Objective

Exercise the complete feature across project/media lifecycles, close contract gaps, and establish regression/performance coverage before considering the feature complete.

### Dependencies

Phases 1–7.

### 8.1 Complete integration scenarios

Test the full workflows:

1. Load activity and video, enter sync mode, mark one stop and one turn, run sync, and apply a candidate.
2. Drag a landmark, observe stale disabled candidates, rerun, and apply a new result.
3. Change sensitivity, observe bands update and prior search rerun.
4. Save, close, reload, and reproduce landmarks/sensitivities without persisted candidates.
5. Load v1, observe empty/default manual state, then save as v2.
6. Create, drag, persist, and delete a video-only location landmark with no map candidate.
7. Inject a resolved location fixture and exercise map-only, compatible, and conflict candidates.
8. Replace activity and video independently and verify their distinct lifecycle behavior.

### 8.2 Add explicit error and empty states

Cover:

- no video;
- no activity;
- insufficient usable landmarks;
- missing speed;
- missing heading;
- no detected events for one or both marked types;
- no two-landmark consensus;
- calculation failure;
- calculation superseded by newer inputs;
- stale candidates;
- diagnostic telemetry gap at playhead.

Do not collapse these into one generic empty state when the user can take a different corrective action.

### 8.3 Localization and accessibility

- Add all user-facing strings to existing locale resources following current fallback conventions.
- Give toolbar, mark, clear, delete, candidate, slider, and landmark-drag controls accessible names.
- Expose slider effective values and orientation semantics to assistive technology.
- Ensure stale/error messages are announced appropriately without repeatedly announcing playhead-driven diagnostics.
- Preserve keyboard focus when candidate data refreshes where the same logical card remains.
- Ensure color is not the only type/status signal; icons and text carry the same meaning.

### 8.4 Performance validation

Create representative fixtures or deterministic generators for:

- 5,000 and 10,000 samples;
- 1 Hz, irregular, and 40 Hz cadence;
- event-sparse and event-dense activities;
- missing-data gaps.

Measure in development tooling/tests:

- detector runtime;
- candidate runtime with five landmarks;
- graph preparation per viewport frame;
- SVG path point count;
- React commit behavior during zoom/pan;
- absence of detector/matcher work during playhead movement.

Target graph preparation below approximately 8 ms per frame on representative desktop hardware. If it fails, profile before changing architecture. Optimize local allocations/caching first; use Canvas or a worker only from measured evidence.

### 8.5 Run verification without a production build

Run focused tests throughout each phase, then the complete supported suites:

```text
cd app
npx vitest run
pnpm lint

cd ../src-tauri/ovrley_core
cargo test
```

Also run the Tauri project-file unit tests through the applicable `src-tauri` crate test command if they are not included by the core-crate command. Do not invoke `pnpm build`, `pnpm tauri build`, or the root build wrapper without explicit user permission.

### Phase 8 exit criteria

- All focused and existing test suites pass.
- ESLint/Prettier checks pass.
- Project-file migration and strict validation pass in Rust.
- Complete workflows match the updated specification.
- Zoom/pan remains responsive for the target dataset.
- No derived state leaks into project/template/render contracts.

---

## Phase Dependency Summary

```text
Phase 1: Contracts, store, project v2
   ├── Phase 2: Detection
   │      └── Phase 3: Matching
   │             └── Phase 4: Calculation lifecycle
   │                    ├── Phase 5: Toolbar and drawer
   │                    ├── Phase 6: Timeline
   │                    └── Phase 7: Canvas diagnostics
   └───────────────────────────────────────────────┘
                         Phase 8: Integration hardening
```

Phases 6 and 7 are independent after their shared prerequisites and can be implemented in either order. Phase 5 can begin against fixture calculation models once Phase 1 is complete, but it is not considered finished until Phase 4 orchestration is connected.

## Recommended Commit Boundaries

Keep reviewable commits aligned with behavior rather than file type:

1. Project v2 contract, v1 migration, and manual-sync store state.
2. Timestamp-based stop/turn detection with tests.
3. Interval-consensus matching and likelihood scoring with tests.
4. Calculation lifecycle, media invalidation, and atomic candidate application.
5. Toolbar registration, workspace mode, drawer, and marking controls.
6. Timeline SVG graph, event bands, and landmark drag behavior.
7. Canvas diagnostic mode and permanent diagnostic overlays.
8. Integration cases, localization, accessibility, and performance coverage.

Do not combine unrelated cleanup or refactoring with these commits.

## Completion Definition

The feature is complete when a user can mark video-local stop/turn/location landmarks, detect sampling-rate-independent stop/turn activity events, obtain and compare globally aligned offset candidates, apply one candidate while retaining the viewed video frame, and save/reload only the durable manual inputs. The dedicated workspace must remain isolated from project widgets, version-1 projects must continue to open through a strict ingress migration, and timeline navigation must remain responsive at the target activity size.
