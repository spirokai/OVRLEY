# Manual Video Sync — Updated Specification

## Goal

Allow a user to align one video with one activity by marking visually identifiable moments in the video, detecting compatible events in the activity, and applying one of the best shared-offset candidates.

This version supports one video clip. Multiple clips, clock drift, playback-rate correction, and selecting a course point from a map are outside the current scope.

## Terminology

- **Video-local time**: seconds from the start of the imported video.
- **Timeline time**: seconds on the activity/editor timeline.
- **Video offset**: the timeline time at which video-local second zero begins.
- **Video landmark**: a user-created stop, turn, or location mark tied to a video frame.
- **Detected event**: a stop or turn derived from activity telemetry.
- **Resolved location landmark**: a location landmark containing both a video-local time and an activity/course time. The current UI cannot create one, but the matching framework must support it.
- **Candidate**: a proposed value for the single canonical video offset.

The canonical relationship is:

```text
landmark timeline time = video offset + landmark video-local time
```

## Feature Ownership

- The feature lives in `app/src/features/video-sync/`.
- Its toolbar drawer is registered through `app/src/features/toolbar/`.
- Detection, matching, scoring, graph geometry, and formatting are pure feature utilities.
- Reusable stateful presentation behavior lives in feature hooks.
- Components remain presentational.
- Durable manual-sync state is owned by a dedicated Zustand slice and exposed through selector hooks.
- The existing `videoSyncOffsetSeconds` remains the only canonical applied offset. Do not add a landmark-specific or candidate-specific offset.

## Workspace Mode

Opening the Video Sync drawer activates a dedicated video-sync workspace mode. Closing it or selecting another tool restores the normal editor.

While video-sync mode is active:

- Show the source video and the manual landmark controls.
- Hide project/template widgets without deleting or modifying them.
- Show permanent diagnostic speed and route widgets over the video.
- Show the telemetry graph and landmark overlays in the timeline.

Diagnostic widgets are derived UI. They must never be inserted into the project or template widget arrays.

## Toolbar Entry and Drawer

Use a simple clock icon whose outer outline reads as a large circular arrow and which has no tick marks.

### Section 1 — Existing Video Sync Controls

Place the existing video-sync controls from `VideoDrawerContent` at the top. Copy their composition into this feature rather than introducing a shared abstraction; this layout is expected to diverge.

### Section 2 — Landmarks

- The section has a trailing **Clear** action that removes all landmarks.
- List landmarks as one-row cards sorted by ascending video-local time.
- Each card contains:
  - a type-colored left stripe;
  - a type-colored icon;
  - otherwise neutral text;
  - formatted video-local time;
  - a delete action.
- Clicking a landmark card scrubs to its timeline position.
- Limits:
  - at most five landmarks total;
  - at most one location landmark.
- When a limit prevents creation, disable the affected mark action rather than silently ignoring it.

Canonical colors:

- stop: red;
- turn: green;
- location: purple.

### Section 3 — Detection and Candidates

Provide two controls:

- **Speed sensitivity**, backed canonically by a near-stop threshold in km/h.
- **Turn sensitivity**, backed canonically by a minimum turn angle in degrees.

Higher displayed sensitivity always means more detections:

- increasing speed sensitivity raises the near-stop threshold;
- increasing turn sensitivity lowers the minimum turn angle.

Persist the physical thresholds, not an arbitrary 0–100 sensitivity value. Show the effective physical value in supporting text so behavior remains understandable.

The two-column **Landmark Sync** button runs matching. It is disabled while calculation is pending.

Current enablement requires at least two stop/turn landmarks whose corresponding activity telemetry is usable. A video-only location landmark neither enables sync nor participates in matching. The dormant resolved-location framework has separate behavior described below.

Candidate cards:

- show the proposed offset;
- show the absolute **Match score** for ordinary candidates;
- show matched evidence, for example `4 of 5 landmarks matched`;
- are sorted by descending score, except for the pinned map-only candidate;
- are limited to five total candidates;
- apply their offset immediately when selected.

If no offset aligns at least two eligible landmarks, show **No candidate aligns at least two landmarks**. Do not describe the globally matched result as an independent candidate for one landmark.

## Landmark Data Contract

All landmarks contain:

```text
id: stable project-local identifier
type: "stop" | "turn" | "location"
videoSecond: finite seconds from video start
```

`videoSecond` must remain within the imported video duration.

A location landmark additionally contains the required field:

```text
activitySecond: finite activity/course time | null
```

`null` is documented optional absence: the user has marked a video location but has not selected the corresponding point on the course. The current feature always creates location landmarks with `activitySecond: null`. A missing field or malformed present value is invalid version-2 project data and must fail at project ingress.

Stop and turn landmarks must not carry location-only fields. Use one canonical discriminated shape; do not add aliases or compatibility variants.

