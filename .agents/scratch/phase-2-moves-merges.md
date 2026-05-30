# Phase 2 — File Moves, Merges & Renames

**Goal:** Restructure import paths, merge small files, and consolidate duplicate patterns.
Changes import paths and module boundaries — requires thorough verification.

**Risk:** Medium
**Estimated time:** 1.5–2 hours

---

## Acceptance Criteria

- [ ] `npm run lint` passes with zero errors
- [ ] `npm run test` — all test suites pass
- [ ] `npm run build` succeeds
- [ ] `grep -r "old-path"` returns zero for every moved file
- [ ] No circular imports introduced

---

## Step 1: Move `useAvailableFonts.js` to scene-settings feature

### `src/hooks/useAvailableFonts.js`

**Change:**

1. Move to `src/features/scene-settings/hooks/useAvailableFonts.js`
2. Update import in `src/features/scene-settings/components/widgetEditorSections.jsx`:

   ```js
   import useAvailableFonts from "@/hooks/useAvailableFonts";
   ```

   →

   ```js
   import useAvailableFonts from "../../hooks/useAvailableFonts";
   ```

   (or keep the `@/` alias: `@/features/scene-settings/hooks/useAvailableFonts`)

3. Also check `src/features/scene-settings/hooks/useSceneSettingsState.js` — it imports `useAvailableFonts` from `@/hooks/useAvailableFonts`. Update to relative or `@/features/scene-settings/hooks/useAvailableFonts`.
4. Delete original file.

### Affected Files

- `src/hooks/useAvailableFonts.js` — **DELETE**
- `src/features/scene-settings/hooks/useAvailableFonts.js` — **NEW (move)**
- `src/features/scene-settings/components/widgetEditorSections.jsx` — update import
- `src/features/scene-settings/hooks/useSceneSettingsState.js` — update import

---

## Step 2: Split `useAppStoreSelectors.js` by domain

### `src/hooks/useAppStoreSelectors.js`

**Action:** This file defines 6 selector hooks for different domains. Split them to their feature folders.

| Selector            | Move to                                                                                                            |
| ------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `useLayoutStore`    | `src/features/widget-drawer/hooks/useLayoutStore.js` (only consumed by WidgetDrawer.jsx)                           |
| `useAppShellStore`  | `src/features/app-shell/hooks/useAppShellStore.js` (consumed by App.jsx, useActivityImport.js, useAppBootstrap.js) |
| `useBootstrapStore` | `src/features/app-shell/hooks/useBootstrapStore.js` (consumed by useAppBootstrap.js)                               |
| `useActivityStore`  | `src/features/app-shell/hooks/useActivityStore.js` (consumed by useActivityImport.js)                              |
| `useTemplateStore`  | `src/features/template-manager/hooks/useTemplateStore.js` (consumed by useTemplateManagement.js)                   |
| `useRenderStore`    | `src/features/render-video/hooks/useRenderStore.js` (consumed by useRenderWorkflow.js)                             |

**For each split:**

1. Create new file in target location with just that single hook
2. Update ALL imports across the codebase to point to the new location
3. After all 6 are moved, delete `src/hooks/useAppStoreSelectors.js`

**Verify:** `grep -r "useAppStoreSelectors"` returns zero results.

### Affected Files (by hook)

**useLayoutStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/features/widget-drawer/components/WidgetDrawer.jsx` — update import path

**useAppShellStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/App.jsx` — update import path
- `src/features/app-shell/hooks/useActivityImport.js` — update import path
- `src/features/app-shell/hooks/useAppBootstrap.js` — update import path

**useBootstrapStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/features/app-shell/hooks/useAppBootstrap.js` — update import path

**useActivityStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/features/app-shell/hooks/useActivityImport.js` — update import path

**useTemplateStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/features/template-manager/hooks/useTemplateManagement.js` — update import path

**useRenderStore:**

- `src/hooks/useAppStoreSelectors.js` — remove this hook
- `src/features/render-video/hooks/useRenderWorkflow.js` — update import path

---

## Step 3: Merge render-video sub-hooks into `useRenderWorkflow.js`

### Merge 3 tiny hooks into their composer

**Files to merge:**

- `src/features/render-video/hooks/useRenderCompletion.js` (48 lines) → into `useRenderWorkflow.js`
- `src/features/render-video/hooks/useRenderProgressPolling.js` (32 lines) → into `useRenderWorkflow.js`
- `src/features/render-video/hooks/useRenderVideoEffects.js` (70 lines) → into `useRenderWorkflow.js`

**Action:**

1. Inline the `useEffect` bodies from each hook directly into `useRenderWorkflow.js`
2. Replace the hook call invocations:
   ```js
   useRenderProgressPolling({ renderingVideo, setRenderProgress });
   ```
   with the actual `useEffect` body.
3. Delete the three source files.
4. Keep `useRenderDialogState` and `useRenderVideoDerivedState` as separate files (they have distinct responsibilities).

### Affected Files

- `src/features/render-video/hooks/useRenderCompletion.js` — **DELETE**
- `src/features/render-video/hooks/useRenderProgressPolling.js` — **DELETE**
- `src/features/render-video/hooks/useRenderVideoEffects.js` — **DELETE**
- `src/features/render-video/hooks/useRenderWorkflow.js` — absorb the effect bodies

---

## Step 4: Merge player sub-hooks into `usePlaybackEngine.js`

### Merge 2 hooks into their only consumer

**Files to merge:**

- `src/features/player/hooks/usePlaybackSourceHandoff.js` (48 lines) → into `usePlaybackEngine.js`
- `src/features/player/hooks/useTimelinePlaybackLoop.js` (65 lines) → into `usePlaybackEngine.js`

**Action:**

1. Inline the `useEffect` bodies from both hooks into `usePlaybackEngine.js`
2. Replace the hook call invocations with the actual effect bodies
3. Delete the two source files.

### Affected Files

- `src/features/player/hooks/usePlaybackSourceHandoff.js` — **DELETE**
- `src/features/player/hooks/useTimelinePlaybackLoop.js` — **DELETE**
- `src/features/player/hooks/usePlaybackEngine.js` — absorb effect bodies

---

## Step 5: Consolidate time formatting → `playerTimeline.js`

### `useOverlayEditorState.js` uses `formatRangeTime` (local) — `playerTimeline.js` has `formatTimelineTime`

**Action:**

1. In `useOverlayEditorState.js`, delete the local `formatRangeTime` function
2. Import `formatTimelineTime` from `@/features/player/utils/playerTimeline`
3. Note: `formatRangeTime` returns `HH:MM:SS` format; `formatTimelineTime` returns `mm:ss` or `h:mm:ss` depending on value. Verify behavior equivalence for all values. If they differ, keep `formatRangeTime` and rename it to something clearer.

**Investigation needed:** Before executing, compare both functions side-by-side. They may format differently.

### Affected Files

- `src/features/overlay-editor/hooks/useOverlayEditorState.js` — update import, replace function
- Potentially: rename one of them for clarity

---

## Step 6: Consolidate time parsing → `export-range.js`

### `export-range.js` has `timeToSeconds` — `sceneSettingsUtils.js` has `parseTimeOffset`

**Action:**

1. Both parse `HH:MM:SS` format to seconds
2. `parseTimeOffset` additionally handles negative values (prefix `-`)
3. Move `parseTimeOffset` logic into `timeToSeconds` (adding negative support)
4. Have `sceneSettingsUtils.js` import `timeToSeconds` from `@/lib/export-range`
5. Update `useSceneSettingsState.js` callers that use `parseTimeOffset`

### Affected Files

- `src/features/scene-settings/utils/sceneSettingsUtils.js` — remove `parseTimeOffset`, import from lib
- `src/lib/export-range.js` — add negative-offset support to `timeToSeconds`
- `src/features/scene-settings/hooks/useSceneSettingsState.js` — update import if needed

---

## Step 7: Merge `format.js` (render-video) into `codecUtils.js`

### `src/features/render-video/utils/format.js` (46 lines, 2 tiny functions)

**Action:**

1. Move `formatTime` and `formatFps` into `src/features/render-video/utils/codecUtils.js`
2. Update imports in `src/features/render-video/components/RenderProgressPanel.jsx`:
   ```js
   import { formatFps, formatTime } from "../utils/format";
   ```
   →
   ```js
   import { formatFps, formatTime } from "../utils/codecUtils";
   ```
3. Delete `format.js`.

### Affected Files

- `src/features/render-video/utils/format.js` — **DELETE**
- `src/features/render-video/utils/codecUtils.js` — append 2 functions
- `src/features/render-video/components/RenderProgressPanel.jsx` — update import

---

## Step 8: Move `export-range.js` to overlay-editor feature

### `src/lib/export-range.js` → `src/features/overlay-editor/utils/exportRange.js`

**Action:**

1. Move the file
2. Update imports everywhere:
   - `src/features/overlay-editor/index.js` — already re-exports from `@/lib/export-range`, change to relative
   - `src/features/widget-preview/components/ElevationRenderer.jsx` — imports from `@/features/overlay-editor` (barrel), should work via barrel update
   - `src/features/widget-preview/components/RouteRenderer.jsx` — same
   - `src/features/render-video/utils/renderConfig.js` — imports `timeToSeconds` from `@/lib/export-range`, update to `@/features/overlay-editor`

### Affected Files

- `src/lib/export-range.js` — **MOVE** to `src/features/overlay-editor/utils/exportRange.js`
- `src/features/overlay-editor/index.js` — update barrel re-export path
- `src/features/render-video/utils/renderConfig.js` — update import path

---

## Step 9: Rename `saveFile` → `importActivityFile`

### `src/lib/activity/import-activity.js` and `src/features/app-shell/hooks/useActivityImport.js`

**Action:**

1. Rename default export in `import-activity.js` from `saveFile` to `importActivityFile`
2. Update the import in `useActivityImport.js`:
   ```js
   import saveFile from "@/lib/activity/import-activity";
   ```
   →
   ```js
   import importActivityFile from "@/lib/activity/import-activity";
   ```
3. Update call site: `await saveFile(selected, useStore.getState())` → `await importActivityFile(...)`

### Affected Files

- `src/lib/activity/import-activity.js` — rename default export
- `src/features/app-shell/hooks/useActivityImport.js` — update import + call

---

## Execution Order

1. Run baseline: `npm run test && npm run lint`
2. Apply steps 1-9 in order (each step is independent but test after every 3 steps)
3. After steps 1-2: run `npm run test && npm run lint`
4. After steps 3-4: run `npm run test && npm run lint`
5. After steps 5-7: run `npm run test && npm run lint`
6. After steps 8-9: run `npm run test && npm run lint`
