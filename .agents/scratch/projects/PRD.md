Status: ready-for-agent

# Project Save and Load

## Problem Statement

OVRLEY users can currently assemble an editing session from a template, an activity file, an optional source video or background image, synchronization choices, render settings, and editor workspace state, but they cannot save that assembled session as a project and restore it later. Reopening the application requires the user to find and import the same inputs again and manually reconstruct timing, export, viewport, and editor choices.

Templates are reusable, canonical overlay definitions rather than project-owned copies. A project therefore needs to refer to its template and pick up intentional changes made to that template, while still offering a recovery path if the referenced template no longer exists. Activity and video data must remain owned by their source files and be parsed or probed again when the project loads; normalized activity payloads and video contents must not be embedded in ordinary project files.

The current application state is not a persistence contract. It contains non-serializable objects, actions, runtime preview-server identifiers, processing flags, caches, and dialog drafts alongside durable values. Saving a raw store dump would couple project compatibility to internal Zustand layout and could restore invalid runtime state.

## Solution

Add a versioned OVRLEY project JSON format and project lifecycle that explicitly snapshots the durable session state. A project records the canonical template reference, an embedded template fallback used only when that reference cannot be found, source paths for activity and media, video synchronization, committed render settings, timeline state, and selected editor workspace preferences.

When opening a project, OVRLEY resolves the referenced template first. If it exists, OVRLEY loads it without comparing revisions or warning about changes; template changes are expected to flow into every project that uses the template. If it is missing, OVRLEY tells the user and offers to locate it, load the embedded fallback, or cancel.

Activity files are reparsed through their normal ingress pipeline. Videos are reimported, probed, registered with the preview server, and inspected for telemetry through their normal ingress pipeline. Project hydration uses the newly created runtime media state together with the saved synchronization, render, timeline, and workspace settings. The operation validates and stages the project before replacing the current editor session, and it starts the restored session paused.

## User Stories

