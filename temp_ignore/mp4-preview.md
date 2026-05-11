# MP4 Preview Playback Performance Plan

> **Scope**: Fix imported-video preview playback in the overlay editor. This plan covers the interactive preview path only: `useVideoImport.js`, `OverlayPlayer.jsx`, `OverlayCanvas.jsx`, `useVideoPreview.js`, and related store state. It does not change the final MP4 compositing/export pipeline.

---

## Problem Summary

The current preview path does not let the browser play the imported video normally.

`OverlayPlayer.jsx` owns a `requestAnimationFrame` loop that advances global `selectedSecond` through Zustand. `useVideoPreview.js` subscribes to `selectedSecond` and assigns `video.currentTime` whenever the playhead changes. During playback this becomes repeated frame-by-frame seeking.

That is extremely expensive for MP4/MOV/H.264/H.265 video because seeking usually requires decoder flushes, keyframe jumps, and decoding forward to the requested frame. A browser `<video>` can play one 1080p or 4K file smoothly, but only if it is allowed to decode sequentially. The current code turns playback into continuous random access.

The fix is to make the video element the playback clock while video preview is active.

---

## Target Architecture

Use two synchronization modes:

1. **Scrub / manual seek mode**
   - Timeline changes set `video.currentTime`.
   - This is acceptable because the user is explicitly seeking.
   - Preview widgets should update from the committed or transient timeline value.

2. **Playback mode**
   - Seek the video once at playback start.
   - Call `video.play()`.
   - Read time from `video.currentTime`.
   - Drive overlay preview from the video clock using `requestVideoFrameCallback` when available, falling back to `requestAnimationFrame`.
   - Avoid assigning `video.currentTime` every frame.
   - Only correct drift if the difference becomes large enough to indicate a real desync.

---

## Phase 1 - Add Shared Preview Playback State

**Goal**: Let `OverlayPlayer` and `useVideoPreview` coordinate playback without local-only state.

### Deliverables

#### [MODIFY] `app/src/store/slices/createEditorSlice.js`

Add a small preview playback state group:

```js
previewPlaybackState: "paused"; // 'paused' | 'playing' | 'scrubbing'
previewPlaybackSource: "timeline"; // 'timeline' | 'video'
previewPlaybackStartedAtSecond: 0;
```

Add actions:

```js
startPreviewPlayback({ source, second });
pausePreviewPlayback(second);
beginPreviewScrub(second);
updatePreviewScrub(second);
commitPreviewScrub(second);
```

Expected behavior:

- `startPreviewPlayback({ source: 'video', second })` marks playback as video-clock-driven when an imported video is active and visible.
- `startPreviewPlayback({ source: 'timeline', second })` preserves the existing non-video behavior.
- Transient timeline updates should not write to `localStorage`.
- Committed pause/scrub/reset updates can continue to persist selected second where appropriate.

### Manual Tests

1. Start playback with no imported video - existing overlay-only preview still advances.
2. Pause, reset, arrow keys, and timeline scrubbing still update `selectedSecond`.
3. Store state transitions are visible via `window.__OVRLEY_STORE__.getState()` in devtools.

---

## Phase 2 - Replace Per-Frame Seeking With Native Video Playback

**Goal**: Imported video plays via the browser media pipeline, not by repeated `currentTime` assignment.

### Deliverables

#### [MODIFY] `app/src/hooks/useVideoPreview.js`

Refactor the hook into responsibilities:

1. Resolve `videoSrc` from `importedVideoPath`.
2. Seek the video only when:
   - the source changes,
   - playback starts,
   - playback is paused and `selectedSecond` changes,
   - the user is scrubbing,
   - the sync offset changes.
3. In video playback mode:
   - call `video.play()`,
   - do not set `video.currentTime` on every selected-second update,
   - publish the current preview second from the video clock.

Use the following drift rule:

```js
const DRIFT_CORRECTION_SECONDS = 0.25;
```

During playback, only assign `video.currentTime` if:

```js
Math.abs(selectedSecond - videoSyncOffsetSeconds - video.currentTime) > DRIFT_CORRECTION_SECONDS;
```

Never seek for normal frame-to-frame advancement.

#### [MODIFY] `app/src/components/overlay-editor/OverlayCanvas.jsx`

Keep the video element mounted while a video is imported and the background mode is video. Add media attributes:

```jsx
preload = "auto";
muted;
playsInline;
```

Avoid CSS transitions tied to every playhead update. The current `isOutOfRange` opacity switch is fine for static state, but it should not cause expensive transitions during continuous playback.

### Manual Tests

1. Import a 1080p MP4, switch background to video, press play - video advances smoothly for at least 60 seconds.
2. Import a 4K MP4 and repeat - GPU usage may be high, but playback should not collapse to repeated stalls after 2 seconds.
3. Pause and resume - video resumes from the same timeline position.
4. Drag the slider - video seeks during scrubbing, then resumes smoothly after play.
5. Change sync offset - paused preview seeks to the adjusted frame; playing preview corrects once, then continues natively.

---

## Phase 3 - Drive Overlay Preview From Video Frames

**Goal**: Keep widgets synchronized to the actual decoded video frame without over-rendering React.

### Deliverables

