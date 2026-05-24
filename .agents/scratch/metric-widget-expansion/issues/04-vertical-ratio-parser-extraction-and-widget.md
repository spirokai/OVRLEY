Status: ready-for-agent

# Vertical Ratio Parser Extraction and Widget

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Add `vertical_ratio` as a Wave 2 standard metric widget. Extend activity parsing and activity metric export so the metric can be extracted where supported, then wire it into the shared standard metric widget system end-to-end.

The widget should behave like the rest of the standard metric value family: it appears in the widget drawer even when the loaded activity does not provide the metric, uses the shared editor and render paths, and renders a placeholder when the activity data is missing.

## Acceptance criteria

- [ ] `vertical_ratio` is extracted into the parsed activity payload where supported
- [ ] `vertical_ratio` is represented in the standard metric widget contract and can be created from the widget drawer
- [ ] The widget renders end-to-end in the editor preview and Rust export path
- [ ] The widget renders a placeholder when the loaded activity does not provide `vertical_ratio`
- [ ] The widget uses the agreed standard metric widget schema and shared icon/source-of-truth asset rules
- [ ] Frontend automated tests cover widget creation, placeholder behavior, and standard metric integration for `vertical_ratio`
- [ ] Backend automated tests cover parser extraction, formatter/render behavior, and standard metric integration for `vertical_ratio`
- [ ] No lint errors (`pnpm lint`)
- [ ] Relevant frontend tests pass (`cd app && pnpm test`)
- [ ] Relevant Rust tests pass for the touched activity/render/widget areas

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
