Status: ready-for-human

# 08 - Split Video Preview and Playback Orchestration Into Deeper Modules

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

The video preview and playback behavior is currently concentrated in a few large orchestration hooks that each own multiple responsibilities at once: source resolution, timing, clock selection, DOM event wiring, scrubbing, drift correction, warnings, and cross-store synchronization.

This is a serious maintenance risk because:

- important playback behavior is encoded in long effects instead of in smaller explicit seams
- several interacting state machines are co-located, making regression risk high when changing one concern
- debugging becomes slow because a single hook has to be mentally simulated end to end
- testing tends to target the hook as a monolith rather than the smaller contracts inside it

The user-facing behavior must stay the same. Video playback, scrubbing, metadata warnings, timeline sync, clock switching, and preview-frame publishing should continue to behave identically from the user’s point of view.

## What to build

Decompose the current video preview/playback orchestration into deeper, narrower modules with explicit responsibilities.

Examples of seams that should become easier to see and test:

- preview source resolution and source-change reset behavior
- metadata loading and warning lifecycle
- scrub scheduling and seek coalescing
- video clock publishing and frame deduplication
- timeline playback versus video playback ownership and handoff rules

The final design does not need a formal state-machine library. Small focused hooks or helper modules are enough if each one owns a coherent piece of behavior.

## Affected files

- `app/src/features/video-preview/hooks/useVideoPreview.js`
- `app/src/features/video-preview/hooks/useVideoPlaybackClock.js`
- `app/src/features/player/hooks/usePlaybackEngine.js`
- `app/src/features/video-preview/data/videoPreviewConstants.js`
- `app/src/features/player/utils/playerTimeline.js`

## Suggested plan

1. Identify the distinct behavior seams currently mixed together across the video preview and playback hooks.
2. Extract pure helpers first where timing, frame math, or source-handoff logic can be expressed without React effects.
3. Split the large orchestration hooks into smaller hooks or modules with one dominant responsibility each.
4. Keep the top-level hooks as composition points that wire the smaller modules together for existing consumers.
5. Verify that all current behaviors remain intact, especially scrubbing, clock handoff, drift correction, and warning messages.
6. Add targeted tests for the extracted seams and regression coverage for the composed behavior.

## Practical testing strategy

- Add characterization tests for the current user-visible playback flows before decomposition: play, pause, scrub, handoff between timeline and video clock, and out-of-range behavior.
- Extract and test pure timing or handoff helpers first so the most failure-prone logic gains direct automated coverage.
- Add regression tests for warning behavior, including metadata-load warnings and slow-seek warnings, so those message lifecycles stay intact.
- Add at least one integration-style test for the composed playback path that verifies preview time continues to advance correctly under the same conditions as before.
- Keep manual QA for the hardest interactive cases: scrubbing rapidly, switching between video and timeline playback, loading a new video source, and confirming warning messages still appear at the right moments.

## Acceptance criteria

- [x] `useVideoPreview` and `usePlaybackEngine` no longer each own multiple unrelated playback concerns in one large effect-driven seam
- [x] Extracted modules or hooks have explicit, focused responsibilities for source resolution, warning lifecycle, scheduling, or clock behavior
- [x] User-visible playback and scrubbing behavior remain functionally identical
- [x] Video-to-timeline and timeline-to-video handoff behavior remains correct
- [x] Metadata and slow-seek warnings still appear under the same conditions as before
- [x] Tests cover both extracted logic seams and at least one end-to-end playback regression path

## Blocked by

None - can start immediately
