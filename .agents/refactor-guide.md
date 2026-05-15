# ReactJS Refactoring Guide

## Purpose

This document defines the standards, patterns, and process for refactoring Cyclemetry's React frontend. Every refactor must strictly preserve existing functionality while improving code architecture, separation of concerns, maintainability, clarity, render stability, and long-term scalability.

The primary goal is NOT architectural redesign.

The primary goal is safe, incremental extraction and cleanup while preserving:

- behavior
- synchronization
- rendering
- timing
- performance
- playback correctness
- editor interaction correctness

This codebase contains performance-sensitive rendering, playback synchronization, drag interactions, overlay compositing, timeline synchronization, and Tauri integration. Refactors must therefore be conservative and deliberate.

---

# 1. Core Principles

## 1.1 Functionality Never Changes

- **No behavioral changes**: The rendered output, user interactions, and data flow must remain identical before and after refactoring.
- **No new features or fixes**: Refactoring is purely structural. Bug fixes or feature additions must be handled separately.
- **Verify before and after**: For every refactored module, confirm that imports resolve, props match, and the component renders identically.

## 1.1.1 Definition of Behavioral Equivalence

A refactor is only valid if ALL of the following remain identical:

### UI & DOM

- Rendered UI output
- DOM structure where relied upon
- Scroll behavior
- Focus behavior
- Console warnings/errors

### Interactions

- Keyboard interactions and hotkeys
- Mouse interactions
- Drag behavior
- Event ordering

### Timing & Synchronization

- Timing of updates and side effects
- Animation timing
- Playback synchronization
- FPS/timeline synchronization
- Debounce/throttle behavior

### Data & Persistence

- Store update semantics
- Serialization formats
- Persistence behavior (localStorage keys, values, timing)
- Floating point calculations

### Async & Error

- Network request timing
- Async cancellation behavior
- Error handling behavior

If uncertain whether a change alters behavior:

- assume it DOES
- preserve the original implementation

---

## 1.2 Understand Before You Touch

Before changing any file:

1. Read the entire file.
2. Trace its data flow: where do props come from? What store selectors does it use? What side effects does it trigger?
3. Identify every place the file is imported (use `grep`).
4. Map out the component tree it participates in.
5. Understand WHY the current implementation exists before replacing it.

Some patterns may intentionally exist due to:

- playback synchronization
- stale closure prevention
- animation timing
- imperative library integration
- canvas lifecycle constraints
- Tauri integration
- browser compatibility
- drag/drop behavior
- performance constraints

Do not assume older-looking code is incorrect.

---

## 1.3 Separation of Concerns

Every file must have a single, clear responsibility:

- **UI components** = visual rendering only, no business logic.
- **Hooks** = stateful logic only, no JSX.
- **Utils** = pure functions, no React imports.
- **Data modules** = constants, configurations, lookup tables.

### UI Components

- visual rendering only
- no business logic
- no complex state orchestration

### Hooks

- stateful logic only
- no JSX
- no rendering concerns

### Utils

- pure functions
- no React imports
- no side effects

### Data Modules

- constants
- configuration
- lookup tables
- mappings

**Important**: `data/` is for static data ONLY — constants, enums, config objects, lookup tables. ALWAYS extract all constants into a single file per feature.
Pure helper functions that operate on data belong in `utils/`, NOT in `data/`.

If a file under `data/` contains function definitions (other than simple one-line accessors like `Object.fromEntries(...)`), it is in the wrong directory.

#### GOOD — data/constants.js

```js
export const OUTPUT_FORMATS = [...]
export const FORMAT_BY_VALUE = Object.fromEntries(...)
```

#### BAD — data/constants.js (functions mixed in)

```js
export const OUTPUT_FORMATS = [...]
export function getFormatLabel(value) { ... }  // ← belongs in utils/
export function isFormatMp4(value) { ... }      // ← belongs in utils/
```

#### GOOD — utils/formatUtils.js (pure functions in utils/)

```js
export function getFormatLabel(value) { ... }
export function isFormatMp4(value) { ... }
```

### File Size and Granularity Guidelines

**Target range: 50–250 lines per file.** A file significantly larger than 250 lines likely has multiple responsibilities. A file under ~30 lines likely didn't need extraction.

Group related functions into files by cohesive domain, NOT one-per-function:

```
// GOOD — cohesive domain, 3 related exports in one file
routeGeometry.js:
  - normalizeRouteGeometry
  - fitPointsToWidget
  - buildFallbackRoute

// BAD — one file per function, creates import noise
normalizeRouteGeometry.js export normalizeRouteGeometry
fitPointsToWidget.js       export fitPointsToWidget
buildFallbackRoute.js      export buildFallbackRoute
```

Split a file only when its exports serve distinct, independently reusable purposes. Do not split purely to reduce line count.

---

## 1.4 Feature-Based Organization

Group code by feature, not by technical role. Each feature owns its components, hooks, utils, and sub-components:

```txt
src/
  features/
    video-preview/
      components/
      hooks/
      utils/
      data/

    widget-editor/
      components/
      hooks/
      utils/
      data/

  components/
  hooks/
  lib/
```

---

## 1.5 Refactor Hierarchy

Prefer the least invasive refactor possible.

Order of preference:

1. Extract constants → `data/`
2. Extract pure helper functions → `utils/`
3. Extract pure presentational components → `components/`
4. Extract focused hooks → `hooks/`
5. **Extract container hook for remaining component logic** → if after steps 1–4 the component still has store selectors, side effects, or derived state computations inline, extract them into a `use<ComponentName>State.js` hook. The component should be left with only prop-wiring and JSX rendering.
6. Introduce local composition
7. Introduce feature boundaries
8. Introduce context/providers
9. Architectural redesign

Always prefer lower-impact refactors first. Step 5 is mandatory when applicable — a component with inline store access, effects, or derived state after steps 1–4 is not fully refactored.

Do NOT redesign architecture if extraction alone solves the problem.

---