1. As an OVRLEY user, I want to save my current work as a project, so that I can continue the same editing session later.
2. As an OVRLEY user, I want to open a previously saved project, so that I do not have to reconstruct my session manually.
3. As an OVRLEY user, I want a project to remember which template I used, so that the correct overlay definition is loaded.
4. As an OVRLEY user, I want a project to use the current contents of its referenced template, so that intentional template improvements flow into projects that use it.
5. As an OVRLEY user, I do not want warnings whenever a referenced template changes, so that normal template iteration does not interrupt project loading.
6. As an OVRLEY user, I want the project to contain a fallback copy of its template, so that a missing template does not necessarily make the project unusable.
7. As an OVRLEY user, I want to be told when the referenced template cannot be found, so that I understand why normal loading cannot continue.
8. As an OVRLEY user, I want to locate a missing template manually, so that I can repair a moved template reference.
9. As an OVRLEY user, I want to load the embedded fallback when a template is missing, so that I can recover the saved session.
10. As an OVRLEY user, I want to cancel loading when a template is missing, so that my current session remains untouched.
11. As an OVRLEY user, I want unsaved template edits resolved before saving a project, so that the canonical template and project reference do not disagree.
12. As an OVRLEY user, I want the project to remember my activity source file, so that the same activity is restored.
13. As an OVRLEY user, I want activity data reparsed when the project opens, so that the project does not duplicate normalized activity data.
14. As an OVRLEY user, I want malformed activity input to fail through the normal activity import contract, so that a project never silently repairs invalid telemetry.
15. As an OVRLEY user, I want to be informed when an activity file is missing, so that the project does not silently open with incomplete required data.
16. As an OVRLEY user, I want to locate a moved activity file, so that I can restore and repair the project.
17. As an OVRLEY user, I want the project to remember my imported video, so that the same source video is restored.
18. As an OVRLEY user, I want the video to be probed again when the project opens, so that preview and render metadata come from the actual source file.
19. As an OVRLEY user, I want the video preview to work after reopening a project, so that stale preview URLs are never restored from disk.
20. As an OVRLEY user, I want embedded video telemetry to be extracted again on load, so that activity derived from the video follows the normal telemetry pipeline.
21. As an OVRLEY user, I want a separately loaded activity to remain the active activity when the project also has video telemetry, so that media restoration preserves the saved source relationship.
22. As an OVRLEY user, I want to be informed when a video file is missing, so that OVRLEY does not pretend the project is fully restored.
23. As an OVRLEY user, I want to locate a moved video file, so that I can repair the project.
24. As an OVRLEY user, I want an imported background image path restored when one is part of the session, so that image-backed preview sessions can be reopened.
25. As an OVRLEY user, I want my video synchronization offset restored, so that the activity and video remain aligned.
26. As an OVRLEY user, I want my video synchronization timezone choice restored, so that ambiguous camera timestamps retain my decision.
27. As an OVRLEY user, I want the project to remember whether export is transparent or composited with video, so that the render workflow reopens with the intended pipeline.
28. As an OVRLEY user, I want the project to remember my output FPS, so that subsequent renders use my chosen frame rate.
29. As an OVRLEY user, I want the project to remember my widget update rate, so that overlay sampling behavior is restored.
30. As an OVRLEY user, I want the project to remember my codec, so that I do not need to select it for every reopened project.
31. As an OVRLEY user, I want the project to remember my bitrate in Mbps, so that compressed output quality is restored.
32. As an OVRLEY user, I want the project to remember my export range, so that subsequent renders target the same timeline interval.
33. As an OVRLEY user, I want project loading to retain a selected codec even if it is unavailable on the current computer, so that opening a project does not produce hardware-related warning spam.
34. As an OVRLEY user, I want codec availability enforced when I open export settings or start rendering, so that an unsupported render cannot begin.
35. As an OVRLEY user, I want confirmed render settings to become the project's committed render settings, so that the next project save captures what I last used.
36. As an OVRLEY user, I want unconfirmed render-dialog edits to remain temporary, so that cancelling the dialog does not change the project.
37. As an OVRLEY user, I want the current playhead position restored, so that I return to the same moment in the activity.
38. As an OVRLEY user, I want the timeline's visible range restored, so that its zoom and pan position return with the project.
39. As an OVRLEY user, I want timeline views fitted to all media, video, or activity to restore naturally, so that the timeline looks as I left it without storing redundant view-mode state.
40. As an OVRLEY user, I want a saved timeline viewport clamped to the reparsed media duration, so that changed source duration cannot create an unusable view.
41. As an OVRLEY user, I want the project to remember whether video preview audio is muted, so that reopening does not unexpectedly change playback sound.
42. As an OVRLEY user, I want the editor canvas zoom restored, so that the overlay workspace returns at the same scale.
43. As an OVRLEY user, I want the editor background mode restored, so that video, image, checker, black, or white preview mode returns as saved.
44. As an OVRLEY user, I want grid visibility restored, so that my positioning workspace returns as saved.
45. As an OVRLEY user, I want snap-to-grid restored, so that widget movement behaves as it did before closing the project.
46. As an OVRLEY user, I want my selected widgets restored when they still exist in the loaded template, so that I can continue editing the same widgets.
47. As an OVRLEY user, I want invalid saved widget selections reconciled against the current canonical template, so that removed widgets are not represented as active selections.
48. As an OVRLEY user, I want preview interpolation preference restored, so that preview behavior remains consistent across sessions.
49. As an OVRLEY user, I want a restored project to open paused, so that media does not start playing unexpectedly.
50. As an OVRLEY user, I want project loading to avoid restoring a previous render or import operation, so that runtime jobs never masquerade as durable session state.
51. As an OVRLEY user, I want an invalid or unsupported project file rejected clearly, so that malformed state is not repaired with hidden defaults.
52. As an OVRLEY user, I want project loading to preserve my current session when validation or required dependency restoration fails, so that a failed open does not destroy current work.
53. As an OVRLEY user, I want undo and redo history reset after loading a project, so that history from another session cannot mutate the restored project.
54. As an OVRLEY user, I want project changes tracked separately from template changes, so that project workspace state never makes the canonical template appear modified.
55. As an OVRLEY user, I want relative media paths supported where possible, so that moving a project together with its source folder can preserve its references.
56. As an OVRLEY user, I want project files to remain reasonably small, so that videos and parsed telemetry are not duplicated inside them.

