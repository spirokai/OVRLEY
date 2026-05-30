Status: ready-for-agent

# 09 - Extract Preview Domain From Overlay Editor

## Parent

`.agents/scratch/bad-practice/PRD2.md`

## Why this issue exists

The `overlay-editor` module currently acts as a mixed seam. It exports editor-session behavior, but it also exports pure preview-domain behavior that `widget-preview`, `player`, and `video-preview` consume directly.

This is a bad fit because:

- unrelated features now depend on `overlay-editor` for interpolation, export-window math, and preview assets
- the `overlay-editor` interface is shallow because callers must know about behavior that does not belong to the editor session
- preview-domain changes risk creating editor coupling even when no editor behavior changed
- tests and future refactors have weaker locality because pure preview behavior is not concentrated behind one seam

The user-facing preview behavior should stay the same. The improvement is architectural: preview-domain behavior should live behind its own deeper module.

## What to build

Create a dedicated preview-domain module that owns the pure behavior currently leaking through `overlay-editor`: interpolation, export-window scoping, effective preview FPS calculation, preview constants, and preview assets.

The goal is to make `overlay-editor` a caller again instead of a mixed seam that exports both editor-session behavior and preview-domain behavior. `widget-preview`, `player`, and `video-preview` should depend on the new preview-domain seam directly, while user-visible preview behavior remains unchanged.

## Affected files

- `app/src/features/overlay-editor/index.js`
- `app/src/features/overlay-editor/utils/overlayEditorUtils.js`
- `app/src/features/overlay-editor/utils/exportRange.js`
- `app/src/features/overlay-editor/data/overlayEditorConfig.js`
- `app/src/features/overlay-editor/data/metricWidgetAssets.js`
- `app/src/features/widget-preview/components/RouteRenderer.jsx`
- `app/src/features/widget-preview/components/ElevationRenderer.jsx`
- `app/src/features/widget-preview/components/MetricRenderer.jsx`
- `app/src/features/widget-preview/utils/metricWidgetPreviewModel.js`
- `app/src/features/widget-preview/utils/formatUtils.js`
- `app/src/features/player/hooks/usePlaybackEngine.js`
- `app/src/features/video-preview/hooks/useVideoPlaybackClock.js`

## Suggested plan

1. Identify every pure preview-domain export and cross-feature import that currently goes through `overlay-editor`.
2. Introduce a dedicated preview-domain module or feature seam with a focused public interface.
3. Move interpolation, export-window helpers, preview FPS logic, and preview assets behind that seam.
4. Update `widget-preview`, `player`, and `video-preview` to depend on the new seam directly.
5. Reduce the `overlay-editor` barrel so it only exposes editor-session behavior and editor-owned data.
6. Verify that preview rendering and playback behavior remain unchanged through regression tests and manual QA.

## Module-shape guardrails

- Pure in concern is the goal here; tiny files are not.
- Prefer at most one new top-level preview-domain module unless another new file is a real adapter or replaces an existing shallow module.
- Strongly prefer absorbing or reshuffling logic from existing files before creating new ones, as long as pure concerns stay intact.
- Prefer one deeper preview-domain module over several tiny helper files.
- Do not extract a new file unless it replaces logic used by multiple callers or allows deleting an existing shallow module.
- Bias toward pure concerns and explicit seams, not file-count growth.
- If a helper would only wrap one existing function or one existing import path, keep it in the deeper module instead of creating a new file.
- If new files are introduced, remove or absorb the old leakage point so the net module graph gets simpler.

## Practical testing strategy

- Add or extend pure tests around interpolation, export-window range resolution, rebased progress, and effective preview FPS so the new preview-domain seam is verified directly.
- Extend widget-preview regression coverage so route, elevation, and metric preview behavior remains unchanged after the import path moves.
- Keep one integration-style playback regression that proves `player` and `video-preview` still consume the same preview-domain behavior at runtime.
- Use existing tests as prior art: `playerTimeline.test.js`, `metricWidgetPreviewModel.test.js`, `wave1Formatting.test.js`, and `useVideoPlaybackClock.test.jsx`.
- Manually verify route/elevation widgets, metric interpolation, and playback timing in the editor after the extraction.

## Acceptance criteria

- [ ] A dedicated preview-domain seam owns interpolation, export-window math, preview FPS, and preview assets that are currently exported from `overlay-editor`
- [ ] `overlay-editor` stops re-exporting pure preview behavior that belongs outside the editor session
- [ ] `widget-preview`, `player`, and `video-preview` consume the new seam without user-visible behavior changes
- [ ] Regression tests cover the extracted pure behavior and at least one composed runtime path
- [ ] Route, elevation, metric, and timeline preview behavior remain functionally equivalent

## Blocked by

None - can start immediately
