# Skia Icons + Units Parity Plan

## Goal

Add icon and unit rendering for the Skia renderer in `src-tauri/cyclemetry_core` so these widgets match the React overlay preview with effectively 100% visual parity:

- `speed`
- `heartrate`
- `cadence`
- `power`
- `time`
- `temperature`

Primary parity targets:

- same icon presence/absence
- same icon glyph per widget type
- same icon color
- same icon size
- same icon X/Y offsets
- same unit visibility rules
- same unit strings
- same relative alignment between icon, value, and unit text
- same value vertical offset behavior
- same widget-level opacity behavior
- same overall occupied bounds so overlay positioning remains stable

## Current State

### React preview behavior

The frontend preview builds metric-like widgets in [`app/src/components/overlay-editor/WidgetPreview.jsx`](/abs/h:/tools/cyclemetry/app/src/components/overlay-editor/WidgetPreview.jsx):

- Outer layout: `inline-flex w-max flex-col items-center gap-2`
- Main row: `inline-flex items-center gap-2 whitespace-nowrap`
- Icon wrapper:
  - `marginRight: Math.max(fontSize * 0.08, 8)`
  - `transform: translate(icon_offset_x, icon_offset_y)`
  - `opacity: widget opacity`
- Icon itself:
  - `width/height = icon_size`
  - `color = icon_color`
- Value + units group:
  - `inline-flex items-end gap-2`
  - units font size = `Math.max(fontSize * 0.28, 12)`
- Main row vertical shift:
  - non-gradient widgets use `transform: translateY(value_offset)`
- Visibility defaults:
  - `show_icon ?? widget.type !== 'gradient'`
  - `show_units ?? ['speed', 'temperature'].includes(widget.type)`

Relevant editor controls are in:

- [`app/src/components/widgets/MetricWidgetEditor.jsx`](/abs/h:/tools/cyclemetry/app/src/components/widgets/MetricWidgetEditor.jsx)
- [`app/src/components/widgets/TimeWidgetEditor.jsx`](/abs/h:/tools/cyclemetry/app/src/components/widgets/TimeWidgetEditor.jsx)
- [`app/src/components/widgets/TemperatureWidgetEditor.jsx`](/abs/h:/tools/cyclemetry/app/src/components/widgets/TemperatureWidgetEditor.jsx)
- shared icon/unit controls in [`app/src/components/widgets/widgetEditorSections.jsx`](/abs/h:/tools/cyclemetry/app/src/components/widgets/widgetEditorSections.jsx)

Icon mapping currently comes from [`app/src/components/overlay-editor/constants.js`](/abs/h:/tools/cyclemetry/app/src/components/overlay-editor/constants.js):

- `speed -> Gauge`
- `heartrate -> Activity`
- `cadence -> Timer`
- `power -> Zap`
- `time -> Clock3`
- `temperature -> Thermometer`

### Skia renderer behavior

The Skia renderer currently formats each value widget into a single plain text string, then draws it once:

- text formatting in [`src-tauri/cyclemetry_core/src/render/format.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/format.rs)
- text style resolution and draw in [`src-tauri/cyclemetry_core/src/render/text.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/text.rs)
- frame render loop in [`src-tauri/cyclemetry_core/src/render/mod.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/mod.rs)

Important consequence:

- `show_icon`, `icon_color`, `icon_size`, `icon_offset_x`, `icon_offset_y` exist in `ValueConfig`, but are not rendered
- units are appended into the same string instead of being rendered as a separate smaller text run
- current Skia output cannot match React layout because it lacks:
  - icon drawing
  - per-run typography
  - per-run positioning
  - explicit baseline alignment between value and units

## Constraints and Parity Risks

### 1. Layout parity is the hard part

The missing feature is not only “draw an icon”. React uses a three-part inline layout:

- icon wrapper
- value text
- unit text

Skia needs equivalent measurement and placement for all three parts.

### 2. Coordinate semantics must remain stable

Today `ValueConfig.x` / `y` are used as the draw origin for one text string. After the change, that same origin must still produce the same perceived widget anchor as React. If this anchor drifts, users will see existing widgets move after switching renderer.

### 3. Units are not just formatting

React renders units with:

- smaller font size
- separate horizontal gap
- bottom alignment against value text

Skia must stop treating units as part of one string for the affected widget types.

### 4. Icon source parity matters

React uses Lucide icons. To avoid glyph mismatch, Skia should render the same Lucide vector definitions, not approximate them with font icons or custom redrawing.

### 5. Opacity stacking must match React behavior

React applies the widget opacity to:

- main text row
- icon wrapper

Unit text inherits that same effective opacity. Skia should apply one effective widget opacity across icon, value, and units.

## Recommended Architecture

## 1. Introduce a dedicated metric widget renderer path

Create a dedicated renderer for value widgets that need multipart layout.

Recommendation:

- keep the existing generic `draw_text()` path for simple text-only widgets
- add a new metric-value path for:
  - `speed`
  - `heartrate`
  - `cadence`
  - `power`
  - `time`
  - `temperature`

Likely file additions:

- `src-tauri/cyclemetry_core/src/render/widgets/value.rs`
- optional shared geometry helpers in `src-tauri/cyclemetry_core/src/render/widgets/common.rs`

Then update [`src-tauri/cyclemetry_core/src/render/mod.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/mod.rs) so `config.values` are split into:

