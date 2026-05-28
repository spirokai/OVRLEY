Status: ready-for-agent

# Animated Gauge Widgets

## Problem Statement

OVRLEY currently displays metric values as text widgets (icon + value + unit). Users cannot display metrics as visual gauges — there is no support for bar charts, arc dials, or corner-mounted gauge visualizations in either the Rust backend or the React frontend preview.

Users creating professional cycling/running video overlays want richer visual representations of their metrics beyond plain text, matching the gauge-based dashboard aesthetics common in sports telemetry dashboards.

## Solution

Add five display types to the existing value widget system: text (current behavior), linear bar, segmented bars, arc dial, and corner gauge. The user selects the display type via a dropdown in the widget editor. All gauge types share common styling options (track thickness, colors, border, min/max labels) and derive min/max ranges automatically from the loaded activity data.

The implementation is Skia-backend-first with the React frontend preview mirrored to produce identical visual output via SVG.

## User Stories

1. As an overlay editor user, I want to choose a display type (text, linear, bars, arc, corner) for any metric value widget via a dropdown, so that I can switch between text and visual gauge representations without creating separate widgets.
2. As an overlay editor user, I want the linear gauge to show only a continuous filled bar with no text value or units, so that the metric is communicated purely visually.
3. As an overlay editor user, I want to orient linear gauges horizontally or vertically, so that I can fit them into different layout orientations.
4. As an overlay editor user, I want the bars gauge to show a discrete array of bars separated by configurable gaps, so that I can create a stepped/segmented visual indicator.
5. As an overlay editor user, I want to configure the number of bars and the gap between them, so that I can control the granularity of the stepped display.
6. As an overlay editor user, I want each bar to be either fully filled or fully empty (not partially filled), so that the bars act as discrete buckets.
7. As an overlay editor user, I want to orient bars gauges horizontally or vertically, so that I can fit them into different layout orientations.
8. As an overlay editor user, I want the arc gauge to render a track shaped as a circular arc with a configurable angle, so that I can create dial-style visualizations.
9. As an overlay editor user, I want the arc angle to range from 30° to 360°, with 180° producing a half-circle symmetric along the vertical axis, so that I can create anything from a small arc to a full circle.
10. As an overlay editor user, I want the arc gauge fill to sweep from the leftmost point of the arc to the rightmost point, so that the reading direction is always left-to-right.
11. As an overlay editor user, I want the metric text widget (value + unit + optional icon) to remain visible in the center of arc gauges, so that I get both visual and numeric readouts.
12. As an overlay editor user, I want to customize the position of the inner metric widget within the arc via x/y offset from center, so that I can avoid overlap with the arc track.
13. As an overlay editor user, I want the unit to appear below the value (not next to it) in arc/corner gauges, so that the layout fits better inside the arc geometry.
14. As an overlay editor user, I want to toggle the icon on/off in arc/corner gauges, so that I can control visual density.
15. As an overlay editor user, I want the corner gauge to be a fixed 90° arc oriented as top-left, top-right, bottom-left, or bottom-right, so that I can place gauges in the corners of my overlay.
16. As an overlay editor user, I want the corner gauge fill to sweep left-to-right (clockwise for bottom corners, counter-clockwise for top corners), so that the reading direction is consistent.
17. As an overlay editor user, I want the corner gauge to keep the metric text widget visible and customizable like the arc gauge, so that I get both visual and numeric readouts.
18. As an overlay editor user, I want to resize all gauge types in both width and height using standard resize handles, so that I can fit them into any overlay layout.
19. As an overlay editor user, I want to configure track thickness, corner radius, border thickness, and border color for all gauge types, so that I can match the gauge style to my overlay design.
20. As an overlay editor user, I want to configure separate colors and opacity for the empty track and the filled portion of the track, so that I can create high-contrast visual indicators.
21. As an overlay editor user, I want to toggle min/max labels on and off, configure their font size and color, so that viewers can understand the gauge's scale.
22. As an overlay editor user, I want the min/max range to be automatically derived from the activity data for that metric, so that the gauge always fills meaningfully without manual configuration.
23. As an overlay editor user, I want the gauge preview to show a placeholder (0–100 range, 50% fill) when no activity is loaded, so that I can design the gauge appearance before loading data.
24. As an overlay editor user, I want the widget preview in the overlay editor to match the exported render exactly, so that what I see during editing is what I get in the final video.
25. As a template author, I want existing templates with value widgets to continue working unchanged (defaulting to text display type), so that my templates remain backward-compatible.
26. As an overlay editor user, I want the empty track, borders, and labels to be rendered into the static cached layer (not redrawn each frame), so that the per-frame rendering cost is minimized.
27. As an overlay editor user, I want only the filled portion of the track and the dynamic value text to be redrawn each frame, so that gauge rendering is performant at high framerates.

