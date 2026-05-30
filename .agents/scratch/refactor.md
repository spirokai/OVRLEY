# Refactoring Analysis — `app/src/`

> **Implementation plans:** See `.agents/scratch/FE-refactor/` for phased execution.
> - [Phase 1 — Clean Slate](FE-refactor/phase-1-clean-slate.md): Duplicates, dead code, library consolidation
> - [Phase 2 — Store Hygiene](FE-refactor/phase-2-store-hygiene.md): Mutable state, leaks, module boundaries
> - [Phase 3 — State Architecture](FE-refactor/phase-3-state-architecture.md): God objects, data flow, helpers anti-pattern
> - [Phase 4 — Component Cleanup](FE-refactor/phase-4-component-cleanup.md): Prop flow, dispatch, CSS, inconsistent APIs

Every source file under `app/src/` has been read. This document identifies concrete, actionable refactoring targets ranked by severity.

---

## CRITICAL — Duplicate Code & Drift

### 1. `bitrateDefaults.js` duplicated in two locations
**Files:** `features/render-video/data/bitrateDefaults.js` and `features/render-video/utils/bitrateDefaults.js`

Two identical files with the same purpose in different directories. Pick one location (the `data/` directory is canonical for static config) and delete the utility copy. Ensure all imports point to the survivor.

### 2. `isTauri()` / `hasTauriRuntime()` duplicated check
**Files:** `api/backend.js:30` and `features/app-shell/utils/backendDebug.js:149`

The same runtime-detection logic appears in two places:
```js
// backend.js
const isTauri = () => typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined'
// backendDebug.js
export function hasTauriRuntime() { ... }
```
Move the check into a single shared location (`lib/` or `api/backend.js` already has it as a module-private function) and have both consumers import it from one place.

### 3. `combineSeries` and `combineSeriesPreferDerived` are ~90% identical
**File:** `lib/activity/metric-series.js:300,328`

Two nearly identical 20-line functions that differ only in argument order and the fallback direction. Merge into a single function with an option flag or unified parameter order.

### 4. `setSelectedSecond` vs `setSelectedSecondTransient` — copy-paste
**File:** `store/slices/createEditorSlice.js:199-215`

`setSelectedSecondTransient` is literally `setSelectedSecond` with one extra perf-counter call. Delete the transient variant and move the perf counter into a middleware or subscriber on the store, or inline it at the call site.

### 5. Duplicate "cached promise with pending guard" pattern
**Files:** `hooks/useAvailableFonts.js:7-16` and `store/slices/createVideoImportSlice.js:3-4,98-127`

Both implement a `cachedPromise + pendingPromise` memoization pattern but with different variable naming and error handling. Extract a shared utility to `lib/`.

### 6. `clamp` is re-exported redundantly
**Files:** `lib/geometryUtils.js:8-9`, `features/widget-editor/utils/widgetUtils.js`

`geometryUtils.js` does `import { clamp } from './utils'; export { clamp };`. `widgetUtils.js` does the same. This is noise — just import `clamp` directly from `@/lib/utils` everywhere.

---

## SEVERE — Monolithic Files & God Objects

### 7. `App.jsx` — 100+ line `AppShell` component that wires everything
**File:** `app/src/App.jsx:32-131`

The `AppShell` function destructures 4 hooks and constructs deeply nested grouped prop objects (e.g. `activityControls`, `editorControls`, `renderControls`, `templateControls`, `videoControls`) that get spread back into destructuring in child components. This is not "orchestration" — it's indirection that obscures the data flow.

**Fix:** Collapse the grouped-props pattern. Let `AppHeader` call its own hooks or accept flat props for the values it actually needs. Remove the intermediate grouping objects that serve only to be immediately destructured.

### 8. `template-state.js` — 530 lines doing too many things
**File:** `lib/template-state.js`

This module owns:
- Global defaults management (lines 26-106)
- Scene normalization for save/load (lines 119-168)
- Label/value/plot normalization (lines 152-210)
- Durable template state serialization (lines 257-266)
- Editor-effective config materialization (lines 418-453)
- Global-to-config sync logic (lines 478-529)

