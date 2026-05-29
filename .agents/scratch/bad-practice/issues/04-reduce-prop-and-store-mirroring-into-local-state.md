Status: ready-for-agent

# 04 - Reduce Prop and Store Mirroring Into Local State

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

Several hooks initialize local state from props or store values and then use `useEffect` to push the local copy back into sync whenever the upstream value changes. Examples include selector modes, derived form states, and temporary display modes.

This is sometimes necessary for user-editable draft inputs, but it is currently overused for values that are mostly derivable. The cost is:

- more sources of truth than the behavior requires
- more synchronization effects than the feature actually needs
- extra stale-state and edge-case risk when upstream values change during editing

The goal is not to remove all local state. Draft input state that exists for a good reason should remain. The goal is to keep only the local state that represents true UI-only intent, while moving pure derivations back to derived values.

## What to build

Audit the current prop/store mirroring patterns and simplify the affected hooks so each local state field has a clear reason to exist.

The target behavior should preserve the current user experience:

- settings controls still show the same values
- render dialog still chooses the same codecs and FPS behavior
- offset inputs and draft fields still behave the same while the user is editing

Use local draft state only where the user can temporarily diverge from committed state. Use derived values where the UI does not need an independent draft lifecycle.

## Affected files

- `app/src/features/scene-settings/hooks/useSceneSettingsState.js`
- `app/src/features/render-video/hooks/useRenderVideoDialogState.js`
- `app/src/features/render-video/hooks/useRenderVideoEffects.js`
- `app/src/features/render-video/hooks/useRenderDialogState.js`
- `app/src/components/ui/blur-input.jsx`

## Suggested plan

1. Classify each mirrored local state field as one of:
   - pure derivation
   - user-editable draft
   - transient UI mode
2. Remove local state for pure derivations and compute those values directly from props/store.
3. For true drafts, encapsulate the draft lifecycle so synchronization behavior is deliberate rather than spread across generic effects.
4. Simplify effect dependencies after removing redundant mirrored state.
5. Verify that form controls still preserve in-progress editing behavior where required.
6. Add tests for external updates arriving while dialogs or settings panels are open.

## Practical testing strategy

- Add characterization tests for each targeted hook or component that currently mirrors state, focusing on externally visible behavior rather than hook internals.
- Add tests that verify true draft fields keep user-entered intermediate values until blur/commit where that behavior is expected.
- Add tests that simulate upstream prop/store changes while the relevant panel or dialog is open and assert the correct fields update or stay draft-local.
- Add regression tests for render dialog FPS and codec behavior and for scene settings controls that currently derive from config/store values.
- Keep manual QA for a few input-heavy flows: editing scene settings, adjusting FPS/update rate, and using dialog controls while upstream state changes.

## Acceptance criteria

- [ ] Local state that only mirrors props or store values without a real draft lifecycle is removed from the targeted hooks
- [ ] Draft input behavior that users rely on is preserved
- [ ] Synchronization `useEffect` usage is reduced in the targeted hooks because fewer values need repair
- [ ] Settings and render dialogs still display the same committed values and make the same updates as before
- [ ] External state changes while a panel or dialog is open behave predictably and are covered by tests

## Blocked by

None - can start immediately
