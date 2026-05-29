Status: ready-for-agent

# 06 - Replace Stringify-Driven Change Detection and Cloning in Hot Paths

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

`JSON.stringify` is currently used in several places as a generic substitute for deep clone, deep equality, and effect dependency signatures. While this can work for plain serializable data, it carries several downsides:

- intent is unclear because one mechanism is solving several different problems
- work scales with object size, even in paths that are touched frequently
- callers inherit hidden assumptions about serializability and property ordering

This issue is not about eliminating every legitimate use of JSON serialization. It is about removing it from places where it is acting as structural glue for state management or render coordination.

The visible behavior must stay the same within a session: dirty tracking, template normalization, widget updates, and moveable remeasurement should continue to behave as they do today. This issue must not add any new app-close persistence behavior.

## What to build

Replace stringify-driven structural checks in the targeted modules with clearer, purpose-built mechanisms.

Examples of acceptable replacements include:

- explicit structural comparison helpers for known shapes
- version counters or revision tokens where callers only need "did relevant data change?"
- `structuredClone` or shape-aware clone helpers where cloning is still required

The refactor should focus first on hot or central paths, especially state comparison, config dirty tracking, and effect dependency signatures tied to rendered widget data.

## Affected files

- `app/src/store/store-utils.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/store/slices/createTemplateSlice.js`
- `app/src/features/template-manager/utils/templateSnapshot.js`
- `app/src/features/overlay-editor/hooks/useOverlayEditorState.js`
- `app/src/lib/config-utils.js`

## Suggested plan

1. Separate current stringify usage by purpose: cloning, equality, and dependency invalidation.
2. For each purpose, choose the narrowest replacement that matches the actual contract.
3. Replace hot-path dependency signatures with explicit revision or comparison logic.
4. Replace central dirty-state comparisons with helpers that make the compared shape and semantics obvious.
5. Keep compatibility with plain serializable template data and existing in-session config behavior.
6. Add targeted tests around dirty tracking, template equality, and widget remeasurement triggers.

## Practical testing strategy

- Add characterization tests around dirty-state transitions before refactoring so the semantics are frozen independent of implementation.
- Add targeted tests for template normalization/equality helpers using representative config shapes, including nested widget data.
- Add regression tests for widget remeasurement triggers so relevant widget-data changes still cause the same downstream behavior.
- Benchmark-style automation is optional, but add at least one focused test that proves the new mechanism is semantic, not stringify-based.
- Keep manual QA for interactive hotspots touched by these comparisons, especially widget editing and selection/moveable behavior after data changes.

## Acceptance criteria

- [x] The targeted hot or central paths no longer rely on `JSON.stringify` as a generic equality, clone, or dependency mechanism
- [x] Dirty-state tracking remains functionally equivalent for template and config edits within a session
- [x] Widget remeasurement still triggers when relevant widget data changes and does not regress selection behavior
- [x] Any remaining JSON serialization is limited to true serialization concerns, not general state coordination
- [x] Tests cover the behavior that replaced stringify-based equality or dependency logic

## Blocked by

None - can start immediately
