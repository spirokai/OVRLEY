Status: done

# 04 — Frontend Preview (SVG Tape + Indicator)

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Implement the React frontend widget preview for the heading widget, producing visual output identical to the Skia backend using SVG.

The frontend preview mirrors the backend rendering strategy:

- The 360° tape (ticks + labels) is rendered as an SVG `<pattern>` with `patternUnits="userSpaceOnUse"` and width `360 × pixelsPerDegree` px. A `<rect>` fills the widget bounds with `fill="url(#tape-pattern)"`. Scrolling is achieved via `patternTransform="translate(-offset, 0)"` — this directly mirrors Skia's `TileMode::Repeat`.
- The indicator (chevron or highlight bar) is rendered as separate SVG elements (`<polygon>`, `<rect>`) layered on top of the taped rect.
- All visual properties (tick lengths, colors, label positioning, shadows via SVG `<filter>`) come from the widget's data object, matching the backend config schema.

A shared utility module (`headingGeometry.js`) computes tick positions, label placements, and indicator geometry given widget dimensions and current heading. This module is pure and testable. The same geometry calculations must produce identical results to the Rust implementation.

Wire the `HeadingRenderer.jsx` component into `WidgetPreview.jsx` dispatch, following the same pattern as `RouteRenderer.jsx` and `ElevationRenderer.jsx`.

The preview must render correctly even before an activity is loaded (using a demo heading value or static mock data, matching the pattern used by route/elevation previews which render a fallback shape when no data exists).

## Acceptance criteria

- [x] `HeadingRenderer.jsx` renders a scrolling tape with ticks and labels via SVG `<pattern>`
- [x] Indicator (chevron or highlight bar) renders as SVG elements on top of the tape
- [x] All visual properties match backend output: tick lengths, colors, label positioning, cardinal override, alignment (`"below"` / `"centered"`), indicator style and placement
- [x] Shadows render on ticks, labels, and indicator via SVG `<filter>`, matching the backend's shadow model
- [x] `headingGeometry.js` computes tick/label/indicator positions correctly
- [x] The 0°/360° wrap boundary renders seamlessly in the preview
- [x] Preview renders a fallback display when no activity/heading data is loaded
- [x] `WidgetPreview.jsx` dispatches heading widgets to `HeadingRenderer` (not the generic text/metric renderer)
- [x] `headingGeometry.js` unit tests pass: verify tick positions, label placement, cardinal override, wrap behavior

## Blocked by

- [03 — Indicator Overlay](./03-indicator-overlay.md)