#### [NEW] `app/src/hooks/useVideoPlaybackClock.js`

Create a hook that receives:

```js
videoRef;
isActive;
videoSyncOffsetSeconds;
onPreviewSecond;
```

Behavior:

- Prefer `HTMLVideoElement.requestVideoFrameCallback`.
- Fallback to `requestAnimationFrame`.
- On each decoded video frame, compute:

```js
previewSecond = video.currentTime + videoSyncOffsetSeconds;
```

- Push `previewSecond` through `setSelectedSecondTransient`, but throttle to the effective preview FPS so widget rendering does not exceed the overlay update rate.
- Cancel callbacks on pause, unmount, source change, or background mode change.

#### [MODIFY] `app/src/hooks/useVideoPreview.js`

Use `useVideoPlaybackClock` during video playback mode.

### Manual Tests

1. Add metric widgets and play imported video - widgets advance in sync with video.
2. Set update rate lower than full FPS - widgets update at reduced rate while video remains smooth.
3. Hide widgets - video performance remains smooth.
4. Add route/elevation widgets - playback remains usable, with any remaining performance issue isolated to widget rendering rather than video decode.

---

## Phase 4 - Keep Overlay-Only Playback Behavior Intact

**Goal**: Do not regress the existing preview mode when no imported video is active.

### Deliverables

#### [MODIFY] `app/src/components/OverlayPlayer.jsx`

Split play behavior:

- If `backgroundMode === 'video'` and `importedVideoPath` exists:
  - dispatch video-clock playback state,
  - do not run the old `requestAnimationFrame` playhead loop.
- Otherwise:
  - keep the existing overlay-only loop.

This likely requires passing background mode to `OverlayPlayer`, or storing editor background mode centrally enough for the player to know whether video playback is visible.

The play/pause/reset buttons should operate on the shared playback state rather than only local `isPlaying`.

### Manual Tests

1. Black/checker/white background with imported video still selected - overlay-only preview behaves as before.
2. Video background - video-clock playback is used.
3. Switching away from video background pauses the video and leaves the playhead at the correct second.
4. Switching back to video background seeks the video to the current playhead while paused.

---

## Phase 5 - Fix Timeline Scrubbing Semantics

**Goal**: Make scrubbing intentional and remove the broken `isUpdatingFromTimeline` read.

### Deliverables

#### [MODIFY] `app/src/hooks/useVideoPreview.js`

Remove the subscription to:

```js
state.isUpdatingFromTimeline;
```

This state does not exist in the Zustand store. Replace it with explicit playback state:

```js
previewPlaybackState === "scrubbing";
```

#### [MODIFY] `app/src/components/OverlayPlayer.jsx`

Use the new actions:

- `beginPreviewScrub`
- `updatePreviewScrub`
- `commitPreviewScrub`

During scrubbing:

- pause native video playback,
- seek video to the scrubbed time,
- update widgets from the scrubbed preview second,
- resume only if the user explicitly presses play again.

### Manual Tests

1. Dragging the slider updates video and widgets.
2. Releasing the slider commits the chosen time.
3. Scrubbing does not leave the video in a hidden playing state.
4. Rapid scrubbing may be choppy, but normal playback after scrubbing is smooth.

---

## Phase 6 - Performance Verification

**Goal**: Prove the root problem is gone and catch regressions.

### Deliverables

#### [NEW] Lightweight Development Instrumentation

Add optional dev-only logging behind a localStorage flag:

```js
localStorage.setItem("ovrley:preview-perf", "true");
```

Track:

- number of `video.currentTime` assignments per second,
- video frame callbacks per second,
- transient `selectedSecond` updates per second,
- React preview updates per second.

Expected target during video playback:

- `video.currentTime` assignments: near zero after playback starts.
- video frame callbacks: roughly source video FPS.
- transient selected-second updates: capped to effective preview FPS.

### Manual Tests

1. Enable the perf flag and play 1080p for 60 seconds.
2. Enable the perf flag and play 4K for 60 seconds.
3. Confirm no continuous stream of `currentTime` assignments during playback.
4. Confirm scrubbing does create seeks, but playback does not.
5. Compare GPU usage before and after the change.

---

## Acceptance Criteria

- Imported video preview uses native `<video>` playback during play.
- `video.currentTime` is not assigned every frame.
- 1080p playback remains smooth for at least 60 seconds with no widgets.
- 4K playback does not collapse to near-1fps after a few seconds with no widgets.
- Widgets remain synchronized to video time.
- Existing overlay-only preview still works without an imported video or when video background is not active.
- Timeline scrubbing and reset remain predictable.
- No changes are made to the transparent export pipeline.

---

## Implementation Notes

- Do not use canvas readback or `drawImage(video, ...)` for editor preview unless there is a separate compositing reason. A DOM `<video>` element is the fastest preview path.
- Do not use React state for every decoded video frame unless the update is intentionally throttled.
- `requestVideoFrameCallback` gives the best synchronization to actual decoded video frames. Keep a fallback because not every WebView/runtime guarantees it.
- Treat 4K preview as a decode/display workload. It may still use significant GPU, but it should be continuous decode rather than repeated random seeks.
- Keep export/compositing work separate. Preview playback should not depend on the ffmpeg render path.

---

## Clean up
