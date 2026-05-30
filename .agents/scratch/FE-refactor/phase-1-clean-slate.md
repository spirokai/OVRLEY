# Phase 1 — Clean Slate: Duplicate Code, Dead Code & Library Consolidation

**Goal:** Eliminate all duplicated code, dead code, and redundant re-exports without changing any behavior. This phase is purely mechanical — the safest changes that yield the highest clarity-to-risk ratio.

**Refactor.md issues addressed:** #1, #2, #3, #4, #5, #6, #14, #22 (partial), #25, #27

---

## TDD — Tests to Write FIRST (RED)

### Test 1: `app/src/tests/lib/color-utils.test.js`
Verify `normalizeHexColor` and `isColorFieldKey` behavior. These functions already exist but have zero tests. Write characterization tests:
- 3-digit hex → 6-digit expansion
- 4-digit hex → truncation
- 6-digit hex → lowercase output
- 8-digit hex → strip alpha channel
- invalid input → fallback
- `isColorFieldKey` for `'color'`, `'xyz_color'`, `'color_xyz'`, `'name'`

### Test 2: `app/src/tests/lib/fonts.test.js`
Verify `normalizeFontKey`, `getFontFamilyName`, `createFontSelection`, `formatFontLabel`:
- Known recommended font → returns font family name
- Unknown font → passthrough trimmed
- Empty string → 'Arial' fallback
- CreateFontSelection returns `{ font, font_family }` shape
- FormatFontLabel strips extension

### Test 3: `app/src/tests/lib/metric-series-combine.test.js`
Characterization tests for `combineSeries` and `combineSeriesPreferDerived` BEFORE merging them:
- Direct series with values → source 'direct'
- Both null → source 'missing'
- Mixed sources → 'mixed'
- Derived-series prefer variant same tests
- Equal-length arrays
- These must pass identically AFTER the two functions merge into one.

### Test 4: `app/src/tests/store/editor-slice-transient.test.js`
Verify `setSelectedSecond` and `setSelectedSecondTransient` produce the same `selectedSecond` value. Characterization test that proves they are behaviorally equivalent modulo the perf counter side-effect.

### Test 5: `app/src/tests/lib/activity-cache.test.js`
Verify `setCurrentActivityCache`, `getCurrentParsedActivity`, `clearCurrentActivityCache`:
- Set/get round-trip
- Clear resets to null
- No window leak in test environment

### Test 6: `app/src/tests/store/useStore.window-leak.test.js`
Verify the store does NOT leak to `window` in production mode (simulated with `vi.stubEnv`).

### Test 7: `app/src/tests/lib/tauri-runtime.test.js`
Verify a single shared `hasTauriRuntime()` function returns `true`/`false` correctly when `window.__TAURI_INTERNALS__` is present/absent.

---

## Implementation Steps (GREEN)

### Step 1: Resolve duplicate `bitrateDefaults.js`
**Files:**
- `features/render-video/utils/bitrateDefaults.js` — **DELETE**
- `features/render-video/data/bitrateDefaults.js` — **KEEP**
- All files importing from `utils/bitrateDefaults` — change import to `../data/bitrateDefaults`

1. Run a grep for `bitrateDefaults` imports across the codebase
2. Change every import pointing to `utils/bitrateDefaults` to point to `data/bitrateDefaults`
3. Delete `utils/bitrateDefaults.js`
4. Run existing tests to confirm nothing broke

### Step 2: Consolidate `isTauri()` / `hasTauriRuntime()` into one location
**Files:**
- `api/backend.js:30` — remove private `isTauri()`, import from shared location
- `features/app-shell/utils/backendDebug.js:149` — remove export, import from shared location
- New: `app/src/lib/tauri-runtime.js` — single shared module

1. Create `lib/tauri-runtime.js` with `export function hasTauriRuntime()`
2. Update `api/backend.js` to import from `@/lib/tauri-runtime`
3. Update `backendDebug.js` to import from `@/lib/tauri-runtime`
4. Run existing tests

### Step 3: Merge `combineSeries` + `combineSeriesPreferDerived`
**File:** `lib/activity/metric-series.js:300–347`

1. Create a single `combineSeries(primarySeries, fallbackSeries, { preferDerived = false } = {})` function
2. Replace all call sites — there are 2 that use the "prefer derived" variant (gradient, vertical_speed)
3. Delete the old two functions
4. Tests from Test 3 must pass

### Step 4: Remove `setSelectedSecondTransient`
**File:** `store/slices/createEditorSlice.js:207-215`

