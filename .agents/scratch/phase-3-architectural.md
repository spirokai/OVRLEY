# Phase 3 — Architectural Changes

**Goal:** Resolve data-flow anti-patterns, extract shared patterns, and remove direct store access from non-React utilities. These changes affect how data flows through the app.

**Risk:** Higher — requires behavioral verification beyond unit tests
**Estimated time:** 1.5–2 hours

---

## Acceptance Criteria

- [ ] `npm run lint` passes with zero errors
- [ ] `npm run test` — all test suites pass
- [ ] `npm run build` succeeds
- [ ] Manual verification: import a GPX/FIT file, add widgets, render video (spot-check)
- [ ] No `useStore.getState()` calls outside of hooks and store slices (except in `test-setup.js` and tests)

---

## Step 1: Remove `useStore.getState()` from `render-video.js` utility

### Problem

`src/features/render-video/utils/render-video.js` calls `useStore.getState()` in a non-React utility function. The caller (`useRenderWorkflow.handleRenderVideoConfirm`) already has access to all store state.

### Action

**Change the signature of `renderVideo()`:**

Currently:
```js
export default async function renderVideo(overrides = {}) {
  const {
    availableCodecs,
    config: baseConfig,
    exportCodec,
    exportRange,
    globalDefaults,
    importedVideoDuration,
    importedVideoFps,
    importedVideoFpsDen,
    importedVideoFpsNum,
    importedVideoPath,
    setActiveRenderId,
    setRenderingVideo,
    setRenderProgress,
    updateRate,
    videoSyncOffsetSeconds,
  } = useStore.getState()
  const parsedActivity = useStore.getState().parsedActivity
  ...
}
```

Changed to:
```js
export default async function renderVideo(overrides = {}) {
  const {
    availableCodecs,
    config: baseConfig,
    exportCodec,
    exportRange,
    globalDefaults,
    importedVideoDuration,
    importedVideoFps,
    importedVideoFpsDen,
    importedVideoFpsNum,
    importedVideoPath,
    parsedActivity,
    setActiveRenderId,
    setRenderingVideo,
    setRenderProgress,
    updateRate,
    videoSyncOffsetSeconds,
  } = overrides
  ...
}
```

**Also fix the error handler** at line ~130 which calls `useStore.getState()`:
```js
// Old:
const { setActiveRenderId, setRenderingVideo, setRenderProgress } = useStore.getState()

// New: pass these in via overrides
```

**Update the caller** in `useRenderWorkflow.js`:
```js
const result = await renderVideo({
  config: nextConfig,
  updateRate: nextUpdateRate,
  exportRange: nextExportRange,
  exportCodec: renderSettingsDraft.exportCodec,
  exportBitrate: renderSettingsDraft.exportBitrate,
  // Add the store state it now needs:
  availableCodecs: useStore.getState().availableCodecs,
  globalDefaults,
  importedVideoDuration: useStore.getState().importedVideoDuration,
  importedVideoFps: useStore.getState().importedVideoFps,
  importedVideoFpsDen: useStore.getState().importedVideoFpsDen,
  importedVideoFpsNum: useStore.getState().importedVideoFpsNum,
  importedVideoPath: useStore.getState().importedVideoPath,
  parsedActivity: useStore.getState().parsedActivity,
  videoSyncOffsetSeconds: useStore.getState().videoSyncOffsetSeconds,
  setActiveRenderId,
  setRenderingVideo,
  setRenderProgress,
})
```

**Note:** The caller still uses `useStore.getState()` but it's inside a React hook (`useRenderWorkflow`), which is acceptable. The key change is that the utility function no longer reaches into the store itself.

### Affected Files
- `src/features/render-video/utils/render-video.js` — accept all state via params, remove `useStore` import
- `src/features/render-video/hooks/useRenderWorkflow.js` — pass additional params

---

## Step 2: Remove `useStore.getState()` from `import-activity.js`

### Problem

`saveFile` in `src/lib/activity/import-activity.js` falls back to `useStore.getState()` when `storeActions` is not passed. The only caller (`useActivityImport`) already passes it.

### Action

1. In `import-activity.js`:
   - Remove `import useStore from '@/store/useStore'`
   - Make `storeActions` required (remove fallback)
   ```js
   // Old: const store = storeActions || useStore.getState()
   // New: const store = storeActions
   ```
