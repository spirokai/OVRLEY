# Phase 1 — Mechanical & Safe Changes

**Goal:** Pure removals, deduplication, and import path fixes. No behavioral changes.
Every change has a clear before/after — tests should pass identically.

**Risk:** Low
**Estimated time:** 30 min

---

## Acceptance Criteria

- [ ] `npm run lint` passes with zero errors
- [ ] `npm run test` — all 51 test suites pass (identical count to baseline)
- [ ] App builds: `npm run build` succeeds

---

## Step 1: Remove 1-line re-export file

### `src/features/overlay-editor/data/metricWidgetAssets.js`

**Action:** Delete the file.
**Affected:** `src/features/overlay-editor/index.js` — update the import of `METRIC_ICON_SVGS` to import from `@/lib/widget-icon-data` instead.
**Verify:** `grep -r "metricWidgetAssets"` returns zero results.

### Affected Files
- `src/features/overlay-editor/data/metricWidgetAssets.js` — **DELETE**
- `src/features/overlay-editor/index.js` — change line referencing `metricWidgetAssets` to `@/lib/widget-icon-data`

---

## Step 2: Deduplicate `normalizeSelectionIds` and `getPrimarySelectionId`

### `src/store/slices/createEditorSlice.js` + `src/features/overlay-editor/utils/overlayEditorHelpers.js`

**Action:** These two functions exist verbatim in both files:
- `normalizeSelectionIds(widgetIds, orderedWidgetIds)` 
- `getPrimarySelectionId(widgetIds, preferredId)`

The versions in `overlayEditorHelpers.js` are the primary source (more general, used in more callers). The versions in `createEditorSlice.js:19-37` are local duplicates used only by `setWidgetSelectionState` and `reconcileSelection`.

**Change in `createEditorSlice.js`:**
1. Import `getPrimarySelectionId` and `normalizeSelectionIds` from `../../features/overlay-editor/utils/overlayEditorHelpers`
   - Actually, this creates a circular-ish dependency risk (store importing from feature). Better approach:
2. Export them from `overlayEditorHelpers.js` (already exported) and import in `createEditorSlice.js`:
   ```js
   import { getPrimarySelectionId, normalizeSelectionIds } from '../../features/overlay-editor/utils/overlayEditorHelpers'
   ```
3. Delete the local `normalizeSelectionIds()` function (lines 19-22)
4. Delete the local `getPrimarySelectionId()` function (lines 31-37)

**Potential issue:** Store slice importing from a feature utility. If this introduces a circular dependency or feels wrong, the alternative is to keep the copy in the store but annotate it as a delegating wrapper. **Decision: Use the import approach** — these are pure utility functions with no feature-specific dependencies.

### Affected Files
- `src/store/slices/createEditorSlice.js` — delete lines 19-37, add import

---

## Step 3: Deduplicate `updateUnrenderedChanges` → use `updateConfigPersistence`

### `src/store/slices/createTemplateSlice.js`

**Action:** `updateUnrenderedChanges` at line 41-48 is a near-copy of `updateConfigPersistence` from `store-utils.js:106-112`.

**Change:**
1. Remove `updateUnrenderedChanges` function (lines 41-48)
2. Replace both call sites with `updateConfigPersistence(state)`:
   - Line 144: `updateUnrenderedChanges(state, nextConfig)` → `updateConfigPersistence(state)` (param no longer needed since it reads `state.config` and `state.lastRenderedConfig` internally)
   - Line 194: same pattern

**Note:** `updateConfigPersistence` already reads `state.config` directly (which is what `nextConfig` would become after the set block). So the behavior is identical.

### Affected Files
- `src/store/slices/createTemplateSlice.js` — delete lines 41-48, update lines ~144 and ~194

---

## Step 4: Remove duplicate `import './index.css'` from App.jsx

### `src/App.jsx`

**Action:** `main.jsx` already imports `./index.css` on line 7. Remove the duplicate import on line 29 of App.jsx.

### Affected Files
- `src/App.jsx` — delete `import './index.css'` on line 29

---

## Step 5: Remove `DEBUG_MODE_ENABLED` export from App.jsx

### `src/App.jsx`

**Action:** Line 33: `export { DEBUG_MODE_ENABLED }` — the only consumer (`useEditorShellState.js`) already imports directly from `@/lib/dev-config.js`. Remove line 33.

**Verify:** `grep -r "from.*App\.jsx" | grep DEBUG` returns zero results.

### Affected Files
- `src/App.jsx` — delete line 33

---

## Step 6: Remove misleading `async` from `setGpxFilename`

### `src/store/slices/createMediaSlice.js`

**Action:** Line 72: `setGpxFilename: async (filename) => {` — the function body is purely synchronous (`set()` call without `await`). Remove the `async` keyword.

### Affected Files
- `src/store/slices/createMediaSlice.js` — remove `async` from line 72

---

## Step 7: Merge `hasTauriRuntime` into `src/api/backend.js`

### `src/lib/tauri-runtime.js`

**Action:** The 3-line function `hasTauriRuntime()` is only consumed by `src/api/backend.js` and re-exported by:
- `src/features/app-shell/utils/backendDebug.js`
- `src/features/app-shell/index.js`

**Change:**
1. Move the `hasTauriRuntime` function definition into `src/api/backend.js` (above `getInvoke` where it's used)
2. Delete `src/lib/tauri-runtime.js`
3. Update imports in `src/api/backend.js` — remove `import { hasTauriRuntime } from '@/lib/tauri-runtime'`
4. Update `src/features/app-shell/utils/backendDebug.js` — change `import { hasTauriRuntime } from '@/lib/tauri-runtime'` → `import { hasTauriRuntime } from '@/api/backend'`
5. Update `src/features/app-shell/index.js` — change `export { hasTauriRuntime } from './utils/backendDebug'` — (actually it already does this, so no change needed there since backendDebug will now re-export from the new source)

### Affected Files
- `src/lib/tauri-runtime.js` — **DELETE**
- `src/api/backend.js` — add function, remove import
- `src/features/app-shell/utils/backendDebug.js` — update import path

---

## Step 8: Merge `use-refs.js` into `compose-refs.js`

### `src/hooks/use-refs.js`

**Action:** The three exports (`useAsRef`, `useIsomorphicLayoutEffect`, `useLazyRef`) are only consumed by `src/lib/compose-refs.js` and `src/components/ui/color-picker.jsx`. Both already import from `@/hooks/use-refs`.

**Change:**
1. Move the three function definitions into `src/lib/compose-refs.js`
2. Delete `src/hooks/use-refs.js`
3. Update imports in `src/components/ui/color-picker.jsx`:
   ```js
   import { useAsRef, useIsomorphicLayoutEffect, useLazyRef } from '@/hooks/use-refs'
   ```
   →
   ```js
   import { useAsRef, useIsomorphicLayoutEffect, useLazyRef } from '@/lib/compose-refs'
   ```

### Affected Files
- `src/hooks/use-refs.js` — **DELETE**
- `src/lib/compose-refs.js` — append 3 functions, update internal import
- `src/components/ui/color-picker.jsx` — update import path

---

## Execution Order

1. Run baseline: `npm run test` and `npm run lint` — capture baseline output
2. Apply steps 1-8 in order
3. Run `npm run lint`
4. Run `npm run test`
5. All must pass identically to baseline

---

## Rollback

Each step is individually reversible via `git checkout -- <files>`. Steps are ordered so later steps don't depend on earlier ones (except step 7 depends on step 4 for the `hasTauriRuntime` re-export chain, which is handled atomically).