## 1.6 Incremental Refactor Philosophy

Refactors must be:

- incremental
- isolated
- reversible
- easy to verify

Prefer:

- extracting one hook
- verifying
- extracting one component
- verifying

Avoid:

- rewriting entire features
- moving dozens of files simultaneously
- changing data flow during extraction

Large rewrites dramatically increase regression risk.

## 1.6.1 One Feature at a Time — Strict Rule

**Only ever refactor one feature per request. This is a hard rule.**

When the user asks to refactor something, they will explicitly name the feature or file to work on. Do NOT:

- refactor multiple features in a single pass
- refactor a feature the user did not ask about
- opportunistically "also clean up" unrelated files
- pre-emptively restructure code that happens to be nearby

If during work you discover an issue in another feature:

- document it for later discussion
- do NOT fix it
- stay scoped to the requested feature

This rule exists because:

- each feature has unique synchronization and rendering constraints
- cross-feature changes make verification impossible
- failures become difficult to attribute
- rollbacks become complex

---

## 1.7 Modern React Principles

The refactored codebase must follow modern React architectural principles.

### Prefer

- composition over inheritance
- explicit data flow
- colocated related logic
- pure functions
- derived state
- isolated side effects
- feature ownership
- predictable state ownership

### Avoid

- hidden dependencies
- effect-driven state synchronization
- duplicated derived state
- deeply coupled features
- unnecessary abstraction
- premature optimization
- implicit data flow

---

# 2. Target Directory Structure

## 2.1 Features (`src/features/`)

Each feature is a self-contained module. A feature owns a vertical slice of functionality:

```txt
src/features/
  <feature-name>/
    components/
      <Component>.jsx

    hooks/
      use<hookName>.js

    utils/
      <utilName>.js

    data/
      <dataName>.js

    index.js
```

---

## 2.2 Feature Public APIs

Each feature exposes a minimal public API through `index.js`.

Internal modules should not be imported directly across features unless absolutely necessary.

### GOOD — Cross-feature import (via barrel)

```jsx
import { RenderVideoDialog } from "@/features/render-video";
```

### GOOD — Intra-feature import (relative path)

```jsx
// Inside features/render-video/RenderVideoDialog.jsx
import { useRenderProgress } from "./hooks/useRenderProgress";
import { OUTPUT_FORMATS } from "./data/outputFormats";
```

### BAD — Cross-feature import reaching into internals

```jsx
import { useRenderPolling } from "@/features/render-video/hooks/internal/useRenderPolling";
```

Rules:

- Intra-feature: use **relative imports** (`./hooks/useX`, `../data/constants`)
- Cross-feature: always go through the **barrel index.js** (`@/features/render-video`)
- Do NOT import across features using relative path traversal (`../../other-feature/...`)

This prevents:

- tight coupling
- fragile imports
- accidental internal dependencies
- import path breakage when features move

---

## 2.3 Shared Components (`src/components/`)

Only components reused across multiple features.

```txt
src/components/
  ui/
    button.jsx
    select.jsx
    slider.jsx

  <SharedComponent>.jsx
```

---

## 2.4 Shared Hooks (`src/hooks/`)

Only hooks reused across multiple features:

```txt
src/hooks/
  useAsRef.js
  useIsomorphicLayoutEffect.js
  useLazyRef.js
  useAvailableFonts.js
  useAppStoreSelectors.js
```

---

## 2.5 Shared Utils (`src/lib/`)

Domain-agnostic pure functions:

```txt
src/lib/
  colorUtils.js
  composeRefs.js
  utils.js
```

---

## 2.6 State (`src/store/`)

Zustand store slices (already well-structured — keep as-is):

```txt
src/store/
  slices/
    createEditorSlice.js
    createMediaSlice.js
    createTemplateSlice.js
    createVideoImportSlice.js

  useStore.js
  store-utils.js
```

Do NOT restructure store architecture unless explicitly requested.

---

# 3. Correct Patterns (with Codebase Examples)

## 3.1 Pure Presentational Component

```jsx
function ExportRangeSettings({ exportRange, onExportRangeChange }) {
  return (
    <div>
      <BlurInput value={exportRange.start} onChange={...} />
      <BlurInput value={exportRange.end} onChange={...} />
    </div>
  )
}
```

### Rules

- No `useStore()`
- No side effects
- No business logic
- Accept callbacks via props
- Rendering only

---

## 3.2 Container + Presentational Separation

```jsx
<AppHeader
  activityControls={{...}}
  editorControls={{...}}
  renderControls={{...}}
  templateControls={{...}}
  videoControls={{...}}
/>
```

```jsx
function AppHeader({
  activityControls,
  editorControls,
  renderControls,
  templateControls,
  videoControls,
}) {
  ...
}
```

### Rules

- Logic in container/hooks
- UI in presentational component
- Group related props
- No hidden store access

---

## 3.3 Custom Hook Extracting Logic

```jsx
function OverlayEditor(props) {
  const editorState = useOverlayEditorState(props);

  return (
    <div>
      <OverlayCanvas {...editorState.canvasProps} />
      <OverlayMoveable {...editorState.moveableProps} />
    </div>
  );
}
```

### Rules

- Hooks own stateful orchestration
- Hooks compose smaller hooks
- Hooks stay focused
- Prefer hooks under ~150 lines

---

## 3.4 Hook Composing Sub-Hooks

```jsx
function useOverlayEditorState(props) {
  const draftState = useWidgetDraftState();
  const moveableHandlers = useOverlayMoveableHandlers(props);
  const pointerHandlers = useOverlayPointerHandlers(props);

  return {
    ...draftState,
    ...moveableHandlers,
    ...pointerHandlers,
  };
}
```

### Rules

- Single concern per sub-hook
- Colocate related hooks
- Compose rather than monolithically expanding

---

## 3.5 Zustand Store Access via Selector Hooks

