# Codebase Refactoring Analysis

## Files < 50 LOC — Validity Assessment

| File | LOC | Verdict | Recommendation |
|------|-----|---------|---------------|
| `src/test-setup.js` | 5 | Valid | Vitest setup — required by framework |
| `src/lib/dev-config.js` | 5 | Valid | Single flag used across app; reasonable extraction |
| `src/lib/tauri-runtime.js` | 8 | Weak | Merge into `src/api/backend.js` where it's the only consumer |
| `src/lib/utils.js` | 20 | Valid | `cn()` and `clamp()` used across entire codebase |
| `src/lib/theme.js` | 27 | Valid | `getThemeColor()` used by 5+ widget editors and template-state |
| `src/lib/cached-promise.js` | 34 | Valid | Self-contained utility with clear contract; used by 3 modules |
| `src/hooks/use-refs.js` | 26 | Weak | Move to `src/lib/compose-refs.js` — it's only consumed there and is a React utility, not a feature hook |
| `src/store/slices/createLayoutSlice.js` | 32 | Valid | Zustand slice pattern is consistent across all slices |
| `src/features/overlay-editor/data/metricWidgetAssets.js` | 1 | **Remove** | Single re-export line. Callers should import directly from `src/lib/widget-icon-data.js` |
| `src/styles/moveable.css` | 30 | Valid | Moveable library overrides — reasonable |
| `src/styles/fonts.css` | 32 | Valid | Font-face declarations |
| `src/styles/theme.css` | 57 | Valid | Tailwind v4 @theme block |
| `src/styles/utilities.css` | 60 | Valid | App-shell scaling and grid utilities |
| `src/lib/compose-refs.js` | 73 | Valid | Radix-UI pattern — reasonable |
| `src/lib/color-utils.js` | 69 | Valid | Color normalization used by multiple modules |
| `src/lib/fonts.js` | 73 | Valid | Font utilities used by widget-editor, template-state, widget-icons |
| `src/lib/previewPerf.js` | 78 | Valid | Development diagnostics — well-isolated concern |
| `src/lib/widget-presentation.js` | 73 | Valid | Widget grouping for sidebar — clean separation from widget-config.js |
| `src/features/render-video/utils/format.js` | 46 | Weak | Merge into `src/features/render-video/utils/codecUtils.js` — only 2 tiny functions |
| `src/features/video-preview/data/videoPreviewConstants.js` | 53 | Valid | Constants file |
| `src/features/scene-settings/data/sceneSettingsConstants.js` | 43 | Valid | Constants file |
| `src/features/scene-settings/utils/sceneSettingsUtils.js` | 33 | Valid | 2 pure functions used by useSceneSettingsState |
| `src/features/template-manager/utils/templateFileUtils.js` | 45 | Valid | File dialog helpers — self-contained |
| `src/features/widget-editor/components/TextWidgetEditor.jsx` | 12 | Weak | Only renders `FontSection`. Justify: per-type dispatch pattern in SidebarWidgetsTab requires consistent component signature per type. OK. |
| `src/features/widget-editor/components/TimeWidgetEditor.jsx` | 14 | Weak | Same justification as TextWidgetEditor. OK. |
| `src/features/widget-preview/utils/textWidgetPreviewModel.js` | 57 | Valid | Self-contained preview model builder |

## Severe Bad Practices

### 1. Duplicate functions in `createEditorSlice.js` and `overlayEditorHelpers.js`

Two functions are defined identically in both files:
- `normalizeSelectionIds` (createEditorSlice.js:19-22, overlayEditorHelpers.js:83-90)
- `getPrimarySelectionId` (createEditorSlice.js:31-37, overlayEditorHelpers.js:100-104)

**Recommendation:** Delete the duplicates from `createEditorSlice.js` and import them from `overlayEditorHelpers.js`. The store slice should not own selection normalization logic that the editor also needs.

### 2. Duplicate `updateUnrenderedChanges` logic

`createTemplateSlice.js:41-48` has `updateUnrenderedChanges` which is a near-copy of `updateConfigPersistence` in `store-utils.js:106-112`.

**Recommendation:** Delete the duplicate and use `updateConfigPersistence` from `store-utils.js`.

### 3. `useCommunityTemplate.js` directly mutates store via `useStore.getState()`

This hook calls `useStore.getState()` 4 times inside the async callback and imperatively calls `setGpxFilename`, `setDummyDurationSeconds`, `setStartSecond`, `setEndSecond`, `setSelectedSecond`, and `setConfig` as raw calls. It also uses `alert()` directly.

**Recommendation:** Move the demo GPX fallback logic into a store action (e.g., `setDemoActivity`) and have the hook call that single action. Remove the `alert()` call — use the standard error message store mechanism.

