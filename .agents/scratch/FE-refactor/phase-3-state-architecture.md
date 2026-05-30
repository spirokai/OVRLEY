# Phase 3 — State Architecture: Decompose God Objects & Fix Data Flow

**Goal:** Split the three largest modules (`template-state.js`, `widget-config.js`, `useOverlayEditorState.js`) into focused concerns without adding unnecessary file count. Restructure the flat return objects and threaded-helpers anti-patterns.

**Refactor.md issues addressed:** #8, #9, #10, #11, #12, #13

---

## TDD — Tests to Write FIRST (RED)

### Test 1: Expand `app/src/tests/lib/templateState.test.js`
This test file exists but only has one test. Add characterization tests for every public export:
- `normalizeGlobalDefaults` — picks only known keys, fills missing defaults, normalizes colors
- `normalizeTemplateConfig` — strips derived fields from scene, normalizes labels/values/plots
- `createDurableTemplateState` — full round-trip (existing test, add edge cases)
- `createEditorEffectiveConfig` — merges globals into labels/values/plots (existing test, add edge cases)
- `syncGlobalDefaultsToConfig` — pushes changed globals into widget data
- `getEffectiveWidgetData` — resolves widget data per category (labels, values, plots)
- `applyGlobalDefaults` — backward-compatible alias

### Test 2: Expand `app/src/tests/lib/widget-config.test.js`
This test file also exists. Add characterization tests for:
- `updateWidgetsInConfig` — batch update with multiple widgets
- `deleteWidgetsInConfig` — batch delete
- `replaceWidgetInConfig` — replace with new data while preserving id
- `groupWidgetsForSidebar` — grouping and sorting behavior
- `ensureWidgetIdsInConfig` — legacy id upgrade with collision avoidance

### Test 3: `app/src/tests/features/overlay-editor/useOverlayEditorState-boundary.test.jsx`
Characterization test for the god hook BEFORE decomposing:
- Mock all store selectors
- Render a harness component that calls the hook
- Assert the returned shape: must include `widgets`, `sceneSize`, `selectedWidgetId`, `zoomLevel`, moveable handlers, etc.
- **This test pins down the current contract so decomposition can't accidentally change what callers receive.**

### Test 4: `app/src/tests/features/scene-settings/useSceneSettingsState-boundary.test.jsx`
The test file exists at `useSceneSettingsState.test.jsx`. Expand it to cover:
- `sceneStyleValue` helper behavior
- The `customResolutionAnchor` / `customFpsAnchor` transient mode logic
- `handleAspectRatioChange` → correct resolution preset selected
- `handleFpsModeChange` → correct FPS applied
- `handleOffsetBlur` → time parsing and rounding

### Test 5: `app/src/tests/lib/activity/gap-utils-direct.test.js`
New test file — test `insertIdleGapSamples`, `buildElapsedSeries`, `buildDistanceSeries` using direct imports (not through the helpers bag):
- Pass mock helpers as individual arguments where needed, or import them directly
- Verify the helpers-bag anti-pattern is NOT used in the new tests

---

## Implementation Steps (GREEN)

### Step 1: Decompose `template-state.js` (530 lines → 3 modules)
**File:** `lib/template-state.js`

Current file owns: defaults, normalization, effective config creation, global-to-config sync.

**New structure:**
- `lib/template-state.js` (~150 lines) — the orchestration layer: `createDurableTemplateState`, `createEditorEffectiveConfig`, `syncGlobalDefaultsToConfig`, `applyGlobalDefaults`, `getEffectiveWidgetData`
- `lib/template-defaults.js` (~80 lines) — all constants and defaults: `DEFAULT_GLOBAL_DEFAULTS`, `SCENE_STYLE_DEFAULTS`, `GLOBAL_DEFAULT_KEYS`, `SCENE_STYLE_KEYS`, `SCENE_DERIVED_SETTING_KEYS`, `SCENE_GLOBAL_DEFAULT_KEYS`
- `lib/template-normalization.js` (~220 lines) — all `normalize*` functions: `normalizeGlobalDefaults`, `normalizeTemplateConfig`, `normalizeScene`, `normalizeLabel`, `normalizeValue`, `normalizePlot`, `normalizePointLabel`, internal helpers `pickDefined`, `cloneSerializable`, `applyPreviewOverrides`, `mergeSceneGlobalDefaults`

**Rules:**
- `template-state.js` imports from both sub-modules and composes them
- No function is split into a file smaller than 50 lines
- The public API (`index.js` barrel or direct imports) does not change — callers still import from `@/lib/template-state`
- `template-defaults.js` re-exports through `template-state.js` so there's one import path

