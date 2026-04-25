# Bug Fix Implementation Plan

## Scope

This plan covers the seven reported issues across:

- frontend render lifecycle and export UI
- Rust backend render progress / FFmpeg encode configuration
- frontend preview interpolation and timeline behavior
- Rust route/elevation progress math for trimmed exports
- frontend activity parsing for FIT/GPX idle gaps

Where the current code already shows the failure mechanism, it is called out explicitly. Where the root cause is not yet proven from code alone, the plan uses a short diagnostic step before the minimal fix.

## 1. Second render opens the previous video instead of showing the progress overlay

### Current codepath

- [app/src/api/renderVideo.jsx](/h:/tools/cyclemetry/app/src/api/renderVideo.jsx)
- [app/src/App.jsx](/h:/tools/cyclemetry/app/src/App.jsx)
- [src-tauri/cyclemetry_core/src/encode/video.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/video.rs)

### Observed cause in current code

The frontend sets `renderingVideo=true` before `backend.renderVideo()` finishes starting the new render. While that request is still in flight, the polling effect in `App.jsx` immediately calls `backend_progress`. If the backend controller still contains the previous render’s terminal state (`status === "complete"` and previous `filename`), the frontend effect treats that as the new render finishing and calls `backend.openVideo(filename)`. The backend render then continues in the background because it actually did start.

### Implementation steps

1. Introduce a render-session guard between “user clicked Render” and “this progress belongs to the active render”.
2. Preferred fix: extend the Rust `RenderController` progress payload with a monotonically increasing `render_id`.
3. Increment `render_id` inside `try_start()` in [src-tauri/cyclemetry_core/src/encode/video.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/video.rs) when a new render is accepted.
4. Return that `render_id` from `backend_render()` in [src-tauri/cyclemetry_core/src/commands/mod.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/commands/mod.rs).
5. Store `activeRenderId` in the frontend store and only accept polled progress/open-video completion events when `renderProgress.render_id === activeRenderId`.
6. Clear any terminal progress fields before enabling polling for a new render:
   `status`, `filename`, `encoded`, `current`, `total`, `estimatedSecondsRemaining`.
7. Prevent `backend.openVideo()` from firing on stale progress payloads by making the completion effect depend on `render_id` equality.
8. Keep the existing overlay behavior; only fix the lifecycle/state ownership.

### Validation

- Start render A, wait for completion, start render B immediately.
- Confirm the overlay stays visible for render B and no previous file opens.
- Confirm only the new render completion opens a file.
- Confirm cancel/retry still works.

## 2. `prores_ks_vulkan` color mismatch, and default `mbs_per_slice` must be `2`

### Current codepath

- [src-tauri/cyclemetry_core/src/encode/ffmpeg.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/ffmpeg.rs)
- [src-tauri/cyclemetry_core/src/encode/video.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/video.rs)

### Constraints

- Do not make any other `prores_ks_vulkan` changes besides:
  - fixing the color mismatch
  - defaulting `mbs_per_slice` to `2`

### What is currently known

The Vulkan path currently uses:

- raw input pix fmt: `rgba`
- FFmpeg input pix fmt in settings: `vulkan`
- filter chain: `hwupload,scale_vulkan=format=yuva444p10le:out_range=tv`

The reported channel shifts are large enough to require a precise format/range diagnosis before changing the encode path.

### Implementation steps

1. Add a short diagnostic comparison for `prores_ks_vulkan` only:
   - render a controlled frame with white, turquoise, red, and transparency
   - export once with CPU `prores_ks`
   - export once with `prores_ks_vulkan`
   - inspect decoded PNG frames from both outputs
2. Verify whether the mismatch comes from:
   - channel ordering (`rgba` vs `bgra`)
   - `scale_vulkan` conversion to `yuva444p10le`
   - range conversion (`out_range=tv`)
3. Apply the minimal fix in [src-tauri/cyclemetry_core/src/encode/ffmpeg.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/ffmpeg.rs) only after that comparison proves which of those is wrong.
4. Set default `-mbs_per_slice 2` for `prores_ks_vulkan` when the config does not specify it.
5. Keep all other Vulkan defaults unchanged.

### Validation

- White remains white.
- Turquoise remains turquoise.
- Red remains red.
- Alpha output and container/profile parity remain unchanged.
- Explicitly verify that `mbs_per_slice` is `2` by default and still overridable from config.

## 3. Remove VP9 everywhere, and grey out VideoToolbox on Windows

### Current codepath

- [app/src/components/SidebarSettingsTab.jsx](/h:/tools/cyclemetry/app/src/components/SidebarSettingsTab.jsx)
- [app/src/api/renderVideo.jsx](/h:/tools/cyclemetry/app/src/api/renderVideo.jsx)
- [app/src/store/slices/createTemplateSlice.js](/h:/tools/cyclemetry/app/src/store/slices/createTemplateSlice.js)
- [src-tauri/cyclemetry_core/src/encode/ffmpeg.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/ffmpeg.rs)

### Implementation steps

