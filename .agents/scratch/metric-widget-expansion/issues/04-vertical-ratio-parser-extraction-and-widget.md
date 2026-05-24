Status: completed

# Vertical Oscillation Parser Extraction and Widget

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What was built

Added `vertical_oscillation` as a Wave 2 standard metric widget. `vertical_oscillation` was already extracted in the frontend FIT/GPX parsers and Rust schema but was not registered as a standard metric widget. This issue wired it end-to-end through the shared standard metric widget system.

The widget behaves like the rest of the standard metric value family: it appears in the widget drawer even when the loaded activity does not provide the metric, uses the shared editor and render paths, and renders a placeholder when the activity data is missing.

`vertical_oscillation` uses an extracted Lucide `ArrowUpDown` SVG asset as its shared icon.

## Acceptance criteria

- [x] `vertical_oscillation` is represented in the standard metric widget contract and can be created from the widget drawer
- [x] `vertical_oscillation` uses `mm` as its default display unit, with `cm` conversion support
- [x] The widget renders end-to-end in the editor preview and Rust export path
- [x] The widget renders a placeholder when the loaded activity does not provide `vertical_oscillation`
- [x] The widget uses the agreed standard metric widget schema and shared icon/source-of-truth asset rules
- [x] `vertical_oscillation` ships with the extracted Lucide `ArrowUpDown` SVG asset in the shared icon catalog
- [x] Frontend automated tests cover widget creation, placeholder behavior, and standard metric integration for `vertical_oscillation`
- [x] Backend automated tests cover formatter/render behavior and standard metric integration for `vertical_oscillation`
- [x] No lint errors (`pnpm lint`)
- [x] Relevant frontend tests pass (`cd app && pnpm test`) — 68/68
- [x] Relevant Rust tests pass for the touched activity/render/widget areas — all pass

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