2. In `useActivityImport.js` — already passes `useStore.getState()`, no change needed.

### Affected Files
- `src/lib/activity/import-activity.js` — remove Zustand import, make storeActions required
- No changes to callers (they already pass storeActions)

---

## Step 3: Extract `useFpsMode` hook

### Problem

`useRenderVideoDialogState.js` and `useSceneSettingsState.js` both implement identical FPS mode logic:
- `customFpsAnchor` state
- `fpsMode` derivation
- `handleFpsModeChange(callback)`
- `handleCustomFpsChange(callback)`

### Action

Create a new shared hook: `src/hooks/useFpsMode.js` (or `src/lib/useFpsMode.js`)

```js
import { useState, useCallback } from 'react'
import { getFpsModeValue, sanitizeIntegerFps, PRESET_FPS_VALUES } from '@/lib/update-rate'

export function useFpsMode({ fps, onFpsChange, onUpdateRateChange, updateRate }) {
  const [customFpsAnchor, setCustomFpsAnchor] = useState(null)
  const fpsMode = customFpsAnchor !== null && Number(fps) === customFpsAnchor ? 'custom' : getFpsModeValue(fps)

  const handleFpsModeChange = useCallback((value) => {
    if (value === 'custom') {
      setCustomFpsAnchor(Number(fps))
      return
    }
    setCustomFpsAnchor(null)
    const nextFps = sanitizeIntegerFps(value)
    onFpsChange(nextFps)
  }, [fps, onFpsChange])

  const handleCustomFpsChange = useCallback((rawValue) => {
    const nextFps = sanitizeIntegerFps(rawValue)
    setCustomFpsAnchor(PRESET_FPS_VALUES.includes(nextFps) ? null : nextFps)
    onFpsChange(nextFps)
  }, [onFpsChange])

  return { fpsMode, handleFpsModeChange, handleCustomFpsChange, customFpsAnchor, setCustomFpsAnchor }
}
```

**Replace in `useSceneSettingsState.js`:**
- Remove local `customFpsAnchor` state and `fpsMode` derivation
- Remove local `handleFpsModeChange` and `handleCustomFpsChange`
- Add: `const { fpsMode, handleFpsModeChange, handleCustomFpsChange } = useFpsMode({ fps: scene?.fps, onFpsChange: ..., updateRate })`

**Replace in `useRenderVideoDialogState.js`:**
- Same pattern — the hook's `handleFpsModeChange` already calls `onSettingsChange(...)`, so pass that as the `onFpsChange` callback
- Note: `useRenderVideoDialogState` also calls `normalizeUpdateRateForFps` inside `handleFpsModeChange`. The shared hook should NOT own update-rate logic — the callers provide that.

### Affected Files
- `src/hooks/useFpsMode.js` — **NEW**
- `src/features/scene-settings/hooks/useSceneSettingsState.js` — use new hook, delete local impl
- `src/features/render-video/hooks/useRenderVideoDialogState.js` — use new hook, delete local impl
- `src/features/render-video/hooks/useRenderVideoDerivedState.js` — may need `fpsMode` from parent, pass through from dialog state

---

## Step 4: Extract shared `isInteractiveElement` utility

### Problem

- `src/features/overlay-editor/utils/overlayEditorHelpers.js` has `isEditableElement(target)`
- `src/features/player/hooks/usePlayerKeyboard.js` has `isPlaybackShortcutTarget(target)`

Both check if focus is in input/textarea/select/contenteditable.

### Action

1. Add to `src/lib/utils.js`:
```js
export function isInteractiveElement(target) {
  if (!(target instanceof HTMLElement)) return false
  return Boolean(target.closest('input, textarea, select, button, a, [role="slider"], [contenteditable="true"]'))
}
```
2. Update `overlayEditorHelpers.js` — replace `isEditableElement` with re-export/import of `isInteractiveElement`
3. Update `usePlayerKeyboard.js` — replace `isPlaybackShortcutTarget` with `isInteractiveElement`

### Affected Files
- `src/lib/utils.js` — add function
- `src/features/overlay-editor/utils/overlayEditorHelpers.js` — delete local, import from lib
- `src/features/player/hooks/usePlayerKeyboard.js` — delete local, import from lib

---

## Step 5: Fix `useCommunityTemplate.js` — remove `alert()`, use proper error flow

### Problem

`useCommunityTemplate.js` calls `alert()` on fetch failure and calls `useStore.getState()` multiple times imperatively.