1. Remove `libvpx-vp9` from the frontend codec select in [app/src/components/SidebarSettingsTab.jsx](/h:/tools/cyclemetry/app/src/components/SidebarSettingsTab.jsx).
2. Remove the VP9-specific frontend override branch in [app/src/api/renderVideo.jsx](/h:/tools/cyclemetry/app/src/api/renderVideo.jsx).
3. Remove `libvpx-vp9` support from the Rust FFmpeg settings builder in [src-tauri/cyclemetry_core/src/encode/ffmpeg.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/encode/ffmpeg.rs).
4. Update the unsupported-codec error message to remove VP9.
5. Add an explicit frontend platform capability source for codec UI gating.
6. Preferred implementation: add a small Tauri command that returns the current OS (`windows`, `macos`, `linux`) and hydrate that into the frontend once at startup.
7. Use that flag to render `prores_videotoolbox` as disabled/greyed out on Windows.
8. Keep VideoToolbox enabled on macOS only; leave Linux behavior intentionally disabled unless separately requested.

### Validation

- VP9 no longer appears in the codec UI.
- Backend rejects any stale template/store value that still requests VP9 with a clear unsupported-codec error.
- On Windows, VideoToolbox is visible but disabled and not selectable.

## 4. Export range incorrectly affects the frontend preview player

### Current codepath

- [app/src/components/overlay-editor/useOverlayEditorState.js](/h:/tools/cyclemetry/app/src/components/overlay-editor/useOverlayEditorState.js)
- [app/src/components/OverlayPlayer.jsx](/h:/tools/cyclemetry/app/src/components/OverlayPlayer.jsx)
- [app/src/components/overlay-editor/previewInterpolation.js](/h:/tools/cyclemetry/app/src/components/overlay-editor/previewInterpolation.js)
- [app/src/components/overlay-editor/WidgetPreview.jsx](/h:/tools/cyclemetry/app/src/components/overlay-editor/WidgetPreview.jsx)

### Observed cause in current code

The editor preview clamps `previewSecond` to `scene.start/scene.end`, and preview interpolation also builds frame data over a caller-supplied start/end window. That couples export trimming to the editor/player preview even though the player should represent the full activity.

### Implementation steps

1. Define two separate concepts in the frontend:
   - `previewSecond`: full-activity editor/player time
   - `exportRange`: render-only trim window
2. Remove the `scene.start/scene.end` clamp from `previewSecond` calculation in [app/src/components/overlay-editor/useOverlayEditorState.js](/h:/tools/cyclemetry/app/src/components/overlay-editor/useOverlayEditorState.js).
3. Make the player duration source the full parsed activity duration, not the export window.
4. Update preview interpolation calls so widget preview data is built against the full activity timeline for editor playback.
5. Keep export-range handling only in [app/src/api/renderVideo.jsx](/h:/tools/cyclemetry/app/src/api/renderVideo.jsx) when constructing the backend render payload.
6. Audit any component still reading `scene.start/scene.end` as if it were a UI playback range and replace that with full-activity duration.

### Validation

- Set a custom export range.
- Scrub and play outside that range in the editor.
- Confirm route, elevation, and metrics keep updating across the full activity.
- Confirm exported output still respects the custom range.

## 5. Trimmed export uses full route/elevation geometry, but reveal/progress is wrongly normalized to the clip duration

### Current codepath

- [src-tauri/cyclemetry_core/src/activity/trim.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/activity/trim.rs)
- [src-tauri/cyclemetry_core/src/activity/interpolate.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/activity/interpolate.rs)
- [src-tauri/cyclemetry_core/src/render/widgets/common.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/common.rs)
- [src-tauri/cyclemetry_core/src/render/widgets/route.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/route.rs)
- [src-tauri/cyclemetry_core/src/render/widgets/elevation.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/elevation.rs)

### Observed cause in current code

`trim_activity()` renormalizes `sample_distance_progress` to the trimmed span:

- start of clip becomes `0`
- end of clip becomes `1`

That is correct for “clip-local progress”, but the route/elevation widgets are built from full-route/full-profile geometry and should reveal only the real distance progress covered by the trimmed section. The current normalization makes the whole route/profile appear to finish within the shorter clip.

### Implementation steps

1. Split progress semantics in the Rust activity pipeline into two explicit series:
   - `absolute_distance_progress`: progress along the full activity, always in full-activity 0..1 space
   - `clip_elapsed_seconds`: time inside the exported clip, 0..clipDuration
2. Stop renormalizing route/elevation reveal progress in [src-tauri/cyclemetry_core/src/activity/trim.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/activity/trim.rs) for widget use.
3. Preserve the trimmed window in elapsed time, but keep distance progress values in full-activity coordinates.
4. Update `DenseActivityReport` / trimmed schema so route/elevation code can consume full-activity progress directly.
5. Keep full geometry generation from `activity.sample_course_points` / `activity.sample_elevations` exactly as today.
6. Update `frame_progress_values()` in [src-tauri/cyclemetry_core/src/render/widgets/common.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/common.rs) so route/elevation reveal uses the absolute full-activity progress for each exported frame.
7. Verify marker interpolation and completed-segment construction in both widget renderers against the new absolute-progress series.

### Validation