## Creating and Editing Landmarks

Show three controls in the lower-left corner of the actual video preview:

- **Mark Stop**;
- **Mark Turn**;
- **Mark Location**.

All three controls are enabled when the playhead resolves to a frame inside the video and the applicable landmark limit permits creation. They remain visible but disabled outside the video range.

Creating a landmark stores the current video-local time. A location landmark is usable now for testing creation, dragging, persistence, scrubbing from the list, and deletion even though it has no activity-side course point.

Dragging an in-view timeline landmark changes only its `videoSecond`; it never changes the video offset. Commit the change when dragging ends.

Moving the video—through lane dragging, manual offset entry, automatic timestamp sync, or candidate application—does not rewrite landmark times. Their timeline positions move because they are derived from the video offset.

## Activity Event Detection

Detection consumes the canonical activity timestamp, speed, and heading series. Source-format parsing and unit normalization remain at the existing activity ingress.

The detector must be independent of input format and sampling rate from 1–40 Hz:

- use elapsed-time windows and duration-weighted operations, never sample counts;
- do not give high-rate samples proportionally more weight;
- use interpolation only to estimate a crossing time between adjacent valid samples;
- split processing across a significant missing-data gap rather than interpolating through it;
- initially define a significant gap as greater than `max(3 seconds, 3 × median sample interval)`.

Keep all detector ranges and timing constants as named values in the feature `data/` module so they can be tuned without rewriting the algorithm.

### Near-stop Detection

Initial tunable values:

- threshold default: 5 km/h;
- threshold range: 1–10 km/h;
- entry dwell: 2 seconds below the threshold;
- exit dwell: 2 seconds above `threshold + 2 km/h`;
- exit hysteresis: 2 km/h.

A near-stop is the transition from established movement into a sustained low-speed state. It does not require a reported speed of exactly zero.

- Require a preceding moving state; beginning an activity below the threshold is not a stop event.
- Timestamp the event at the interpolated initial downward threshold crossing, not at the end of the dwell period.
- Hysteresis and dwell prevent repeated events caused by sensor drift near the threshold.
- Represent a detected stop as a point time plus its supporting low-speed interval for visualization.

### Turn Detection

Initial tunable values:

- minimum-angle default: 90 degrees;
- minimum-angle range: 90–360 degrees;
- maximum qualifying duration: 10 seconds;
- elapsed-time heading smoothing window: 1 second.

Unwrap heading circularly and find a coherent signed heading change that reaches the configured minimum angle within the maximum duration. A gradual change such as 180 degrees over two minutes is not a turn.

- Do not derive turns while the detector is in the near-stop state because low-speed GPS heading is unreliable.
- Merge overlapping or adjacent qualifying windows into one detected turn interval.
- Retain start, end, signed change, and representative time in the detected event model.
- A video turn landmark may refer to any moment inside the detected interval; it is not forced to the center.

## Candidate Generation — Interval Consensus

Use deterministic interval-consensus matching. Do not bin the timeline, use sample-index derivatives, or independently fit each landmark.

For each eligible video landmark and each compatible activity event, derive the video offsets that could align them:

- stop: detected stop time minus video-local landmark time, with ±2 seconds of user timing tolerance;
- turn: the detected turn interval minus video-local landmark time, expanded by ±2 seconds.

Overlapping offset support from different landmarks forms a candidate hypothesis. Refine each hypothesis to the shared offset that maximizes its joint match likelihood.

Rules:

- Respect landmark type.
- Use one shared offset across all evidence.
- Use a one-to-one assignment; one detected activity event cannot explain multiple video landmarks.
- With exactly two eligible landmarks, both must match.
- With three to five eligible landmarks, partial consensus is allowed, but every unmatched landmark penalizes the score.
- A candidate must match at least two eligible landmarks.
- Merge near-identical offset hypotheses using a named, tunable merge tolerance.
- Keep at most five distinct candidates after map-only pinning and score ordering.

This handles the important minimum case of one stop plus one turn by testing type-compatible activity stop/turn pairs and selecting the shared offset that aligns the stop most precisely while placing the video turn within the detected turn interval.

## Match Score

The score is an absolute normalized likelihood, not a probability that the candidate is correct and not a score rescaled relative to the other candidates.

For a matched landmark, define residual `r` as:

- stop: absolute distance from the detected stop time;
- turn: zero while the aligned mark is inside the detected turn interval, otherwise distance to the nearest interval boundary.

Use 2 seconds as the timing-error scale. For `m` matched landmarks out of `n` eligible landmarks:

```text
chiSquare = sum((r / 2)^2 for each matched landmark)
timingLikelihood = exp(-chiSquare / (2 * m))
coverage = m / n
matchScore = round(100 * coverage * timingLikelihood)
```

