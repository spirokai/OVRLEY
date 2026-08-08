# 03 — Deliver the activity-wide Lap Time Log

**What to build:** Let users add and configure a Lap Time Log that shows the activity's completed lap history through the current frame plus the live in-progress lap. The log must retain original activity lap identities and include earlier completed laps when an exported scene starts later in the activity, with matching editor preview and rendered video.

**Blocked by:** 01 — Deliver Current Lap and Best Lap widgets.

**Status:** ready-for-agent

- [ ] The Lap Timer catalog options and editor mode control include Lap Time Log and create the canonical `lap_timer` display in lap-log mode.
- [ ] The widget supports the shared position, typography, color, opacity, and label controls and defaults its label to `Lap Times`.
- [ ] The log draws a `LAP`, `TIME`, `DELTA` header at 70% of the main text opacity, with numeric columns right-aligned for differing lap-number and duration widths.
- [ ] At each frame, the log contains every activity lap completed by that point and a final live row for the current in-progress lap; during the activity out-lap it contains only the header.
- [ ] A scene beginning during a later lap immediately shows all earlier completed activity laps, preserves the activity's user-facing lap numbers, and continues the current lap from its actual start.
- [ ] A lap intersected by the scene trim remains a normal activity lap and joins the completed history if its activity completion occurs by the current frame.
- [ ] Completed rows show formatted lap durations and signed deltas using the activity-wide lap history available at that point, while the in-progress row updates live.
- [ ] The header and completed rows are prepared by completed-lap state so only the in-progress row requires per-frame drawing between lap boundaries.
- [ ] Automated tests cover out-lap, first lap, multiple completions, a scene starting mid-session and mid-lap, lap completion inside the scene, column alignment, and preview/render parity.