```jsx
export function useAppShellStore() {
  return useStore(
    useShallow((s) => ({
      config: s.config,
      generatingImage: s.generatingImage,
      globalDefaults: s.globalDefaults,
      setConfig: s.setConfig,
      setErrorMessage: s.setErrorMessage,
    })),
  );
}
```

### Rules

- Prefer selector hooks
- Use `useShallow` for object selectors
- Avoid broad subscriptions
- Centralize store access patterns

---

## 3.6 Widget Editor Composition Pattern

```jsx
import { FontSection, PositionSection, DimensionsSection } from "./widgetEditorSections";

function MetricWidgetEditor({ widget, updateWidgetData }) {
  return (
    <div>
      <FontSection widget={widget} />
      <IconSection widget={widget} />
      <UnitsControlRow widget={widget} />
    </div>
  );
}
```

### Rules

- Shared sections extracted
- Editors remain thin
- Shared controls reused

---

## 3.7 Memoized Sub-Components

```jsx
const OverlayCanvasWidget = memo(
  function OverlayCanvasWidget({ widget, selected }) {
    return <WidgetPreview widget={widget} />;
  },
  (prev, next) => prev.widget === next.widget && prev.selected === next.selected,
);
```

### Rules

- Memoize hot render paths
- Document custom comparators
- Only optimize where necessary

---

## 3.8 State Ownership Principles

Every piece of state must have a single clear owner.

### Rules

- Server state != UI state
- Derived state should not be duplicated
- Avoid syncing multiple state sources
- Prefer computed values over duplicated state
- Minimize effect-driven synchronization
- Store transient interaction state locally
- Store global/editor-wide state in Zustand

### GOOD

```jsx
const selectedWidget = widgets.find((w) => w.id === selectedId);
```

### BAD

```jsx
const [selectedWidget, setSelectedWidget] = useState(...)
```

when the value can be derived.

---

## 3.9 Effects Should Synchronize, Not Compute

`useEffect` should only:

- synchronize external systems
- attach listeners
- manage timers
- synchronize imperative APIs

### BAD

```jsx
useEffect(() => {
  setFilteredWidgets(filterWidgets(widgets));
}, [widgets]);
```

### GOOD

```jsx
const filteredWidgets = useMemo(() => filterWidgets(widgets), [widgets]);
```

Avoid:

- prop synchronization effects
- derived state effects
- transformation effects

---

## 3.10 Render Stability and Performance

This application contains:

- animation
- playback
- drag interactions
- overlay rendering
- canvas rendering
- high-frequency updates

Render stability is critical.

### Rules

- Stable callback references
- Avoid inline object creation in hot paths
- Memoize expensive calculations
- Avoid broad Zustand subscriptions
- Prevent cascading renders
- Isolate animation loops from React where possible

### Important

Do NOT prematurely optimize.

Only optimize when:

- render frequency is high
- profiling shows impact
- the component is expensive

---

## 3.11 Context Usage Rules

React Context is NOT a replacement for proper component composition.

### Use Context only when:

- state is genuinely cross-cutting
- prop drilling becomes excessive
- state changes relatively infrequently

### Avoid Context for:

- playback state
- animation state
- timeline frame state
- rapidly changing render data

Prefer explicit props where practical.

---

# 4. Anti-Patterns (with Codebase Examples to Fix)

## 4.1 Massive Components Handling Multiple Concerns

### BAD

`RenderVideoDialog.jsx` (954 lines)

Contains:

- constants
- business logic
- sub-components
- form rendering
- polling logic

### Fix

```txt
features/render-video/
  data/renderConstants.js
  hooks/useRenderProgress.js
  components/RenderVideoDialog.jsx
  components/RenderProgressPanel.jsx
```

---

## 4.2 Store Access in Deep Leaf Components

### BAD

```jsx
function OverlayRouteWidget({ widget }) {
  const exportRange = useStore((s) => s.exportRange);
}
```

### Fix

Pass explicitly through props or selector hooks.

---

## 4.3 Mixed UI + Business Logic

### BAD

```jsx
function SidebarWidgetsTab() {
  const store = useStore();

  const addWidget = (type) => {
    const config = JSON.parse(JSON.stringify(DEFAULT_WIDGET_CONFIGS[type]));

    store.addWidget(config);
  };
}
```

### Fix

Move logic into hooks/services.

---

## 4.4 Duplicate Components

### BAD

`RenderProgressPanel` duplicated in multiple files.

### Fix

Extract shared component.

---

## 4.5 Deep Prop Drilling

### BAD

Passing:

- config
- scene
- exportRange
- defaults

through 4+ layers unchanged.

### Fix

- consolidate related props
- use composition
- use context sparingly

---

## 4.6 `JSON.parse(JSON.stringify(...))`

### BAD

```jsx
const config = JSON.parse(JSON.stringify(obj));
```

### GOOD

```jsx
const config = structuredClone(obj);
```

---

## 4.7 Ref-Heavy Mixed State

### BAD

Large combinations of refs + state interleaved.

### Fix

Extract orchestration hooks.

---

## 4.8 Hardcoded Constants Inside Components

### BAD

```jsx
const OUTPUT_FORMATS = [...]
```

inside component files.

### Fix

Extract constant values to `data/<domain>.js`. If the constants have associated pure helper functions, extract those to `utils/<domain>.js` separately — do NOT put functions in `data/`.

```txt
// GOOD — separated by type
data/renderConstants.js     → OUTPUT_FORMATS, ACCELERATION_OPTIONS (constants only)
utils/codecUtils.js         → getFormatLabel(), isFormatMp4()      (functions only)

// BAD — mixed in same file
data/renderConstants.js     → OUTPUT_FORMATS, getFormatLabel(), isFormatMp4()
```

See Section 1.3 for the exact boundary between `data/` and `utils/`.

---

## 4.9 Inline Helper Functions

### BAD

```jsx
const sanitizeNumber = () => ...
```

inside components.

### Fix

Move to reusable `utils/`. If the helper operates on domain constants, the function goes in `utils/` and the constants stay in `data/` — they are separate concerns (see Section 4.8).