These are distinct concerns. The "seam" between durable and effective config is a good idea but all of it lives in one file. Move the normalization helpers to a dedicated normalization module and keep `template-state.js` as the orchestration layer.

### 9. `widget-config.js` — 428 lines of config mutation
**File:** `lib/widget-config.js`

Widget ID management, find/update/replace/delete operations (single and batch), plus sidebar grouping. The file has 8 exported functions and 3 internal-only helpers. Consider whether `buildConfigWidgets` and `groupWidgetsForSidebar` (sidebar-presentation concerns) belong in the same module as the config mutation functions.

### 10. `useOverlayEditorState.js` — the "god hook" at 384 lines
**File:** `features/overlay-editor/hooks/useOverlayEditorState.js`

This hook composes 6+ other hooks and resolves derived state for the entire overlay editor. It returns a flat object mixing state, handlers, and computed values. The hook is the single point of coupling between the canvas, moveable, keyboard, viewport, and pointer systems.

**Fix:** Split into focused hooks that each own one concern (viewport, selection, widget-moveable binding) and compose them at the component level, not inside a single mega-hook.

### 11. `gap-utils.js` — `helpers` parameter threaded through 10+ functions instead of using imports
**File:** `lib/activity/gap-utils.js`

Every internal function receives a `helpers` bag (`{ isFiniteNumber, roundValue, safeNumber, ... }`) as its last parameter. The helpers are already available as direct imports from `parse-helpers.js`. Drop the `helpers` parameter from all internal functions and import directly.

---

## SEVERE — Poor Separation of Concerns

### 12. Sidebar `ControlPanel.jsx` imports feature internals directly
**File:** `features/app-shell/components/ControlPanel.jsx`

```
import { SidebarSettingsTab } from '@/features/scene-settings'
import { SidebarWidgetsTab } from '@/features/widget-editor'
```

The app-shell feature cross-imports scene-settings and widget-editor internals. This makes feature boundaries porous. Have `SidebarSettingsTab` and `SidebarWidgetsTab` be composed at the `App.jsx` level or use the feature `index.js` barrel exports.

### 13. `useSceneSettingsState` returns everything as one flat object
**File:** `features/scene-settings/hooks/useSceneSettingsState.js`

The return value mixes store state, store actions, derived values, local UI state, and handler functions — over 40 keys flat. This forces `SidebarSettingsTab.jsx` to manually deconstruct and pass each field. Group the return value into logical blocks (e.g. `overlaySettings`, `videoSyncSettings`, `globalSettings`) so consumers can pass them as coherent groups.

### 14. `OnboardingState.jsx` drags in `react-bootstrap` for a single unused component
**File:** `features/app-shell/components/OnboardingState.jsx`

Imports `Alert` from `react-bootstrap` while the rest of the app uses shadcn/ui. This component appears to be dead code — nothing imports it. Remove the file and the `react-bootstrap` dependency if it exists only for this.

### 15. `SidebarWidgetsTab.renderWidgetEditor` — massive if-else chain
**File:** `features/widget-editor/components/SidebarWidgetsTab.jsx`

A 20-line if-else chain dispatches widget type to editor component. Replace with an object lookup:
```js
const EDITOR_MAP = {
  label: TextWidgetEditor,
  course: RouteMapWidgetEditor,
  elevation: ElevationWidgetEditor,
  // ... etc
}
```
where the fallback to `isStandardMetricWidgetType` is checked first.

### 16. `widgetEditorSections.UnitsControlRow` — inconsistent prop API
**File:** `features/widget-editor/components/widgetEditorSections.jsx`

`UnitsControlRow` takes both `widget`+`updateWidgetData` (convenience) AND individual `checked`/`onCheckedChange` (explicit) props with default-fallback logic that does the same thing. Pick one contract: either always pass widget/updater or always pass explicit values. Don't do both.

### 17. CSS sprawl — `index.css` is 500+ lines
**File:** `app/src/index.css`

