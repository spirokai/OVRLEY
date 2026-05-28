Status: ready-for-agent

## Parent

`.agents/scratch/animated-gauges/PRD.md`

## What to build

Implement the bars gauge display type end-to-end: Skia backend rendering with static/dynamic layer split, React frontend SVG preview, and widget editor controls. This slice reuses all shared track styling and static layer infrastructure established in the linear gauge slice.

**Backend behavior:**
- When `display_type` is `"bars"`, the widget renders as a discrete array of bars separated by configurable gaps. No text value or units visible.
- Min/max auto-derived from activity data (same as linear gauge).
- Bars are discrete — each bar is either fully filled or fully empty, never partially filled.
- Bar `i` (0-indexed) is filled if `value >= min + ((i+1) / bar_count) * (max - min)`.
- Given widget width W, bar_count N, gap G: each bar width = `(W - (N-1)*G) / N`.
- Gap is clamped to ensure minimum 2px bar width.
- Orientation: `"horizontal"` arranges bars left-to-right; `"vertical"` arranges bars bottom-to-top.
- Static layer: empty bar tracks + border + min/max labels (if enabled).
- Dynamic per frame: filled bars only.
- Track corner radius applied to individual bar corners.

**Frontend behavior:**
- SVG preview renders identically to the Skia backend for the same config.
- Editor controls: display type dropdown, orientation toggle, bar count, gap, plus all shared track styling controls.

## Acceptance criteria

- [ ] `bar_count` and `bar_gap` fields added to `ValueConfig` with `#[serde(default)]`
- [ ] Bars gauge static layer (empty bar tracks + border + min/max labels) baked into cached `SkiaImage`
- [ ] Bars gauge dynamic fill rendered per-frame with discrete bucket logic
- [ ] Bar `i` correctly filled/empty based on value threshold formula
- [ ] Bar width calculation: `(W - (N-1)*G) / N` with 2px minimum clamp
- [ ] Horizontal orientation: bars arranged left-to-right, fill progresses left-to-right
- [ ] Vertical orientation: bars arranged bottom-to-top, fill progresses bottom-to-top
- [ ] Track corner radius applied to individual bar corners
- [ ] Frontend SVG preview renders bars gauge identically to Skia backend
- [ ] Frontend editor controls for bar count, gap, and orientation
- [ ] Rust unit tests for bar bucket determination and bar sizing calculation
- [ ] Frontend tests for bar geometry calculations

## Blocked by

- `#01-config-schema-and-display-type-plumbing.md`