Only assignments within the ±2-second supported matching interval count as matched. This score is the geometric-mean likelihood under the declared timing-error model, reduced by incomplete coverage. It has stable meaning across candidate sets.

Multiple candidates may legitimately have the same high score. Do not force the best candidate to 100 or lower other scores merely to create separation.

## Resolved Location Framework

No current UI automatically detects a location or lets the user select a course point. Therefore, ordinary location landmarks have `activitySecond: null`, and none of the behavior in this section activates for them.

Activate resolved-location behavior only when the same location landmark contains both a valid `videoSecond` and valid `activitySecond`.

Use the authoritative-plus-diagnostic policy:

1. Calculate `mapOffset = activitySecond - videoSecond`.
2. Pin one **Map only** candidate at the top of the candidate list.
3. The map-only candidate has no match score and applies `mapOffset` exactly.
4. Combined candidates must agree with `mapOffset` within the ±2-second video-mark tolerance.
5. Strong stop/turn-only candidates that disagree may still be shown, but must be labelled that the map landmark was excluded and conflicts with the candidate.
6. A resolved location landmark may enable Landmark Sync by itself; an unresolved video-only location landmark may not.

All map guards and classification happen once in matching/container logic. Presentational components receive explicit candidate variants and must not inspect optional raw landmark fields repeatedly.

## Candidate Freshness and Recalculation

Landmark Sync is the explicit action that performs the first candidate search.

After a landmark is added, deleted, cleared, or committed at a new dragged position:

- keep the existing candidate cards visible;
- mark the complete result set stale;
- disable applying stale candidates;
- show **Landmarks changed—run Landmark Sync again**.

After a sensitivity control is committed:

1. recalculate detected events and graph bands;
2. if a candidate search has previously run and the inputs remain eligible, rerun matching automatically;
3. otherwise do not initiate the first candidate search.

Do not recalculate detection or matching while a sensitivity thumb is moving. Do not recalculate candidates merely because the playhead moves.

Replacing underlying media follows the lifecycle rules below and clears candidates rather than retaining cards referring to another source.

## Applying a Candidate

Selecting an enabled, fresh candidate:

1. immediately writes its value to the canonical `videoSyncOffsetSeconds`;
2. changes the playhead by the same offset delta, regardless of whether it was inside the video;
3. clamps the resulting playhead only to valid timeline bounds;
4. preserves the viewed video-local frame relative to the offset change;
5. highlights a candidate whose offset matches the currently applied offset.

Applying a candidate does not rewrite landmarks or make candidates stale. Selecting another candidate or editing the manual offset remains immediately reversible.

## Timeline Graph

Show the graph between the timeline ruler and timeline lanes only in video-sync mode. It shares the timeline viewport, zoom, pan, width measurement, and time-to-x transform.

Plot:

- speed in the stop color;
- signed heading-change rate in the turn color, presented to users as **Turning** rather than raw compass heading.

Raw 0–360-degree heading must not be plotted because north-crossing wraparound creates false visual spikes.

### Rendering

Use inline SVG with one path per series. Do not add a plotting dependency initially.

- Select visible source samples by timestamp using binary search.
- When visible samples outnumber available horizontal pixels, bucket them by x-coordinate and retain local minima and maxima.
- When zoomed in enough, plot all visible samples.
- Rebuild derived SVG geometry at most once per animation frame while zooming or panning.
- Keep source telemetry and detected events unchanged; only rendered geometry is recalculated.
- Bound output path complexity approximately by viewport pixel width.

### Vertical Scales

Compute robust activity-wide vertical scales once when activity data changes. Vertical scaling must not change during zoom or pan.

- Speed uses a fixed scale beginning at zero.
- Turning uses a fixed symmetric scale around zero.
- A documented robust percentile may clip isolated display spikes, but it must not modify detector input or matching data.

### Event and Landmark Overlays

- Show detected stop and turn intervals as translucent vertical bands in their type colors.
- Expand stop visualization by ±2 seconds around its event time.
- Show a turn's detected interval, expanded by the same ±2-second user tolerance.
- Show video landmarks as type-colored vertical lines with a rectangular icon handle at the top.
- In-view landmark lines span the ruler, graph, and timeline lanes similarly to the playhead.
- In-view handles are draggable.

For landmarks outside the viewport:

- clamp a visual indicator to the appropriate viewport edge;
- make the clamped indicator noninteractive and nondraggable;
- do not scrub when it is clicked;
- use the landmark list card when the user wants to scrub to that mark.

## Diagnostic Canvas Widgets

Show two permanent diagnostic widgets on the right side of the video preview:

- current activity speed at the playhead timeline time;
- course route with a small bright-red current-position marker and an opaque white route.

These widgets sample activity at timeline time, while the displayed video frame is resolved through the current video offset. This makes changing the offset immediately visible as a different activity state over the same video frame.

