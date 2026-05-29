Status: ready-for-agent

# 03 - Unify Overlay Selection Ownership

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

Overlay selection is currently split between hook-local multi-selection state and a global primary-selection value in the store. Effects and refs then synchronize those values back and forth to keep them aligned.

That pattern increases complexity because:

- There are two sources of truth for closely related concepts.
- Synchronization behavior is distributed across effects instead of being owned by one coherent module.
- Escape-hatch refs are needed to suppress feedback loops, which usually signals that the seam is too shallow.

The user experience must not change. Single selection, multi-selection, marquee selection, deletion behavior, and primary-selection behavior should all feel identical in the editor.

## What to build

Choose one clear owner for overlay selection semantics and move the rest behind it.

The deepened module should own:

- the ordered list of selected widget ids
- the primary selected widget id
- normalization when widgets appear, disappear, or reorder
- transitions between single select, multi-select, and transient group-drag states

The external interface should let callers express selection intents without manually synchronizing local and global state.

## Affected files

- `app/src/features/overlay-editor/hooks/useOverlayEditorState.js`
- `app/src/features/overlay-editor/hooks/useEditorKeyboard.js`
- `app/src/features/overlay-editor/utils/createOverlayPointerHandlers.js`
- `app/src/features/overlay-editor/utils/overlayEditorHelpers.js`
- `app/src/store/slices/createEditorSlice.js`

## Suggested plan

1. Document the current selection states and transitions: click select, additive select, marquee select, group drag, widget removal, and config replacement.
2. Decide whether selection should live entirely in the overlay editor seam or entirely in the store, then make that ownership explicit.
3. Consolidate normalization rules and primary-id derivation into the owning module.
4. Replace effect-based state repair with direct intent-based actions where possible.
5. Keep the same public behavior for keyboard actions, pointer actions, and moveable integration.
6. Add tests covering widget removal, reorder, multi-select, and group-drag edge cases.

## Practical testing strategy

- Add characterization tests for the current selection behaviors before changing ownership: single click select, additive select, marquee select, delete selected, and widget removal while selected.
- Add tests for primary-selection behavior when multiple widgets are selected and when the selected widget list is normalized after reorder or removal.
- Add regression tests for config replacement so stale widget ids are dropped and a valid selection remains.
- Add at least one integration-style test that exercises pointer-driven selection plus keyboard delete to protect the full user-facing flow.
- Keep manual QA for the trickiest interactions: marquee select, group drag, delete, and selecting after config changes or widget creation.

## Acceptance criteria

- [x] Overlay selection has one clear owning seam rather than local and global state repairing each other with effects
- [x] Primary selection and selected-id list remain consistent after widget add, remove, reorder, and config replacement flows
- [x] Multi-selection and group-drag behavior remain functionally unchanged for users
- [x] Keyboard delete and pointer-driven selection flows still work exactly as before
- [x] Any synchronization refs used only to suppress feedback loops are removed or reduced to truly imperative concerns
- [x] Tests cover the main selection transitions and stale-selection edge cases

## Blocked by

None - can start immediately
