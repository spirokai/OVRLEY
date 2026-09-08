# Sync Doctor: manual video-to-data sync design

Status: wayfinder:map

## Destination

A locked decision on which manual video-to-data sync workflow(s) OVRLEY should support, plus a schematic UI blueprint of the sync-doctor screen, ready to hand off to implementation. No prototype is built as part of this map.

## Notes

- **Domain**: OVRLEY desktop app (React 19 + Tauri + Rust/Skia core). Users are casual cyclists / action-cam owners importing short ride videos and `.fit`/`.gpx` activities.
- **Standing facts from charting**:
  - Sync problem is purely **time-offset** (no stretch/compress).
  - Offset range: a few seconds up to several hours.
  - Typical clip today: one video, a few minutes. Roadmap: multiple clips, aligned **one at a time**.
  - Target precision: **within ~1 second is fantastic**.
  - GPS, speed, elevation, heading are effectively always present.
  - Video + data overlay currently live on a single screen.
  - **Image/motion analysis is explicitly out of scope** for this effort.
- **Skills every session should consult**: `grilling` for HITL decision tickets; `wayfinder` for map discipline.
- **Preferences for this effort**: optimize for layman convenience over power-user control; prefer established/recognizable patterns; keep the screen scannable.

## Decisions so far

- [What manual video-to-data sync workflows exist in the industry?](issues/01-industry-workflows.md): Industry converges on automatic metadata, single-event sync, and numeric/slider offset; two-point sync is a video/audio multicam pattern, not a telemetry-overlay pattern. Recommendation is a combination of automatic metadata default + one-point fallback + numeric slider with live preview.
- [Which activity signals and events are most usable for manual sync?](issues/02-sync-signals.md): Movement state changes (start/stop) and stops/pauses are the strongest layman anchors; heading changes, speed peaks, and elevation crests/dips are secondary; optional sensors and map landmarks are niche.
- [Which sync workflow should OVRLEY support?](issues/03-chosen-workflow.md): Primary fallback is **multi-landmark turn/stop correlation** (2–3 landmarks) with live preview and the existing numeric nudge; a single map-landmark mode is viable only after real map tiles are added and is deferred to a separate spec.
- [What is the schematic UI blueprint for the sync-doctor screen?](issues/04-ui-blueprint.md): Persistent toolbar screen with a video player + live overlay widgets, a map/data context panel, a toolbar for marking stop/turn/map landmarks, and a right panel listing landmarks + candidate-offset toggle group + numeric fine-tuning + reset/clear actions.
- [How should the sync doctor handle multiple clips sequentially?](issues/05-multiple-clips.md): Per-clip workflow; clip selection lives in the left toolbar drawer with auto-sync / manual-sync status icons; each clip keeps its own offset, landmarks, and candidates.

## Not yet specified

- Exact interaction gestures for marking a sync point (click, drag playhead, keyboard shortcut, scrub + snap).
- Error/edge cases: clip starts before activity, clip ends after activity, duplicate sync, reset/undo behavior.
- Whether sync settings should persist per clip / per project and how that surfaces elsewhere in the app.
- Keyboard accessibility and shortcut policy for the sync doctor.
- How the sync doctor is reached from the existing import/editor flow.
- Telemetry or onboarding hints for first-time users.

## Out of scope

- Image/motion-analysis-based auto-sync (explicitly excluded by request).
- New automatic sync algorithms beyond existing metadata/GPS/filename heuristics.
- Building a runnable UI prototype or production implementation.