This file contains:
- Font-face declarations (4 fonts)
- CSS custom properties (~50 vars)
- Tailwind `@theme inline` block
- Base layer styles
- Utility classes (`bg-overlay-grid-muted`, `.sidebar-scrollbar`)
- Moveable library overrides
- `input[type='number']` resets

The tailwind-theme inline block alone is ~110 lines that could live in `tailwind.config` or a separate `theme.css`. Fonts could live in `fonts.css`. Moveable overrides could be in a component-scoped CSS module.

---

## MODERATE — Code Location & Splitting Issues

### 18. `TitleBar.jsx` crashes outside Tauri
**File:** `features/app-shell/components/TitleBar.jsx:1`

```js
import { getCurrentWindow } from '@tauri-apps/api/window'
```

This runs at module evaluation time. In a browser context this will throw. Guard with a lazy import or a platform check.

### 19. `useEditorShellState` imports `DEBUG_MODE_ENABLED` from `App.jsx`
**File:** `features/app-shell/hooks/useEditorShellState.js:26`

A hook in `features/app-shell` imports a development flag from the top-level `App.jsx`. This creates a circular-ish dependency where a feature reaches into the app root. Move `DEBUG_MODE_ENABLED` to a dedicated constants or config file.

### 20. `useActivityImport` dynamically imports `saveFileFromPath`
**File:** `features/app-shell/hooks/useActivityImport.js:59`

```js
const { default: saveFileFromPath } = await import('@/lib/activity/import-activity')
```

This dynamic import inside a callback means the activity parser is code-split, but the hook already depends on `@/lib/activity/import-activity` logically. Either make this a static import (simpler) or document why it's code-split (bundle size).

### 21. `import-activity.js` reaches into the zustand store directly
**File:** `lib/activity/import-activity.js:275`

```js
const storeState = useStore.getState()
```

A library module (`lib/`) directly accesses the zustand store. This couples the pure activity-parsing layer to the application state management. Pass the store state (or callbacks) in as parameters from the calling hook instead.

### 22. `lib/activity/cache.js` exposes a mutable module-level singleton
**File:** `lib/activity/cache.js:5`

```js
let currentParsedActivity = null
```

Plus it leaks onto `window` in dev mode (line 35). Module-level mutable state is fragile and untestable. Move this to the zustand store where the rest of the application state lives, or use a proper cache layer.

### 23. `useStore.js` leaks the store onto `window` in dev mode
**File:** `store/useStore.js:52-57`

Three different window properties set (`useStore`, `__OVRLEY_STORE__`, `__STORE__`). Pick one name. Also, this has a side effect at module evaluation time — it runs before any component mounts.

### 24. `overlayEditorUtils.js` exports `getEffectivePreviewFps` that delegates to another module
**File:** `features/overlay-editor/utils/overlayEditorUtils.js`

The function `getEffectivePreviewFps` is a one-liner that delegates to a shared FPS resolver. Either inline it at the call site or just have callers import the shared resolver directly.

---

## MODERATE — JSDoc Noise / Comment Bloat

Nearly every file has JSDoc comments on every function that restate the function name in prose. Examples:

```
/**
 * Handles timestamp ms.
 * @param {*} value ...
 */
function timestampMs(value) { ... }
```

```
/**
 * Checks whether is tauri.
 * @returns {boolean} Whether the condition is satisfied.
 */
const isTauri = () => ...
```

Generic `@param {*}` annotations with meaningless descriptions ("Value for X") add zero information. Either write meaningful JSDoc where the function contract is non-obvious, or drop the boilerplate. Do not batch-reformat — do this gradually as files are touched for other reasons.

---

## LOW — Naming & Consistency

### 25. Inconsistent filename conventions
- `use-as-ref.js` vs `useAppStoreSelectors.js` (kebab vs PascalCase)
- `color-utils.js` vs `widget-icon-data.js` (kebab vs kebab with single dash)
- Most imports use `@/` aliases but a few peers use relative `../` imports

Pick one convention and align. The most common pattern is kebab-case for utility modules and PascalCase for component/hook files, which is fine — just fix the few outliers.

### 26. `metric-series.js` functions are not exported but are tested via `parser.js`
**File:** `lib/activity/metric-series.js`