---

## 4.11 Mixed UI + Store Logic (Incomplete Container Extraction)

### BAD

After extracting constants, sub-components, and helpers, a component still contains:

```jsx
function RenderVideoDialog({ phase, settings, ... }) {
  const storeVal = useStore((s) => s.someValue)  // store access
  const derivedVal = compute(storeVal, phase)     // derived state
  useEffect(() => { ... }, [storeVal])            // side effects
  const handleChange = () => { ... }             // business logic

  return ( ... )  // rendering
}
```

### Fix

Extract all store access, effects, and derived state into a container hook, leaving the component as pure JSX:

```jsx
function RenderVideoDialog({ phase, settings, ... }) {
  const state = useRenderVideoDialogState({ phase, settings })

  return ( ... )  // rendering only, no store/effects/logic
}
```

This is step 5 of the Refactor Hierarchy (Section 1.5).

---

## 4.10 Massive Hook Files

### BAD

`useRenderWorkflow.js`

- polling
- dialog state
- completion handling
- orchestration

### Fix

Split into focused hooks.

---

# 5. Refactoring Process

## Step 1: Analyze

1. Read the file completely.
2. Identify responsibilities.
3. Identify UI vs logic vs data.
4. Trace imports and usage.
5. Understand WHY patterns exist.

---

## Step 2: Plan

1. Decide feature ownership.
2. Decide extraction targets.
3. Preserve functionality.
4. Define target structure.

---

## Step 3: Extract

1. Create new file.
2. Move logic byte-identically.
3. Update imports.
4. Remove original code.
5. Verify behavior.

---

## Step 4: Verify

1. Build passes.
2. No runtime errors.
3. No visual regressions.
4. No synchronization regressions.
5. No render regressions.
6. No changed interactions.
7. No changed keyboard behavior.
8. No changed playback behavior.
9. No changed drag behavior.

---

## Step 5: Document

1. Add JSDoc to **every** exported function, hook, and component — including all utility functions in `utils/`.
2. Document module purpose at the top of the file.
3. Explain unusual patterns.
4. Explain custom memoization.
5. Run `grep "^export function"` on every file you touched and verify each has JSDoc. A function without JSDoc is an incomplete refactor.

---

## 5.1 Extraction Safety Rules

When extracting code:

- Preserve function signatures
- Preserve prop names
- Preserve hook ordering
- Preserve dependency arrays
- Preserve callback identities
- Preserve async timing
- Preserve error handling
- Preserve key props
- Preserve ref forwarding
- Preserve memoization behavior

Do NOT simplify logic unless behavior is fully understood.

---

## 5.2 Refactor Completion Checklist

A refactor is only complete when:

- App builds successfully
- No new warnings/errors
- No import cycles introduced
- No TypeErrors during interaction
- No visual regressions
- No performance regressions
- No additional renders in hot paths
- No state desynchronization
- No broken focus behavior
- No broken keyboard shortcuts
- No changed drag behavior
- No playback desynchronization
- No animation timing changes

Additionally, verify the extraction boundaries are correct:

- **No mixed concerns in `data/`**: Every file under `data/` must contain ONLY constants, config, and lookup tables. If an exported function definition exists in a `data/` file (beyond simple one-liners like `Object.fromEntries`), it is a violation of Section 1.3 — the function must move to `utils/`.
- **No residual container logic**: After extracting constants, helpers, sub-components, and sub-hooks, verify the component has no remaining store selectors, side effects, or derived state computations. If it does, a container hook (step 5 of Section 1.5) must be extracted.
- **No undocumented exported functions**: Every exported function in the refactored files must have complete JSDoc (`@param` + `@returns`). Verify with `rg "^export (function|default)"` on every file touched — any result without JSDoc is a documentation gap.

---

## 5.3 Incremental Refactor Strategy

Prefer:

- one extraction at a time
- one hook at a time
- one component at a time
- **one feature at a time** (see Section 1.6.1)

Avoid:

- massive rewrites
- full architectural redesign
- large-scale movement in a single pass
- **refactoring multiple features concurrently**
- **touching features the user did not request**

Each extraction must remain independently verifiable.

Only the feature explicitly named by the user may be refactored. If the user says "refactor X", do X and nothing else. Do not interpret the request as permission to also restructure Y and Z.

## 5.4 Worked Example: Extracting RenderProgressPanel from RenderVideoDialog

This example demonstrates the refactoring process on a real file from the codebase.

### Target

`RenderVideoDialog.jsx` (954 lines) contains an inline `RenderProgressPanel` sub-component that is nearly identical to the standalone `RenderProgressOverlay.jsx`. We extract the shared panel.

### Step 1 — Analyze

1. Read `RenderVideoDialog.jsx`. Identify that lines ~313-~450 define a `RenderProgressPanel` function component used only within this file.
2. It reads from zustand store via `useStore` selectors: `renderingVideo`, `renderProgress`, `activeRenderId`, `setRenderProgress`, `cancelRender`.
3. It is rendered conditionally when `phase === 'progress'`.
4. `RenderProgressOverlay.jsx` has near-identical JSX — the main difference is styling (dialog vs full-screen overlay).

### Step 2 — Plan

Extract `RenderProgressPanel` into a shared component that both `RenderVideoDialog` and `RenderProgressOverlay` can import. The extracted component must accept all data as props (no store access).

### Step 3 — Extract

Create `src/features/render-video/components/RenderProgressPanel.jsx`:

```jsx
/**
 * Displays render progress bar, status message, ETA, and cancel button.
 * Pure presentational — all data comes from props.
 */
function RenderProgressPanel({ progress, activeRenderId, onCancel, ...containerProps }) {
  // ... byte-identical JSX from RenderVideoDialog
}
```

Update `RenderVideoDialog.jsx`:

```jsx
// Before (inline):
function RenderProgressPanel({ ... }) { ... }
// Usage:
<RenderProgressPanel ... />

// After (imported):
import { RenderProgressPanel } from "./RenderProgressPanel";
```

