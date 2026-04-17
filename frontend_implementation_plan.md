# Frontend Refactor Implementation Plan (FINAL)

This plan outlines the steps to refactor the frontend of `cyclemetry-reloaded` to match the requirements specified in `frontend-refactor.md`. The goal is to create a pixel-perfect, interactive overlay editor with a tabbed sidebar, backed by Zustand and Immer.

## Critical Constraints

> [!IMPORTANT]
>
> - **Frontend Only**: Under no circumstances should any files outside the `app/` directory (backend, scripts, etc.) be modified.
> - **Phase-by-Phase Execution**: I will stop and wait for manual testing and approval after completing each phase.

## Proposed Changes

---

### Phase 1: Architecture & State Management

**Goal**: Prepare the foundation for reactive updates and complex widget state, prioritizing performance.

- **Immer Integration**: Introduce `immer` to simplify deep state updates within the Zustand store.
- **Performance First**: Ensure the Zustand store is optimized to avoid unnecessary and excessive rerenders. Updates from the canvas should only commit on action completion (e.g., `onDragEnd`, `onResizeEnd`).
- **Store Expansion**: Update `useStore.js` to include the new global settings:
  - Widget update rate (1/1, 1/2, 1/4, 1/8)
  - Export range (custom/all)
  - Default font and color tokens (border, shadow, opacity, scale)
- **Standardized Elements schema**: Define a strict JSON schema in Zustand for all supported widgets, reusing the `templates/safa_brian_a_4k_gradient.json` structure where feasible.
- **Decoupling**: Do NOT hook up the new Zustand state with the Rust/Python backend yet.

#### [MODIFY] app/src/store/useStore.js

---

### Phase 2: Sidebar UI Refactoring (Tabs & Global Settings)

**Goal**: Reorganize `ControlPanel.jsx` to use a tabbed interface and implement the new Global Settings.

- **Tabs Component**: Introduce a `Tabs` layout (`Settings` and `Widgets`).
- **Settings Tab**:
  - Keep the Template section at the top.
  - Implement Aspect Ratio dropdown (automatically lock height based on width).
  - Implement FPS and Widget Update Rate sliders.
  - Implement Export Range selectors.
  - Implement Global Defaults (Fonts, Colors, Borders, Shadows, Opacity, UI Scale).
  - Add a top-right reset icon.

#### [MODIFY] app/src/components/ControlPanel.jsx

#### [NEW] app/src/components/SidebarSettingsTab.jsx

---

### Phase 3: Widgets Tab & Quickmenu

**Goal**: Implement the Quickmenu for adding widgets and the Accordion for managing them.

- **Quickmenu**: A 2x5 grid of icon buttons. Clicking a button creates a default widget in the store.
- **Accordion**: Each accordion item represents an active widget. Only one can be expanded at a time. The widgets are ordered in a way that same type widgets are grouped together and ordered by their type name alphabetically, the groups are separated by a horizontal line. The default active widget is the last active widget. if no active widget is selected, the first widget is selected.
- **Clicking the widget**: Expends the widget's configuration panel in the Accordion; currently clicking it redirects the user to a different tab/page which is wrong.
- **Widget Customization Forms**: Specific forms for each widget type (Text, Telemetry, Time, Gradient, etc.).
- **Reset Icons**: Each widget configuration panel must have a small top-right reset icon.
- **Refresh Preview**: Remove this, the preview should update automatically.

#### [MODIFY] app/src/components/ControlPanel.jsx

#### [NEW] app/src/components/SidebarWidgetsTab.jsx

---

### Phase 4: Interactive Overlay Editor (Canvas)

**Goal**: Replace the static preview with an interactive `react-moveable` canvas.

- **Canvas Implementation**: A 2D workspace reflecting the aspect ratio/resolution.
- **react-moveable**: Implement dragging, resizing, and snapping with automatic guidelines.
- **Text Resizing**: Dragging the corner of a text label calculates and updates the `font_size`.
- **Performance**: Use `onDragEnd` and `onResizeEnd` to commit state changes.

#### [MODIFY] app/src/App.jsx

#### [NEW] app/src/components/OverlayEditor.jsx

---

### Phase 5: GPX/FIT Handling

**Goal**: Parse incoming GPX and FIT data directly on the frontend.

- **Data Parsing**: Frontend parsing for both GPX and FIT (using `fit-file-parser`).
- **Preview Readiness**: Store parsed data to allow future preview/filtering.

#### [MODIFY] app/src/api/gpxUtils.jsx

#### [NEW] app/src/api/fitParserUtils.js

## Verification Plan

### Manual Verification Steps (After Each Phase)

**Phase 1**: Confirm Zustand state structure in DevTools; verify no backend files were touched.
**Phase 2**: Verify Tab switching and Aspect Ratio locking; test reset icons in global settings.
**Phase 3**: Verify Quickmenu widget creation and Accordion functionality.
**Phase 4**: Test dragging/resizing on the canvas; verify guides appear; check performance (no lag during drag).
**Phase 5**: Upload GPX and FIT files; verify successful parsing in console/store.
