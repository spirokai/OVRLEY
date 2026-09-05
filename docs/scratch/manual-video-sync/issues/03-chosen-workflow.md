# Which sync workflow should OVRLEY support?

Status: closed
Labels: wayfinder:grilling

## Question

Based on the research into industry workflows and usable sync signals, which manual offset-correction workflow should OVRLEY lock in? The decision must fit a layman user, a pure time-offset problem, offsets from seconds to hours, one clip aligned at a time, and target precision of ~1 second. If multiple variants are warranted (e.g., a quick one-point mode and a precise two-point mode), state the conditions and default.

## Blocking

- [What manual video-to-data sync workflows exist in the industry?](01-industry-workflows.md)
- [Which activity signals and events are most usable for manual sync?](02-sync-signals.md)

## Comments

## Resolution

OVRLEY’s manual fallback workflow is **multi-landmark turn/stop correlation** (up to 3 landmarks), not a blind single-point match.

Key decisions:

- **Primary path:** the user marks 2–3 recognizable turn/stop landmarks in the video. The UI detects corresponding heading-change and stop events in the activity data, computes candidate offsets from the combinations, and presents the best matches with a live video preview so the user can confirm.
- **Minimum viable landmark:** a single map landmark can work once OVRLEY has real map tiles, because spatial context removes ambiguity. That mode is deferred to a separate spec/plan because map tiles are not currently available.
- **No standalone single turn/stop landmark:** one turn or stop is too ambiguous in a long activity with many similar events.
- **Fine-tuning:** the existing numeric offset input with arrow keys remains the final adjustment step.
- **Out of scope for this workflow:** image/motion analysis; automatic metadata sync is already implemented and is the default path.
