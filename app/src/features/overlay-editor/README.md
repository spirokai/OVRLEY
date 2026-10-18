# Overlay editor interactions

Moveable supplies gesture coordinates and selection-border vertices. The shared
[WASM widget session](../wasm-widgets/README.md) owns live configuration and
presented content geometry.

- `useWidgetSelection` reads `getWidgetInteraction(node).geometry()` for badges.
  `buildWidgetBadges` projects content bounds and the rotation pivot at viewport
  scale, preserving fractional placement and excluding shadows. This avoids one
  scene rectangle read plus one per visible badge on every refresh.
- `useSelectionBorder` measures scene bounds and viewport clipping on mount,
  container/zoom changes, ResizeObserver notifications, scrolling, and window
  resizing. Changing Moveable vertices only runs `getPixelAlignedBorders` against
  those measurements. Keeping vertices out of the measurement effect avoids
  repeated layout reads and the extra React commits caused by setting border
  state after every render. Zoom remains an explicit dependency because CSS
  transforms do not trigger ResizeObserver.

With 15 selected widgets, 180 measured input frames, 30 warm-up frames, one mouse
move per frame, and a paused playhead, these changes reduced rectangle reads from
120 to 98 and React commits from six to three per drag input frame. Rectangle
measurement time fell 68–77% across text and mixed groups; total React render time
did not halve. Recheck with `pnpm benchmark:interactions`; `--rect-stacks` attributes
reads but adds overhead, so use separate runs for timing.

The remaining 98 reads are two Moveable passes of 49: one during input handling
before scheduled preview placement, and one after placement to align handles.
Rapid input can repeat the first pass within a frame. Keep the post-placement
refresh until those passes are coordinated; removing it alone can leave stale
handles. Moveable's `throttleDrag` controls coordinate increments, not event rate.
