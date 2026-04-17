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

All these options must be supported for the widgets:

# Text Widget

-Font size (slider+input)
-Color (color picker)
-Text (text input)

# Speed/Heart rate/Cadence/Power Widgets

-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y
-Font size (slider+input)
-Font color (color picker)
-Display units (switch)
-Units for speed (dropdown; kmh, mph, kn, m/s)

# Time & Date Widget

-Format (dropdown with different formats displayed as DD-MM-YYYY, 24 and 12h time formats etc)
-Include format options with only date, only time, or mixed
-Font size (slider+input)
-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y

# Gradient/Slope Widget

-Value font size (slider+input)
-Value offset (slider)
-Value color (color picker)
-Triangle color positive/negative (2 color pickers)
-Show sign (switch)
-Display +/- (switch)
-Decimals (slider 0-2)
-Display triangle shape (switch)
-Triangle shape width (slider)

# Temperature Widget

-Icon display (switch)
-Icon color (color picker)
-Icon size (slider)
-Icon offset x and y (slider)
-Font size (slider+input)
-Font color (color picker)
-Display units (switch)
-Units C/F (switch with units inside)

# Route Map

-Line thickness (slider 0-20) - separately for completed/not completed
-Line color (color picker) - separately for completed/not completed
-Line opacity (slider 0-100) - separately for completed/not completed
-Marker size (slider 0-50)
-Marker color (color picker)
-Marker opacity (slider 0-100)
-Map rotation (0-360 degrees, number input, or some slider?)

# Elevation profile

-Line thickness (slider 0-20) - separately for completed/not completed
-Line color (color picker) - separately for completed/not completed
-Line opacity (slider 0-100) - separately for completed/not completed
-Marker size (slider 0-50)
-Marker color (color picker)
-Marker opacity (slider 0-100)
-Show elevation metric (switch)
-Show elevation imperial (switch)
-Offset for both elevation labels (x,y with respect to the marker; slider)

---

### Phase 4: Template handling

**Goal**: Ensure saving and loading templates works.

- **Save templates**: Templates should be saved via the element in the header of the app into the local folder "Documents/Cyclemetry" or its macOS equivalent (provide a suggestion here). ALL settings from "settings" tab and "widgets" tab should be saved, including canvas size and resolution and rendering settings as a json/xml (whatever is most approporiate).
- **Load templates**: Templates should be loaded via the element in the header of the app. ALL the global and widget settings should be loaded and recovered in the sidebar from the template
- **Draft badge**: The current draft badge should be display only if a template has been modified after loading (or never saved). i.e. if there are changes to be saved
- **Global settings**: Changeing the colors in the "settings" tab of the sidebar should change the color of the respective properties in widgets. E.g. changing "values" color should change speed/temperature/heartrate/cadence/power etc outputs; changing "labels" color should change color of all labels and text, changing "icons" color should change color of all icons in widgets. Do not link the opacity to anything, that should be a separate property for now.

#### [MODIFY] app/src/components/TemplateEditor.jsx

#### [MODIFY] app/src/components/TemplatesSection.jsx

#### [MODIFY] app/src/store/useStore.js

---

### Phase 5: GPX/FIT Handling

**Goal**: Parse incoming GPX and FIT data directly on the frontend.

- **Data Parsing**: Frontend parsing for both GPX and FIT (using `fit-file-parser`). Both file types should produce identical data format after parsing.
- **GPX parsing**: The project contains some form of data parsing, although not sure if in front end or back end. Check this and reuse this code. In any case, both GPX and FIT parsing must be done by the front end. Back end will eventually receive only the clean data and template layout.
- **Preview Readiness**: Store parsed data to allow future preview/filtering.
- **Schema**: The data should following the existing schema being fed into the python backend to ensure backwards compatibility of code.
- **Supported data**: The following data must supported. Check official SDK/API of GPX/FIT to understand the format. Some of the data is not commonly supported by GPX but they are typically supported within the <extensions> tag. The following data must be supported:
  General metadata of the activity
  Latitude / Longitude
  Course/heading
  Altitude
  Timestamp
  Speed
  Pace
  Distance
  Vertical speed
  Heart rate
  Cadence
  Power
  Left/right balance
  Torque
  Ground contact time
  Vertical oscillation
  Stride length
  Temperature
  Air pressure
  Slope/gradient
  Stroke rate (rowing)
  G-Force

#### [MODIFY] app/src/api/gpxUtils.jsx

#### [NEW] app/src/api/fitParserUtils.js

---

### Phase 6: Interactive Overlay Editor (Canvas)

**Goal**: Replace the static preview with an interactive `react-moveable` canvas.

- **Canvas Implementation**: A 2D workspace reflecting the aspect ratio/resolution.
- **react-moveable**: Implement dragging, resizing, and snapping with automatic guidelines.
- **Text Resizing**: Dragging the corner of a text label calculates and updates the `font_size`.
- **Performance**: Use `onDragEnd` and `onResizeEnd` to commit state changes.

#### [MODIFY] app/src/App.jsx

#### [NEW] app/src/components/OverlayEditor.jsx

## Verification Plan

### Manual Verification Steps (After Each Phase)

**Phase 1**: Confirm Zustand state structure in DevTools; verify no backend files were touched.
**Phase 2**: Verify Tab switching and Aspect Ratio locking; test reset icons in global settings.
**Phase 3**: Verify Quickmenu widget creation and Accordion functionality.
**Phase 4**: Test saving/loading templates and global settings default color affect all widgets.
**Phase 5**: Upload GPX and FIT files; verify successful parsing in console/store.
**Phase 6**: Test dragging/resizing on the canvas; verify guides appear; check performance (no lag during drag).