- generic text-like values rendered by `draw_text`
- metric widgets rendered by the new multipart widget renderer

This is cleaner than forcing `draw_text()` to understand icons, multiple runs, gaps, and inline-flex-like layout.

## 2. Split formatting into structured parts

Add a structured formatter for metric widgets, instead of returning a single string.

Suggested type:

```rust
pub struct MetricDisplayParts {
    pub value_text: String,
    pub unit_text: Option<String>,
    pub show_icon: bool,
    pub icon_kind: Option<MetricIconKind>,
}
```

Suggested enum:

```rust
pub enum MetricIconKind {
    Gauge,
    Activity,
    Timer,
    Zap,
    Clock3,
    Thermometer,
}
```

Implementation approach:

- keep `format_value()` for legacy callers
- add a parallel `format_metric_parts()` for the new renderer
- for speed and temperature, units should be emitted separately, not appended into `value_text`
- for heartrate, cadence, and power:
  - React preview computes `unitText` (`BPM`, `RPM`, `W`)
  - but does not show it by default because `show_units` defaults false for these types
  - Skia should preserve the same behavior
- for time:
  - no units
  - icon support only

## 3. Represent widget layout explicitly

Define a layout model for the main row.

Suggested measured layout:

```rust
pub struct MetricWidgetLayout {
    pub icon_rect: Option<Rect>,
    pub value_origin: Point,
    pub unit_origin: Option<Point>,
    pub total_bounds: Rect,
}
```

Layout rules should mirror React:

- `icon_gap = max(font_size * 0.08, 8.0) * scale`
- `value_units_gap = 8.0 * scale`
- `units_font_size = max(font_size * 0.28, 12.0) * scale`
- `icon_size = value.icon_size.unwrap_or(28.0) * scale`
- `value_offset_y = value.value_offset.unwrap_or(0.0) * scale`
- `icon_offset_x/y` applied only to icon geometry, not to value/unit positions

## 4. Measure text with real font metrics

Use Skia font measurement for:

- value width
- value ascent/descent
- units width
- units ascent/descent

Reason:

- React uses inline layout and `items-end`
- the Skia equivalent should bottom-align unit text to value text using measured text boxes, not guessed constants

Recommended rule:

- compute a common row bottom
- position value baseline from value font metrics
- position units baseline from unit font metrics so both runs share the same visual bottom edge

## 5. Use Lucide vector data as the icon source of truth

Recommended source of truth:

- consume Lucide icon node definitions from `app/node_modules/lucide-react/dist/esm/icons/*.js`

Examples already present locally:

- `gauge.js`
- `clock-3.js`
- `thermometer.js`

Implementation recommendation:

- generate a checked-in Rust icon registry from those Lucide node definitions
- do not parse JS at runtime inside the renderer

Best workflow:

1. Add a small build/helper script in `app/scripts/` or `scripts/` that extracts just the needed icons.
2. Convert each icon into a compact Rust representation such as:
   - path commands
   - circles
   - lines
   - polylines
3. Check the generated registry into `cyclemetry_core`, for example:
   - `src-tauri/cyclemetry_core/src/render/icons/lucide.rs`

Why generated-and-checked-in is better:

- deterministic builds
- no runtime dependency on Node
- exact icon parity with frontend source
- easy future refresh if frontend icon choice changes

## 6. Render icons in a normalized 24x24 viewport

Lucide icons use a 24x24 coordinate system with stroke-based primitives. Preserve that.

Recommended icon rendering pipeline:

1. Build a Skia path or primitive list in 24x24 coordinates.
2. Scale uniformly to `icon_size`.
3. Translate to the layout slot.
4. Apply `icon_offset_x/y`.
5. Render with:
   - stroke color = `icon_color`
   - stroke width matching Lucide defaults after scaling
   - round caps / joins
   - no fill unless the primitive explicitly requires it