### Step 2: Split sidebar-presentation concern from `widget-config.js`
**File:** `lib/widget-config.js` (428 lines)

Current file mixes config mutation (CRUD) with sidebar presentation (build/group).

**New structure:**
- `lib/widget-config.js` (~300 lines) — widget identity and mutation: `ensureWidgetIdsInConfig`, `findWidgetInConfig`, `updateWidgetInConfig`, `updateWidgetsInConfig`, `replaceWidgetInConfig`, `deleteWidgetInConfig`, `deleteWidgetsInConfig`
- `lib/widget-presentation.js` (~100 lines) — presentation helpers: `buildConfigWidgets`, `groupWidgetsForSidebar`
- Internal helpers (`isDurableWidgetId`, `getNextGeneratedId`, `getStartingGeneratedId`, `updateWidgetEntry`) stay in `widget-config.js`

**Rationale:** `buildConfigWidgets` and `groupWidgetsForSidebar` are only used by the widget-drawer and widget-editor sidebar. They have no reason to live in the same module as the config mutation functions that are used everywhere (store, hooks, keyboard handlers).

### Step 3: Decompose `useOverlayEditorState.js` (384 lines → split into composed hooks)
**File:** `features/overlay-editor/hooks/useOverlayEditorState.js`

This is the god hook. It composes: `useEditorViewport`, `useEditorKeyboard`, `useOverlayPointerHandlers`, `useOverlayMoveableHandlers`, plus store selectors, derived state, selection logic, and widget draft merging.

**New structure — compose at the component level instead of in one hook:**

1. Keep `useOverlayEditorState` but reduce it to: store selectors + derived state only (widgets with defaults, scene size, preview second, global opacity/scale). ~150 lines.
2. Move selection logic (`normalizeSelectionIds`, `getPrimarySelectionId`, selection reconciliation) into a dedicated `useWidgetSelection.js` hook (~60 lines). This is used by BOTH the canvas and the sidebar.
3. Move pointer handler composition (wheel, mousedown, marquee) into the `OverlayEditor.jsx` component directly — they are already in `createOverlayPointerHandlers.js`.
4. Move moveable handler composition into `OverlayEditor.jsx` — it's already a simple composition (was `createOverlayMoveableHandlers.js`, deleted in Phase 2).
5. `OverlayEditor.jsx` calls:
   - `useOverlayEditorState()` (derived state)
   - `useWidgetSelection()` (selection management)
   - `useEditorViewport()` (viewport tracking)
   - `useEditorKeyboard()` (keyboard shortcuts)
   - `useOverlayPointerHandlers()` (wheel + pointer events)
   - And composes the moveable handlers inline (the 4 interaction hooks)