### 4. `import-activity.js` imports `useStore` from Zustand in a non-React utility

`saveFile` at line 268 calls `useStore.getState()` to access store selectors. This module is a pure utility but has a hidden dependency on Zustand.

**Recommendation:** The caller (`useActivityImport.js`) already passes `storeActions` as a second argument. Make `storeActions` required instead of falling back to `useStore.getState()`, removing the Zustand import from this utility.

### 5. `render-video.js` (the utility) calls `useStore.getState()` in a non-React function

The default export `renderVideo()` uses `useStore.getState()` extensively. This is invoked from `useRenderWorkflow.handleRenderVideoConfirm`.

**Recommendation:** Accept all required store state as an explicit parameter object instead of reaching into Zustand. The caller already constructs a params object; expand it to include the store state.

### 6. App.jsx exports `DEBUG_MODE_ENABLED`

Line 33: `export { DEBUG_MODE_ENABLED }` — this is a side effect that also happens to be a re-export. The only consumer that imports `DEBUG_MODE_ENABLED` from App.jsx is `useEditorShellState.js`, which also imports `src/lib/dev-config.js` directly.

**Recommendation:** Remove the export from App.jsx. All consumers already import from `src/lib/dev-config.js`.

### 7. `widgetIconData.js` uses fragile relative imports from `../../../assets/`

Every SVG import uses `../../../assets/widget-icons/...` paths. Two imports (gradient, course, elevation, label) even cross into `src/components/widgets/icons/`.

**Recommendation:** Use Vite path aliases (`@/assets/widget-icons/...`) for consistency and maintainability.

### 8. `index.css` imported twice

Line 7 of `main.jsx` and line 29 of `App.jsx` both import `./index.css`. The App.jsx import is redundant.

**Recommendation:** Remove the `import './index.css'` from App.jsx.

### 9. `setGpxFilename` declared `async` in `createMediaSlice.js` but never `await`s

Line 72: `setGpxFilename: async (filename) => {` — the function body is synchronous. `async` is misleading.

**Recommendation:** Remove the `async` keyword.

## Duplicate Code (Non-DRM)

### 1. FPS Mode / Custom FPS pattern duplicated

`useRenderVideoDialogState.js` and `useSceneSettingsState.js` both implement:
- `customFpsAnchor` state tracking
- `fpsMode` derivation
- `handleFpsModeChange` 
- `handleCustomFpsChange`

**Recommendation:** Extract a `useFpsMode` hook in `src/lib/` or a shared feature-level hooks directory.

### 2. Editable element detection duplicated

`overlayEditorHelpers.js` has `isEditableElement()` and `usePlayerKeyboard.js` has `isPlaybackShortcutTarget()`. Both check if focus is in an input/textarea/select/contenteditable.

**Recommendation:** Extract a shared `isInteractiveElement(target)` to `src/lib/utils.js` or similar.

### 3. Time formatting duplicated

`useOverlayEditorState.js` has `formatRangeTime()` and `playerTimeline.js` has `formatTimelineTime()`. Both format seconds to `HH:MM:SS`.

**Recommendation:** Keep `formatTimelineTime` in `playerTimeline.js`, have `formatRangeTime` import it.

### 4. Time parsing duplicated

`export-range.js` has `timeToSeconds()` and `sceneSettingsUtils.js` has `parseTimeOffset()`. Nearly identical purpose.

**Recommendation:** Consolidate into a single `parseTimeStringToSeconds()` in `export-range.js` (or `src/lib/utils.js`).

## Poor Separation of Concerns

### 1. `src/hooks/useAppStoreSelectors.js` — mixed concerns

This file defines 6 selector hooks (`useLayoutStore`, `useAppShellStore`, `useBootstrapStore`, `useActivityStore`, `useTemplateStore`, `useRenderStore`) that serve completely different feature domains.

**Recommendation:** Split by domain:
- `useLayoutStore` → `src/features/widget-drawer/` (its only consumer)
- `useAppShellStore` → `src/features/app-shell/`
- `useBootstrapStore` → already only used by app-shell
- `useActivityStore` → `src/lib/activity/` (consumed by app-shell)
- `useTemplateStore` → `src/features/template-manager/`
- `useRenderStore` → `src/features/render-video/`

### 2. `src/hooks/useAvailableFonts.js` location

Defined in `src/hooks/` but only consumed by `scene-settings` feature.

**Recommendation:** Move to `src/features/scene-settings/hooks/useAvailableFonts.js`.

### 3. `src/lib/tauri-runtime.js`

Re-exported by both `src/features/app-shell/utils/backendDebug.js` and `src/features/app-shell/index.js`, used by `src/api/backend.js`.

