Status: ready-for-agent

# Heading Tape Widget

## Problem Statement

OVRLEY already parses heading data from both FIT and GPX activity files, and derives fallback heading from course bearing when direct heading data is unavailable. However, heading cannot be displayed in the overlay because no heading widget exists — neither the Rust backend nor the React frontend recognizes heading as a renderable metric, and no graphical compass tape rendering logic has been implemented.

Users recording motorcycling, cycling, hiking, sailing, and paragliding activities need to display their current heading as a visual compass tape in video overlays, matching the professional dashboard/HUD aesthetic common in action-sports telemetry overlays.

## Solution

Implement a heading graphical widget: a horizontal compass tape that smoothly scrolls as heading changes, with fully customizable appearance. The widget renders as a 360-degree wrapped tape (seamlessly looping through 0°/360°) with configurable major/minor ticks, numeric/cardinal labels, and a center indicator.

The implementation is Skia-backend-first, with the React frontend preview mirrored to produce identical visual output via SVG. Both sides share the same algorithmic geometry — tick spacing, label placement, shadow behavior — ensuring perfect preview-to-export parity.

## User Stories

1. As an overlay editor user, I want to add a heading compass tape widget, so that I can show current heading in motorcycling, cycling, hiking, sailing, or paragliding video overlays.
2. As an overlay editor user, I want the heading tape to scroll smoothly as heading changes over time, so that it matches the dynamic dashboard aesthetic of professional action-sports telemetry.
3. As an overlay editor user, I want the tape to wrap seamlessly at 0°/360°, so that there is no visual jump when crossing north.
4. As an overlay editor user, I want to resize the heading widget in both width and height, so that I can fit it into various overlay layouts.
5. As an overlay editor user, I want to control the horizontal scale via a pixels-per-degree setting, so that I can make the tape more or less zoomed in.
6. As an overlay editor user, I want major ticks at 15° intervals, so that the tape aligns neatly with cardinal and intercardinal positions.
7. As an overlay editor user, I want minor ticks at 5° intervals (3 subdivisions per major tick), so that I can read intermediate bearings precisely.
8. As an overlay editor user, I want to toggle major ticks on and off independently, so that I can control visual density.
9. As an overlay editor user, I want to toggle minor ticks on and off independently, so that I can simplify the tape when needed.
10. As an overlay editor user, I want to configure tick length as a percentage of widget height, so that taller widgets naturally get proportionally longer ticks.
11. As an overlay editor user, I want to configure tick thickness in pixels, so that I can match the line weight to my overlay style.
12. As an overlay editor user, I want to configure tick color, so that I can theme the tape to my overlay's color palette.
13. As an overlay editor user, I want cardinal+intercardinal ticks (N/NE/E/SE/S/SW/W/NW at 45° multiples) to have their own configurable color, so that I can visually emphasize the 8 cardinal directions.
14. As an overlay editor user, I want tick shadows to match the scene shadow defaults, so that tick rendering integrates with the existing shadow system.
15. As an overlay editor user, I want to choose between ticks aligned below the centerline or centered on both sides of the centerline, so that I can match the tape style to different dashboard aesthetics.
16. As an overlay editor user, I want numeric degree labels (15, 30, 45, 60...) displayed below the ticks, so that I can read exact bearing values at a glance.
17. As an overlay editor user, I want to toggle numeric labels on and off independently, so that I can reduce clutter.
18. As an overlay editor user, I want to configure numeric label color independently, so that I can emphasize or de-emphasize the degree values.
19. As an overlay editor user, I want cardinal+intercardinal labels (N, NE, E, SE, S, SW, W, NW) displayed at their respective positions, so that I can read compass directions instantly.
20. As an overlay editor user, I want cardinal labels to take priority over numeric labels when both are enabled at the same position, so that N replaces 0, NE replaces 45, etc. — with both label types sharing a single row.
21. As an overlay editor user, I want to toggle cardinal labels on and off independently of numeric labels, so that I can choose to show only compass letters or only degrees.
22. As an overlay editor user, I want to configure cardinal label color independently, so that compass letters stand out from numeric degrees.
23. As an overlay editor user, I want to configure label font size, so that I can balance readability with space constraints.
24. As an overlay editor user, I want labels to have shadows that match the scene shadow defaults, so that they are legible over varying video backgrounds.
25. As an overlay editor user, I want a heading indicator (fixed center marker) that never moves while the tape scrolls beneath it, so that I always know what the current heading is.
26. As an overlay editor user, I want to toggle the indicator on and off, so that I can use the tape without an explicit marker when using another indicator element in the overlay.
27. As an overlay editor user, I want to choose between indicator styles: a chevron/triangle pointing at the tape from above or below, and a semi-transparent highlight bar spanning the full tape height at center, so that I can pick the visual treatment that fits my layout.
28. As an overlay editor user, I want to choose indicator placement: top, bottom, or both sides of the tape, so that I can position the indicator where it is most visible in my layout.
29. As an overlay editor user, I want to configure indicator color, so that it pops against the tape background.
30. As an overlay editor user, I want to configure indicator size in pixels, so that I can make the chevron or bar appropriately sized for the widget.
31. As an overlay editor user, I want the widget preview in the overlay editor to match the exported render exactly, so that what I see during editing is what I get in the final video.
32. As an overlay editor user, I want the heading widget to appear in the widget drawer alongside other widgets, so that I can discover and add it naturally.
33. As an overlay editor user, I want heading to freeze at the last known value when GPS heading data is temporarily missing, so that the widget never displays an empty or glitching tape.
34. As an overlay editor user, I want the heading widget to support rotation for non-horizontal layouts, consistent with other plot widgets.
35. As a template author, I want heading widget configuration to serialize cleanly into OVRLEY template JSON, so that I can share templates that include heading tapes.