Important:

- React `<Icon style={{ width, height, color }} />` preserves aspect ratio inside the icon box
- Skia should do the same and never stretch non-uniformly

## Detailed Implementation Plan

## Phase 1: Extract the current React layout contract

Goal: document exact frontend behavior before changing Rust.

Tasks:

- codify the metric widget layout constants from `WidgetPreview.jsx`
- confirm the exact icon mapping used by preview, not sidebar/editor icons
- confirm default visibility behavior for `show_icon` and `show_units`
- confirm unit strings and casing from `app/src/components/overlay-editor/utils.js`
- capture at least 6 screenshot fixtures from the React preview:
  - speed with icon + units
  - speed without icon
  - speed without units
  - time with icon
  - temperature with icon + units
  - one case with large `icon_size` and non-zero `icon_offset_x/y`

Deliverable:

- a parity reference set that Rust can be compared against

## Phase 2: Refactor Rust formatting into structured parts

Goal: stop flattening everything into one string for the affected widgets.

Tasks:

- add a `format_metric_parts()` API in `render/format.rs`
- make it return:
  - `value_text`
  - optional `unit_text`
  - icon kind
  - whether the icon should render
- keep existing `format_value()` unchanged initially to reduce blast radius
- ensure unit casing matches React exactly:
  - `KM/H`
  - `MPH`
  - `KN`
  - `M/S`
  - `C`
  - `F`
  - `BPM`
  - `RPM`
  - `W`

Deliverable:

- structured value data usable by a multipart renderer

## Phase 3: Add text measurement helpers

Goal: support deterministic placement instead of hardcoded offsets.

Tasks:

- extend `render/text.rs` with helpers to:
  - resolve typeface without drawing
  - measure text width
  - expose ascent/descent/cap-height-like metrics needed for alignment
- keep drawing and measuring consistent by using the same `Font` creation path
- add a helper for building a `Font` from `ResolvedTextStyle` or equivalent style input

Deliverable:

- reusable measurement primitives for value and units text

## Phase 4: Build metric widget layout computation

Goal: compute icon, value, and unit positions exactly once, then draw from that layout.

Tasks:

- create `compute_metric_widget_layout(...)`
- inputs should include:
  - `ValueConfig`
  - scale
  - measured value text
  - measured units text
  - icon visibility
  - icon size
- compute:
  - icon slot width/height
  - icon wrapper right gap
  - value text origin
  - unit text origin
  - row bottom alignment
  - total widget bounds

Resolved anchor behavior:

- Skia should match the current React preview exactly.
- `value.x` / `value.y` should therefore represent the widget row position as it behaves today in React preview.
- When the icon is enabled, the text shifts right because the icon occupies space on the left.
- When the icon is disabled, the text shifts left again.

Implementation consequence:

- layout should be computed from the row start, not from a permanently fixed value-text anchor
- icon presence must change the value text X position exactly like React
- there should be no compatibility layer that pins the text in place when icons are toggled

Deliverable:

- a deterministic layout function with documented anchor semantics

## Phase 5: Add Lucide icon rendering in Rust

Goal: render the same icon geometry the frontend uses.

Tasks:

- add a Rust-side icon registry for:
  - Gauge
  - Activity
  - Timer
  - Zap
  - Clock3
  - Thermometer
- represent the minimum Lucide primitive set needed by those icons:
  - `path`
  - `circle`
  - possibly `line` / `polyline` if present in selected icons
- implement a renderer that converts the normalized icon definition into Skia draw calls
- ensure icon paint uses:
  - anti-aliasing
  - round stroke caps
  - round joins
  - stroke color from `icon_color`
  - effective alpha from widget opacity

Deliverable:

- a reusable `draw_metric_icon()` function

## Phase 6: Add the multipart metric widget draw path

Goal: draw icon + value + units together for the supported widget types.

Tasks:

- add a new renderer in `render/widgets/value.rs`
- for supported widget types:
  - call `format_metric_parts()`
  - measure value and units
  - compute layout
  - draw icon
  - draw value text
  - draw units text with reduced font size
- for unsupported or text-only values:
  - continue using the existing `draw_text()` path
- update `render/mod.rs` so the frame loop dispatches correctly

Suggested dispatch:

- `time`, `speed`, `temperature`, `heartrate`, `cadence`, `power` -> multipart metric renderer
- everything else -> existing single-text renderer

Deliverable:

- visible icon/unit output in Skia previews

## Phase 7: Preserve opacity and style parity