All functions are module-private. They are only testable through the public `deriveActivityMetricSeries` export. Export the individual derivation functions if they need unit tests, or explicitly mark them as internal.

### 27. `exportRange.js` lives in overlay-editor but is also re-exported from render-video
**File:** `features/overlay-editor/utils/exportRange.js`

The export range helpers are used by both the overlay editor (for scoped visualization) and the render-video feature (for export window settings). Move this to `lib/` if it's shared between features.

---

---

## HOOK CONSOLIDATION AUDIT

Every hook in the codebase was reviewed for size, call count, and logic-to-wrapping ratio. The line counts below are actual (rounded to nearest 5 lines).

### Tier 1 — Should consolidate (tiny wrappers with zero domain logic)

#### A) `use-isomorphic-layout-effect.js` (5 lines) → merge into `use-as-ref.js`
```
hooks/use-isomorphic-layout-effect.js — 5 lines, 1 export
hooks/use-as-ref.js                   — 20 lines, imports it
```
`use-isomorphic-layout-effect` is used **only** by `use-as-ref.js`. It is a single ternary. Move it into `use-as-ref.js` as a file-private constant and delete the dedicated file.

#### B) `use-as-ref.js` (20 lines) + `use-lazy-ref.js` (18 lines) → single `hooks/use-refs.js`
```
hooks/use-as-ref.js  — wraps useRef + useIsomorphicLayoutEffect
hooks/use-lazy-ref.js — wraps useRef with lazy init
```
Both are generic React ref helpers. Consolidate into one ~40-line file with two named exports (`useAsRef`, `useLazyRef`). They share a common concern (ref management) and are always imported together or not at all. A single file is easier to find and reason about than two 20-line files.

#### C) `usePlayerStore.js` (34 lines) → inline into its sole consumer
```
player/hooks/usePlayerStore.js        — 34 lines, used ONLY by useOverlayPlayerState.js
player/hooks/useOverlayPlayerState.js — 39 lines, imports it
```
`usePlayerStore` is a `useStore(useShallow(...))` selector hook used exactly once. Move the selector inline into `useOverlayPlayerState.js`. This eliminates an entire file and one hop in the call chain.

#### D) `useOverlayPlayerState.js` (39 lines) → inline into `OverlayPlayer.jsx`
```
player/hooks/useOverlayPlayerState.js — 39 lines, used ONLY by OverlayPlayer.jsx
player/components/OverlayPlayer.jsx
```
This hook does three things:
1. Calls `usePlayerStore()` (store selectors)
2. Calls `usePlaybackEngine(…)` (playback logic, the real work)
3. Calls `usePlayerKeyboard(…)` (side effect)

And returns the result of step 2 unchanged. It is a pure pass-through composition layer. `OverlayPlayer.jsx` could call these three hooks directly and reduce one level of indirection. No domain logic is lost — the orchestration just moves to the component.

#### E) `createOverlayMoveableHandlers.js` (47 lines) → inline into `useOverlayEditorState.js`
```
overlay-editor/hooks/createOverlayMoveableHandlers.js — 47 lines
overlay-editor/hooks/useOverlayEditorState.js          — 384 lines (imports it)
```
Used **only** by the god hook. Contains zero domain logic — it calls 4 other hooks and merges their results into an object. The god hook already composes 6+ other hooks directly; this one should be no different. Inline it.

### Tier 2 — Keep separate (real logic, but check the note)

#### `useEditorKeyboard.js` (41 lines) + `usePlayerKeyboard.js` (68 lines)
Both are single-effect keyboard hooks. Despite the similar shape (one `useEffect` + `window.addEventListener('keydown')`), they serve different concerns:
- Editor: Delete key → removes widgets
- Player: Space/Arrow keys → playback control

**Do NOT consolidate.** The shared key-handling logic they could extract (the `isPlaybackShortcutTarget` / `isEditableElement` guard) is not identical and would over-generalize.

#### `useRenderProgressPolling.js` (48 lines)
Single `useEffect` with `setInterval` + renderId validation. Real logic. Keep.

#### `useRenderCompletion.js` (62 lines)
Single `useEffect` with zustand `subscribe` + status-machine evaluation. Real logic. Keep.

