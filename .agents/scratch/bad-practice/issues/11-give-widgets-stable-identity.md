Status: ready-for-agent

# 11 - Give Widgets Stable Identity

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

Widget identity is currently derived from array position through ids like `label-0`, `value-2`, and `plot-1`. That means widget identity is not durable, so selection and widget-management code must repeatedly compensate for index changes.

This is a bad fit because:

- deleting or reordering widgets can change identity even when the widget itself did not conceptually change
- selection logic needs remapping behavior to recover locality after config replacement
- sidebar and editor behavior both leak index-derived assumptions through their interfaces
- template persistence has a weaker seam because identity is reconstructed instead of preserved

The user-visible goal is stability, not new functionality. Add, delete, reorder, and selection behavior should continue to feel the same while the underlying identity model becomes durable.

## What to build

Persist stable widget identity inside widget data and stop deriving widget ids from array position. The widget-identity seam should own id creation, legacy-template compatibility, and widget lookup so selection, delete, add, reorder, and config replacement behavior no longer depends on indexes.

The main user-visible outcome is that editor behavior stays the same while selection and widget-management logic stop compensating for unstable ids behind the scenes.

## Affected files

- `app/src/lib/widget-config.js`
- `app/src/store/slices/createEditorSlice.js`
- `app/src/features/widget-editor/hooks/useWidgetManager.js`
- `app/src/features/overlay-editor/hooks/useOverlayEditorState.js`
- `app/src/features/template-manager/utils/templateSnapshot.js`
- `app/src/store/store-utils.js`

## Suggested plan

1. Define how widget identity will be persisted in widget data and carried through template save/load flows.
2. Add compatibility logic so legacy templates without stable widget ids still hydrate successfully.
3. Refactor widget-config helpers so widget lookup and CRUD behavior use durable ids instead of array-derived ids.
4. Remove or simplify selection-remapping logic that only exists to recover from unstable identity.
5. Update widget sidebar and overlay editor behavior to consume the stable identity seam directly.
6. Verify template compatibility plus add/delete/reorder/selection behavior through tests and manual QA.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer changing existing modules first and add at most one new top-level identity module only if it replaces scattered ownership.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer changing existing widget and template modules over creating new tiny identity helpers.
- If a new identity module is introduced, it should own the whole concern: id creation, lookup, and legacy compatibility.
- Delete compensating code as the identity seam deepens; do not leave old remapping paths in place behind new abstractions.
- Favor pure concerns and fewer, deeper modules over many small utilities.
- The refactor should simplify the module graph, not add parallel identity codepaths.

## Practical testing strategy

- Add direct unit coverage for widget-identity helpers: id creation, legacy-template upgrade, widget lookup, and config-to-widget mapping.
- Extend selection regression tests so config replacement, widget delete, and reorder behavior keep the same selected widget by stable id.
- Add template-snapshot coverage proving saved templates preserve widget identity and legacy templates still load correctly.
- Use existing tests as prior art: `createEditorSlice.selection.test.js`, `OverlayEditor.selection.test.jsx`, `templateSnapshot.test.js`, and `useEditorShellState.test.jsx`.
- Manually verify add/delete/reorder flows in the widget sidebar plus single-select and multi-select behavior in the canvas.

## Acceptance criteria

- [x] Widget identity is persisted independently of array position
- [x] Legacy templates without stable widget ids still load successfully
- [x] Selection no longer depends on remapping index-derived widget ids after config replacement
- [x] Add, delete, reorder, and reset behavior preserve the correct widget identity
- [x] Tests cover identity upgrade, template persistence, and selection regression paths

## Blocked by

None - can start immediately
