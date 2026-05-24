Status: ready-for-agent

# 03 — Indicator Overlay

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Add the configurable center indicator to the heading widget in the Skia backend. The indicator is drawn per-frame on top of the scrolling tape.

Two indicator styles are supported:

- **Chevron** (`indicator_style: "chevron"`): a filled triangle pointing toward the tape. Placement (`"top"`, `"bottom"`, `"both"`) determines which edge(s) the chevron sits on. Size in pixels controls chevron height.
- **Highlight bar** (`indicator_style: "highlight_bar"`): a semi-transparent filled vertical rectangle spanning the full tape height, centered horizontally on the widget. Small triangular edge markers at the placement edges (top, bottom, or both). The bar itself is always full-height regardless of placement; placement controls the edge markers.

The indicator is drawn after the tape in the per-frame draw function, so it correctly occludes ticks and labels passing underneath. The indicator has its own color (`indicator_color`) and a shadow matching the widget's shadow override. The indicator is independently toggleable via `show_indicator`.

## Acceptance criteria

- [ ] Chevron indicator renders correctly at top, bottom, or both placements
- [ ] Highlight bar renders as a semi-transparent full-height vertical band with triangular edge markers at the configured placement edges
- [ ] Indicator color is configurable independently of tick/label colors
- [ ] Indicator shadow matches the widget's shadow override
- [ ] Toggling `show_indicator: false` hides the indicator completely
- [ ] Indicator always stays at the widget's horizontal center — it is viewport-static, never scrolls with the tape
- [ ] Backend render test produces correct output with indicator enabled

## Blocked by

- [02 — Tape Rendering: Ticks and Labels](./02-tape-rendering-ticks-and-labels.md)
