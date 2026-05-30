Status: ready-for-agent

# 15 - Concentrate The Template Workspace

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

Template lifecycle behavior currently enters the app through several inconsistent paths:

- backend template loading
- file import/export
- community template fetch
- dirty-state derivation and publication into the current workspace

This is a bad fit because:

- not every source crosses the same normalization seam
- community template loading bypasses some of the stricter template-state rules and performs extra cross-slice writes directly
- load/save/import behavior is spread across multiple hooks, so template workspace semantics have weak locality
- the seam is shallow because source-specific orchestration leaks into callers instead of being concentrated

The user-facing behavior should remain the same: users can create, load, import, save, and switch templates without workflow changes.

## What to build

Create one template-workspace module that owns template publication into the current session, regardless of where the template came from.

That module should handle:

- loading from a source adapter
- normalization into the current workspace representation
- refresh of dirty-state / saved-state bookkeeping
- publication into store/editor adapters

Source-specific differences should live in adapters, not in separate orchestration paths.

## Affected files

- `app/src/features/template-manager/hooks/useTemplateManagement.js`
- `app/src/features/template-manager/hooks/useTemplateFetching.js`
- `app/src/features/template-manager/hooks/useTemplateSaveStatus.js`
- `app/src/features/template-manager/hooks/useCommunityTemplate.js`
- `app/src/features/template-manager/utils/templateSnapshot.js`
- `app/src/features/template-manager/utils/templateFileUtils.js`
- `app/src/store/slices/createTemplateSlice.js`

## Suggested plan

1. Identify the current template sources and the points where they diverge in normalization or workspace publication.
2. Define a template-workspace seam that accepts source adapters and publishes a normalized template session consistently.
3. Move dirty-state refresh and loaded-template bookkeeping behind that seam.
4. Refactor backend, file, and community template paths to use the same publication flow.
5. Remove direct imperative cross-slice updates that only exist because the workspace seam is inconsistent.
6. Verify that template switching, import, save, and community-template flows still behave the same.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level template-workspace module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper template-workspace module with source adapters over separate orchestration files per source.
- Do not create a new file for each template source unless that source has a genuinely distinct adapter concern.
- Keep normalization, publication, and dirty-state behavior concentrated rather than scattering it across tiny utilities.
- Community-template behavior should be absorbed into the common workspace seam, not remain as a parallel orchestration path.
- Success means fewer conceptual seams around template lifecycle, not more files.

## Practical testing strategy

- Add direct tests for the template-workspace seam covering backend-source load, file import, and community-source publication.
- Extend dirty-state regression coverage so `Draft`, `Saved`, and `Modified` behavior remains equivalent after the seam moves.
- Add regression tests ensuring community templates cross the same normalization and publication path as other template sources.
- Use prior art from `app/src/tests/features/template-manager/templateSnapshot.test.js` and any existing shell/template-management interaction tests.
- Manually verify create new template, load backend template, import JSON file, load a community template, and save a modified template.

## Acceptance criteria

- [ ] One template-workspace seam owns publication of the current template session regardless of source
- [ ] Backend, file, and community template flows all cross the same normalization and dirty-state update path
- [ ] Community template loading no longer bypasses template workspace rules with ad hoc cross-slice writes
- [ ] Save/load/import/switch behavior remains functionally equivalent from the user perspective
- [ ] Tests cover the workspace seam directly plus regression paths for dirty-state and source switching

## Blocked by

None - can start immediately