## Implementation Decisions

### Display type model

- `display_type` is added as a property on `ValueConfig` with values: `"text"`, `"linear"`, `"bars"`, `"arc"`, `"corner"`.
- Existing templates without this field default to `"text"` — full backward compatibility.
- The widget editor dropdown sets this field; all other value widget properties (metric kind, position, font, colors, etc.) remain shared.

### Min/max derivation

- Min/max are auto-derived from the activity data's min/max for that metric series.
- Frontend computes min/max locally from parsed metric series. Backend independently computes the same during the render prepare phase.
- When no activity is loaded, frontend shows placeholder with range 0–100 and fill at 50%.
- Values outside min/max (floating-point edge cases) are clamped to 100% fill.

### Fill calculation

- Fill percentage = `(value - min) / (max - min)`, clamped to [0, 1].
- Linear gauge: fill sweeps left-to-right (horizontal) or bottom-to-top (vertical).
- Bars gauge: bar `i` is filled if `value >= min + ((i+1) / bar_count) * (max - min)`. Bars are discrete — fully filled or fully empty.
- Arc gauge: fill sweeps from leftmost arc endpoint clockwise or counter-clockwise to the value position, always reading left-to-right.
- Corner gauge: same as arc but fixed 90°. Sweep direction depends on corner:
  - Top-left: left edge → top edge (counter-clockwise)
  - Top-right: top edge → right edge (counter-clockwise)
  - Bottom-left: left edge → bottom edge (clockwise)
  - Bottom-right: bottom edge → right edge (clockwise)

### Static vs dynamic rendering split

- **Linear/Bars:**
  - Static layer: empty track + border + min/max labels (if enabled). No text widget rendered.
  - Dynamic per frame: filled portion only.
- **Arc/Corner:**
  - Static layer: empty arc track + border + min/max labels (if enabled) + unit label + icon (if enabled).
  - Dynamic per frame: arc fill + value text.
- **Text (current):**
  - Static layer: icon + unit label.
  - Dynamic per frame: value text.

### Gauge config fields on ValueConfig

| Field | Type | Applies to | Description |
|-------|------|------------|-------------|
| `display_type` | `"text"` \| `"linear"` \| `"bars"` \| `"arc"` \| `"corner"` | all | Visual representation mode |
| `orientation` | `"horizontal"` \| `"vertical"` | linear, bars | Track direction |
| `bar_count` | int | bars | Number of discrete bars |
| `bar_gap` | px | bars | Gap between bars in pixels |
| `arc_angle` | degrees (30–360) | arc | Sweep angle of the arc |
| `corner_orientation` | `"top-left"` \| `"top-right"` \| `"bottom-left"` \| `"bottom-right"` | corner | Which corner the gauge occupies |
| `track_thickness` | px | all gauges | Stroke width for arc/corner, bar height for linear, bar width for bars |
| `track_corner_radius` | px | all gauges | Rounds linear track corners, individual bar corners, arc end caps |
| `track_border_thickness` | px | all gauges | Border stroke width around track |
| `track_border_color` | hex | all gauges | Border color |
| `track_empty_color` | hex | all gauges | Empty track fill color |
| `track_empty_opacity` | 0–1 | all gauges | Empty track opacity |
| `track_filled_color` | hex | all gauges | Filled track fill color |
| `track_filled_opacity` | 0–1 | all gauges | Filled track opacity |
| `show_min_max_labels` | bool | all gauges | Toggle min/max labels |
| `min_max_label_font_size` | px | all gauges | Label font size |
| `min_max_label_color` | hex | all gauges | Label color |
| `inner_widget_offset_x` | px | arc, corner | X offset of inner text widget from arc center |
| `inner_widget_offset_y` | px | arc, corner | Y offset of inner text widget from arc center |