**The god hook shrinks from 384 to ~150 lines.** The component grows by ~30 lines of hook calls (acceptable — it's more readable than a single opaque hook).

### Step 4: Restructure `useSceneSettingsState.js` return value
**File:** `features/scene-settings/hooks/useSceneSettingsState.js`

Current: returns ~40 keys flat. Consumers manually deconstruct and pass each field.

1. Group the return value into logical blocks:
```js
return {
  overlaySettings: { aspectRatio, resId, scene, fpsMode, updateRate, updateRateOptions, ... },
  videoSyncSettings: { importedVideoPath, importedVideoDuration, importedVideoFps, videoSyncWarning, ... },
  globalSettings: { globalDefaults, systemFonts, sceneStyleValue, ... },
  handlers: { handleAspectRatioChange, handleResolutionChange, handleFpsModeChange, ... },
}
```
2. Update `SidebarSettingsTab.jsx` to pass the groups:
```jsx
<OverlaySettingsSection {...state.overlaySettings} handlers={state.handlers} />
<VideoSyncSection {...state.videoSyncSettings} handlers={state.handlers} />
<GlobalSettingsSection {...state.globalSettings} handlers={state.handlers} />
```
3. The `handlers` group is passed whole — each section destructures only the handlers it needs

### Step 5: Remove helpers-bag threading from `gap-utils.js`
**File:** `lib/activity/gap-utils.js`

Every internal function receives `helpers` as its last parameter. The helpers are `{ isFiniteNumber, roundValue, safeNumber, safeTimestamp, haversineDistanceMeters, calculateBearingDegrees }` — all already exported from `parse-helpers.js`.

1. Replace `helpers.isFiniteNumber` with `isFiniteNumber` imported directly at the top of `gap-utils.js`
2. Same for `roundValue`, `safeNumber`, `safeTimestamp`, `haversineDistanceMeters`
3. Remove the `helpers` parameter from all internal functions
4. `zeroFilledIdleSample` no longer needs the `helpers` bag — it only uses `roundValue`
5. `insertIdleGapSamples`, `buildElapsedSeries`, `buildDistanceSeries`, `buildProgressSeries` — these are the public exports; they should pass helpers to `createActivityHelpers()` only if truly needed. Since they import directly now, the only thing they need to compute is the `roundValue` — which is directly imported.
6. Tests from Test 5 must pass — they should import directly, no helpers bag.

### Step 6: Fix `ControlPanel.jsx` cross-feature internal imports
**File:** `features/app-shell/components/ControlPanel.jsx`

```js
import { SidebarSettingsTab } from '@/features/scene-settings'
import { SidebarWidgetsTab } from '@/features/widget-editor'
```

These are barrel imports from feature `index.js` — this is actually the CORRECT pattern for feature-to-feature communication. The issue is that both features export their internal components through `index.js`.

**However**, the original analysis flagged this as porous boundaries. Let me reconsider — actually, this is fine. The barrel exports exist precisely so features can consume each other through their public API. This is not a refactoring target. Cross-feature imports through barrel exports are the intended pattern.

**Action:** Remove this from the list of issues (it was a false positive). The `index.js` barrel exports are the feature's public contract.

---

### Step 7: JSDoc cleanup on all touched and new files

**Focus files (created or heavily modified in this phase):**
- `lib/template-state.js` — now an orchestration layer, document what it composes and why the split exists
- `lib/template-defaults.js` — new file, document each constant group and which keys belong to which concern
- `lib/template-normalization.js` — new file, document the normalization contract (what gets stripped, what gets preserved)
- `lib/widget-config.js` — reduced scope, update module-level JSDoc to reflect it no longer owns presentation
- `lib/widget-presentation.js` — new file, document the sidebar grouping logic
- `features/overlay-editor/hooks/useOverlayEditorState.js` — reduced to derived state only, document the new boundary
- `features/scene-settings/hooks/useSceneSettingsState.js` — restructured return, document the grouped return contract
- `lib/activity/gap-utils.js` — removed helpers bag anti-pattern, document the new direct-import approach

**Rule:** Same as previous phases + additionally:
- Every new file MUST have a module-level JSDoc comment (`@file` or `@module`) explaining its role in the architecture
- The split modules (`template-defaults`, `template-normalization`, `widget-presentation`) MUST clearly state what they own vs what their sibling modules own
- The `@module` comment at the top of each file is the primary documentation — individual function JSDoc can be minimal when the module doc covers the contract

---

## Acceptance Criteria

- [ ] All existing tests pass
- [ ] All new characterization tests pass
- [ ] `template-state.js` ≤ 200 lines; normalization logic lives in `template-normalization.js`
- [ ] `widget-config.js` no longer exports `buildConfigWidgets` or `groupWidgetsForSidebar` (those are in `widget-presentation.js`)
- [ ] `useOverlayEditorState.js` ≤ 200 lines; selection logic in separate hook
- [ ] `useSceneSettingsState` returns grouped objects, not a flat 40-key object
- [ ] `gap-utils.js` internal functions do NOT accept a `helpers` parameter
- [ ] All public APIs unchanged — callers import from the same paths
- [ ] All new files have a `@file`/`@module` JSDoc block explaining their architectural role
- [ ] All touched files have meaningful JSDoc — no generic boilerplate on any function
- [ ] ESLint zero errors
- [ ] Prettier zero diffs
- [ ] `OverlayEditor.jsx` renders without regressions (manual smoke test)

## Rollback Strategy

Steps 1-2 (template-state, widget-config) are file splits that preserve the public API — the barrel module re-exports everything, so callers see no change. If anything breaks, revert the split but keep the tests.

Steps 3-4 (god hook decomposition, scene-settings restructure) change internal hook structure but not behavior. The characterization tests pin the contract. If the UI doesn't render correctly, revert these steps as a pair.

Step 5 (gap-utils) is a mechanical change — direct imports replace a parameter bag. If something breaks, the test should catch it.

---

## Estimated File Count Impact

| Action | Files added | Files deleted | Files modified |
|--------|-------------|---------------|----------------|
| Step 1 | 2 | 0 | 1 |
| Step 2 | 1 | 0 | 1 |
| Step 3 | 1 | 0 | 2 |
| Step 4 | 0 | 0 | 2 |
| Step 5 | 0 | 0 | 1 |
| **Total** | **4** | **0** | **~7** |

**Net change:** +4 files, but the 3 god modules are now split into focused concerns. The new files are 80–220 lines each (not tiny).