1. Verify that `setSelectedSecondTransient` is only called from the `usePlaybackEngine` hook
2. Replace those call sites with `setSelectedSecond`
3. Move the `incrementPreviewPerfCounter` call into the `usePlaybackEngine` hook directly (it's a performance diagnostic, not a store concern)
4. Delete `setSelectedSecondTransient` from the slice
5. Tests from Test 4 must pass

### Step 5: Extract shared "cached promise" pattern
**Files:**
- `hooks/useAvailableFonts.js:7-16`
- `store/slices/createVideoImportSlice.js:3-4,98-127`
- New: `app/src/lib/cached-promise.js`

1. Create `lib/cached-promise.js` with `export function createCachedPromise(fn)` returning a function that caches the promise and resets on error
2. Use it in `useAvailableFonts.js` for `loadAvailableFonts`
3. Use it in `createVideoImportSlice.js` for `fetchAvailableCodecs`
4. Existing behavior must be identical — the error-reset and pending-guard semantics must match both implementations exactly.

### Step 6: Fix `clamp` re-exports
**Files:**
- `lib/geometryUtils.js:8-9` — remove `import { clamp } from './utils'; export { clamp }`
- `features/widget-editor/utils/widgetUtils.js` — remove `import { clamp } from '@/lib/utils'; export { clamp }`
- Any file importing `clamp` from these intermediary modules — change to `import { clamp } from '@/lib/utils'`

1. Grep for `} from '@/lib/geometryUtils'` and `} from.*widgetUtils` to find all clamp consumers through these intermediaries
2. Update each to import directly from `@/lib/utils`
3. Remove the re-exports

### Step 7: Remove dead `OnboardingState.jsx`
**File:** `features/app-shell/components/OnboardingState.jsx` — **DELETE**

1. Grep for `OnboardingState` imports — confirm zero imports
2. Delete the file
3. If `react-bootstrap` was installed only for this file, remove the dependency from `package.json`

### Step 8: Move `exportRange.js` to `lib/`
**Files:**
- `features/overlay-editor/utils/exportRange.js` → `lib/export-range.js`
- All importers in overlay-editor and render-video features

1. Move the file
2. Update all import paths
3. Re-export from `features/overlay-editor/index.js` for backward compatibility (deprecation comment)
4. Update render-video imports

### Step 9: Consolidate tiny hook files
**Files:**
- `hooks/use-isomorphic-layout-effect.js` — **DELETE** (inline into `use-as-ref.js`)
- `hooks/use-as-ref.js` + `hooks/use-lazy-ref.js` → `hooks/use-refs.js`
- `features/player/hooks/usePlayerStore.js` — **DELETE** (inline into `useOverlayPlayerState.js`)

1. Move the ternary from `use-isomorphic-layout-effect.js` into `use-as-ref.js` as a file-private constant
2. Combine `use-as-ref.js` and `use-lazy-ref.js` into `hooks/use-refs.js` with both exports
3. Inline `usePlayerStore` into `useOverlayPlayerState.js`
4. Update all imports

### Step 10: Fix filename inconsistencies
- Rename `use-as-ref.js` → already handled in step 9
- Ensure `useAppStoreSelectors.js` follows the same convention (it does — it's a "use" hook)
- No other outliers identified — the codebase is already fairly consistent

---

## Acceptance Criteria

- [ ] All existing tests pass (`pnpm test`)
- [ ] All new characterization tests pass
- [ ] `bitrateDefaults.js` exists in exactly one location
- [ ] `hasTauriRuntime()` exists in exactly one location (`lib/tauri-runtime.js`)
- [ ] `combineSeries` is a single function with the `preferDerived` option
- [ ] `setSelectedSecondTransient` no longer exists in the store slice
- [ ] Cached promise pattern lives in `lib/cached-promise.js`
- [ ] `clamp` is imported directly from `@/lib/utils` everywhere (no intermediaries)
- [ ] `OnboardingState.jsx` is deleted and `react-bootstrap` dependency removed
- [ ] `exportRange.js` lives in `lib/`
- [ ] `use-isomorphic-layout-effect.js` file deleted (logic in `use-refs.js`)
- [ ] `usePlayerStore.js` file deleted (logic in `useOverlayPlayerState.js`)
- [ ] ESLint reports zero errors (`pnpm lint`)
- [ ] Prettier reports zero diffs (`pnpm format`)

## Rollback Strategy

Every step is independent and atomic. Each step is a single commit. If any step breaks tests, revert that commit. The TDD tests for each step are written first and must pass before the step is considered done.

## Estimated File Count Impact

| Action | Files added | Files deleted | Files modified |
|--------|-------------|---------------|----------------|
| Step 1 | 0 | 1 | ~3 |
| Step 2 | 1 | 0 | 2 |
| Step 3 | 0 | 0 | 1 |
| Step 4 | 0 | 0 | ~2 |
| Step 5 | 1 | 0 | 2 |
| Step 6 | 0 | 0 | ~5 |
| Step 7 | 0 | 1 | 0 |
| Step 8 | 1 | 1 | ~5 |
| Step 9 | 1 | 2 | ~5 |
| **Total** | **4** | **5** | **~25** |