## Implementation Decisions

- Introduce a dedicated, versioned OVRLEY project JSON contract. Project serialization must use an explicit durable-state projection and must not serialize the Zustand store wholesale.
- Give project files an unambiguous project-specific filename extension and native open/save dialog filter. The logical contract remains JSON and includes a format identifier, schema version, and save timestamp.
- Treat the referenced template as the canonical overlay definition. The reference must represent both backend-managed templates and user-selected template files through one canonical source descriptor.
- Embed the normalized durable template state as a recovery fallback whenever the project is saved. The fallback is not a second active source and is consulted only when the referenced template cannot be resolved.
- When the referenced template exists, load it without content hashing, revision comparison, or change notification. External changes are treated as intentional.
- When the referenced template is missing, present exactly the recovery choices needed to locate it, load the embedded fallback, or cancel. Do not silently select the fallback.
- Require the current canonical template to be saved before a project that references it is saved. This prevents unsaved in-memory template edits from being captured only in the fallback while the normal reference points at older content.
- Store activity source descriptors only. Do not store `parsedActivity`, activity summaries, normalized series, or other derived activity data in the project.
- Preserve the original native source path for every supported local activity format at activity-ingress time. The current filename-only state is insufficient for project restoration.
- Reparse the activity through its existing format-specific ingress and canonical finalization boundary during project loading. Project loading must not add an alternate activity parser or normalization path.
- Store video and optional background-image source descriptors only. Do not embed media bytes in the project.
- Reimport and probe the video through the existing video import boundary. Runtime preview URL, preview-server import ID, preview warnings, and probed metadata are reconstructed and never serialized as authoritative project state.
- Re-extract embedded video telemetry through the existing telemetry boundary. When both a separate activity and video telemetry are present, restore the separate activity as active and retain video telemetry according to the existing media-source model.
- Persist committed video synchronization state, including offset and the optional timezone interpretation selected for ambiguous metadata. Do not persist transient drag-preview offset or derived warning text.
- Use a canonical path locator that explicitly states whether a path is relative to the project file or absolute. Resolve relative paths from the project location. Missing required sources must enter an explicit locate-or-cancel flow rather than degrading to empty media state.
- Add one canonical project-owned render-settings aggregate. It owns output FPS, widget update rate, export mode, codec, optional bitrate in Mbps, and export range.
- Promote the currently dialog-local committed render choices into application state. Opening the export dialog initializes its draft from project render settings; confirming a render commits the draft; cancelling leaves committed settings unchanged.
- Do not separately persist output-format or hardware-acceleration UI selections when the canonical codec identifier already determines them.
- Retain a saved codec during project load regardless of current hardware availability. Availability is resolved at the export dialog or render boundary, not while opening the project.
- Persist timeline playhead and the exact visible timeline range as `viewStart` and `viewEnd`. Do not persist a redundant timeline fit-mode identifier because the matching All, Video, or Activity target is derived from the range.
- Clamp a restored viewport to the timeline produced by reparsed media as documented product behavior. Reject malformed ranges; clamping is not a repair for malformed input.
- Persist editor canvas zoom, background mode, grid visibility, snap-to-grid, video mute, preview interpolation, and widget selection as project workspace state.
- Move editor canvas zoom, background mode, grid visibility, and snap-to-grid from hook-local state into the editor store slice, with strict store actions and selector-hook access. The shell hook continues to own derived UI scale and development-only debug mode.
- Keep template state, project state, and runtime state separate. Project workspace changes must not participate in template dirty tracking.
- Validate and normalize the complete project once at the project ingress boundary. Consumers receive canonical validated fields and must not repeat coercion or defensive validation.
- Stage project parsing, template resolution, external source checks, media reconstruction, and target state construction before replacing the current editor document. A failure before commit leaves the current session intact.
- Hydrate the complete target session through a dedicated project operation rather than replaying ordinary import setters in an order that could overwrite saved timing, synchronization, render, or workspace values.
- Reset undo and redo history after successful project hydration. Restore the playhead but force playback state to paused.
- Exclude editor instances, actions, processing flags, render progress, active render IDs, errors, template lists, platform information, codec caches, playback/scrubbing state, open dialogs, hovered widgets, active drags, selection rectangles, DOM measurements, and undo history from the project contract.
- Add narrowly scoped native project read and write commands. Project writes validate the project contract before writing, following the existing validated file-write boundary rather than exposing an unrestricted persistence API.
- Track the loaded project identity and last-saved project snapshot separately from template save status so Save and Save As behavior can operate without conflating project and template ownership.