- Export a 30-second mid-activity clip.
- Confirm the full map route and full elevation profile are visible as context.
- Confirm the completed route/profile only advances through the real slice covered by those 30 seconds.
- Confirm the marker starts at the correct mid-route point and ends at the correct later point instead of completing the full route.

## 6. Elevation profile looks much worse than the map route

### Current codepath

- Route: [src-tauri/cyclemetry_core/src/render/widgets/route.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/route.rs)
- Elevation: [src-tauri/cyclemetry_core/src/render/widgets/elevation.rs](/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/elevation.rs)

### What the current code shows

This is not caused by route smoothing. The route path currently:

- fits all valid route samples into the widget
- then simplifies with a 1 px Ramer-Douglas-Peucker-style tolerance

The elevation path currently:

- converts profile samples to `(distance_progress, elevation)`
- downsamples aggressively to roughly `plot.width * 1.0`
- keeps min/max points per bucket
- then renders that reduced polyline

That min/max bucketing is the main reason the elevation profile looks harsher than the route despite the route having fewer visible points.

### Implementation steps

1. Keep the route renderer unchanged unless a later comparison proves it needs adjustment.
2. Improve elevation geometry generation only:
   - replace the current min/max bucket downsampler with a quality-preserving simplifier
   - preferred options: Largest-Triangle-Three-Buckets or distance-aware RDP in profile space
3. Preserve extrema where needed so steep climbs/descents do not disappear.
4. Keep the marker/reveal logic independent from the geometry simplification method.
5. Add a debug comparison artifact path so the old and new elevation geometry can be compared on the same activity/template.
6. Ensure that the preview displays the same data/matches what will be rendered.

### Validation

- Compare old/new elevation profile screenshots on the same template.
- Confirm the profile edge quality and curvature are visibly improved.
- Confirm total point count remains bounded enough for performance.

## 7. Zero-filling idle gaps in FIT/GPX so overlay timing stays aligned with the video

### Current codepath

- FIT parse: [app/src/api/fitParserUtils.js](/h:/tools/cyclemetry/app/src/api/fitParserUtils.js)
- GPX parse: [app/src/api/gpxUtils.jsx](/h:/tools/cyclemetry/app/src/api/gpxUtils.jsx)
- Activity finalization: [app/src/api/activityParserUtils.js](/h:/tools/cyclemetry/app/src/api/activityParserUtils.js)

### What is currently known

The parser is frontend-side right now. The Rust backend only consumes the already parsed `parsedActivity` payload. That means zero-fill must be added in the frontend parse/finalization pipeline, not in Rust first.

The current activity builder derives elapsed time from explicit `elapsedSeconds` or timestamps, but it does not insert synthetic idle samples when there are long gaps with no recorded points. That can let metric/route progression jump ahead while the source video continues through idle periods.

### Implementation steps

1. Add a gap-detection pass before `finalizeParsedActivity()` completes the parsed payload.
2. Use timestamp/elapsed-time deltas to detect missing idle spans larger than the normal recording interval.
3. Insert synthetic samples for those idle spans with:
   - carried-forward position/course point
   - carried-forward cumulative distance
   - zero values for motion-derived metrics where appropriate (`speed`, likely `cadence`, likely `power`, likely `gradient` only if explicitly desired after rules review)
   - carried-forward values for non-motion metrics where zero would be wrong (`temperature`, `heartrate`, etc.)
4. Make the fill rules explicit per metric instead of applying one blanket rule to every series.
5. Ensure `sample_elapsed_seconds`, `sample_distance_progress`, `course`, and `elevation` all include those synthetic idle samples so marker movement respects the pause.
6. Re-run derived-series generation after insertion so speed/pace/vertical-speed stay aligned with the final timebase.
7. Persist enough debug info in the parse debug payload to show:
   - detected gaps
   - inserted sample count
   - timestamps of inserted ranges

### Validation

- Create or use a FIT/GPX sample with a known pause where the video continues recording.
- Confirm route marker stays stationary during the idle gap.
- Confirm elevation progress also pauses.
- Confirm playback/export timing remains aligned after the pause.
- Confirm no false idle insertion on normal sparse-but-moving data.

## Recommended implementation order

1. Fix render-session ownership bug for repeated renders.
2. Decouple preview/player time from export range.
3. Fix trimmed export progress semantics in Rust.
4. Remove VP9 and gate VideoToolbox by platform.
5. Fix `prores_ks_vulkan` color path and default `mbs_per_slice`.
6. Improve elevation geometry quality.
7. Add idle-gap zero-fill in parsed activity generation.

## Verification checklist

- Repeated render start no longer opens stale output.
- Export range affects only final export, not editor/player preview.
- Mid-activity trimmed exports show full route/profile context but only partial real progress.
- VP9 is gone from both UI and backend.
- VideoToolbox is disabled on Windows.
- `prores_ks_vulkan` colors match CPU ProRes and defaults `mbs_per_slice=2`.
- Elevation profile quality is visibly improved without a route regression.
- Idle gaps remain time-aligned and freeze route/elevation progress correctly.

## Clarifying questions

No blocking questions at this stage. The current codebase is specific enough to plan the fixes without guessing implementation ownership or behavior.
