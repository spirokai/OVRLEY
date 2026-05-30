Status: ready-for-agent

# Architecture Deepening for `app/`

## Problem Statement

The `app/` frontend currently contains several shallow modules whose interfaces leak too much implementation detail across feature seams. This shows up in four concrete ways:

1. `overlay-editor` exports both editor UI and pure preview-domain behavior, so unrelated features depend on it for interpolation, export-window math, and preview assets.
2. Preview playback ownership is split across store flags and several hooks, so changing one timing rule requires understanding multiple modules at once.
3. Widget identity is derived from array position, so selection and widget-management behavior must constantly compensate for unstable ids.
4. The overlay editor session is spread across a wide orchestration hook and multiple callback bundles, which makes interactive behavior difficult to trace, test, and evolve.

From the user's perspective, the editor should keep the same selection, preview, playback, and widget-editing behavior. The change is architectural: deeper modules, smaller seams, and better locality for future work.

## Solution

Create a focused remediation track for the `app/` frontend that deepens the four weakest seams:

1. Extract a dedicated preview-domain module from `overlay-editor`.
2. Collapse preview playback ownership behind one playback seam.
3. Persist stable widget identity instead of deriving it from indexes.
4. Turn the overlay editor session into a deeper module with narrower React adapters.

The implementation should preserve current user-visible behavior while making the codebase more testable, easier to navigate, and safer to change.

## User Stories

1. As an overlay editor user, I want preview playback to remain synchronized with the imported video, so that refactoring does not change my editing workflow.
2. As an overlay editor user, I want route, elevation, metric, and text previews to render the same way as before, so that architecture work does not alter my template output.
3. As an overlay editor user, I want scrubbing and playback handoff to remain accurate, so that I can trust the playhead while editing.
4. As an overlay editor user, I want widget selection and multi-selection to behave consistently after add, delete, and reorder operations, so that editing remains predictable.
5. As an overlay editor user, I want drag, resize, scale, and rotate interactions to remain stable, so that refactoring does not introduce interaction regressions.
6. As a template author, I want saved templates to preserve widget identity correctly, so that future reorder and selection behavior is reliable across sessions.
7. As a maintainer, I want preview-domain behavior to live behind one explicit seam, so that changes to interpolation or export-window logic do not leak through `overlay-editor`.
8. As a maintainer, I want playback timing rules to be concentrated in one module, so that timing bugs can be fixed with better locality.
9. As a maintainer, I want widget identity rules to be owned by one module, so that callers stop encoding array-position assumptions.
10. As a maintainer, I want the overlay editor session to expose a smaller interface, so that interactive behavior is easier to reason about and change safely.
11. As an AFK agent, I want clear affected modules and acceptance criteria for each architecture candidate, so that implementation work can proceed without extra human context.
12. As an AFK agent, I want practical regression tests around timing, selection, and preview behavior, so that the highest-risk paths are guarded before deeper refactors land.
13. As a reviewer, I want the new seams to be explicit and behavior-focused, so that code review can evaluate interfaces instead of chasing effects across files.
14. As the project owner, I want architecture work to improve AI navigability without changing product behavior, so that future maintenance gets cheaper instead of noisier.

## Implementation Decisions

- Introduce a dedicated preview-domain module that owns interpolation, export-window scoping, effective preview FPS calculation, and preview assets currently leaking through `overlay-editor`.
- Keep `overlay-editor` focused on editor-session behavior and canvas concerns; after the refactor it should consume preview-domain behavior rather than re-exporting it.
- Collapse preview playback ownership behind one playback module interface that owns clock selection, handoff rules, frame publication, and scrub lifecycle while using store and DOM adapters behind the seam.
- Persist stable widget identity inside widget data and template payloads, with compatibility logic that upgrades legacy templates which still rely on index-derived identity.
- Remove selection-remapping work that only exists to recover from unstable widget identity; selection should follow durable widget ids rather than array position.
- Deepen the overlay editor session so that selection policy, draft state, viewport state, and gesture lifecycle are concentrated in one module with smaller React adapters.
- Prefer extracting pure helpers before splitting orchestration hooks so the riskiest logic becomes testable without React effects.
- Preserve all current user-visible behavior unless a regression test proves the old behavior was already incorrect.

## Testing Decisions

- A good test exercises external behavior at the seam: playback time published, selection retained, preview output derived, or widget identity preserved. It should not assert hook internals, effect ordering, or temporary refs unless that is the public contract.
- The new preview-domain seam should be covered with pure unit tests around interpolation, export-window scoping, progress rebasing, and preview-FPS behavior.
- The playback seam should be covered with characterization and regression tests for play, pause, scrub, end-of-playback handling, and video/timeline handoff.
- The widget identity seam should be covered with tests for legacy-template upgrade, stable selection across config replacement, and add/delete/reorder behavior.
- The overlay editor session should be covered with interaction-style regression tests around selection, marquee, drag ownership, and draft commit behavior.
- Prior art already exists in the codebase and should be extended rather than replaced: `usePlaybackEngine.test.jsx`, `playerTimeline.test.js`, `useVideoPlaybackClock.test.jsx`, `useVideoPreview.test.jsx`, `OverlayEditor.selection.test.jsx`, `createEditorSlice.selection.test.js`, and `templateSnapshot.test.js`.
- Manual QA should stay focused on the highest-risk interactive paths: rapid scrubbing, switching between timeline and video playback, loading a new video, selecting and deleting multiple widgets, and manipulating widgets through moveable handles.

## Preferred Implementation Order

Implement the `09` to `16` issues in this order to maximize leverage and minimize rework:

1. `11` Give widgets stable identity
2. `13` Separate durable template state from effective config
3. `09` Extract preview domain from `overlay-editor`
4. `10` Collapse preview playback ownership
5. `15` Concentrate the template workspace
6. `16` Concentrate activity and video timing synchronization
7. `14` Collapse the render job lifecycle
8. `12` Deepen the overlay editor session

This order is preferred because:

- `11` deletes compensating logic early and makes later editor work less brittle.
- `13` creates the representation seam that `15` and `14` should build on.
- `09` removes preview-domain leakage before playback and editor-session refactors deepen those seams.
- `10` should establish one playback owner before `16` concentrates timing rules around that owner.
- `15` should land before `14` when possible so render lifecycle work depends on a cleaner template workspace.
- `12` should come last because it benefits from the cleaner seams created by `09`, `10`, and `11` and carries the highest risk of accidental micro-file proliferation.

Key dependency rules:

- Prefer `13` before `15`, and ideally before `14`.
- Prefer `09` before `12`.
- Prefer `10` before `16`.
- Prefer `11` before `12`.

## Out of Scope

- No visual redesign of the editor, player, or widget previews.
- No change to the Rust rendering pipeline.
- No change to template authoring workflows beyond durable widget identity where required.
- No migration away from Zustand, React hooks, or the current feature-folder layout as a primary goal.
- No unrelated cleanup of store slices, persistence, or render workflow beyond what these four candidates require.

## Further Notes

- This PRD is the architecture-deepening follow-up to the `app/` review and is intentionally narrower than the existing `bad-practice/PRD.md`.
- The corresponding implementation issues are:
  - `09` Extract preview domain from `overlay-editor`
  - `10` Collapse preview playback ownership
  - `11` Give widgets stable identity
  - `12` Deepen the overlay editor session
  - `13` Separate durable template state from effective config
  - `14` Collapse the render job lifecycle
  - `15` Concentrate the template workspace
  - `16` Concentrate activity and video timing synchronization
- The issues carry the specific affected-file lists and practical testing strategy so implementation can start immediately.
