# MP4 Activity Mini Plan

Goal: wire MP4 embedded telemetry into the existing single-activity model with
minimal code churn.

This is not dual-activity support. There is still only one active
`parsedActivity`.

## Important Boundary

This plan only concerns the embedded telemetry portion of an MP4.

Do not replace, remove, or reinterpret the existing imported-video metadata
flow. The render process still needs the current video metadata fields,
including:

- `importedVideoPath`
- `importedVideoDuration`
- `importedVideoFps`
- `importedVideoFpsNum`
- `importedVideoFpsDen`
- `importedVideoResolution`
- `importedVideoCreationTime`
- `importedVideoImportId`
- `importedVideoPreviewUrl`
- `videoSyncOffsetSeconds`
- preview warnings/errors

MP4 telemetry extraction is an additional optional step after video import. It
must not block video preview, metadata probing, sync calculation, or composite
render setup.

When MP4 telemetry is the active `parsedActivity`, it is telemetry from the same
video, not an external activity to synchronize against. In that state,
`videoSyncOffsetSeconds` should stay or become `0`. Keep the existing
activity-to-video sync behavior for real FIT/GPX/SRT activity imports.

The existing video metadata and sync fields remain owned by video import. MP4
telemetry must not replace or shortcut that metadata path.

## Behavior

- When a video is imported, try to extract MP4 telemetry.
- If extraction returns a `ParsedActivity`, store it as the current
  `parsedActivity` only when no external activity is loaded.
- If a FIT/GPX/SRT activity is imported later, it overwrites `parsedActivity`
  with its own data.
- If that external activity is cleared while MP4 telemetry is still available,
  restore the MP4 telemetry back into `parsedActivity`.
- If the video is cleared, remove the stored MP4 telemetry. If no external
  activity is loaded, also clear `parsedActivity`.
- If the MP4 has no telemetry, keep video import successful and leave
  `parsedActivity` unchanged.

## Store Shape

Add only the minimum state needed:

```js
parsedActivity: null,
activitySummary: null,
parsedActivitySource: null, // 'activity-file' | 'video-telemetry' | null
hiddenVideoParsedActivity: null,
```

`parsedActivity` remains the current render/preview payload.

Do not store a second copy of the active activity-file telemetry. When a
FIT/GPX/SRT activity is active, it lives only in `parsedActivity`.

`hiddenVideoParsedActivity` stores MP4 telemetry only while it is hidden behind
an active FIT/GPX/SRT activity. When MP4 telemetry itself is active, it lives in
`parsedActivity` and `hiddenVideoParsedActivity` should be `null`.

`parsedActivitySource` records which source currently owns `parsedActivity`.

This means there is no steady-state duplicate active payload:

- video-only: `parsedActivity` is MP4 telemetry,
  `hiddenVideoParsedActivity = null`;
- activity-file only: `parsedActivity` is FIT/GPX/SRT telemetry,
  `hiddenVideoParsedActivity = null`;
- activity-file plus imported MP4 telemetry: `parsedActivity` is FIT/GPX/SRT
  telemetry, `hiddenVideoParsedActivity` is the MP4 telemetry that can be
  restored if the activity file is cleared.

## Store Action Placement

Put the new parsed-activity ownership state and actions in
`app/src/store/slices/createMediaSlice.js`, because that slice already owns
`parsedActivity`, `activitySummary`, and `activityFilename`.

Add explicit actions instead of requiring feature hooks to mutate several fields
manually:

```js
activateActivityFile(activity)
loadVideoTelemetry(activity)
clearActivityFile(options)
clearVideoTelemetry()
```

Expected ownership behavior:

- `activateActivityFile(activity)`
  - if `parsedActivitySource === 'video-telemetry'`, moves the current
    `parsedActivity` into `hiddenVideoParsedActivity` before replacing it;
  - sets `parsedActivity`;
  - sets `parsedActivitySource = 'activity-file'`;
  - updates `activitySummary`;
  - runs the existing activity-to-video sync calculation.

- `loadVideoTelemetry(activity)`
  - if `parsedActivitySource === 'activity-file'`, stores the payload in
    `hiddenVideoParsedActivity` and leaves the active external activity alone;
  - otherwise sets `parsedActivity` to the MP4 telemetry;
  - when it activates MP4 telemetry, sets
    `parsedActivitySource = 'video-telemetry'`, keeps
    `hiddenVideoParsedActivity = null`, updates `activitySummary` without
    running activity-to-video sync, sets `activityFilename = null`, sets
    `videoSyncOffsetSeconds = 0`, and clears `videoSyncWarning`.

- `clearActivityFile({ restoreVideoTelemetry = true, clearFilename = true } = {})`
  - only affects an active activity-file payload;
  - clears `activityFilename` when `clearFilename` is true;
  - if `restoreVideoTelemetry` and `hiddenVideoParsedActivity` exists, moves
    `hiddenVideoParsedActivity` into `parsedActivity`, clears the hidden field,
    and marks `parsedActivitySource = 'video-telemetry'`;
  - otherwise clears `parsedActivity`, `activitySummary`, and
    `parsedActivitySource`.

