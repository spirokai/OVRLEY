Status: ready-for-human

# Core Temperature Parser Extraction and Widget

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Add `core_temperature` as a Wave 2 standard metric widget. Extend activity parsing and activity metric export so the metric can be extracted where supported, then wire it into the shared standard metric widget system end-to-end.

The widget should behave like the rest of the standard metric value family: it appears in the widget drawer even when the loaded activity does not provide the metric, uses the shared editor and render paths, supports the approved temperature-unit behavior, and renders a placeholder when the activity data is missing.

`core_temperature` uses an extracted Lucide `Thermometer` SVG asset as its shared icon.

## Acceptance criteria

- [x] `core_temperature` is extracted into the parsed activity payload where supported
- [x] `core_temperature` is represented in the standard metric widget contract and can be created from the widget drawer
- [x] `core_temperature` uses `°C` as its default display unit and supports the approved temperature-unit behavior through the shared standard metric widget schema
- [x] The widget renders end-to-end in the editor preview and Rust export path
- [x] The widget renders a placeholder when the loaded activity does not provide `core_temperature`
- [x] The widget uses the agreed shared icon/source-of-truth asset rules
- [x] `core_temperature` ships with the extracted Lucide `Thermometer` SVG asset in the shared icon catalog
- [x] Frontend automated tests cover widget creation, unit behavior, placeholder behavior, and standard metric integration for `core_temperature`
- [x] Backend automated tests cover parser extraction, formatter/render behavior, and standard metric integration for `core_temperature`
- [x] No lint errors (`pnpm lint`)
- [x] Relevant frontend tests pass (`cd app && pnpm test`)
- [x] Relevant Rust tests pass for the touched activity/render/widget areas

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
