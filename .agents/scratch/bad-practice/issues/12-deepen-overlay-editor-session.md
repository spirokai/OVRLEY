Status: ready-for-agent

# 12 - Deepen the Overlay Editor Session

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

The overlay editor session is currently spread across a wide orchestration hook and several callback bundles. The logic is real, but the interface is shallow because one seam owns too many unrelated concerns at once.

This is a maintenance problem because:

- understanding one interaction often requires chasing behavior across selection, drafts, viewport state, pointer handling, keyboard handling, and moveable wiring
- interactive bugs have weak locality because the editor session is not concentrated behind one clear seam
- callback bundles between hooks and adapters grow large, which increases incidental coupling
- testing tends to target broad React composition instead of smaller behavior seams

The user-facing interaction model should remain the same. The goal is to make the editor session deeper and easier to evolve safely.

## What to build

Deepen the overlay editor session so selection policy, draft ownership, viewport behavior, and gesture lifecycle are concentrated behind a smaller seam. The result should be a clearer editor-session module with narrower React adapters for canvas, pointer handling, keyboard behavior, and moveable integration.

This is a behavior-preserving architecture change. Canvas interactions should continue to feel the same to the user while the implementation gains better locality and a smaller public interface.

## Affected files

- `app/src/features/overlay-editor/hooks/useOverlayEditorState.js`
- `app/src/features/overlay-editor/components/OverlayEditor.jsx`
- `app/src/features/overlay-editor/components/OverlayCanvas.jsx`
- `app/src/features/overlay-editor/hooks/createOverlayMoveableHandlers.js`
- `app/src/features/overlay-editor/utils/createOverlayPointerHandlers.js`
- `app/src/features/overlay-editor/hooks/useWidgetDraftState.js`
- `app/src/features/overlay-editor/hooks/useEditorViewport.js`
- `app/src/features/overlay-editor/hooks/useEditorKeyboard.js`
- `app/src/features/overlay-editor/utils/widgetDomHelpers.js`

## Suggested plan

1. Identify the editor-session responsibilities currently mixed inside `useOverlayEditorState`.
2. Extract pure selection and draft-state helpers where those rules can be expressed without React effects.
3. Introduce a deeper editor-session seam that owns selection policy, transient draft ownership, viewport coordination, and gesture lifecycle.
4. Narrow the React adapters for canvas, pointer handling, keyboard handling, and moveable integration so they delegate into that seam.
5. Preserve the current top-level editor behavior and prop contracts where existing consumers rely on them.
6. Verify equivalent interaction behavior through characterization tests and manual QA on the most interactive paths.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level editor-session module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- This issue must not turn into a proliferation of tiny hooks.
- Prefer one deeper editor-session module and a small number of real adapters over many single-purpose files.
- Only create a new file when it owns a coherent concern with a stable seam, not just to shorten one source file.
- If behavior stays tightly coupled, keep it together for locality rather than splitting by implementation detail.
- The target is pure concerns and clearer seams, not smaller files for their own sake.

## Practical testing strategy

- Add characterization tests around the editor-session behavior before reshaping the seam: single-select, multi-select, marquee selection, delete behavior, and draft commit during interaction end.
- Extract pure selection and draft-state helpers where possible, then add direct unit coverage for those helpers before changing the React composition.
- Extend integration-style editor tests so the composed canvas behavior remains stable after the session seam is deepened.
- Use current tests as prior art: `OverlayEditor.selection.test.jsx`, `WidgetDrawer.test.jsx`, and `WidgetButtonGrid.test.jsx`.
- Manually verify marquee selection, drag, resize, scale, rotate, hover badges, and keyboard deletion after the refactor.

## Acceptance criteria

- [ ] The overlay editor session exposes a smaller seam that owns selection, draft state, viewport, and gesture lifecycle coherently
- [ ] Canvas, pointer, keyboard, and moveable integrations become narrower adapters instead of each owning cross-cutting editor behavior
- [ ] Selection and widget-manipulation behavior remain functionally equivalent from the user perspective
- [ ] Regression tests cover both extracted session logic and composed interaction behavior
- [ ] Manual QA confirms marquee, drag, resize, scale, rotate, and delete flows still behave correctly

## Blocked by

None - can start immediately