- `clearVideoTelemetry()`
  - clears `hiddenVideoParsedActivity`;
  - if `parsedActivitySource === 'video-telemetry'`, clears `parsedActivity`,
    `activitySummary`, and `parsedActivitySource`;
  - if `parsedActivitySource === 'activity-file'`, leaves `parsedActivity`
    alone.

`createVideoImportSlice.js` can call these media-slice actions through `get()`.
Do not duplicate the parsed-activity ownership state in the video slice.

Keep legacy actions as compatibility wrappers while migrating call sites:

```js
setParsedActivity(activity)
clearActivitySummary()
```

`setParsedActivity(activity)` can remain a direct low-level setter for now if
tests still use it, but new import code should prefer `activateActivityFile` or
`loadVideoTelemetry`.

`clearActivitySummary()` should delegate to:

```js
get().clearActivityFile({
  restoreVideoTelemetry: false,
  clearFilename: false,
})
```

This preserves the current import flow's expectation that calling
`clearActivitySummary()` before parsing a replacement activity does not restore
MP4 telemetry as an intermediate active activity.

## Activity Summary And Sync

Do not call the current `setActivitySummary(activity)` blindly for MP4-derived
telemetry.

Current code calls `computeVideoSync(summary)` after every summary update. That
is correct for external FIT/GPX/SRT activity, but wrong for telemetry extracted
from the same MP4.

Implementation options:

1. Change `setActivitySummary` to accept an option:

```js
setActivitySummary(activity, { computeVideoSync = true } = {})
```

Then MP4 activation calls:

```js
setActivitySummary(videoParsedActivity, { computeVideoSync: false })
```

2. Or extract a pure `buildActivitySummary(activity)` helper and have
   `loadVideoTelemetry` write `activitySummary` directly without invoking
   `computeVideoSync`.

Either option is fine. The required behavior is:

- external activity summary updates may compute video sync;
- MP4 telemetry summary updates must not compute external-activity video sync;
- when MP4 telemetry is active, set `videoSyncOffsetSeconds` to `0` and
  `videoSyncWarning` to `null`.

Also update video-sync recomputation after `setImportedVideo(metadata)`.

Prefer adding a media-slice action such as:

```js
syncVideoMetadataWithActiveActivity()
```

Then `createVideoImportSlice.js` can call
`get().syncVideoMetadataWithActiveActivity()` after storing imported video
metadata instead of directly inspecting media-slice state.

The action should behave as follows:

- if `parsedActivitySource === 'activity-file'`, keep the existing
  `computeVideoSync(activitySummary)` behavior;
- if there is no external activity or video telemetry is active, do not compute
  sync against `activitySummary`; keep/reset `videoSyncOffsetSeconds = 0` and
  `videoSyncWarning = null`.

## Frontend API

File: `app/src/api/backend.js`

Add:

```js
export async function extractVideoTelemetry(filePath) {
  return apiCall('backend_extract_video_telemetry', { filePath })
}
```

Tauri argument casing matters: Rust receives `file_path`, JS passes `filePath`.

The backend already returns either a `ParsedActivity` JSON value or `null`.

This is an actual required file edit, not just an API shape note.

## Video Import

File: `app/src/features/video-preview/hooks/useVideoImport.js`

After `importPreviewVideo(selected)` succeeds and `setImportedVideo(metadata)`
has run:

1. Start `extractVideoTelemetry(selected)`.
2. If it returns a payload:
   - call `loadVideoTelemetry(payload)`;
   - if no activity file is active, this makes the MP4 telemetry active via
     `parsedActivity`;
   - if an activity file is active, this stores the MP4 telemetry in
     `hiddenVideoParsedActivity` and does not touch the active
     `parsedActivity` or `activitySummary`;
   - when MP4 telemetry becomes active, update `activitySummary` from that
     payload so existing render/readiness code keeps working;
   - when MP4 telemetry becomes active, do not run external-activity video sync;
     the active video telemetry offset is `0`.
3. If it returns `null`, leave the video import successful and do not activate
   video telemetry. Previous MP4 telemetry should already have been cleared at
   the start of this video import.
4. If it throws, log a warning and keep the video import successful.

Do not await telemetry extraction inside the same loading phase that controls
`importingVideo`.

Current `useVideoImport.js` sets `setImportingVideo(true)` before preview import
and `setImportingVideo(false)` in `finally`. If telemetry extraction is simply
added as another awaited call inside that `try`, the spinner will stay up during
telemetry extraction. Avoid that.

Use one of these structures:

- Commit preview metadata, set background mode, then start telemetry extraction
  as fire-and-forget with `void extractAndStoreVideoTelemetry(selected)`.
- Or split preview import and telemetry extraction into separate async helpers
  so `setImportingVideo(false)` runs immediately after preview import finishes.

