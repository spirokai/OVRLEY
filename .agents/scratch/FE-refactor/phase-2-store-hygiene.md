# Phase 2 — Store Hygiene: Mutable State, Leaks & Module Boundaries

**Goal:** Fix module-level mutable singletons, dev-mode window leaks, cross-boundary store access from `lib/`, unsafe Tauri imports, and circular imports between feature hooks and app root.

**Refactor.md issues addressed:** #18, #19, #20, #21, #22 (full), #23

---

## TDD — Tests to Write FIRST (RED)

### Test 1: `app/src/tests/store/useStore.window-leak.test.js`
**If not written in Phase 1.** Verify the store does NOT set `window.useStore`, `window.__OVRLEY_STORE__`, or `window.__STORE__` in production mode:
- Use `vi.stubEnv('PROD', true)` or mock `import.meta.env.DEV` to simulate production
- Import the store (or the store creation module)
- Assert no window properties are set
- In dev mode, assert exactly ONE window property is set (not three)

### Test 2: `app/src/tests/lib/activity-cache.test.js`
Full characterization of the module-level cache:
- `setCurrentActivityCache` stores data
- `getCurrentParsedActivity` returns stored data
- `clearCurrentActivityCache` resets to null
- Calling `setCurrentActivityCache` twice overwrites
- The cache does NOT leak to `window` outside dev mode
- Concurrent access pattern (set while another consumer is reading)

### Test 3: `app/src/tests/lib/import-activity-boundary.test.js`
Verify the boundary between `lib/activity/import-activity.js` and the store:
- Mock `useStore.getState()` to return a fake store snapshot
- Call `saveFile` with a mock GPX `File` object
- Assert the store's `setGpxFilename`, `setActivitySummary`, `setProcessing` are called with expected values
- Assert `syncSceneDurationWithActivity` updates scene timing correctly
- **This test validates current behavior before the refactor.** After refactoring (step 5), the same test should pass with the store state passed as a parameter instead of imported.

### Test 4: `app/src/tests/features/app-shell/useEditorShellState-boundary.test.jsx`
Verify `useEditorShellState` does NOT crash in a non-Tauri (jsdom) environment:
- Render a component that calls `useEditorShellState()`
- Assert it returns default values without throwing
- `debugModeEnabled` should be `false` in non-DEV

### Test 5: `app/src/tests/features/app-shell/TitleBar-boundary.test.jsx`
Verify `TitleBar` renders without crashing in jsdom (no Tauri):
- Mock `@tauri-apps/api/window` to return a fake `getCurrentWindow`
- Render `<TitleBar />`
- Assert the three window control buttons exist
- Or: if lazy-loaded, assert the component gracefully degrades