Update `RenderProgressOverlay.jsx` to use the same shared component.

### Step 4 — Verify

1. Build passes (`npm run build` or equivalent).
2. Open the render dialog — progress panel renders identically.
3. Start a render — progress bar updates, cancel works.
4. Full-screen render progress overlay (from system tray or notification) renders identically.
5. No new console errors.

### Step 5 — Document

Add JSDoc to `RenderProgressPanel` explaining:

- It is a presentational component
- All data must be passed as props
- It is shared between dialog and overlay contexts

### Result

- 200+ lines removed from `RenderVideoDialog.jsx`
- Duplicate code eliminated
- One shared, documented component
- Zero behavioral changes
- Each step independently verifiable

---

## 5.5 Circular Dependency Prevention

Splitting code into features can introduce circular dependencies. A circular dependency exists when feature A imports from feature B and feature B (transitively) imports from feature A.

### Detection

After each extraction, run the build. A circular dependency produces a webpack/vite module build error with a cycle trace.

### Prevention

- Features should only depend on shared components, hooks, and lib, NOT on other features
- If two features need to share logic, extract that logic into `src/lib/` or `src/hooks/` rather than having the features import each other
- If a circular dependency appears, the code is telling you the boundary is wrong — reconsider which feature owns the shared code
- Prefer barrel exports (`index.js`) as the single entry point; never import across features with deep relative paths
- File renaming (e.g. kebab-case to camelCase) should NOT be done as part of a behavioral refactor — rename files only when explicitly asked, and verify all import paths

### Example

```txt
// BAD: features import each other
features/render-video/  -->  features/overlay-editor/
features/overlay-editor/  -->  features/render-video/

// GOOD: shared code lives in lib/hooks
features/render-video/  -->  src/lib/render-utils.js
features/overlay-editor/  -->  src/lib/render-utils.js
```

---

# 6. JSDoc Documentation Standards

Every exported declaration — functions, hooks, components, and utility functions — must have JSDoc. There are no exceptions for "simple" or "self-documenting" functions.

### GOOD — Utils function with full JSDoc

```jsx
/**
 * Checks whether the scene resolution differs from the imported video resolution.
 * @param {object} scene - Scene dimensions ({ width, height }).
 * @param {object} videoResolution - Imported video dimensions ({ width, height }).
 * @returns {boolean} True if resolutions do not match.
 */
export function resolutionsMismatch(scene, videoResolution) {
  ...
}
```

### BAD — No JSDoc (self-documenting is NOT acceptable)

```jsx
// ❌ Missing @param and @returns — will be flagged by the completion checklist
export function resolutionsMismatch(scene, videoResolution) {
  ...
}
```

### BAD — Incomplete JSDoc

```jsx
/**
 * Checks resolution mismatch.
 */
// ❌ Missing @param and @returns — every param and return must be documented
export function resolutionsMismatch(scene, videoResolution) {
  ...
}
```

### Component/Hook JSDoc

```jsx
/**
 * Provides editor overlay state management including widget selection,
 * drag handling, and multi-selection behavior.
 *
 * @param {object} props
 * @param {object} props.config
 * @param {function} props.onConfigChange
 *
 * @returns {{
 *   selectedWidgetId: string|null,
 *   selectWidget: Function
 * }}
 */
```

### Rules

- **Every** exported function declaration MUST have JSDoc — including utility functions, helpers in `utils/`, and internal helper functions that happen to be exported
- `@param` for all params (with type and description)
- `@returns` for all returns (with type and description)
- Brief module description at the top
- Use `@async` where applicable
- "The name is self-documenting" is NOT a valid reason to skip JSDoc

### Internal Section Comments — Required (All Hooks)

Every hook file, regardless of length, must use **internal section comments** to partition its body. Each logical group of statements must be preceded by a comment that names the concern. The section header must use `// <Name>` format.

#### GOOD — Hook with sections

```jsx
/**
 * Container hook for RenderVideoDialog.
 * Orchestrates derived state, side effects, and event handlers.
 */
function useRenderVideoDialogState({ phase, settings, ... }) {
  // Store selectors — shallow-pick zustand state needed for render dialog
  const { renderingVideo, renderProgress, ... } = useStore(useShallow(...))

  // Derived state — computed values derived from store and props
  const derived = useRenderVideoDerivedState({ settings })

  // Local UI state — dialog phase, FPS mode, codec selection
  const [fpsMode, setFpsMode] = useState(...)

  // Side effects — sync progress polling lifecycle with render state
  useRenderVideoEffects({ settings, derivedState: derived, ... })

  // Cancel handler — aborts the active render and resets progress
  const handleCancel = useCallback(async () => { ... }, [])

  // Backdrop click to close — closes the dialog when clicking outside
  const handleBackdropPointerDown = (event) => { ... }

  // Codec change handler — updates output format and resets bitrate to default
  const handleOutputFormatChange = (value) => { ... }

  return { ... }
}
```

#### BAD — Hook without structure (missing or terse section comments)

```jsx
function useRenderVideoDialogState({ phase, settings, ... }) {
  const derived = useRenderVideoDerivedState({ settings })
  const [fpsMode, setFpsMode] = useState(...)
  useRenderVideoEffects({ settings, derivedState: derived, ... })
  const handleCancel = useCallback(async () => { ... }, [])
  const handleBackdropPointerDown = (event) => { ... }
  const handleOutputFormatChange = (value) => { ... }
  return { ... }
}
```

Also BAD — terse section comments that don't explain what the section does:

```jsx
function useSceneSettingsState({ config, onConfigChange }) {
  // Side effects            ← Too terse — what effects? Why?
  useEffect(() => { ... }, [...])
  useEffect(() => { ... }, [...])

  // Handlers                ← Too terse — what handlers? For what domain?
  const handleAspectRatioChange = (v) => { ... }
  const handleOffsetBlur = (val) => { ... }
}
```