Re-importing a video, including the same path, should clear the previous
MP4 telemetry state with `clearVideoTelemetry()` before extraction starts and
attempt extraction again after the new preview import succeeds. Do not reuse
stale telemetry across video imports.

## Activity Import

File: `app/src/lib/activity/import-activity.js`

When FIT/GPX/SRT import succeeds:

1. Call `activateActivityFile(parsedActivity)`.
2. This sets `parsedActivity` to the activity-file payload.
3. This sets `parsedActivitySource = 'activity-file'`.
4. This preserves MP4 telemetry in `hiddenVideoParsedActivity` if MP4 telemetry
   was active before the activity import.
5. This updates `activitySummary`.
6. Keep the existing video metadata sync behavior for this external activity.

This intentionally overrides MP4 telemetry.

The current import flow calls `store.clearActivitySummary()` before parsing. Do
not use the new explicit activity-clear action with its default restore behavior
at the start of import, because that would briefly restore MP4 telemetry and
then overwrite it again.

Use one of these approaches:

- parse first, then commit with `activateActivityFile(parsedActivity)`;
- or call `clearActivityFile({ restoreVideoTelemetry: false, clearFilename: false })`
  before parsing.

The net result after a successful import must be external activity active.

## Activity Clear

File: `app/src/store/slices/createMediaSlice.js`

Change the clear activity action so it clears only external activity first:

1. If `parsedActivitySource === 'activity-file'`, clear the active activity-file
   payload from `parsedActivity`.
2. Clear `activityFilename` for explicit user-initiated activity clear. This is
   a deliberate addition; the current `clearActivitySummary` does not do it.
3. If `hiddenVideoParsedActivity` exists:
   - move `hiddenVideoParsedActivity` into `parsedActivity`;
   - clear `hiddenVideoParsedActivity`;
   - set `parsedActivitySource` to `'video-telemetry'`;
   - set `activityFilename` to `null`;
   - set `activitySummary` from the video telemetry without computing
     activity-to-video sync;
   - set `videoSyncOffsetSeconds = 0`;
   - clear `videoSyncWarning`.
4. Otherwise:
   - set `parsedActivity` to `null`;
   - set `parsedActivitySource` to `null`;
   - set `activitySummary` to `null`.

## Video Clear

File: `app/src/store/slices/createVideoImportSlice.js`

When the imported video is cleared:

1. Call `get().clearVideoTelemetry()` from the existing `clearImportedVideo`
   action.
2. If `parsedActivitySource === 'video-telemetry'`:
   - set `parsedActivity` to `null`;
   - set `parsedActivitySource` to `null`;
   - set `activitySummary` to `null`.
3. If `parsedActivitySource === 'activity-file'`, leave `parsedActivity`
   alone.

## Summary Derivation

Reuse the existing summary shape, but not the unconditional sync side effect for
MP4 telemetry.

For this mini plan, do not introduce `videoTelemetrySummary`,
`telemetrySessionsSummary`, or `metricSourceMap`.

The MP4 telemetry is pretending to be the one active activity when no external
activity exists. That is the point of this minimized design.

## Tests

Add focused frontend tests:

- MP4 import with telemetry sets `parsedActivity` when no external activity
  exists.
- MP4 import with telemetry stores `hiddenVideoParsedActivity` when an activity
  file is active.
- MP4 import without telemetry does not change `parsedActivity`.
- MP4 telemetry extraction failure does not fail video import.
- MP4 telemetry activation does not call external-activity sync calculation,
  leaves `videoSyncOffsetSeconds` at `0`, and clears `videoSyncWarning`.
- Video preview loading state ends after preview metadata import, not after
  telemetry extraction.
- Re-importing a video clears previous MP4 telemetry and extracts telemetry for
  the new import, even when the selected path is the same.
- Activity import overwrites active MP4 telemetry.
- Activity import still runs the existing video metadata sync behavior when an
  imported video is present.
- Beginning an activity import does not briefly restore MP4 telemetry through
  the explicit activity-clear action.
- Clearing external activity restores MP4 telemetry as `parsedActivity`.
- Clearing video removes MP4 telemetry and clears `parsedActivity` only when
  video telemetry is active.
- Clearing video does not disturb an active external activity.
- `backend.extractVideoTelemetry(filePath)` invokes
  `backend_extract_video_telemetry` with `{ filePath }`.

## Out Of Scope

- No dual telemetry.
- No source map.
- No metric picker.
- No per-metric source routing.
- No route/elevation source split.
- No `extended_attributes` cleanup.
- No backend dense synthesis changes.

## Acceptance Criteria

- Existing activity-only workflows still use `parsedActivity` as before.
- Imported MP4 telemetry can power the existing preview/render path as a normal
  `ParsedActivity`.
- External activity import always takes precedence over MP4 telemetry.
- Removing the external activity restores MP4 telemetry if it is still available.
