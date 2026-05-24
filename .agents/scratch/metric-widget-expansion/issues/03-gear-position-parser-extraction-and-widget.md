Status: ready-for-human

# Gear Position Parser Extraction and Widget

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Add `gear_position` as a Wave 2 standard metric widget. Extend activity parsing and activity metric export so the metric can be extracted where supported, then wire it into the shared standard metric widget system end-to-end.

The widget should behave like the rest of the standard metric value family: it appears in the widget drawer even when the loaded activity does not provide the metric, uses the shared editor and render paths, and renders a placeholder when the activity data is missing.

`gear_position` requires a final custom shared SVG asset rather than an extracted Lucide asset.

## Acceptance criteria

- [x] `gear_position` is extracted into the parsed activity payload where supported
- [x] `gear_position` is represented in the standard metric widget contract and can be created from the widget drawer
- [x] `gear_position` uses the agreed default display behavior as a unitless standard metric widget
- [x] The widget renders end-to-end in the editor preview and Rust export path
- [x] The widget renders a placeholder when the loaded activity does not provide `gear_position`
- [x] The widget uses the agreed standard metric widget schema and shared icon/source-of-truth asset rules
- [x] `gear_position` ships with a final custom shared SVG asset that fits the Lucide visual language and current backend SVG subset
- [x] Frontend automated tests cover widget creation, placeholder behavior, and standard metric integration for `gear_position`
- [x] Backend automated tests cover parser extraction, formatter/render behavior, and standard metric integration for `gear_position`
- [x] No lint errors (`pnpm lint`)
- [x] Relevant frontend tests pass (`cd app && pnpm test`)
- [x] Relevant Rust tests pass for the touched activity/render/widget areas

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
