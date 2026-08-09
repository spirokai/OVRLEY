# 01 — Deliver Current Lap and Best Lap widgets

**What to build:** Let users add and configure Current Lap and Best Lap text widgets whose editor previews match rendered video. Both widgets must preserve the activity's original lap identity and timing regardless of scene trim: Current Lap counts from the real activity lap boundary, and Best Lap considers every eligible lap completed before the current frame, including laps outside the exported scene.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] The widget catalog presents one Lap Timer entry with Current Lap and Best Lap readouts, and selecting either creates the canonical `lap_timer` display with the corresponding mode.
- [ ] Users can change the mode, position, font, font size, color, opacity, label visibility, and label text, with default labels of `Current Lap` and `Best Lap` and the existing scene-level text effects applied consistently.
- [ ] Current Lap shows `--:--.--` during the activity out-lap and otherwise shows live elapsed time measured from the actual activity lap start, including when the scene begins partway through a lap.
- [ ] Best Lap shows `--:--.--` during the out-lap, falls back to the live current-lap time before any lap has completed, and then shows the fastest lap completed before the current frame across the whole activity.
- [ ] Lap durations use `MM:SS.ss` below one hour and extend to `HH:MM:SS.ss` when necessary.
- [ ] Dense video-frame timing preserves lap-boundary resets instead of interpolating between the end of one lap and the start of the next.
- [ ] Best Lap's quasi-static label and value are prepared by lap state so unchanged completed-lap content is not redrawn every frame.
- [ ] Automated tests cover out-lap, first in-progress lap, a lap boundary, a later lap, a scene beginning mid-lap, and a best lap completed before the scene trim, with matching preview and rendered text behavior.