Missing or gapped external telemetry must produce an explicit unavailable diagnostic state; do not fabricate speed, position, or heading. A missing activity metric also makes corresponding landmarks ineligible for matching, but does not prevent the user from creating or retaining those video observations.

## Media Lifecycle

- Replacing or deleting the video clears all landmarks and candidates because video-local landmark times refer to the old footage.
- Replacing or deleting the activity preserves landmarks and sensitivity settings, but clears detected events and candidates.
- Loading a project may stage/replace media first and hydrate the project's saved landmarks afterward.
- Activity replacement reruns detection only when the new activity is ready.

## Persistence and Project Version 2

Persist only:

- landmarks;
- near-stop threshold in km/h;
- minimum turn angle in degrees.

Store them as required canonical manual-sync state under the project's `sync` domain. Detected events, graph geometry, candidates, scores, calculation status, stale status, and diagnostic widget state are derived and must not be persisted.

Saving or changing landmarks and sensitivities participates in normal project dirty-state comparison.

Increment the project schema to version 2. Version 1 remains operable through an explicit migration at the Rust project-file ingress:

- a valid v1 project becomes the canonical v2 shape in memory;
- migrated manual-sync state contains an empty landmark array and the documented default thresholds;
- malformed v1 or v2 present data still fails loudly;
- consumers only receive the canonical v2 shape;
- loading does not overwrite the archive;
- the next normal save writes version 2.

This migration is the single versioned external-boundary adapter. Do not add frontend fallback branches for missing v2 fields.

## Execution and Concurrency

Run detection, candidate generation, scoring, and graph preparation as frontend feature utilities initially.

- Set calculation state before scheduling work so the disabled button renders.
- Capture an input revision for each calculation.
- Discard results if activity, landmarks, or sensitivities change before completion.
- Do not commit partial results.
- Surface calculation failures explicitly and keep stale candidates disabled.
- Add Rust IPC or a Web Worker only if profiling shows the frontend calculation violates the frame budget.

Target inputs of 5,000–10,000 samples and sampling rates from 1–40 Hz. Detection should remain linear in activity samples. Candidate hypothesis generation should remain proportional to eligible landmarks × compatible detected events; do not enumerate every multi-landmark event combination.

## Required Tests

### Detection utilities

- equivalent events at 1 Hz, irregular cadence, and 40 Hz;
- near-stop sensor drift that never reaches zero;
- dwell and hysteresis around the threshold;
- activity beginning stationary;
- threshold-crossing time interpolation;
- heading wraparound at 0/360 degrees;
- sharp qualifying turns and gradual non-qualifying direction changes;
- turns suppressed during near-stop state;
- missing telemetry gaps split detection.

### Matching and scoring

- one stop plus one turn finds the correct shared offset;
- turn marks at start, middle, and end of a detected turn interval;
- type mismatches are never assigned;
- one activity event cannot satisfy two video landmarks;
- exactly two landmarks require both matches;
- three to five landmarks allow penalized partial consensus;
- likelihood score is absolute and stable across candidate sets;
- near-identical offsets merge deterministically;
- maximum five candidates;
- unresolved location landmark has no matching effect;
- resolved map-only candidate is pinned and unscored;
- map-compatible and map-conflicting candidate classification.

### State and interaction

- maximum five total landmarks and one location landmark;
- mark controls disabled outside video range;
- landmark timeline position derives from video-local time plus offset;
- dragging a landmark changes only video-local time;
- moving the video moves all landmark timeline positions without rewriting them;
- landmark edits retain but invalidate candidate cards;
- sensitivity commit reruns an existing search;
- stale candidates cannot be applied;
- applying a candidate updates the canonical offset and compensates the playhead by the same delta;
- offscreen indicators remain noninteractive;
- video replacement clears landmarks;
- activity replacement preserves landmarks but clears derived results.

### Persistence

- strict v2 round trip for landmarks and physical sensitivity thresholds;
- candidates and detected events are absent from saved projects;
- valid v1 project migrates in memory to empty landmarks and default thresholds;
- migrated project writes v2 only on normal save;
- malformed present manual-sync fields fail at ingress.

### Graph performance and geometry

- min/max pixel bucketing preserves brief extrema;
- zoomed-in ranges retain all visible samples when decimation is unnecessary;
- vertical scales remain unchanged across zoom and pan;
- output point count remains bounded by viewport width;
- graph geometry does not recompute when only the playhead moves;
- representative 10,000-point zoom and pan remain within the UI frame budget.

## Explicit Non-goals

- Multiple video clips.
- Selecting or detecting an activity course point from the map UI.
- Automatic location matching.
- Clock-drift or playback-rate estimation.
- Persisting candidates or detected events.
- Adding diagnostic widgets to templates or rendered output.
- Introducing a charting dependency, worker, or Rust matching command without profiling evidence.