### Action

1. Replace `alert(...)` with `useStore.getState().setErrorMessage(...)`
2. Move the demo GPX fallback logic (setting dummy duration, start/end second, gpxFilename) into a new store action `setDemoActivity` on the media slice:
   ```js
   setDemoActivity: () => set((state) => {
     const demoDuration = 7946
     state.gpxFilename = 'demo.gpxinit'
     state.dummyDurationSeconds = demoDuration
     state.startSecond = 0
     state.endSecond = demoDuration
     state.selectedSecond = 0
   })
   ```
3. In `useCommunityTemplate.js`, call `useStore.getState().setDemoActivity()` instead of the 5 individual setter calls.
4. The config replacement (`useStore.getState().setConfig(data)`) and editor sync can stay since they need the actual config object — but wrap them in a helper that takes the data as a parameter.

**Note:** This hook still uses `useStore.getState().setConfig(data)` which is a store action — this is less bad since it's calling a registered action. But ideally we'd move this into a named store action too: `importTemplateConfig(config)`.

### Affected Files
- `src/features/template-manager/hooks/useCommunityTemplate.js` — replace alert, use store actions
- `src/store/slices/createMediaSlice.js` — add `setDemoActivity` action
- `src/store/slices/createTemplateSlice.js` or editor slice — add `importTemplateConfig` action (optional, for cleaner separation)

---

## Step 6: Fix fragile SVG relative imports in `widgetIconData.js`

### Problem

`src/lib/widget-icon-data.js` uses fragile relative paths like `../../../assets/widget-icons/widget-speed.svg?raw`.

### Action

1. Replace all `../../../assets/widget-icons/` with `@/assets/widget-icons/`
2. Replace `../components/widgets/icons/widget-gradient.svg?raw` with `@/components/widgets/icons/widget-gradient.svg?raw`

**Verify:** Vite resolves `@` to `src/`. If the `assets/` directory is at the project root (not under `src/`), the alias won't work. Check Vite config.

### Investigation needed

Check `vite.config.js` for alias resolution. If `@` points to `src/` and assets are at `D:\github\cyclemetry-reloaded\app\assets\`, then `@/assets/` won't resolve. May need a separate alias or keep as-is.

### Affected Files
- `src/lib/widget-icon-data.js` — update every SVG import

---

## Step 7: Rename ambiguous function names

### A. `handleStep` → `handleStepByDirection` in `usePlaybackEngine.js`

**Action:** Rename the function. Update consumer: `usePlayerKeyboard.js` — the call `handleStep(direction)` becomes `handleStepByDirection(direction)`.

### B. `setGpxFilename` → `setActivityFilename` in `createMediaSlice.js`

**Action:** Rename the action in the store slice. Update all references:
- `src/store/slices/createMediaSlice.js`
- Every call to `store.setGpxFilename(...)` or `state.gpxFilename`
- The selector in `useAppStoreSelectors.js` (will already be split by now from Phase 2)

### C. `buildScopedRouteSamples` → `buildExportWindowRouteSamples` in `export-range.js`

**Action:** Rename. Update callers:
- `src/features/widget-preview/components/RouteRenderer.jsx`
- `src/features/overlay-editor/index.js` (barrel re-export)

### Affected Files
- `src/features/player/hooks/usePlaybackEngine.js` — rename function
- `src/features/player/hooks/usePlayerKeyboard.js` — rename call
- `src/store/slices/createMediaSlice.js` — rename action + property
- Various store consumers — rename `gpxFilename` → `activityFilename`
- `src/lib/export-range.js` — rename function
- `src/features/widget-preview/components/RouteRenderer.jsx` — rename import/call
- `src/features/overlay-editor/index.js` — rename re-export

---

## Execution Order

1. Run baseline: `npm run test && npm run lint`
2. Apply step 1 (render-video.js store access) → test
3. Apply step 2 (import-activity.js store access) → test
4. Apply step 4 (isInteractiveElement) → test
5. Apply step 3 (useFpsMode extraction) → test
6. Apply step 5 (useCommunityTemplate alert removal) → test
7. Apply step 6 (SVG imports) — conditional on alias investigation
8. Apply step 7 (renames) — run `npm run lint` to verify no dead references
9. Final: `npm run test && npm run lint && npm run build`
10. Manual verification: import GPX, add widgets, render preview frame, render video
