Status: ready-for-agent

# 14 - Collapse The Render Job Lifecycle

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

The render job lifecycle is currently spread across several hooks and one imperative utility:

- dialog-local draft settings
- render request building
- render start / optimistic progress state
- progress polling
- completion and error handling
- backend side effects such as opening the finished video

This is a maintenance problem because:

- no single module owns the render job end to end
- `useRenderWorkflow` and `render-video.js` both participate in orchestration, which weakens locality
- polling and completion logic reach through the store imperatively instead of crossing one explicit seam
- tests are pushed toward broad integration behavior rather than a single render-session interface

The user-facing render workflow should remain the same: open dialog, adjust settings, start render, see progress, cancel if needed, and open the output when complete.

## What to build

Create one render-session module that owns the render job lifecycle from draft settings to completion. Store access and backend calls should be adapters behind that seam, not competing orchestration layers.

The result should keep the current UI behavior while making render jobs testable through one explicit interface.

## Affected files

- `app/src/features/render-video/hooks/useRenderWorkflow.js`
- `app/src/features/render-video/hooks/useRenderDialogState.js`
- `app/src/features/render-video/hooks/useRenderVideoDialogState.js`
- `app/src/features/render-video/hooks/useRenderVideoDerivedState.js`
- `app/src/features/render-video/hooks/useRenderVideoEffects.js`
- `app/src/features/render-video/hooks/useRenderProgressPolling.js`
- `app/src/features/render-video/hooks/useRenderCompletion.js`
- `app/src/features/render-video/utils/render-video.js`
- `app/src/store/slices/createMediaSlice.js`

## Suggested plan

1. Trace the current render job lifecycle end to end and identify where orchestration is duplicated or split.
2. Extract a render-session seam that owns draft settings, request construction, start state, polling, completion, cancellation, and failure handling.
3. Move backend IPC calls and store updates behind adapters used by that seam.
4. Keep the render dialog hooks as composition points only where the UI still needs them.
5. Remove duplicated orchestration between `useRenderWorkflow` and `render-video.js`.
6. Verify that render start, progress, cancellation, completion, and preview-frame behavior remain equivalent.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level render-session module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper render-session module over separate micro-files for start, poll, complete, and cancel.
- Store and backend adapters may be separate if they are real seams, but orchestration should stay concentrated.
- Do not create tiny pass-through hooks whose only purpose is moving a few lines into another file.
- Optimize for pure concerns and end-to-end locality, not for minimizing per-file line counts.
- If new files are added, old orchestration layers should shrink or disappear.

## Practical testing strategy

- Add direct tests for the render-session seam covering start, progress update, cancel, completion, and error paths.
- Preserve existing dialog-level regression coverage so render settings still derive and normalize correctly from the UI point of view.
- Add regression tests for polling/completion behavior to ensure render progress still updates and terminal states still resolve correctly.
- Add at least one test that proves successful completion still publishes the finished filename and opens the output through the backend adapter.
- Use prior art from `app/src/tests/features/render-video/useRenderVideoDialogState.test.jsx` and store-level render-progress expectations in existing store tests.
- Manually verify opening the dialog, starting a render, cancelling a render, and completing a render with the expected progress and output-opening behavior.

## Acceptance criteria

- [ ] One render-session seam owns the render job lifecycle end to end
- [ ] Store updates and backend operations are adapters behind that seam rather than separate orchestration layers
- [ ] The render dialog still behaves the same from the user perspective
- [ ] Progress, completion, cancellation, and error handling remain functionally equivalent
- [ ] Tests cover both the render-session seam directly and at least one full UI-triggered regression path

## Blocked by

None - can start immediately