Goal: make the final look match React, not just the structure.

Tasks:

- use the same effective opacity for icon, value text, and unit text
- reuse existing font family, font size, color, border, and shadow settings for the value run
- decide unit style inheritance:
  - React unit text inherits color and opacity from parent
  - units should therefore use the same color/border/shadow treatment as the value text, only with smaller font size
- apply `value_offset` to the whole row, not just the value text
- apply `icon_offset_x/y` to icon only

Deliverable:

- styling behavior that matches the React hierarchy

## Phase 8: Add parity tests

Goal: make regressions detectable.

Recommended test layers:

### Unit tests

- `render/format.rs`
  - speed units remain uppercase and separate
  - temperature units remain `C` / `F`
  - time widgets produce no units

### Layout tests

- pure Rust tests for `compute_metric_widget_layout(...)`
- assert icon/value/units positions for:
  - icon on/off
  - units on/off
  - custom icon size
  - custom icon offsets
  - large font sizes

### Visual snapshot tests

- render Skia PNG fixtures for representative widgets
- compare against approved snapshots
- if possible, generate matching React preview fixtures from the same config and diff them

Deliverable:

- automated evidence that parity holds

## Phase 9: Add a debug/parity harness

Goal: speed up tuning for the last 5% of spacing differences.

Recommended tooling:

- a small debug command or script that renders:
  - React preview screenshot
  - Skia preview PNG
  - image diff
- one fixture per widget type and layout variant

This is likely the fastest way to converge on pixel-level placement.

## Exact Parity Rules To Implement

These should be treated as acceptance criteria.

### Icons

- icon glyph must match React Lucide icon for the widget type
- icon color must use `icon_color`
- icon box must use `icon_size`
- icon translation must use `icon_offset_x/y`
- icon visibility must follow `show_icon`

### Units

- unit visibility must follow `show_units`
- unit strings must match React casing exactly
- unit font size must be `max(fontSize * 0.28, 12)`
- units must be rendered as a separate text run
- units must align to the bottom of the value text, matching React `items-end`

### Main row positioning

- `value_offset` must move the full displayed row vertically
- icon offset must not move value/unit text
- widget opacity must affect icon and both text runs equally
- widget width/occupied space must match React closely enough that overlay positioning remains visually stable

## Files Likely To Change

Rust:

- [`src-tauri/cyclemetry_core/src/render/mod.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/mod.rs)
- [`src-tauri/cyclemetry_core/src/render/format.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/format.rs)
- [`src-tauri/cyclemetry_core/src/render/text.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/text.rs)
- [`src-tauri/cyclemetry_core/src/render/widgets/common.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/common.rs)
- new: `src-tauri/cyclemetry_core/src/render/widgets/value.rs`
- new: `src-tauri/cyclemetry_core/src/render/icons/*`
- [`src-tauri/cyclemetry_core/src/render/widgets/mod.rs`](/abs/h:/tools/cyclemetry/src-tauri/cyclemetry_core/src/render/widgets/mod.rs)

Optional helper generation:

- new: `app/scripts/export_lucide_metric_icons.mjs`

Frontend/test harness:

- optional debug fixture script(s) under `app/scripts/`

## Open Questions

These should be answered before implementation starts, because they affect anchor semantics and scope.

1. “100% parity” is defined against the React preview exactly as it exists today, including:
   - `Clock3` for time in preview
   - the current `marginRight = max(fontSize * 0.08, 8)`
   - the current units size formula `max(fontSize * 0.28, 12)`

2. Do we want parity only for preview PNG rendering, or also for final encoded video frames in the same milestone?
   Recommendation: implement in the shared frame renderer so both preview and final render paths benefit.

3. Is a checked-in generated Rust icon registry acceptable?
   Recommendation: yes. It is the most stable path and keeps the renderer independent of Node at runtime.

## Recommended Execution Order

1. Add structured metric formatting.
2. Add text measurement helpers.
3. Add metric layout computation using React-equivalent row-start semantics.
4. Add generated Lucide icon registry.
5. Add multipart metric widget rendering.
6. Tune spacing against React screenshot fixtures.
7. Add snapshot/layout tests.

## Acceptance Criteria

- Skia renders icons for `speed`, `heartrate`, `cadence`, `power`, `time`, and `temperature`
- Skia renders units with the same visibility and casing as React
- icon size and icon offsets match the editor controls
- units size and bottom alignment match the React preview
- `value_offset` moves the full row consistently
- no existing non-metric value widgets regress
- representative screenshot diffs between React and Skia are visually negligible
