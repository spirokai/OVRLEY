Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Implement the linear gauge display type end-to-end: Skia backend rendering with static/dynamic layer split, React frontend SVG preview, and widget editor controls. This slice establishes the shared infrastructure (track styling fields, gauge cache pattern, min/max label rendering, auto-derived min/max) that all subsequent gauge types reuse.

**Backend behavior:**
- When `display_type` is `"linear"`, the widget renders as a continuous filled bar with no text value or units visible.
- Min/max are auto-derived from the activity data for that metric. When no activity is loaded (during preview preparation), a fallback range of 0–100 is used.
- Fill percentage = `(value - min) / (max - min)`, clamped to [0, 1].
- Orientation: `"horizontal"` fills left-to-right; `"vertical"` fills bottom-to-top.
- Static layer: empty track + border + min/max labels (if enabled). No text widget rendered.
- Dynamic per frame: filled portion only.
- Track styling: thickness, corner radius, border thickness/color, empty/filled color and opacity — all configurable via new `ValueConfig` fields.
- Min/max labels: toggleable, configurable font size and color.

**Frontend behavior:**
- SVG preview renders identically to the Skia backend for the same config.
- Editor controls: display type dropdown, orientation toggle, track thickness, corner radius, border settings, empty/filled colors with opacity, min/max label toggle + font size + color.

**Shared infrastructure established:**
- All gauge track styling fields on `ValueConfig`: `track_thickness`, `track_corner_radius`, `track_border_thickness`, `track_border_color`, `track_empty_color`, `track_empty_opacity`, `track_filled_color`, `track_filled_opacity`, `show_min_max_labels`, `min_max_label_font_size`, `min_max_label_color`.
- Gauge static layer preparation pattern (baked into `SkiaImage` during `prepare_render_assets`).
- Frontend `gaugeGeometry.js` pure utility module for fill percentage and layout calculations.

## Acceptance criteria

- [ ] All shared gauge styling fields added to `ValueConfig` with `#[serde(default)]`
- [ ] `GaugeWidgetCache` struct created with static `SkiaImage` and per-frame state, following `RouteWidgetCache`/`ElevationWidgetCache` pattern
- [ ] Linear gauge static layer (empty track + border + min/max labels) baked into cached `SkiaImage` during `prepare_render_assets`
- [ ] Linear gauge dynamic fill rendered per-frame in `render_frame_rgba`
- [ ] Horizontal orientation: fill sweeps left-to-right
- [ ] Vertical orientation: fill sweeps bottom-to-top
- [ ] Min/max labels rendered correctly when enabled, hidden when disabled
- [ ] Track corner radius applied to track rectangle corners
- [ ] Border thickness and color rendered around track
- [ ] Empty and filled track colors with opacity rendered correctly
- [ ] `display_type: "text"` continues to render as before (no regression)
- [ ] Frontend `gaugeGeometry.js` module with pure fill percentage and layout functions
- [ ] Frontend SVG preview renders linear gauge identically to Skia backend
- [ ] Frontend editor controls for all linear gauge options
- [ ] Min/max derived from activity data; placeholder 0–100 range shown when no activity loaded
- [ ] Rust unit tests for fill percentage calculation, bar sizing formula, and config deserialization
- [ ] Frontend tests for `gaugeGeometry` calculations and editor control dispatch

## Blocked by

- `#01-config-schema-and-display-type-plumbing.md`
