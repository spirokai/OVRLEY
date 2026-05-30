# Phase 4 — Component Architecture: Prop Flow, CSS & Presentation

**Goal:** Fix the prop-drilling anti-pattern in `App.jsx`, replace the if-else widget editor dispatching, normalize inconsistent prop APIs in shared sections, and reorganize CSS. Also address the `OverlayEditor.jsx` dead code and pointless delegation.

**Refactor.md issues addressed:** #7, #15, #16, #17, #24

---

## TDD — Tests to Write FIRST (RED)

### Test 1: `app/src/tests/features/app-shell/AppHeader-integration.test.jsx`
Render `AppHeader` with all grouped prop objects. Verify each child section receives the correct props:
- `ActivitySection` receives `activityLabel`, `onOpenActivityFile`, template CRUD props
- `EditorToolbar` receives `backgroundMode`, `zoomLevel`, grid/snap props
- `ActionButtons` receives `renderDisabled`, `renderingVideo`, etc.
- **This pins the current prop contract before the grouped-props pattern is unwound.**

### Test 2: `app/src/tests/features/widget-editor/SidebarWidgetsTab-dispatch.test.jsx`
Test the widget-type-to-editor dispatch:
- Render `SidebarWidgetsTab` with mock store state containing one widget of each type
- Verify the correct editor component renders for each type:
  - `label` → `TextWidgetEditor`
  - `speed` (standard metric) → `MetricWidgetEditor`
  - `course` → `RouteMapWidgetEditor`
  - etc.
- **After refactoring to a dispatch map, this same test must pass identically.**

### Test 3: `app/src/tests/features/widget-editor/widgetEditorSections-api.test.jsx`
Test the `UnitsControlRow` component with both prop contracts:
- Pass `widget` + `updateWidgetData` — verify it works
- Pass explicit `checked` + `onCheckedChange` — verify it works
- **After refactoring to a single contract, only one variant should exist.**

### Test 4: `app/src/tests/features/overlay-editor/OverlayCanvas-background.test.jsx`
Render `OverlayCanvas` with different `backgroundMode` values:
- `'checker'` → checkered background div rendered
- `'black'` → black background
- `'white'` → white background
- `'video'` → video element rendered (mock video source)
- **This tests the background rendering that is part of the grouped-props pattern.**

### Test 5: `app/src/tests/features/overlay-editor/overlayEditorUtils.test.js`
Characterization test for `getEffectivePreviewFps`:
- Verify it delegates to the shared FPS resolver
- Verify it returns the same value as calling the shared resolver directly
- **After inlining, the test moves to cover the shared resolver directly.**

---

## Implementation Steps (GREEN)

### Step 1: Unwind grouped-props pattern in `App.jsx`
**File:** `app/src/App.jsx:32-131`

Current pattern:
```js
const activityControls = { activityLabel, onOpenActivityFile }
const editorControls = { backgroundMode, gridVisible, zoomLevel, ... }
const renderControls = { ... }
const templateControls = { ... }
const videoControls = { ... }
// Then each gets spread into AppHeader, which destructures them back
```

**New pattern:** Pass individual props directly. `AppHeader` already destructures the grouped objects into individual variables — this is just collapsing an unnecessary intermediate layer.

1. Remove the grouped object creation in `AppShell`
2. Pass the individual values directly to `AppHeader` as flat props
3. `AppHeader` no longer destructures grouped objects — it receives flat props and passes them directly to `ActivitySection`, `EditorToolbar`, `ActionButtons`
4. Each child section already accepts flat props — no change needed there

**Alternative (if too many props):** Keep the grouping but make each group a meaningful domain object. The current names (`activityControls`, `editorControls`) are already fine. The REAL issue is that the groups are created by hand in `AppShell` instead of coming from the hooks that own them. Consider having hooks return the group objects directly:
```js
const { activityControls, mediaControls } = useActivityImport()
const { editorPresentation } = useEditorShellState()
```
This is a middle ground — fewer groups, each owned by the hook that produces the data.

**Recommendation:** Go with the middle ground. Create a `useAppShellComposition` hook (in `App.jsx` itself, no new file) that calls the existing hooks and returns grouped objects ready for `AppHeader`. This keeps `App.jsx` clean (~50 lines) while maintaining the grouping pattern that `AppHeader` already depends on.

### Step 2: Replace if-else chain with dispatch map in `SidebarWidgetsTab.jsx`
**File:** `features/widget-editor/components/SidebarWidgetsTab.jsx`

The 20-line `renderWidgetEditor` if-else chain:

```js
// Before
function renderWidgetEditor(widget, updateWidgetData, setNumericField, sceneFontSize) {
  if (widget.type === 'label') return <TextWidgetEditor ... />
  if (isStandardMetricWidgetType(widget.type)) return <MetricWidgetEditor ... />
  if (widget.type === 'time') return <TimeWidgetEditor ... />
  if (widget.type === 'gradient') return <GradientWidgetEditor ... />
  // ...etc
}
```

```js
// After
const WIDGET_EDITOR_MAP = {
  label: TextWidgetEditor,
  time: TimeWidgetEditor,
  gradient: GradientWidgetEditor,
  course: RouteMapWidgetEditor,
  elevation: ElevationWidgetEditor,
  heading: HeadingWidgetEditor,
}

function renderWidgetEditor(widget, updateWidgetData, setNumericField, sceneFontSize) {
  if (isStandardMetricWidgetType(widget.type)) {
    return <MetricWidgetEditor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} />
  }
  const Editor = WIDGET_EDITOR_MAP[widget.type]
  if (!Editor) return null
  return <Editor widget={widget} updateWidgetData={updateWidgetData} setNumericField={setNumericField} sceneFontSize={sceneFontSize} />
}
```

