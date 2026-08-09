# 02 — Deliver the Delta widget

**What to build:** Let users add and configure a Delta widget that shows whether the current activity lap is faster or slower than the fastest eligible lap completed before the current frame. Its reference remains activity-wide when the scene is trimmed, and the editor preview matches rendered video.

**Blocked by:** 01 — Deliver Current Lap and Best Lap widgets.

**Status:** ready-for-agent

- [ ] The Lap Timer catalog options and editor mode control include Delta and create the canonical `lap_timer` display in delta mode.
- [ ] The widget supports the shared position, typography, opacity, and label controls, defaults its label to `Delta`, and lets users configure positive and negative delta colors independently.
- [ ] Delta always renders with an explicit sign and two decimals, including `+0.00` when no completed-lap reference exists and for an exact zero delta.
- [ ] Values greater than or equal to zero use the positive delta color, negative values use the negative delta color, and the label retains the widget's main text color.
- [ ] The comparison uses the best lap completed before the current frame across the whole activity, including a reference lap outside the exported scene, without using a future lap.
- [ ] Current delta content updates for every frame while preserving the activity's original lap timing through scene trims.
- [ ] Automated tests cover no reference, exact zero, faster and slower values, a reference lap before the scene trim, color selection, and preview/render parity.
