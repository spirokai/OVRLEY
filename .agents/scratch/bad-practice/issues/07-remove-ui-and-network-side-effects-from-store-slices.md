Status: ready-for-agent

# 07 - Remove UI and Network Side Effects from Store Slices

## Parent

`.agents/scratch/bad-practice/PRD.md`

## Why this issue exists

Some Zustand slice actions currently do much more than state transitions. In particular, template-related store actions perform network requests, coordinate multiple domain mutations, reach into UI objects imperatively, and surface errors through browser UI primitives.

This is a serious architectural problem because:

- callers cannot tell from the store interface that they are triggering fetches and UI side effects
- the state seam is no longer a clean state owner; it is acting as a controller, adapter, and view bridge at the same time
- testing becomes unnecessarily difficult because verifying state behavior also requires accounting for network and UI behavior
- failures become harder to handle consistently because error presentation is buried inside store actions

The user-facing behavior should remain the same within the app session. Template lists should still load, community templates should still load, editor content should still update, and error handling should remain visible to the user. The change is about moving orchestration to a better seam, not changing workflows.

## What to build

Move UI and network side effects out of store slices and into explicit orchestration hooks or modules.

The store should own state transitions and domain updates. Higher-level orchestration seams should own:

- fetching template data
- deciding how to handle failures
- coordinating editor instance updates
- composing multiple store actions into one user-facing workflow

The goal is not to eliminate async behavior from the app. The goal is to ensure the async and UI behavior lives in a seam where callers can see it clearly and tests can target it directly.

## Affected files

- `app/src/store/slices/createTemplateSlice.js`
- `app/src/features/app-shell/hooks/useAppBootstrap.js`
- `app/src/features/template-manager/hooks/useTemplateManagement.js`
- `app/src/hooks/useAppStoreSelectors.js`
- `app/src/api/backend.js`

## Suggested plan

1. Inventory every store action that performs network I/O, browser UI work, or imperative editor manipulation.
2. Separate pure state transitions from orchestration concerns in the template-loading flows.
3. Move template fetching and community-template loading into explicit orchestration hooks or helper modules that call narrower store actions.
4. Replace direct `alert()` and hidden imperative editor updates with explicit error and editor-sync flows at the orchestration layer.
5. Preserve the current user-visible workflow for app bootstrap, template list refresh, and community template selection.
6. Add tests that verify store actions remain state-focused and orchestration modules handle async success/failure paths.

## Practical testing strategy

- Add characterization tests for current template-loading workflows from the user’s perspective before moving orchestration code.
- Add unit tests for narrowed store actions that verify they only perform state transitions and do not trigger fetches or UI work.
- Add orchestration-level tests that mock network success and failure for bootstrap template loading and community template selection.
- Add regression tests that verify editor content still updates correctly after template load without the store directly reaching into the editor.
- Keep manual QA for bootstrap loading, template refresh, community template selection, and visible error handling on failure.

## Acceptance criteria

- [ ] Targeted store actions no longer perform fetches, backend calls, `alert()` calls, or imperative editor manipulation directly
- [ ] Template-related network and UI orchestration lives in explicit hooks or modules outside the store slices
- [ ] App bootstrap still loads templates and codecs successfully
- [ ] Community template selection still updates the editor and related state correctly from a user perspective
- [ ] Error handling remains visible to the user but is no longer buried inside the store layer
- [ ] Tests cover at least one happy path and one failure path for the moved orchestration logic

## Blocked by

None - can start immediately
