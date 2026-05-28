Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Implement the arc gauge display type end-to-end: Skia backend rendering with static/dynamic layer split, React frontend SVG preview, widget editor controls, and the inner text widget layout system. This slice reuses all shared track styling and static layer infrastructure from the linear gauge slice, and introduces arc geometry and inner widget layout that the corner gauge slice will reuse.

**Backend behavior:**
- When `display_type` is `"arc"`, the widget renders a circular arc track with configurable angle (30°–360°). The metric text widget (value + unit + optional icon) remains visible inside the arc.
- Arc is symmetric along the vertical axis. 180° produces a half-circle starting and ending on a horizontal line.
- Arc radius derived from widget bounding box: `min(width, height) / 2 - padding` (padding accounts for track thickness and labels).
- Fill sweeps from the leftmost arc endpoint to the rightmost arc endpoint, always reading left-to-right.
- Static layer: empty arc track + border + min/max labels (if enabled) + unit label + icon (if enabled).
- Dynamic per frame: arc fill + value text. Value text changes per frame; unit and icon are static.
- Inner widget layout: unit appears below value (vertical stacking, not horizontal row). Icon sits to the left of value on the primary line.
- Inner widget position controlled by `inner_widget_offset_x` and `inner_widget_offset_y` relative to arc center.
- No auto-sizing or overlap clamping — user controls icon size and font size, overlap is their responsibility.

**Frontend behavior:**
- SVG preview renders identically to the Skia backend for the same config.
- Editor controls: display type dropdown, arc angle slider/input, inner widget x/y offset, icon toggle, plus all shared track styling controls.

**Infrastructure established:**
- `inner_widget_offset_x` and `inner_widget_offset_y` fields on `ValueConfig`.
- `arc_angle` field on `ValueConfig` (30–360 range).
- Arc geometry calculation functions (start/end angles from arc angle, radius derivation).
- Inner widget vertical stacking layout (unit below value).

## Acceptance criteria

- [ ] `arc_angle`, `inner_widget_offset_x`, `inner_widget_offset_y` fields added to `ValueConfig` with `#[serde(default)]`
- [ ] Arc gauge static layer (empty arc + border + min/max labels + unit label + icon) baked into cached `SkiaImage`
- [ ] Arc gauge dynamic fill rendered per-frame with correct sweep direction (left-to-right)
- [ ] Arc angle range enforced: 30°–360°
- [ ] Arc radius correctly derived from widget bounding box minus padding
- [ ] Inner widget value text rendered per-frame inside arc
- [ ] Inner widget unit label and icon rendered in static layer (vertical stacking layout)
- [ ] Inner widget positioned by x/y offset from arc center
- [ ] Icon toggleable via existing `show_icon` field
- [ ] No auto-sizing or overlap clamping for inner widget
- [ ] Frontend SVG preview renders arc gauge identically to Skia backend
- [ ] Frontend editor controls for arc angle, inner widget offsets, icon toggle
- [ ] Rust unit tests for arc angle geometry (start/end angles for various angles)
- [ ] Frontend tests for arc geometry calculations and inner widget layout

## Blocked by

- `#01-config-schema-and-display-type-plumbing.md`
