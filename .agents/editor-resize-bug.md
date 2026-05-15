# Editor Resize Glitch — Metric Widgets

## Symptom

When dragging a resize handle (top-left or top-right corner) of a metric/value category widget, the selection box jumps downward by roughly one text-line height and immediately snaps back to the correct position. The cycle repeats each frame of the drag gesture. The widget SVG text stays visually in place; only the Moveable selection box "bounces."

The correct dimensions (width, height) are preserved throughout — only the Y position oscillates.

## Root Cause

Two systems race to position the same DOM element during a resize:

1. **Moveable** applies its own calculated `translate` / `top` / `left` to the target element as the user drags a handle.

2. **`applyLiveWidgetStyles`** (called from `onResize` in `useResizeHandlers.js`) immediately overwrites the element's `left` / `top` with its own value computed by `getWidgetSceneOrigin`.

The sequence per animation frame:

| Step | Actor | What happens |
|------|-------|-------------|
| 1 | Moveable | Drags element to new position via its internal transform/position logic |
| 2 | `onResize` handler | Reads `drag.beforeTranslate[1]`, calls `getWidgetVisualBoundsFromTarget(target)` |
| 3 | DOM dataset | Still holds **pre-resize** `data-widget-bounds-*` values because `OverlayCanvasWidget` is memo'd and hasn't re-rendered with the new metric layout |
| 4 | `getWidgetSceneOrigin` | Combines stale `minY` from dataset with the new `y` from the data draft → produces a wrong Y |
| 5 | `applyLiveWidgetStyles` | Sets `target.style.top` to that wrong Y → element jumps down |
| 6 | Next rAF | Moveable re-reads actual element position, detects the delta, immediately corrects it → snap back |

The `data-widget-bounds-*` attributes are written by `OverlayCanvasWidget` during render based on `buildMetricWidgetPreviewModel`. During a resize gesture the React tree does not re-render (the draft update is done imperatively, not via React state for the affected widget), so the dataset becomes stale.

## Observation

- Only affects **metric/value category** widgets (non-plot) because plot widgets use `isPlotWidget ? 1 : globalScale` paths and a different bounds computation branch.
- Height computation is stable because `onResize` sets `nextHeight = Math.max(height / dimensionScale, 8)` directly from the Moveable event — no dataset involved.
- The glitch is purely cosmetic; final position after `onResizeEnd` is correct.

## Suggested Solutions

### A. Skip `applyLiveWidgetStyles` for metric widgets during resize

Since metric widgets derive their visual position from `getWidgetSceneOrigin(…, visualBounds)` and the bounds are computed from text measurement (not from the drag offset), calling `applyLiveWidgetStyles` during resize is unnecessary — the element's actual dimensions are already set by Moveable. Only the draft state needs updating.

**Modification:** In `onResize`, only call `applyLiveWidgetStyles` when `widget.category === 'plots'`. For metric widgets, only update the draft (x, y, width, height via `setLiveWidgetDraft`). After `onResizeEnd`, the next React render will apply the correct position via the `left`/`top` style from the updated data.

**Risk:** Low. Only affects the visual feedback during the drag gesture. The commit path is unchanged.

### B. Update dataset attributes on each resize frame

Before `applyLiveWidgetStyles`, imperatively update the target's `data-widget-bounds-*` attributes to reflect the current dimensions. This requires computing the metric layout on every frame (or reading it from the draft), which adds per-frame text measurement overhead.

**Risk:** Medium. Adds synchronous DOM text measurement on drag frames, potential jank.

### C. Let Moveable own the position entirely during resize

Remove the `applyLiveWidgetStyles` call from `onResize` for all widget categories. Moveable's internal positioning is sufficient for the visual feedback. Only the draft state is needed for the commit step.

**Risk:** Medium. Plot widgets may rely on `applyLiveWidgetStyles` to keep the CSS scale transform in sync during resize. Requires verifying plot resize behavior.

## Recommendation

**Solution A** is the safest: gate the `applyLiveWidgetStyles` call on `widget.category !== 'values'`. This preserves the existing behavior for plot widgets (where dataset bounds are not the source of truth) and eliminates the race for metric widgets where position is fully derived from text measurements.

Roughly 4 lines changed in `useResizeHandlers.js`.