**Recommendation:** This is NOT a library utility — it's a runtime detection for the backend API. Merge the single 3-line function into `src/api/backend.js` where it's the primary consumer.

## Files in `src/lib/` — Feature Folder Assessment

| File | Should stay in lib? | Reason |
|------|---------------------|--------|
| `cached-promise.js` | Yes | Generic utility used by 3+ modules |
| `color-utils.js` | Yes | Used by template-state, widget-config, widget-editor |
| `compose-refs.js` | Yes | Generic React utility |
| `dev-config.js` | Yes | Global development flag |
| `export-range.js` | **No** → `src/features/overlay-editor/` | Only consumed by overlay-editor and widget-preview; already re-exported from overlay-editor/index.js |
| `fonts.js` | Yes | Used by template-state, widget-editor, widget-icons |
| `geometryUtils.js` | Yes | Domain-agnostic; used by multiple features |
| `interpolation.js` | Yes | Domain-agnostic pure functions |
| `previewPerf.js` | Yes | Development diagnostics |
| `standard-metrics.js` | Yes | Canonical metric catalog; used by template-manager, widget-icons, widget-editor |
| `tauri-runtime.js` | **No** → merge into `src/api/backend.js` | Only consumer |
| `template-defaults.js` | Yes | Well-documented sibling module pattern |
| `template-normalization.js` | Yes | Well-documented sibling module pattern |
| `template-state.js` | Yes | Orchestration for the two above |
| `theme.js` | Yes | Used by 5+ modules |
| `update-rate.js` | Yes | Used by render-video, scene-settings, player |
| `utils.js` | Yes | `cn()` and `clamp()` used everywhere |
| `widget-config.js` | Yes | CRUD operations used by widget-editor and store |
| `widget-icon-data.js` | Yes | Icon data registry |
| `widget-icons.jsx` | Yes | Widget icon components and labels |
| `widget-presentation.js` | Yes | Sidebar grouping logic |

## Excessive Fragmentation

### 1. Render-video hooks (7 files for one workflow)

- `useRenderCompletion.js` (48 lines)
- `useRenderDialogState.js` (58 lines)
- `useRenderProgressPolling.js` (32 lines)
- `useRenderVideoDerivedState.js` (99 lines)
- `useRenderVideoDialogState.js` (127 lines)
- `useRenderVideoEffects.js` (70 lines)
- `useRenderWorkflow.js` (187 lines)

Total: ~620 lines across 7 files, all composed by `useRenderWorkflow`.

**Recommendation:** Merge `useRenderCompletion`, `useRenderProgressPolling`, and `useRenderVideoEffects` into `useRenderWorkflow.js`. These are small 30-70 line hooks that are only ever used together. Keep `useRenderVideoDerivedState` and `useRenderVideoDialogState` separate since they have distinct responsibilities (data access vs dialog orchestration).

### 2. Player hooks

- `usePlaybackSourceHandoff.js` (48 lines) — only used by `usePlaybackEngine.js`
- `useTimelinePlaybackLoop.js` (65 lines) — only used by `usePlaybackEngine.js`

**Recommendation:** Merge both into `usePlaybackEngine.js`. They are implementation details of the playback engine, not independent concerns.

### 3. Moveable handler hooks (4 files, identical parameter patterns)

- `useDragHandlers.js` (substancial)
- `useResizeHandlers.js`
- `useScaleHandlers.js`
- `useRotateHandlers.js`

All 4 accept the same 10+ parameter context objects.

**Recommendation:** Keep as-is. Each has distinct, non-trivial logic. The identical parameter pattern is a consequence of Moveable's event model, not poor design. Consider a single shared context object (`ctx`) to reduce boilerplate, but don't merge the files.

### 4. Template-state sibling module trio

- `template-defaults.js` (43 lines)
- `template-normalization.js` (182 lines)
- `template-state.js` (179 lines)

**Verdict:** Well-justified split. Each module has clear ownership declared in file-level JSDoc. Total ~400 lines — splitting into smaller files here improves navigability.

## Overly Long / Ambiguous Function Names

