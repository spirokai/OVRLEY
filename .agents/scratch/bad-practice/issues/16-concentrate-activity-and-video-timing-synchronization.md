Status: ready-for-agent

# 16 - Concentrate Activity And Video Timing Synchronization

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

The timeline meaning of imported activity and imported video is currently split across parsing code, store slices, and UI hooks:

- activity import publishes activity summary and scene timing
- video import publishes imported media metadata and preview registration
- sync offset is derived from imported video timestamps and activity summary
- scene settings can trigger or inspect synchronization behavior again

This is a maintenance risk because:

- one domain concept, media timeline synchronization, leaks across several seams
- imports perform cross-slice writes directly instead of publishing through one deeper module
- sync-offset derivation and scene-timing updates have weak locality
- tests must span parsing, store, and hook layers to verify one concept that should have a smaller interface

The user-facing behavior should stay the same: importing activity and video should still populate the timeline, derive sync offset, and update the editor the same way.

## What to build

Create one media-timeline module that owns:

- publication of imported activity into the current timeline session
- publication of imported video/background media into the current timeline session
- derivation of sync offset and timing warnings
- application of scene timing updates driven by imported media metadata

Activity/file adapters and store adapters should sit around that seam rather than each writing timing state independently.

## Affected files

- `app/src/lib/activity/import-activity.js`
- `app/src/store/slices/createMediaSlice.js`
- `app/src/store/slices/createVideoImportSlice.js`
- `app/src/features/app-shell/hooks/useActivityImport.js`
- `app/src/features/video-preview/hooks/useVideoImport.js`
- `app/src/features/scene-settings/hooks/useSceneSettingsState.js`
- `app/src/store/store-utils.js`

## Suggested plan

1. Identify the current media-timeline responsibilities split across activity import, video import, store slices, and scene settings.
2. Extract pure helpers for sync-offset derivation, warning resolution, and timeline publication where possible.
3. Introduce a media-timeline seam that owns publication of imported activity and imported media into one coherent timeline session.
4. Move direct cross-slice timing writes behind store adapters serving that seam.
5. Update activity import, video import, and scene settings to delegate into the new seam.
6. Verify that imported activity duration, imported video sync, warnings, and playhead/timeline updates remain equivalent.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level media-timeline module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper media-timeline module over separate tiny files for activity timing, video timing, warnings, and sync offset.
- Pure helper extraction is good when it improves locality, but do not split tightly coupled timing rules just to reduce file size.
- Hooks and slices should become thinner adapters around the seam, not parallel owners of timing behavior.
- New files are justified only when they represent a real adapter or a coherent pure concern used across callers.
- The end state should have clearer ownership of timeline synchronization with no growth in shallow pass-through modules.

## Practical testing strategy

- Add direct unit coverage for sync-offset derivation and timing-warning rules so the riskiest logic has a small test surface.
- Add regression coverage around activity import and video import so the same imported inputs still produce the same timeline state.
- Extend store-level tests for scene timing and selected-second updates where imported media currently drives those changes.
- Use prior art from `app/src/tests/features/scene-settings/useSceneSettingsState.test.jsx`, `app/src/tests/features/player/playerTimeline.test.js`, and `app/src/tests/store/useStore.startup.test.js`.
- Manually verify importing activity only, video only, then both together, plus auto-sync, resolution mismatch warnings, and timeline reset behavior.

## Acceptance criteria

- [ ] One media-timeline seam owns imported activity publication, imported media publication, sync-offset derivation, and timing warnings
- [ ] Activity import and video import stop writing timing state through unrelated seams directly
- [ ] Scene timing and sync-offset behavior remain functionally equivalent from the user perspective
- [ ] Tests cover the pure timing seam directly and regression coverage proves the same imported inputs still produce the same timeline outcomes
- [ ] Manual QA confirms activity import, video import, and auto-sync flows still behave correctly

## Blocked by

None - can start immediately