### Test 6: `app/src/tests/features/app-shell/useActivityImport-boundary.test.js`
Verify the `useActivityImport` hook accepts an injected `importActivityFn` (so it doesn't need the dynamic import):
- Pass a mock `importActivity` function
- Trigger `handleGpxFileOpen`
- Assert the mock is called with the selected file

---

## Implementation Steps (GREEN)

### Step 1: Fix dev-mode window leaks in `useStore.js`
**File:** `store/useStore.js:52-57`

Current state sets THREE window properties:
```js
window.useStore = useStore
window.__OVRLEY_STORE__ = useStore
window.__STORE__ = useStore
```

1. Pick one: `window.__OVRLEY_STORE__`
2. Delete the other two assignments
3. Wrap the dev-only block in a check that also guards against SSR: `typeof window !== 'undefined'`
4. Tests from Test 1 must pass

### Step 2: Remove module-level mutable singleton from `activity/cache.js`
**File:** `lib/activity/cache.js`

1. Remove the `let currentParsedActivity = null` module-level variable
2. Remove the `setCurrentActivityCache`, `getCurrentParsedActivity`, `clearCurrentActivityCache` exports
3. Remove the `window.setCurrentActivityCache` dev-only leak (line 34-36)
4. The parsed activity is now stored ONLY in the zustand store (`createMediaSlice.activitySummary`)
5. All code that reads `getCurrentParsedActivity()` must be updated to read from the store
6. Find all callers: `import-activity.js` and `usePlaybackEngine.js` likely
7. In `import-activity.js`, remove the `setCurrentActivityCache` call (the store's `setActivitySummary` already handles this)
8. In `usePlaybackEngine.js`, read activity from `useStore.getState().activitySummary` instead

### Step 3: Prevent `TitleBar.jsx` from crashing outside Tauri
**File:** `features/app-shell/components/TitleBar.jsx`

The import `import { getCurrentWindow } from '@tauri-apps/api/window'` runs at module scope.

1. Wrap in a lazy check:
```js
let getCurrentWindow = () => ({ minimize: () => {}, toggleMaximize: () => {}, close: () => {} })
try {
  const mod = await import('@tauri-apps/api/window')
  getCurrentWindow = mod.getCurrentWindow
} catch {}
```
Or, simpler: guard the import with a dynamic `import()` inside the component and return `null` if not available.

2. The component should return `null` (or a minimal placeholder) when Tauri is unavailable
3. Test from Test 5 must pass

### Step 4: Fix circular dependency — `useEditorShellState` imports from `App.jsx`
**Files:**
- `features/app-shell/hooks/useEditorShellState.js:26` — imports `DEBUG_MODE_ENABLED`
- New: `app/src/lib/dev-config.js` — single-source constants

1. Create `lib/dev-config.js` with `export const DEBUG_MODE_ENABLED = true`
2. Have `App.jsx` import from `@/lib/dev-config` and re-export for backward compatibility
3. Have `useEditorShellState.js` import from `@/lib/dev-config`
4. Tests from Test 4 must pass

### Step 5: Remove direct zustand access from `lib/activity/import-activity.js`
**File:** `lib/activity/import-activity.js:275`

Currently does `useStore.getState()` inside a `lib/` module.

1. Refactor `saveFile` to accept the store state/actions as a parameter instead of reaching into the store
2. The calling hook `useActivityImport.js` already has store access — it can pass the state down
3. New signature: `saveFile(fileOrPath, { storeState })` where `storeState` is `{ setGpxFilename, setActivitySummary, setProcessing, ... }`
4. `syncSceneDurationWithActivity` also takes store actions as parameters (already does)
5. Test from Test 3 must pass identically

### Step 6: Replace dynamic import with static import in `useActivityImport.js`
**File:** `features/app-shell/hooks/useActivityImport.js:59`

```js
const { default: saveFileFromPath } = await import('@/lib/activity/import-activity')
```

This is unnecessary code-splitting for a module that is already logically depended on by this hook.

1. Change to static import: `import saveFile from '@/lib/activity/import-activity'`
2. Remove the `async` from the callback's try block where the dynamic import was
3. If there's a bundle-size concern, document it — but this module is small and always loaded when the app shell mounts

### Step 7: Consolidate remaining hook composition
**Files:**
- `features/player/hooks/useOverlayPlayerState.js` — inline into `OverlayPlayer.jsx` (Phase 1 step 9 already moved `usePlayerStore` into it, now this hook is even more trivial)
- `features/overlay-editor/hooks/createOverlayMoveableHandlers.js` — inline into `useOverlayEditorState.js`

1. Move the three hook calls from `useOverlayPlayerState` into `OverlayPlayer.jsx`
2. Delete `useOverlayPlayerState.js`
3. Move the four hook calls from `createOverlayMoveableHandlers` into `useOverlayEditorState.js`
4. Delete `createOverlayMoveableHandlers.js`

---

## Acceptance Criteria

- [ ] All existing tests pass
- [ ] All new boundary/characterization tests pass
- [ ] `window.__OVRLEY_STORE__` is the ONLY store reference on `window` in dev mode
- [ ] `lib/activity/cache.js` is empty or deleted — no module-level mutable state remains
- [ ] `TitleBar.jsx` renders without crashing in jsdom (no Tauri API available)
- [ ] `useEditorShellState.js` does NOT import from `App.jsx`
- [ ] `lib/activity/import-activity.js` does NOT import or call `useStore`
- [ ] `useActivityImport.js` uses a static import for `import-activity`
- [ ] `useOverlayPlayerState.js` file deleted
- [ ] `createOverlayMoveableHandlers.js` file deleted
- [ ] ESLint zero errors
- [ ] Prettier zero diffs

## Rollback Strategy

Steps 1-3 are independent — revert individually if they break. Steps 4-5 are interdependent (dev-config + App.jsx import) — revert as a pair. Steps 6-7 are independent.

---

## Estimated File Count Impact

| Action | Files added | Files deleted | Files modified |
|--------|-------------|---------------|----------------|
| Step 1 | 0 | 0 | 1 |
| Step 2 | 0 | 1 | ~3 |
| Step 3 | 0 | 0 | 1 |
| Step 4 | 1 | 0 | 2 |
| Step 5 | 0 | 0 | 2 |
| Step 6 | 0 | 0 | 1 |
| Step 7 | 0 | 2 | 2 |
| **Total** | **1** | **3** | **~12** |