## Testing Decisions

- The primary test seam is the project lifecycle orchestrator. Tests should save representative application state to a project payload and load project payloads through mocked native file, template, activity, video, and telemetry boundaries, asserting externally observable restored state and user recovery outcomes.
- Tests should verify behavior rather than implementation details. They should not assert the order or number of internal Zustand setter calls, hook composition, or private helper structure unless ordering is itself an observable media-ownership requirement.
- Project snapshot tests should verify that every agreed durable field is emitted and that runtime, derived, action, history, and process fields are absent.
- Project contract tests should cover supported format/version handling, strict required-field validation, render-setting validation, workspace enum and boolean validation, valid nullable optional media, malformed path locators, and malformed viewport ranges.
- Template-resolution tests should cover an existing canonical template, an intentionally changed existing template with no warning, a missing template located by the user, a missing template restored from fallback, and cancellation that preserves the current session.
- Activity restoration tests should cover each supported source family at the highest practical import boundary and verify that project loading reparses rather than accepting embedded normalized activity data.
- Media lifecycle tests should cover video preview re-registration, fresh metadata, telemetry extraction, separate activity precedence, restored sync offset, missing-video relinking, background-image restoration, and absence of serialized runtime preview identifiers.
- Render workflow tests should verify that project settings initialize the export dialog, confirmed settings become committed project state, cancelled drafts remain transient, bitrate is restored only where valid, and unavailable codecs are retained until the export/render boundary handles availability.
- Timeline tests should verify exact manual zoom/pan restoration, derived fit-target display, viewport clamping after source-duration change, restored playhead, and paused playback.
- Editor workspace tests should verify canvas zoom, background mode, grid visibility, snap-to-grid, mute, interpolation, and selection reconciliation against a changed canonical template.
- Atomicity tests should verify that malformed projects, missing dependencies followed by cancellation, and parser/probe failures do not partially replace the active session.
- Undo-history characterization should verify that successful project load starts a fresh history while project hydration itself does not create undo entries.
- Existing template snapshot tests, activity import-boundary tests, media slice lifecycle tests, render-workflow tests, timeline viewport tests, editor shell tests, and undo-history tests are the prior art to extend.

## Out of Scope

- Embedding parsed or normalized activity data in a project.
- Embedding video, activity, or background-image file bytes in a project.
- A ZIP archive or portable project package that collects external media.
- Automatic template revision detection, template content hashes, or notifications when an existing template changes.
- Silently loading the embedded template fallback.
- Persisting undo/redo history.
- Resuming playback, scrubbing, imports, telemetry extraction, or renders in progress.
- Persisting render progress, error messages, codec detection results, platform state, dialog state, hover state, drag state, DOM measurements, UI scale, or debug mode.
- Autosave, crash recovery, cloud synchronization, collaboration, or project locking.
- Changing activity parsing, video probing, telemetry extraction, template normalization, or rendering algorithms beyond exposing their existing boundaries to project loading.

## Further Notes

- The template fallback exists solely for missing-template recovery. It should be refreshed from the canonical saved template whenever the project is saved.
- Template changes are expected to alter projects that reference the template. This is intentional and is the reason the normal load path always prefers an available canonical template.
- Activity and video contents can change independently after a project is saved because they are reparsed from disk. The project restores the selected sources and project-owned settings; it does not freeze source-file contents.
- Timeline viewport and editor workspace values are durable project presentation state, but they are not part of template serialization and must not mark the template dirty.
- The project format should use the same product name, OVRLEY, in its format identifier and user-facing file dialogs.