#### GOOD — Even a small focused hook gets a section comment

```jsx
function useRenderProgressPolling({ renderingVideo, setRenderProgress }) {
  // Polling — checks render progress via IPC every second while rendering is active
  useEffect(() => {
    if (!renderingVideo) return
    ...
  }, [renderingVideo, setRenderProgress])
}
```

### Rules for Section Headers

- Every hook file MUST have at least one `// <Name>` section comment
- **Provide 1–2 sentences of context** explaining what the section does, unless the header is absolutely self-explanatory. `// Store selectors` or `// Local UI state` are clear enough on their own. But `// Side effects` or `// Handlers` are NOT — they must describe what the effects or handlers actually do (e.g. `// Side effects — sync progress polling lifecycle with render state`).
- Groups must be ordered: data → state → effects → handlers → return
- Headers must be concise (2–5 words for the label portion before the dash, plus the explanatory sentence)
- The format is `// <Label> — <explanation>` for non-obvious sections, or just `// <Label>` for self-explanatory sections

---

# 7. Naming Conventions

| Category            | Convention             | Example                    |
| ------------------- | ---------------------- | -------------------------- |
| Component files     | `PascalCase.jsx`       | `AppHeader.jsx`            |
| Hook files          | `useCamelCase.js`      | `useOverlayEditorState.js` |
| Hook functions      | `camelCase`            | `useOverlayEditorState`    |
| Utility files       | `camelCase.js`         | `colorUtils.js`            |
| Data files          | `camelCase.js`         | `outputFormats.js`         |
| Feature directories | `kebab-case`           | `render-video/`            |
| Components          | `PascalCase`           | `OverlayCanvas`            |
| Callback props      | `onXxx`                | `onConfigChange`           |
| Constants           | `SCREAMING_SNAKE_CASE` | `OUTPUT_FORMATS`           |
| Grouped props       | `xxxControls`          | `editorControls`           |

---

# 8. What NOT to Do

1. Do NOT change behavior.
2. Do NOT add features.
3. Do NOT fix bugs unless explicitly requested.
4. Do NOT add dependencies.
5. Do NOT restructure Zustand architecture.
6. Do NOT prematurely optimize.
7. Do NOT rewrite features wholesale.
8. Do NOT remove legacy Bootstrap components unless requested.
9. Do NOT rename files used by Tauri backend integrations.
10. Do NOT replace patterns without understanding WHY they exist.
11. Do NOT introduce broad React Context usage.
12. Do NOT convert everything into hooks unnecessarily.
13. Do NOT create abstractions without repeated usage justifying them.
14. Do NOT refactor multiple features concurrently — only the one the user explicitly requested.
15. Do NOT opportunistically refactor unrelated code you encounter during work.
16. Do NOT split files into one-per-function granularity. Group related exports by cohesive domain within reasonable file boundaries (50–250 lines).

---

# 9. Testing and Verification

Do not introduce new testing frameworks or major testing infrastructure changes during refactors.

Existing tests must continue to pass unchanged.

If a refactor breaks a test, the refactor is likely behaviorally incompatible.

Manual verification remains mandatory for:

- playback
- overlays
- drag interactions
- synchronization
- export workflows
- timeline behavior
- keyboard shortcuts

---

# 10. Summary of Priority Refactors

Files grouped by target feature. Refactor one feature at a time — the effort, risk, and context is scoped per feature.

---

## 10.1 `features/render-video/`

Centralizes the render dialog, progress tracking, and export range.

| File                        | Lines | Refactor                                                                                                                                                       | Risk   |
| --------------------------- | ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `RenderVideoDialog.jsx`     | 877   | Extract constants → `data/`; extract `RenderProgressPanel` → `components/`; split settings form from progress panel; extract polling to `useRenderProgress.js` | Medium |
| `RenderProgressOverlay.jsx` | 136   | Reuse the shared `RenderProgressPanel` extracted above; becomes thin wrapper                                                                                   | Low    |
| `ExportRangeSettings.jsx`   | 86    | Move as-is — pure presentational, no logic changes                                                                                                             | Low    |
| `useRenderWorkflow.js`      | 306   | Split into `useRenderDialogState.js` + `useRenderProgressPolling.js` + `useRenderCompletion.js`                                                                | Medium |

**Total: 1,405 lines moved/split**

---

## 10.2 `features/overlay-editor/`

The largest feature — central canvas, widget preview, geometry, and all editor state logic.

| File                               | Lines | Refactor                                                                                                                                                               | Risk   |
| ---------------------------------- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `widgetPreviewRenderers.jsx`       | 1326  | Split into per-widget renderer files (`RouteRenderer.jsx`, `ElevationRenderer.jsx`, `MetricRenderer.jsx`, `TextRenderer.jsx`); remove store access pass data via props | High   |
| `useOverlayEditorState.js`         | 502   | Already composes 4 sub-hooks — further split viewport/zoom/keyboard into separate files                                                                                | Medium |
| `geometryUtils.js`                 | 738   | Split into 3 cohesive domain files: `routeGeometry.js`, `elevationGeometry.js`, keep general interpolation/SVG helpers in shared file                                  | Low    |
| `metricTextUtils.js`               | 675   | Split into 2–3 domain files: `textMeasurement.js`, `formatUtils.js`, `shadowUtils.js`                                                                                  | Low    |
| `createOverlayMoveableHandlers.js` | 439   | Split into focused handler groups (drag, resize, scale, rotate)                                                                                                        | Low    |
| `utils.js`                         | 371   | Remove re-exports; let importers reference the split domain files directly; keep barrel export at feature level only                                                   | Low    |
| `overlayEditorHelpers.js`          | 290   | Split widget DOM helpers from bounds/geometry helpers                                                                                                                  | Low    |
| `OverlayCanvas.jsx`                | 294   | Consolidate 18 props into grouped objects; extract `CanvasGrid` already done but simplify prop tunneling                                                               | Medium |
| `OverlayEditor.jsx`                | 267   | Reduce prop drilling by consolidating props passed to children                                                                                                         | Low    |
| `createOverlayPointerHandlers.js`  | 264   | Well-structured — move as-is; consider extracting zoom handler                                                                                                         | Low    |
| `OverlayMoveable.jsx`              | 120   | Move as-is                                                                                                                                                             | Low    |
| `WidgetPreview.jsx`                | 97    | Move as-is                                                                                                                                                             | Low    |