## Implementation Decisions

### Tape model

- The tape is a 360° wrapped strip. Heading 359° → 0° transitions seamlessly because the tape repeats infinitely in both directions.
- Visible degrees span is derived from widget width divided by `pixels_per_degree` — a wider widget or smaller px/° value shows more tape, a narrower widget or larger px/° value shows less. No fixed visible-degree constant.
- The grid is 15° between major ticks, 5° between minor ticks (3 minor ticks per major interval), labels at every 30°.
- The 8 cardinal+intercardinal positions (N/NE/E/SE/S/SW/W/NW at 0°/45°/90°/135°/180°/225°/270°/315°) align naturally with the 15° grid — all 8 positions are multiples of 15.

### Tick and label render model

- Ticks: major and minor — each independently toggleable. Lengths are percentages of widget height. Thickness is in pixels.
- Tick alignment: configurable as `"below"` (tick tops align with centerline, extending downward) or `"centered"` (ticks centered on the centerline, extending equally up and down).
- Two label types share one row below all ticks: cardinal labels show at N/NE/E/SE/S/SW/W/NW positions and take priority over numeric labels at those positions. Numeric labels fill all remaining 30° positions (15, 30, 60, 75, 105...). Each label type has its own on/off toggle and color.
- Shadows on ticks and labels inherit the scene shadow defaults (`shadow_distance`, `shadow_strength`, `shadow_color`) via a single per-widget override, matching existing widget conventions. Labels and ticks share one shadow config.

### Indicator model

