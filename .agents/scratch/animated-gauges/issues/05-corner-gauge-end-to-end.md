Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Implement the corner gauge display type end-to-end: Skia backend rendering with static/dynamic layer split, React frontend SVG preview, and widget editor controls. This slice reuses all arc rendering and inner widget layout infrastructure from the arc gauge slice.

**Backend behavior:**
- When `display_type` is `"corner"`, the widget renders a fixed 90° arc positioned in one of four corners: top-left, top-right, bottom-left, bottom-right.
- The metric text widget (value + unit + optional icon) remains visible and customizable, identical to arc gauge inner widget behavior.
- Fill sweeps left-to-right with direction depending on corner:
  - Top-left: left edge → top edge (counter-clockwise)
  - Top-right: top edge → right edge (counter-clockwise)
  - Bottom-left: left edge → bottom edge (clockwise)
  - Bottom-right: bottom edge → right edge (clockwise)
- Static layer: empty arc track + border + min/max labels (if enabled) + unit label + icon (if enabled).
- Rounded corner-gauge arc ends should reuse the arc gauge's stroke-cap strategy (`Round` caps on stroked arcs), following the same approach observed in the upstream Cyclemetry renderer rather than custom rounded path geometry.
- Dynamic per frame: arc fill + value text.
- All inner widget customization (x/y offset, icon toggle, font size) works identically to arc gauge.

**Frontend behavior:**
- SVG preview renders identically to the Skia backend for the same config.
- Rounded arc ends should be mirrored with SVG round linecaps / equivalent stroke-cap behavior.
- Editor controls: display type dropdown, corner orientation selector (top-left/top-right/bottom-left/bottom-right), plus all shared track styling and inner widget controls.

## Acceptance criteria

- [ ] `corner_orientation` field added to `ValueConfig` with variants `"top-left"`, `"top-right"`, `"bottom-left"`, `"bottom-right"` and `#[serde(default)]`
- [ ] Corner gauge static layer baked into cached `SkiaImage` (reuses arc infrastructure)
- [ ] Corner gauge dynamic fill rendered per-frame with correct sweep direction per corner orientation
- [ ] Rounded corner-gauge arc ends rendered via stroke-cap strategy in both Skia and SVG preview
- [ ] Top-left: counter-clockwise sweep from left edge to top edge
- [ ] Top-right: counter-clockwise sweep from top edge to right edge
- [ ] Bottom-left: clockwise sweep from left edge to bottom edge
- [ ] Bottom-right: clockwise sweep from right edge to bottom edge
- [ ] Inner widget (value + unit + icon) rendered identically to arc gauge
- [ ] Frontend SVG preview renders corner gauge identically to Skia backend
- [ ] Frontend editor controls for corner orientation
- [ ] Rust unit tests for corner sweep direction and start/end angles per orientation
- [ ] Frontend tests for corner geometry calculations

## Blocked by

- `#04-arc-gauge-end-to-end.md`