**Total: ~5,383 lines moved/split** — this is the largest feature and should be broken into multiple refactor passes.

### Refactor Passes

**Pass 1 — Constants Extraction + Utilities & Low-Risk Moves (~2,783 lines)**

Start by extracting **all data-only constants** into `data/overlayEditorConstants.js`. Constants are currently scattered across 5+ files and must be consolidated before splitting utilities, so split files can import from a single source.

**Constants to extract (26 unique values):**

| Source File | Constant(s) | Extraction Target |
|---|---|---|
| `constants.js` (existing) | `DEFAULT_GRADIENT_TRIANGLE_WIDTH`, `EDITOR_GRID_DIVISIONS` → move to data/; keep `FONT_FAMILY_MAP`, `WIDGET_ICONS`, `DEFAULT_ACTIVITY_PREVIEW`, `getEditorGridSize` where they are (data + function mix) | `data/overlayEditorConstants.js` |
| `metricTextUtils.js` | `METRIC_WIDGET_LINE_HEIGHT` (0.92), `METRIC_WIDGET_OUTER_GAP_PX` (8), `METRIC_WIDGET_UNITS_GAP_PX` (8), `GRADIENT_WIDGET_TRIANGLE_GAP_PX` (8), `GRADIENT_ZERO_EPSILON` (0.05), `MAX_GRADIENT_ABS_PERCENT` (25), `GRADIENT_ZERO_LINE_WIDTH_PX` (1), `NUMERIC_PREVIEW_VERTICAL_METRICS_TEXT` | `data/overlayEditorConstants.js` |
| `OverlayMoveable.jsx` | `CORNER_RESIZE_DIRECTIONS`, `EDGE_RESIZE_DIRECTIONS`, `MOVEABLE_ZOOM` (1.5) | `data/overlayEditorConstants.js` |
| `createOverlayMoveableHandlers.js` | `AXIS_LOCK_THRESHOLD` (3) | `data/overlayEditorConstants.js` |
| `OverlayCanvas.jsx` | `CANVAS_BACKGROUND_COLORS` map | `data/overlayEditorConstants.js` |
| `geometryUtils.js` | `ROUTE_FALLBACK_INSET_MAX_RATIO` (0.45), `ELEVATION_FALLBACK_PADDING` (18), `GEOMETRY_EPSILON` (1e-9), `SIMPLIFY_MIN_TOLERANCE` (0.05), `DENSITY_CLAMP_MIN` (0.1), `DENSITY_CLAMP_MAX` (2), `VERTICAL_SCALE_CLAMP_MIN` (0.2), `VERTICAL_SCALE_CLAMP_MAX` (4), `SIMPLIFY_TOLERANCE_CLAMP_MAX` (8) | `data/overlayEditorConstants.js` |
| `useOverlayEditorState.js` | `VIEWPORT_PADDING` (72), `ZOOM_MIN` (0.35), `ZOOM_MAX` (4), `ZOOM_DELTA` (0.05) | `data/overlayEditorConstants.js` |

Then proceed with utility splits and low-risk moves:

| Files | Lines | Work |
| ----- | ----- | ---- |
| `geometryUtils.js` | 738 | Split into `routeGeometry.js`, `elevationGeometry.js`, keep general helpers in shared file |
| `metricTextUtils.js` | 675 | Split into `textMeasurement.js`, `formatUtils.js`, `shadowUtils.js`; remove extracted constants |
| `overlayEditorHelpers.js` | 290 | Split widget DOM helpers from bounds/geometry helpers |
| `utils.js` | 371 | Remove re-exports; direct imports to split domain files; keep barrel export at feature level |
| `createOverlayMoveableHandlers.js` | 439 | Split into focused handler groups (drag, resize, scale, rotate) |
| `createOverlayPointerHandlers.js` | 264 | Move as-is; optionally extract zoom handler |
| `OverlayMoveable.jsx` | 120 | Move as-is; import constants from data/ |
| `WidgetPreview.jsx` | 97 | Move as-is |

Validation: app loads, canvas renders, widgets are movable/resizable.

**Pass 2 — Core Editor, State, & High-Risk Renderer (~2,600 lines)**

Tackle the biggest structural changes once the utility foundation is settled.

| Files | Lines | Work |
| ----- | ----- | ---- |
| `widgetPreviewRenderers.jsx` | 1,326 | Split into per-widget renderers (`RouteRenderer.jsx`, `ElevationRenderer.jsx`, `MetricRenderer.jsx`, `TextRenderer.jsx`); remove store access, pass data via props |
| `useOverlayEditorState.js` | 502 | Further split viewport/zoom/keyboard into separate hook files |
| `OverlayCanvas.jsx` | 294 | Consolidate 18 props into grouped objects; simplify prop tunneling |
| `OverlayEditor.jsx` | 267 | Reduce prop drilling by consolidating props passed to children |

Validation: full editor workflow works — add/edit/remove widgets, drag/resize/rotate, canvas renders correctly.

---

## 10.3 `features/player/`

Timeline playback, scrubbing, keyboard shortcuts.

| File                | Lines | Refactor                                                                                                                             | Risk |
| ------------------- | ----- | ------------------------------------------------------------------------------------------------------------------------------------ | ---- |
| `OverlayPlayer.jsx` | 509   | Extract playback engine to `usePlaybackEngine.js`; extract keyboard handler to `usePlayerKeyboard.js`; remove inline store selectors | High |

**Total: 509 lines**

---