- Configurable style: `"chevron"` (triangle pointing toward tape) or `"highlight_bar"` (semi-transparent vertical band spanning full tape height, with small triangular edge markers at top and bottom).
- Chevron placement: top, bottom, or both. Highlight bar is always full-height (placement becomes the edge markers' position: top, bottom, or both).
- Drawn per-frame on top of the scrolling tape, never baked into the static layer — this is correct for the highlight bar style which must occlude ticks passing beneath.

### Rendering strategy (Skia backend)

- During preparation, the full 360° tape (ticks + labels + shadows) is rendered once into a cached `SkiaImage` surface. Shadow baking into the cached image is correct because shadows are relative to the widget viewport, not the scrolling tape.
- Per-frame: draw the cached tape image offset by `heading × pixels_per_degree` using `image.to_shader(TileMode::Repeat, TileMode::Clamp)` in the X direction. A clip rect confines output to the widget bounds. The shader-tile approach handles the 359°→0° wrap without any boundary logic and uses one-third the memory of a 3× manually-repeated surface.
- After the tape, draw the indicator (chevron or highlight bar) at the widget's horizontal center, positioned per the indicator placement config.
- The per-frame cost is: one tiled image draw (tape) + one or two shape draws (indicator) + shadows on the indicator — negligible overhead.

### Rendering strategy (React frontend SVG preview)

- The 360° tape is rendered as an SVG `<pattern>` with `patternUnits="userSpaceOnUse"` and width equal to `360 × pixelsPerDegree` px. A `<rect>` fills the widget bounds with `fill="url(#tape-pattern)"`, and `patternTransform="translate(-offset, 0)"` scrolls the tape — this directly mirrors Skia's `TileMode::Repeat` behavior.
- The indicator is rendered as separate SVG elements (`<polygon>` for chevrons, `<rect>` for highlight bar) on top of the taped rect, maintaining identical visual output to the Skia render.

### Missing data behavior

- When heading is `None` at a frame, the widget holds the last known valid heading. The tape freezes rather than disappearing or showing a placeholder. This is consistent with how existing graphical widgets (route, elevation) always render rather than conditionally showing missing states.

### Data plumbing

- `MetricKind::Heading` will be added to the Rust `MetricKind` enum.
- `heading: NumericSeries` will be added to `ParsedActivity` (extracted from the `extra` serde map if present, or parsed directly).
- `heading: Vec<Option<f64>>` will be added to `DenseSeriesReport` and `TrimmedActivity`.
- `heading: bool` will be added to `RenderDataRequirements`.
- Heading will be wired through the trim → densify pipeline. During densification, null values will be forward-filled (carry last known value forward), implementing the hold-last-known behavior.
- Heading is already parsed by the frontend FIT/GPX parsers and included in the JSON payload sent to the backend.

### Config schema

The heading widget config struct will contain these parameters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `x`, `y`, `width`, `height` | position/size | Standard widget geometry |
| `rotation` | degrees | Standard widget rotation |
| `opacity` | 0.0–1.0 | Standard widget opacity |
| `pixels_per_degree` | float | Horizontal tape scale |
| `major_tick_interval` | degrees | Default 15 |
| `minor_ticks_per_major` | int | Subdivisions between majors, default 3 (= every 5°) |
| `show_major_ticks` | bool | |
| `show_minor_ticks` | bool | |
| `major_tick_length_pct` | 0–100 | % of widget height |
| `minor_tick_length_pct` | 0–100 | % of widget height |
| `tick_thickness` | px | |
| `tick_color` | hex | Regular (non-cardinal) major and all minor ticks |
| `cardinal_tick_color` | hex | Ticks at N/NE/E/SE/S/SW/W/NW (45° multiples) |
| `tick_alignment` | `"below"` or `"centered"` | |
| `shadow_distance` | px | Override for scene shadow distance (all elements) |
| `shadow_strength` | float | Override for scene shadow strength |
| `shadow_color` | hex | Override for scene shadow color |
| `show_numeric_labels` | bool | |
| `show_cardinal_labels` | bool | |
| `numeric_label_color` | hex | |
| `cardinal_label_color` | hex | |
| `label_font_size` | px | |
| `label_offset` | px | Distance from bottom of ticks to label baseline |
| `indicator_style` | `"chevron"` or `"highlight_bar"` | |
| `indicator_placement` | `"top"`, `"bottom"`, `"both"` | |
| `show_indicator` | bool | |
| `indicator_color` | hex | |
| `indicator_size` | px | Chevron height or bar width |

### Layout conventions

- The tape centerline is at `y + height/2` in widget-local coordinates.
- Labels sit below the ticks. When tick alignment is `"centered"`, labels start below the downward half of the ticks. When `"below"`, labels start directly below the tick bottoms.
- The indicator for chevron style is a filled triangle. For highlight bar style, it is a semi-transparent filled rectangle with small triangle edge markers.

### Module organization

**Rust backend (within `src-tauri/ovrley_core/src/`):**

- `activity/schema.rs` — Add `heading` fields to `ParsedActivity`, `DenseSeriesReport`, `TrimmedActivity`; add `MetricKind::Heading`
- `activity/interpolate.rs` — Wire heading through `densify_activity` with forward-fill of nulls
- `config/mod.rs` — Add `HeadingWidgetConfig` struct, integrate into `RenderConfig.plots`
- `render/widgets/heading/mod.rs` — Module declarations
- `render/widgets/heading/prepare.rs` — Render 360° tape to cached `SkiaImage`, precompute per-frame heading offsets
- `render/widgets/heading/draw.rs` — Per-frame: tiled tape draw via shader + clip + indicator
- `render/widgets/mod.rs` — Integrate heading preparation into `prepare_render_assets`, heading drawing into `render_frame_to_surface`
- `standard_metrics.rs` — Register `heading` in standard metric definition lookup

**React frontend (within `app/src/`):**

- `features/widget-preview/components/HeadingRenderer.jsx` — SVG `<pattern>` tape + indicator
- `features/widget-preview/utils/headingGeometry.js` — Scale calculations, tick/label positioning
- `features/widget-editor/components/HeadingWidgetEditor.jsx` — Full customization panel
- `features/widget-editor/data/widgetDefaults.js` — Heading factory defaults
- `lib/widget-icons.jsx` — Register heading in `QUICKMENU_ITEMS`, `WIDGET_ICONS`
- `features/widget-preview/components/WidgetPreview.jsx` — Add heading dispatch branch
- `features/widget-drawer/components/WidgetButtonGrid.jsx` — Add heading to drawer grid

### Deep module opportunities

- The tick/label positioning logic (computing which ticks and labels are visible at a given heading and widget dimensions) should be a pure, testable function shared conceptually between Rust and JS, even though they are separate implementations. The Rust version produces the cached SkiaImage; the JS version produces SVG elements. Both should produce identical visual output given the same configuration.
- Heading data plumbing through the activity pipeline (trim → densify → render) should follow the exact pattern established by existing metrics, minimizing special cases.

## Testing Decisions

- Good tests should verify external behavior and visual contracts rather than internal rendering details.
- **Rust backend tests:**
  - `MetricKind::Heading` serialization/deserialization round-trip
  - Heading data plumbing through trim → densify (verify forward-fill of nulls)
  - Heading config deserialization from template JSON
  - Render data requirement derivation includes heading when the widget is configured
  - Tick position calculations: verify that for a given heading, scale, and widget width, the correct set of tick positions falls within the visible window
  - Label placement: verify cardinal override priority at the 8 cardinal positions
  - Baseline render test: render a heading widget frame to PNG and verify it is not empty
- **React frontend tests:**
  - Heading widget defaults match the spec (15° major interval, 3 minors per major, etc.)
  - `headingGeometry` tick/label position calculations match expected values
  - Heading widget editor renders all controls and dispatches correct config updates
  - Heading widget appears in the widget drawer
  - Preview-to-export parity: render the same config on both sides and compare visual output
- **Prior art:** Frontend tests follow the existing Vitest + Testing Library pattern in `app/src/tests/`. Backend tests follow the Rust integration/unit test patterns in `src-tauri/ovrley_core/tests/` and `#[cfg(test)] mod tests` blocks.
- Manual verification remains part of signoff for visual appearance across different widget sizes, heading values near 0°/360° wrap, and shadow rendering.

## Out of Scope

- Configurable tick grid density via the frontend editor (15° grid is fixed for initial implementation)
- G-meter widget (lat/lon G-force split) — separate widget
- Configuration of the visible-degree span independent of widget width and px/° scale
- Multiple indicator styles beyond chevron and highlight bar
- Compass rose or circular compass needle variants — this is a tape only
- Magnetic declination / true north adjustment
- Label borders (shadows only, no text-border support)

## Further Notes

- This is the first advanced graphical widget being added since the route map and elevation profile. It establishes the pattern for future graphical widgets (G-meter, etc.).
- The heading data is already fully parsed by the frontend FIT and GPX parsers — no new parser extraction work is needed. The gap is purely in the Rust backend schema and rendering pipeline.
- The tape's "fully customizable" requirement is satisfied by the comprehensive config schema with 30+ independent parameters. Users who want a simple tape can hide both label types and the indicator to get a clean tick-only display; users who want a full HUD can enable everything.
- The SVG `<pattern>` approach for the frontend preview is the closest semantic match to Skia's `TileMode::Repeat`. While the underlying rendering engines differ, the visual output should be pixel-identical for the same geometry inputs.