### Tier 3 — NOT candidates (substantial, keep as-is)

These hooks are 75+ lines with significant domain logic. They earn their file:
- `usePlaybackEngine.js` (~270 lines) — core playback state machine
- `useRenderWorkflow.js` (~260 lines) — render orchestration
- `useRenderVideoDialogState.js` (~160 lines) — dialog state machine
- `useDragHandlers.js` (~196 lines) — drag interaction logic
- `useScaleHandlers.js` (~162 lines) — scale interaction logic
- `useEditorShellState.js` (~130 lines) — editor chrome state
- `useWidgetDraftState.js` (~102 lines) — dual-storage draft system
- `useBackendStatus.js` (~93 lines) — polling + strike counter
- `useResizeHandlers.js` (~97 lines) — resize interaction logic

### Hook consolidation summary

| Action | Files eliminated | Lines saved (net) |
|--------|-----------------|-------------------|
| Merge `use-isomorphic-layout-effect` into `use-as-ref.js` | 1 | ~5 (file overhead) |
| Merge `use-as-ref.js` + `use-lazy-ref.js` → `use-refs.js` | 1 | ~10 (deduplicated imports) |
| Inline `usePlayerStore` → `useOverlayPlayerState.js` | 1 | ~34 |
| Inline `useOverlayPlayerState` → `OverlayPlayer.jsx` | 1 | ~39 |
| Inline `createOverlayMoveableHandlers` → `useOverlayEditorState` | 1 | ~47 |
| **Total** | **5 files removed** | **~135 lines eliminated** |

No domain logic is lost in any of these. No hook that performs actual computation is touched. Only pure composition/indirection layers are removed.

---

## SUMMARY TABLE

| # | Severity | File | Issue |
|---|----------|------|-------|
| 1 | Critical | `render-video/data/` + `utils/` | Duplicate `bitrateDefaults.js` |
| 2 | Critical | `backend.js` + `backendDebug.js` | Duplicate `isTauri` check |
| 3 | Critical | `metric-series.js` | Duplicate combine functions |
| 4 | Critical | `createEditorSlice.js` | Duplicate setSelectedSecond variants |
| 5 | Critical | `useAvailableFonts.js` + `createVideoImportSlice.js` | Duplicate cache pattern |
| 6 | Critical | Multiple | `clamp` re-exported redundantly |
| 7 | Severe | `App.jsx` | Monolithic wiring with grouped props |
| 8 | Severe | `template-state.js` | 530-line god module |
| 9 | Severe | `widget-config.js` | 428-line config mutation module |
| 10 | Severe | `useOverlayEditorState.js` | 384-line god hook |
| 11 | Severe | `gap-utils.js` | Helpers threaded instead of imported |
| 12 | Severe | `ControlPanel.jsx` | Cross-feature internal imports |
| 13 | Severe | `useSceneSettingsState.js` | Flat 40-key return object |
| 14 | Severe | `OnboardingState.jsx` | Dead code, wrong UI library |
| 15 | Severe | `SidebarWidgetsTab.jsx` | if-else chain for editor dispatch |
| 16 | Severe | `widgetEditorSections.jsx` | Inconsistent prop API |
| 17 | Severe | `index.css` | 500+ lines, mixed concerns |
| 18 | Moderate | `TitleBar.jsx` | Tauri import crashes in browser |
| 19 | Moderate | `useEditorShellState.js` | Imports from `App.jsx` |
| 20 | Moderate | `useActivityImport.js` | Dynamic import vs logical dep |
| 21 | Moderate | `import-activity.js` | Direct zustand access from lib |
| 22 | Moderate | `activity/cache.js` | Module-level mutable singleton |
| 23 | Moderate | `useStore.js` | Triple window leak in dev |
| 24 | Moderate | `overlayEditorUtils.js` | Pointless one-liner delegation |
| 25 | Low | Multiple | Inconsistent filename casing |
| 26 | Low | `metric-series.js` | Internal functions untestable |
| 27 | Low | `exportRange.js` | Lives in wrong feature dir |