## 10.4 `features/video-preview/`

Video source management, frame clock, drift correction.

| File                       | Lines | Refactor                                              | Risk |
| -------------------------- | ----- | ----------------------------------------------------- | ---- |
| `useVideoPreview.js`       | 123   | Move as-is (already composes `useVideoPlaybackClock`) | Low  |
| `useVideoPlaybackClock.js` | 184   | Move as-is (already well-factored)                    | Low  |

**Total: 307 lines moved**

---

## 10.5 `features/scene-settings/`

Sidebar settings panel — resolution, FPS, video sync, global defaults.

| File                     | Lines | Refactor                                                                                                                                             | Risk   |
| ------------------------ | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `SidebarSettingsTab.jsx` | 696   | Split by section: video settings, overlay settings, global defaults, export range; extract duplicate FPS/codec logic shared with `RenderVideoDialog` | Medium |
| `ControlPanel.jsx`       | 52    | Move as-is                                                                                                                                           | Low    |

**Total: 748 lines**

---

## 10.6 `features/widget-editor/`

Sidebar widget CRUD and per-type editors.

| File                        | Lines  | Refactor                                                                                                      | Risk   |
| --------------------------- | ------ | ------------------------------------------------------------------------------------------------------------- | ------ |
| `SidebarWidgetsTab.jsx`     | 337    | Extract CRUD logic to `useWidgetManager.js`; replace `JSON.parse(JSON.stringify(...))` with `structuredClone` | Medium |
| `ElevationWidgetEditor.jsx` | 329    | Extract reusable subsections; already uses shared sections pattern                                            | Low    |
| Remaining 6 widget editors  | 25–184 | Move as-is (already thin and well-factored)                                                                   | Low    |
| `widgetEditorSections.jsx`  | 323    | Move as-is (already well-factored shared sections)                                                            | Low    |
| `widgetFormControls.jsx`    | 259    | Move as-is (already well-factored shared controls)                                                            | Low    |
| `widgetDefinitions.js`      | 311    | Move as-is (data module)                                                                                      | Low    |

**Total: ~1,393 lines**

---

## 10.7 `features/template-manager/`

Template lifecycle: create, save, import, switch, dirty tracking.

| File                           | Lines | Refactor                                                                                                          | Risk   |
| ------------------------------ | ----- | ----------------------------------------------------------------------------------------------------------------- | ------ |
| `useTemplateManagement.js`     | 281   | Split save-status tracking → `useTemplateSaveStatus.js`; split file dialog helpers → `utils/templateFileUtils.js` | Medium |
| `NewTemplateConfirmDialog.jsx` | 53    | Clean, only requires moving                                                                                       | Low    |

**Total: 281 lines**

---

## 10.8 `features/app-shell/`

Application chrome — toolbar, title bar, error handling, backend health.

| File                  | Lines | Refactor                                                                                     | Risk |
| --------------------- | ----- | -------------------------------------------------------------------------------------------- | ---- |
| `AppHeader.jsx`       | 402   | Already pure presentational (ideal). Optionally split each control group into sub-components | Low  |
| `useBackendStatus.js` | 143   | Extract debug logging helpers → `utils/backendDebug.js`                                      | Low  |
| `TitleBar.jsx`        | 41    | Move as-is                                                                                   | Low  |
| `ErrorAlert.jsx`      | 47    | Move as-is                                                                                   | Low  |
| `LoadingOverlay.jsx`  | 57    | Move as-is                                                                                   | Low  |

**Total: 690 lines**

---

## 10.9 Suggested Refactor Sequence

Refactor in this order — each builds on the previous without conflicts:

1. **`features/render-video/`** — Self-contained, no dependencies on other features. Simplest starting point.
2. **`features/scene-settings/`** — Extracting FPS/export-range logic shared with render-video is now possible after step 1.
3. **`features/player/`** — Standalone; playback engine extraction has no external coupling.
4. **`features/template-manager/`** — Self-contained hook extraction.
5. **`features/app-shell/`** — Depends on no other feature; pure UI moves.
6. **`features/video-preview/`** — Two hooks that move together; clean.
7. **`features/widget-editor/`** — Mostly moving files as-is; low risk.
8. **`features/overlay-editor/`** — Save for last. Largest, most complex, touches the most files.

## 10.10 Outside Refactor Scope

These files are large but are NOT React components/hooks that need refactoring per the guide's rules:

| File                      | Lines | Type            | Reason                                           |
| ------------------------- | ----- | --------------- | ------------------------------------------------ |
| `color-picker.jsx`        | 1577  | UI primitive    | Already well-structured compound component       |
| `activityMetricSeries.js` | 503   | API utility     | Pure functions, no React                         |
| `activityGapUtils.js`     | 396   | API utility     | Pure functions, no React                         |
| `activityParserUtils.js`  | 364   | API utility     | Pure functions, no React                         |
| `gpxUtils.jsx`            | 350   | API utility     | Backend API, no React rendering                  |
| `export-range.js` (lib)   | 445   | Library utility | Pure functions in `lib/` per guide rules         |
| `template-snapshot.js`    | 398   | Library utility | Pure functions in `lib/` per guide rules         |
| `config-utils.js`         | 281   | Library utility | Pure functions in `lib/` per guide rules         |
| `widget-config.js`        | 304   | Library utility | Pure functions in `lib/` per guide rules         |
| `createTemplateSlice.js`  | 319   | Store slice     | Store architecture — leave as-is per guide rules |
| `backend.js`              | 256   | API module      | Tauri IPC bridge — infrastructure code           |

---

# 11. Final Guiding Principle

Refactor conservatively.

Extraction is preferred over redesign.

The goal is not to produce the most theoretically elegant architecture.

The goal is to:

- reduce complexity
- improve maintainability
- improve clarity
- preserve synchronization correctness
- preserve rendering behavior
- preserve performance stability
- reduce future regression risk

A simpler refactor with lower regression risk is always preferable to a more ambitious redesign.
