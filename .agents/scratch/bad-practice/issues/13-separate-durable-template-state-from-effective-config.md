Status: ready-for-agent

# 13 - Separate Durable Template State From Effective Config

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

Template semantics are currently spread across multiple modules that each know a different part of the same representation split:

- durable template state for file save/load
- effective editor config after applying global defaults
- effective render config after adding render-time fields

This is a bad fit because:

- callers must know when `globalDefaults` are merged versus persisted
- render-only scene fields are stripped in one place and reintroduced in another
- the same normalization rules are repeated across template serialization, store hydration, scene settings, and render preparation
- the seam is shallow because the interface complexity nearly matches the implementation complexity

The user-facing behavior should stay the same. Saving, loading, editing, and rendering templates must continue to work the same way, but the representation rules should be concentrated behind one deeper module.

## What to build

Create one template-state module that owns the transformations between:

- durable template state used for file save/load
- effective editor config used during in-app editing
- effective render config used when building a render request

The rest of the app should stop re-encoding these rules directly. Scene settings, store hydration, template file IO, and render preparation should consume that seam rather than each performing their own partial normalization.

## Affected files

- `app/src/lib/config-utils.js`
- `app/src/store/slices/createTemplateSlice.js`
- `app/src/features/template-manager/utils/templateSnapshot.js`
- `app/src/features/render-video/utils/render-video.js`
- `app/src/features/render-video/hooks/useRenderWorkflow.js`
- `app/src/features/scene-settings/hooks/useSceneSettingsState.js`

## Suggested plan

1. Identify the current template representation variants and where each caller assumes its own rules.
2. Define one template-state seam that can materialize durable, editor-effective, and render-effective shapes explicitly.
3. Move global-default application and render-time scene-field handling behind that seam.
4. Update template hydration, scene settings, and render preparation to consume the new seam rather than reproducing the transformation rules.
5. Remove now-redundant normalization logic from callers.
6. Verify that saved templates, editor behavior, and render requests remain functionally equivalent.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level template-state module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper template-state module over several small conversion helpers.
- New files are justified only when they absorb repeated transformation rules from multiple callers.
- Keep durable-state and effective-config concerns pure and explicit, but not fragmented into tiny files.
- Delete or fold old representation helpers into the new seam rather than layering wrappers on top.
- The module graph should become simpler for template semantics after this issue lands.

## Practical testing strategy

- Add direct unit coverage for the new template-state seam: durable-to-editor, durable-to-render, and editor-to-durable transformations.
- Extend `templateSnapshot.test.js` so save/load behavior still normalizes the same durable template payload.
- Add regression coverage around global-default changes and template hydration so editor-effective config still behaves the same after the seam moves.
- Add render-path regression coverage proving render preparation still emits the same effective scene values from the same committed template state.
- Use prior art from `app/src/tests/features/template-manager/templateSnapshot.test.js`, `app/src/tests/features/scene-settings/useSceneSettingsState.test.jsx`, and `app/src/tests/features/render-video/useRenderVideoDialogState.test.jsx`.
- Manually verify save template, load template, change global defaults, and render preview/video from the same template before and after the refactor.

## Acceptance criteria

- [ ] One deeper template-state seam owns durable template normalization and effective config materialization
- [ ] Scene settings, store hydration, template file IO, and render preparation stop re-encoding those representation rules independently
- [ ] Saving and loading templates produce the same durable file behavior as before
- [ ] Editor-visible config and render-visible config remain functionally equivalent for the same template input
- [ ] Tests cover the transformation seam directly plus at least one editor and one render regression path

## Blocked by

None - can start immediately