| Function | Location | Issue | Suggestion |
|----------|----------|-------|------------|
| `saveFile` | `lib/activity/import-activity.js:265` | It imports an activity, not saves a file | `importActivityFile` |
| `handleStep` | `features/player/hooks/usePlaybackEngine.js` | Ambiguous — step which direction? The callback parameter is `direction` but the name doesn't convey this | `handleStepBySecond` or split into `handleStepForward`/`handleStepBackward` |
| `apiCall` | `src/api/backend.js:60` | Generic — doesn't indicate it wraps Tauri IPC calls | `tauriInvoke` (but this is internal, low priority) |
| `setGpxFilename` | `src/store/slices/createMediaSlice.js:72` | `gpx` is misleading — the app handles both GPX and FIT files | `setActivityFilename` |
| `buildConfigWidgets` | `src/lib/widget-presentation.js:29` | The name sounds like it builds widgets, not transforms config arrays | `flattenWidgetConfig` or `getWidgetsFromConfig` |
| `buildScopedRouteSamples` | `src/lib/export-range.js:203` | "Scoped" is imprecise — these are export-range-window-scoped | `buildExportWindowRouteSamples` |
| `buildScopedElevationSeries` | `src/lib/export-range.js:277` | Same issue | `buildExportWindowElevationSeries` |

## Nested Function Calls (Function as Argument)

### 1. `type` → `do` pattern spread across store slices

Every Zustand action uses the pattern:
```js
setConfig: (val) => {
  const currentState = get()
  ...
  set((state) => { ... })
}
```

**Verdict:** This is standard Zustand/zustand+immer pattern. Not refactorable without changing state management library. Acceptable.

### 2. `OverlayEditor.jsx` — hook composition

The `OverlayEditor` component composes ~8 hooks and then passes their return values directly as props. This is a deliberate "composition at component level" pattern documented in JSDoc.

**Verdict:** Acceptable. Each hook owns one concern. The component body is the composition point.

## Poor Code Location

### 1. `metricWidgetAssets.js`

File at `src/features/overlay-editor/data/metricWidgetAssets.js` contains a single re-export:
```js
export { METRIC_ICON_SVGS } from '@/lib/widget-icon-data'
```

**Recommendation:** Remove this file. Update imports in overlay-editor/index.js to import directly from `@/lib/widget-icon-data`.

### 2. Activity parsing in `src/lib/activity/`

The entire `src/lib/activity/` directory (fit-parser.js, gap-utils.js, import-activity.js, metric-series.js, parse-helpers.js, parser.js) contains activity parsing logic. This is a domain concern, not a "library."

**Recommendation:** The directory is reasonably placed in `lib/` because it's consumed by both the store slices and the app-shell import hook. It has no React dependencies and consists of pure data transformation. Keep as-is.

### 3. `useAppStoreSelectors.js` as a grab-bag

This file in `src/hooks/` is a collection of Zustand selectors for 6 different domains.

**Recommendation:** Already covered above — split by feature domain.

## Summary of Recommended Actions (Priority Order)

### High Priority
1. **Remove `metricWidgetAssets.js`** (1-line re-export file, dead weight)
2. **Deduplicate `normalizeSelectionIds`/`getPrimarySelectionId`** — leave only in `overlayEditorHelpers.js`
3. **Deduplicate `updateUnrenderedChanges`** — use `updateConfigPersistence` from store-utils
4. **Fix `useCommunityTemplate.js`** — remove `alert()`, move to store action pattern
5. **Remove duplicate `import './index.css'` from App.jsx**
6. **Remove redundant exports — `DEBUG_MODE_ENABLED` from App.jsx**

### Medium Priority
7. **Extract `useFpsMode` hook** from the duplicated FPS mode logic in render-video and scene-settings
8. **Deduplicate `isEditableElement`/`isPlaybackShortcutTarget`** into a shared utility
9. **Consolidate `formatRangeTime`/`formatTimelineTime`** (just have the former call the latter)
10. **Consolidate `timeToSeconds`/`parseTimeOffset`** into one function
11. **Merge `hasTauriRuntime` into `src/api/backend.js`** — it's a 3-line function with one consumer
12. **Split `useAppStoreSelectors.js`** by domain
13. **Move `useAvailableFonts.js`** to `src/features/scene-settings/hooks/`
14. **Move `use-refs.js`** into `src/lib/compose-refs.js`
15. **Rename `saveFile`** → `importActivityFile` in import-activity.js
16. **Remove `async` from `setGpxFilename`** in createMediaSlice.js

### Low Priority
17. **Merge `useRenderCompletion`, `useRenderProgressPolling`, `useRenderVideoEffects`** into `useRenderWorkflow.js`
18. **Merge `usePlaybackSourceHandoff` and `useTimelinePlaybackLoop`** into `usePlaybackEngine.js`
19. **Move `export-range.js`** to `src/features/overlay-editor/utils/`
20. **Replace fragile relative SVG imports** in `widgetIconData.js` with `@/` aliases
21. **Pass store state as parameter** to `render-video.js` utility instead of calling `useStore.getState()`
22. **Pass store state as parameter** to `import-activity.js` `saveFile` instead of falling back to `useStore.getState()`
23. **Merge `format.js` (render-video)** into `codecUtils.js`
