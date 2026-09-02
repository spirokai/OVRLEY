# Resize and Scale Interaction Refactor Handoff

## Status

The live metric-text scaling jitter was fixed in commit `5c1f1d2` (`fix: unify live scale geometry ownership`). That fix established the ownership rule this refactor must preserve:

- A `scale` interaction layout exclusively owns live canvas geometry.
- Live scaled data remains available to editor controls.
- Intrinsic data and final position are committed atomically when scaling ends.
- Slider drafts still update both the editor and canvas because they do not have a Moveable interaction layout.

Do not reopen that behavior while consolidating the handlers.

## Problem

Resize and scale are separate end-to-end interaction pipelines:

- `app/src/features/overlay-editor/hooks/useResizeHandlers.js`
- `app/src/features/overlay-editor/hooks/useScaleHandlers.js`

Both implement the same lifecycle independently:

1. Capture an interaction origin.
2. Convert Moveable event geometry into widget geometry.
3. Build data and layout drafts.
4. Publish the draft and apply immediate DOM geometry.
5. Rebuild a rounded update at interaction end.
6. Commit, clear the draft, end the interaction, and reset the origin.

The implementations differ in where coordinate conversion, global scale, anchoring, draft resolution, and commit reconstruction happen. This makes ownership rules difficult to verify and allows equivalent behavior to drift.

## Goal

Create one resize interaction hook that owns the complete lifecycle. Keep `onResize` and `onScale` as thin Moveable event adapters that normalize their event payloads into one canonical internal interaction update.

The unified model must retain two explicit geometry strategies:

- **Frame resize:** changes canonical frame dimensions and position while a frame layout owns the live canvas frame.
- **Intrinsic scale:** keeps captured canvas dimensions fixed, previews through a scale transform, publishes live content values to editor controls, and commits intrinsic values at the end.

The strategies are product behavior. They should not remain separate lifecycle implementations.

## Non-Goals

- Do not change visible resize or scale behavior.
- Do not redesign Moveable configuration or observer behavior.
- Do not change slider or global-scale behavior.
- Do not rewrite renderers, preview models, Zustand state, or widget persistence.
- Do not introduce compatibility adapters or a second draft shape.
- Do not fix clamp discontinuities or alter anchor semantics in the structural refactor. Record those as follow-up correctness work so behavior changes remain reviewable.

## Existing Shared Seams

Build on these instead of adding parallel abstractions:

- `widgetInteractionGeometry.js`
  - `captureWidgetLayout()`
  - `buildFrameInteractionLayout()`
  - `buildScaleInteractionLayout()`
  - `getWidgetInteractionPosition()`
- `widgetResizeScaling.js`
  - `captureResizeOrigin()`
  - `buildResizeUpdate()`
  - `buildScaleDraft()`
- `widgetDomHelpers.js`
  - `updateLiveWidgetDraft()`
- `widget-draft.js`
  - `applyWidgetDrafts()`
  - `applyWidgetDraftsForCanvas()`

`applyWidgetDraftsForCanvas()` now treats `layout.mode === 'scale'` as canvas-inert data. Preserve this rule.

## Target Responsibilities

The unified hook should own:

- Interaction start/end bookkeeping.
- Captured immutable origin state.
- Canonical scene-to-widget coordinate conversion.
- Strategy selection for frame resize versus intrinsic scale.
- Draft publication through `updateLiveWidgetDraft()`.
- Commit reconstruction and rounding.
- Draft cleanup and interaction cleanup.

The Moveable adapters should only extract event inputs:

- Resize: `width`, `height`, and `drag.beforeTranslate`.
- Scale: uniform scale and `drag.beforeTranslate`.

Each adapter should pass a normalized update to the shared lifecycle rather than publishing or committing drafts itself.

## Canonical Interaction Result

Prefer one internal result shape produced by either strategy:

```js
{
  data,
  layout,
  position,
}
```

- `data` is the canonical partial widget update exposed to editor controls.
- `layout` is the sole live canvas geometry owner.
- `position` is the canonical logical position intended for commit.

Do not add aliases for resize and scale fields. Strategy-specific event data should be consumed before producing this result.

## Required Behavior

### Frame Resize

- Convert Moveable dimensions from scene coordinates using global scale exactly once.
- Preserve the current minimum frame size.
- Preserve backdrop and active metric variant resolution.
- Publish live frame/content data through the shared draft path.
- Build a `frame` interaction layout from the captured layout.
- Commit rounded frame geometry and scaled content through `buildResizeUpdate()`.

### Intrinsic Scale

- Convert Moveable scale to canonical widget scale exactly once.
- Preserve visual anchoring based on captured visual bounds and translation.
- Publish live `font_size`, `icon_size`, icon offsets, gradient fields, and position to editor drafts.
- Keep those data changes out of the live canvas through the existing `scale` projection rule.
- Build a `scale` interaction layout from the captured layout.
- Commit rounded intrinsic data and final position atomically.

### Cleanup

Both strategies must use one shared end path:

1. Produce the final rounded update.
2. Commit once.
3. Clear the widget draft.
4. End the interaction.
5. Reset `interactionStartRef`.

## Suggested Migration

1. Read `.agents/refactor-guide.md` before editing frontend code.
2. Add characterization tests for current resize and scale start/update/end behavior.
3. Extract pure strategy functions for frame-resize and intrinsic-scale origin/update/commit calculations.
4. Introduce one lifecycle hook that delegates calculations to those strategies.
5. Replace `useResizeHandlers()` and `useScaleHandlers()` wiring in `OverlayEditor.jsx` with the unified hook.
6. Delete the old hooks only after all characterization tests pass unchanged.
7. Run focused editor tests and lint. Do not run a build without user permission.

Keep each step behavior-preserving and reviewable. Do not combine the structural refactor with geometry corrections.

## Tests

Existing relevant tests:

- `app/src/tests/features/overlay-editor/useScaleHandlers.test.js`
- `app/src/tests/features/overlay-editor/widgetResizeScaling.test.js`
- `app/src/tests/features/overlay-editor/OverlayEditor.selection.test.jsx`
- `app/src/tests/features/overlay-editor/OverlayMoveable.test.jsx`
- `app/src/tests/lib/widget-draft.test.js`

Add focused characterization coverage for:

- Resize and scale lifecycle cleanup.
- Non-unit global scale for both strategies.
- Position preservation from every handle direction represented by Moveable translation.
- Text metric font/icon fields during scale.
- Gradient offsets during scale.
- Backdrop resize projection and commit.
- Metric display-variant resize projection and commit.
- Multiple update events using the immutable captured origin rather than accumulated drafts.
- One commit per completed interaction.

## Follow-Up Correctness Work

Handle separately after the behavior-preserving consolidation:

- Clamp-aware effective scale: the live transform can exceed clamped font/icon values near limits and then jump at commit.
- Exact commit anchoring: fixed pixel gaps and minimum text dimensions mean final intrinsic bounds are not always a uniform multiple of starting bounds.

The consolidated strategy boundary should make these corrections local, but they are not acceptance criteria for the refactor itself.

## Acceptance Criteria

- One hook owns resize and scale interaction lifecycle.
- `onResize` and `onScale` are thin event adapters.
- Frame resize and intrinsic scale remain explicit strategies behind one canonical result contract.
- Global-scale conversion and position anchoring each have one owner per strategy.
- Both modes publish drafts through the same function and finish through the same commit/cleanup path.
- No widget-specific canvas projection path is introduced.
- Existing resize, scale, slider, and global-scale behavior remains unchanged.
- Focused tests and ESLint pass.
