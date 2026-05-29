Status: ready-for-agent

# 02 - Replace Timer-Based Config Synchronization Guard with Explicit Update Flow

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

Config update coordination currently depends on a module-global flag that is set before certain updates and cleared later by a timeout. This works as an implicit loop-prevention mechanism, but it makes correctness depend on timing instead of explicit ownership.

That pattern is risky because:

- Overlapping updates can race with the timeout window.
- The logic is hard to reason about from call sites because the important state lives outside the store and is not visible in the update interface.
- Tests have to indirectly depend on timing to verify behavior.

The visible behavior must stay the same within a session. Editing timeline values, loading templates, changing config, and recalculating dirty state should continue to behave exactly as they do now from a user perspective. This issue must not reintroduce app-close persistence.

## What to build

Replace the timer-driven semaphore with an explicit config update flow that expresses whether an update is:

- originating from config hydration or template application
- originating from timeline edits
- intended to synchronize timeline state and config state together

The module should make loop prevention structural instead of time-based. The result should concentrate config/timeline synchronization rules behind a small interface that callers can use without knowing about a hidden global flag.

## Affected files

- `app/src/store/store-utils.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/store/slices/createTemplateSlice.js`

## Suggested plan

1. Map the current config update paths: template hydration, direct config replacement, timeline edits, and playhead-related mutations.
2. Define a narrow update interface or action set that makes the origin of each mutation explicit.
3. Move loop-prevention logic into that interface so callers no longer depend on a hidden module-global variable.
4. Remove the timeout-based reset behavior entirely.
5. Keep dirty-state calculation and timeline synchronization behavior equivalent to current behavior while removing any dependence on app-close persistence.
6. Add focused tests for repeated rapid edits, template hydration, and mixed config/timeline updates.

## Practical testing strategy

- Write characterization tests first for the current in-session behaviors: timeline edits updating config scene timing, config replacement updating timeline state, and dirty-state recalculation.
- Add focused tests that perform rapid consecutive updates so the refactor is verified without relying on timer windows.
- Add a test that hydrates or replaces config, then immediately performs timeline edits, to cover mixed-origin update ordering.
- Add a regression test for edge values like start/end changes at boundaries to ensure synchronization rules stay intact.
- Keep a manual QA pass for quick user flows: load a template, scrub timeline bounds, edit start/end, and confirm the UI remains consistent.

## Acceptance criteria

- [ ] No config synchronization behavior depends on a module-global boolean plus delayed timeout reset
- [ ] The flow for config-originated updates versus timeline-originated updates is explicit in the store or its helper seam
- [ ] Timeline edits still update config scene timing correctly
- [ ] Config replacement and template hydration still update timeline state correctly
- [ ] Dirty-state tracking remains functionally equivalent within a session and no app-close persistence is reintroduced
- [ ] Tests cover consecutive rapid updates without relying on timer timing windows

## Blocked by

None - can start immediately