### Arc geometry

- Arc is symmetric along the vertical axis. 0° reference is at top (12 o'clock).
- For angle θ, the arc spans from `270° - θ/2` to `270° + θ/2` in standard angle convention.
- Arc radius is derived from widget bounding box: `min(width, height) / 2 - padding` (padding accounts for track thickness and labels).
- Track thickness is a fixed pixel value; arc radius auto-adjusts to fit within the bounding box.

### Inner widget layout in arc/corner

- Unit appears below value (vertical stacking), not beside it (horizontal row).
- Layout: `[icon] [value]` on primary line, `[unit]` centered or left-aligned below value.
- Icon is toggleable (existing `show_icon` field respected).
- User controls icon size and value font size. No auto-sizing or overlap clamping — overlap is the user's responsibility.
- Position controlled by `inner_widget_offset_x` and `inner_widget_offset_y` relative to arc center.

### Bar sizing

- Given widget width W, bar_count N, gap G: each bar width = `(W - (N-1)*G) / N`.
- Gap is clamped to ensure minimum 2px bar width.

### Rendering strategy (Skia backend)

- During `prepare_render_assets`, gauge static layers are baked into `SkiaImage` surfaces per widget, following the existing route/elevation widget cache pattern.
- Per-frame in `render_frame_rgba`: restore base layer, draw dynamic fill portions and value text.
- Arc drawing uses `canvas.draw_arc()` or `canvas.draw_path()` with appropriate start/end angles.
- Linear/bars use `canvas.draw_rect()` with corner radius via `RRect`.
- Track fill uses `Paint` with configured color and opacity.

### Rendering strategy (React frontend SVG preview)

- Linear gauge: SVG `<rect>` elements for empty and filled tracks.
- Bars gauge: SVG `<rect>` elements per bar, filled or empty based on discrete bucket logic.
- Arc/corner: SVG `<path>` with arc commands (`A`), or `<circle>`/`<ellipse>` with `stroke-dasharray` for partial arcs.
- Min/max labels: SVG `<text>` elements.
- Inner widget: SVG `<text>` elements following the vertical stacking layout.
- Preview geometry calculations mirror the Skia backend exactly.

### Config schema changes

- `ValueConfig` gains gauge-specific fields listed above.
- `render_data_requirements()` unchanged — gauge display type does not affect which telemetry series are needed (same metric kind, different visual representation).

### Module organization

**Rust backend (within `src-tauri/ovrley_core/src/`):**

- `config/mod.rs` — Add gauge fields to `ValueConfig`
- `render/widgets/value/mod.rs` — Add gauge display type dispatch
- `render/widgets/value/gauge.rs` — New module: gauge geometry calculations and Skia drawing
- `render/widgets/value/gauge/linear.rs` — Linear gauge rendering
- `render/widgets/value/gauge/bars.rs` — Bars gauge rendering
- `render/widgets/value/gauge/arc.rs` — Arc and corner gauge rendering
- `render/widgets/value/gauge/layout.rs` — Shared gauge layout math (fill percentage, bar sizing, arc geometry)
- `render/widgets/value/gauge/static.rs` — Static layer preparation for all gauge types
- `render/mod.rs` — Integrate gauge static layer into base layer preparation

**React frontend (within `app/src/`):**

- `features/widget-preview/components/GaugeRenderer.jsx` — SVG gauge rendering for all types
- `features/widget-preview/utils/gaugeGeometry.js` — Pure geometry calculations (fill %, bar sizing, arc angles, positions)
- `features/widget-editor/components/GaugeWidgetEditor.jsx` — Gauge-specific editor controls (display type dropdown, track styling, orientation, arc angle, etc.)
- `features/widget-editor/data/widgetDefaults.js` — Gauge factory defaults
- `features/widget-preview/components/MetricRenderer.jsx` — Dispatch to gauge rendering when display_type is not "text"
- `features/widget-preview/components/WidgetPreview.jsx` — No change needed (dispatches by widget.type, gauge is a value widget)

### Deep module opportunities

- `gaugeGeometry.js` should be a pure, testable module containing all geometry calculations (fill percentage, bar bucket determination, arc start/end angles, bar sizing). The Rust `gauge/layout.rs` should implement the same logic independently. Both should produce identical results given the same inputs.
- Gauge static layer preparation follows the existing `RouteWidgetCache`/`ElevationWidgetCache` pattern — a `GaugeWidgetCache` struct holding the pre-rendered static `SkiaImage` and per-frame state.

## Testing Decisions

- Good tests should verify external behavior and visual contracts rather than internal rendering details.
- **Rust backend tests:**
  - Gauge config fields deserialize correctly from template JSON
  - `display_type` defaults to `"text"` for existing templates
  - Fill percentage calculation: verify correct values for in-range, min, max, and out-of-range inputs
  - Bar bucket determination: verify which bars are filled for given values
  - Arc angle geometry: verify start/end angles for various arc angles and corner orientations
  - Bar sizing: verify bar widths given widget width, bar count, and gap
  - Render data requirements unchanged when gauge display type is set (same metric series needed)
  - Baseline render test: render a gauge widget frame to PNG and verify it is not empty
- **React frontend tests:**
  - Gauge widget defaults match the spec
  - `gaugeGeometry` calculations match expected values for all gauge types
  - Gauge widget editor renders all controls and dispatches correct config updates
  - Display type dropdown correctly switches between text/linear/bars/arc/corner
  - Preview-to-export parity: render the same config on both sides and compare visual output
- **Prior art:** Frontend tests follow the existing Vitest + Testing Library pattern in `app/src/tests/`. Backend tests follow the Rust integration/unit test patterns in `src-tauri/ovrley_core/tests/` and `#[cfg(test)] mod tests` blocks.
- Manual verification remains part of signoff for visual appearance across different widget sizes, arc angles, corner orientations, and track styling combinations.

## Out of Scope

- Manual min/max override — min/max are always auto-derived from activity data
- Gradient fills along the track (single color per empty/filled state)
- Needle/pointer indicators on arc gauges (fill level is the indicator)
- Animated easing/transitions between frames — "animated" means per-frame value changes using existing interpolation
- Gauge types beyond the five specified (text, linear, bars, arc, corner)
- SVG text rendering in the Rust backend — Skia `draw_str()` is used for all backend text
- Auto-sizing or overlap clamping for inner widgets in arc/corner gauges

## Further Notes

- This extends the existing value widget system rather than creating a new widget type. The same metric data binding, positioning, and lifecycle are reused — only the visual representation changes.
- The Skia-backend-first approach is non-negotiable. Backend rendering must be implemented and verified before the frontend preview is built. The frontend must mirror the backend approach exactly.
- This is the first major extension to value widget rendering since the gradient widget with slope triangle. It establishes the pattern for future visual metric representations.
- The existing parity testing infrastructure (canvas parity tests comparing Skia-rendered frames against browser SVG captures) should be extended to cover gauge widgets.
