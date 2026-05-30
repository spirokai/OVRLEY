Status: ready-for-agent

# 10 - Collapse Preview Playback Ownership

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

Preview playback ownership is currently spread across store flags and several orchestration hooks. The timing rules are real, but the seam is shallow because no single module owns them cleanly.

This is a serious maintenance risk because:

- changing one playback rule requires understanding multiple hooks plus store transitions
- timing bugs have poor locality because handoff, scrubbing, frame publication, and clock choice are split apart
- the same domain facts repeat across module interfaces: playback state, source, playhead second, FPS, and sync offset
- tests are pushed toward broad orchestration coverage instead of direct coverage of the riskiest timing seams

The user-facing behavior must remain the same. Playback, scrubbing, warning behavior, and handoff should continue to work exactly as they do now.

## What to build

Collapse preview playback ownership behind one deeper playback seam that owns clock selection, scrub lifecycle, frame publication, and handoff between timeline-backed and video-backed playback.

The design should reduce the amount of timing logic split between store flags and orchestration hooks while preserving current user-visible behavior: play, pause, scrub, end-of-playback handling, drift correction, warning behavior, and playback ownership handoff must continue to work from the editor user's point of view.

## Affected files

- `app/src/features/player/hooks/usePlaybackEngine.js`
- `app/src/features/player/hooks/usePlaybackSourceHandoff.js`
- `app/src/features/player/hooks/useTimelinePlaybackLoop.js`
- `app/src/features/player/hooks/useOverlayPlayerState.js`
- `app/src/features/player/utils/playerTimeline.js`
- `app/src/features/video-preview/hooks/useVideoPreview.js`
- `app/src/features/video-preview/hooks/useVideoPlaybackClock.js`
- `app/src/features/video-preview/hooks/useVideoPreviewWarnings.js`
- `app/src/store/slices/createEditorSlice.js`

## Suggested plan

1. Identify the timing responsibilities currently split across `player`, `video-preview`, and the store.
2. Extract pure helper logic first for handoff rules, frame publication, and timing calculations where possible.
3. Introduce one deeper playback seam that owns clock choice, scrub lifecycle, and playback ownership.
4. Move store writes and DOM/video event wiring behind adapters that serve the new playback seam.
5. Keep top-level hooks as composition points only where existing consumers still need them.
6. Verify equivalence through playback regression tests and manual QA around boundary conditions.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level playback ownership module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper playback module over multiple micro-hooks.
- Existing shallow hooks should be absorbed, merged, or reduced where possible rather than preserved behind new wrappers.
- Do not split start, pause, handoff, polling, or frame publication into separate files unless each is a real seam with more than one adapter or caller.
- Optimize for pure concerns and locality, not minimal line count per file.
- A successful implementation keeps the playback module graph flatter than it is today.

## Practical testing strategy

- Add characterization tests for current playback flows before deepening: play, pause, scrub, handoff from timeline to video, handoff from video to timeline, and final-frame/end behavior.
- Extract pure timing and handoff helpers first, then add direct tests for those helpers so regressions become easier to localize.
- Preserve integration-style tests around the composed playback path using existing `usePlaybackEngine` and `useVideoPreview` coverage as prior art.
- Keep dedicated regression coverage for warning behavior and frame publication so metadata warnings, slow-seek warnings, and video-frame deduplication do not drift.
- Manually verify rapid scrubbing, playback near video boundaries, source reloads, and switching between background modes.

## Acceptance criteria

- [ ] One deeper playback seam owns clock selection, handoff, scrub lifecycle, and frame publication rules
- [ ] Store state and DOM event wiring are treated as adapters behind that seam instead of each owning timing behavior directly
- [ ] Play, pause, scrub, and end-of-playback behavior remain functionally equivalent
- [ ] Timeline/video ownership handoff remains correct at both entry and exit boundaries
- [ ] Warning behavior and frame publication continue to behave the same under regression tests

## Blocked by

None - can start immediately
