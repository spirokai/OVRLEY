Status: ready-for-agent

# Core Temperature Parser Extraction and Widget

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Add `core_temperature` as a Wave 2 standard metric widget. Extend activity parsing and activity metric export so the metric can be extracted where supported, then wire it into the shared standard metric widget system end-to-end.

The widget should behave like the rest of the standard metric value family: it appears in the widget drawer even when the loaded activity does not provide the metric, uses the shared editor and render paths, supports the approved temperature-unit behavior, and renders a placeholder when the activity data is missing.

## Acceptance criteria

- [ ] `core_temperature` is extracted into the parsed activity payload where supported
- [ ] `core_temperature` is represented in the standard metric widget contract and can be created from the widget drawer
- [ ] The widget renders end-to-end in the editor preview and Rust export path
- [ ] The widget supports the approved temperature unit-selection behavior through the shared standard metric widget schema
- [ ] The widget renders a placeholder when the loaded activity does not provide `core_temperature`
- [ ] The widget uses the agreed shared icon/source-of-truth asset rules
- [ ] Frontend automated tests cover widget creation, unit behavior, placeholder behavior, and standard metric integration for `core_temperature`
- [ ] Backend automated tests cover parser extraction, formatter/render behavior, and standard metric integration for `core_temperature`
- [ ] No lint errors (`pnpm lint`)
- [ ] Relevant frontend tests pass (`cd app && pnpm test`)
- [ ] Relevant Rust tests pass for the touched activity/render/widget areas

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