This is a drop-in replacement — no behavioral change.

### Step 3: Normalize `UnitsControlRow` prop API
**File:** `features/widget-editor/components/widgetEditorSections.jsx`

Current: `UnitsControlRow` accepts both `widget`+`updateWidgetData` AND individual `checked`/`onCheckedChange` with defaults that fall back to the widget-based path.

1. Pick one contract: always pass `checked` and `onCheckedChange` explicitly
2. Remove the `widget` and `updateWidgetData` props from `UnitsControlRow`
3. Update the two call sites:
   - `MetricWidgetEditor.jsx` — already passes explicit `checked`/`onCheckedChange`
   - `IconSection` — passes `unitsField` as a pre-built element; update to pass explicit values
4. `IconSection` already supports both paths via the `unitsField` optional prop. Simplify: if `showUnitsToggle` is true, always render `UnitsControlRow` with explicit props.

### Step 4: Inline `getEffectivePreviewFps` delegation
**File:** `features/overlay-editor/utils/overlayEditorUtils.js`

1. Find where `getEffectivePreviewFps` is called — likely one place in `useOverlayEditorState.js` or `usePlaybackEngine.js`
2. Replace the call with a direct import from the shared FPS resolver (already in `lib/update-rate.js`? Or wherever it delegates to)
3. Remove `getEffectivePreviewFps` from `overlayEditorUtils.js`
4. Test from Test 5 must confirm identical behavior

### Step 5: CSS reorganization
**File:** `app/src/index.css` (500+ lines)

**Split into:**
- `app/src/index.css` (~100 lines) — only the essential: Tailwind imports, root CSS custom properties, `body`/`#root` base styles
- `app/src/styles/fonts.css` (~30 lines) — `@font-face` declarations for Evogria, Furore, Saira Stencil, Teko
- `app/src/styles/theme.css` (~110 lines) — the `@theme inline` block (all `--color-*` and `--radius-*` mappings)
- `app/src/styles/moveable.css` (~40 lines) — Moveable library overrides (`.moveable-control`, `.moveable-direction`, `.moveable-bold`, etc.)
- `app/src/styles/utilities.css` (~80 lines) — utility classes: `.bg-overlay-grid-muted`, `.sidebar-scrollbar`, `.app-shell` scaling, `input[type='number']` resets

**Import chain:** `index.css` imports the four new files via `@import`.

**This is the lowest-risk step.** CSS `@import` in Vite is resolved at build time. The cascade order is preserved.

---

### Step 6: JSDoc cleanup on all touched files

**Focus files (created or heavily modified in this phase):**
- `app/src/App.jsx` — now ~60 lines, document the composition hook inline
- `features/widget-editor/components/SidebarWidgetsTab.jsx` — dispatch map, document the map entries
- `features/widget-editor/components/widgetEditorSections.jsx` — simplified API, document the settled contract
- `features/overlay-editor/utils/overlayEditorUtils.js` — removed delegation, document remaining exports
- `app/src/styles/*.css` — new CSS files, add a comment at the top of each explaining what styles it owns

**Rule for CSS documentation:**
- Each new CSS file gets a 1-2 line comment at the top: `/* Font-face declarations for bundled overlay fonts */`
- `index.css` gets a comment listing the `@import` order and rationale

**Rule for component JSDoc:**
- Components that changed their prop contract MUST have updated `@param` docs
- The `WIDGET_EDITOR_MAP` object in SidebarWidgetsTab gets an inline comment explaining the dispatch pattern
- `UnitsControlRow` gets its settled prop contract documented

---

## Acceptance Criteria

- [ ] All existing tests pass
- [ ] All new component/integration tests pass
- [ ] `App.jsx` `AppShell` function ≤ 60 lines (down from ~100)
- [ ] `SidebarWidgetsTab.jsx` no longer contains an if-else chain for widget type dispatch
- [ ] `UnitsControlRow` accepts exactly one prop contract (explicit checked/onCheckedChange)
- [ ] `getEffectivePreviewFps` no longer exists in `overlayEditorUtils.js`
- [ ] CSS is split into 5 files, `index.css` ≤ 100 lines
- [ ] Visual regression: the app loads and renders identically (manual smoke test — open the app, check the header, sidebar, canvas, CSS-styled elements)
- [ ] All new CSS files have a top-of-file comment explaining what styles they own
- [ ] All touched components have accurate `@param` JSDoc matching their new prop contracts
- [ ] No generic `@param {*}` boilerplate remains in any touched file
- [ ] ESLint zero errors
- [ ] Prettier zero diffs

## Rollback Strategy

Steps 1-4 are independent and can be reverted individually. Step 5 (CSS split) is the riskiest visually — if layout breaks, revert the CSS split and keep the old single file, but document why it wasn't split.

## Estimated File Count Impact

| Action | Files added | Files deleted | Files modified |
|--------|-------------|---------------|----------------|
| Step 1 | 0 | 0 | 2 (App.jsx, AppHeader.jsx) |
| Step 2 | 0 | 0 | 1 |
| Step 3 | 0 | 0 | 3 |
| Step 4 | 0 | 0 | 2 |
| Step 5 | 4 | 0 | 1 |
| **Total** | **4** | **0** | **~9** |
