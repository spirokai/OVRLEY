Status: ready-for-agent

# Wave 1 Standard Metric Widgets

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Add the full Wave 1 standard metric widget set end-to-end on top of the new shared standard metric widget core:

- `pace`
- `g_force`
- `air_pressure`
- `ground_contact_time`
- `left_right_balance`
- `stride_length`
- `stroke_rate`
- `torque`
- `vertical_speed`

This slice should make the new widget types available in the widget drawer, create them with the correct defaults, expose the agreed editor controls, render them consistently in the React preview and the Rust export path, and serialize them through the new standard metric widget schema.

This slice also includes parser-time normalization of `left_right_balance` into a canonical left-percent scalar series, plus the agreed display-format dropdown for the widget. Wave 1 widgets must always be available in the drawer regardless of activity coverage, and when activity data is missing they must render placeholders rather than disappearing.

Final production-quality icons are required as part of this slice. Lucide-based icons must be extracted into shared SVG assets. Custom icons must fit the existing shared SVG subset and match the Lucide visual language.

Wave 1 icon mapping is fixed as follows:

- extracted Lucide SVG assets:
  `pace` -> `Footprints`
  `air_pressure` -> `Wind`
  `left_right_balance` -> `Scale`
  `stride_length` -> `Ruler`
  `stroke_rate` -> `Waves`
  `vertical_speed` -> `TrendingUp`
- custom shared SVG assets:
  `g_force`
  `ground_contact_time`
  `torque`

## Acceptance criteria

- [ ] The widget drawer exposes all nine Wave 1 standard metric widgets
- [ ] Each Wave 1 widget can be created, edited, previewed, serialized, and exported end-to-end
- [ ] Wave 1 widgets use these default display units and formats:
  `pace` -> `min/km`
  `g_force` -> `g`
  `air_pressure` -> `hPa`
  `ground_contact_time` -> `ms`
  `left_right_balance` -> `52% / 48%`
  `stride_length` -> `m`
  `stroke_rate` -> `spm`
  `torque` -> `Nm`
  `vertical_speed` -> `m/s`
- [ ] `pace` defaults to `m:ss min/km` and supports `min/km` and `min/mi`
- [ ] `g_force` supports `g` and `m/s²`
- [ ] `air_pressure` supports `hPa`, `mbar`, `inHg`, and `mmHg`
- [ ] `ground_contact_time` renders in `ms`
- [ ] `stride_length` supports `m`, `cm`, `ft`, and `in`
- [ ] `stroke_rate` renders in `spm`
- [ ] `torque` renders in `Nm`
- [ ] `vertical_speed` supports `m/s`, `ft/min`, and `m/h`
- [ ] `left_right_balance` is normalized at parse time into a left-percent scalar series
- [ ] `left_right_balance` supports `52 / 48`, `L52 / R48`, `52% / 48%`, and `52L / 48R`, defaulting to `52% / 48%`
- [ ] The `left_right_balance` placeholder uses a format-aware single-string empty form such as `-- / --`
- [ ] The `0 / 1 decimal` control reuses `decimals`
- [ ] The `0 / 1 decimal` control appears for `g_force`, `stride_length`, `torque`, and `vertical_speed`
- [ ] The `0 / 1 decimal` control does not appear for `pace`, `air_pressure`, `ground_contact_time`, `stroke_rate`, or `left_right_balance`
- [ ] Wave 1 widgets always stay available in the drawer even when the loaded activity lacks those metrics
- [ ] Missing metric data renders placeholders while preserving widget layout
- [ ] Wave 1 icons are final, shared, and render consistently in both preview and export
- [ ] All Wave 1 extracted Lucide icons are committed as shared SVG files in `assets/widget-icons/` rather than used only as runtime icon components
- [ ] Wave 1 uses the agreed icon mapping, with extracted Lucide SVG assets for `pace`, `air_pressure`, `left_right_balance`, `stride_length`, `stroke_rate`, and `vertical_speed`
- [ ] Wave 1 includes final custom shared SVG assets for `g_force`, `ground_contact_time`, and `torque`
- [ ] Frontend automated tests cover Wave 1 widget configuration, placeholder behavior, formatting behavior, and preview behavior
- [ ] Backend automated tests cover Wave 1 formatter behavior, parser normalization, render-data requirements, and export rendering behavior
- [ ] No lint errors (`pnpm lint`)
- [ ] Relevant frontend tests pass (`cd app && pnpm test`)
- [ ] Relevant Rust tests pass for the touched activity/render/widget areas

## Blocked by

- `.agents/scratch/metric-widget-expansion/issues/01-standard-metric-widget-core-and-template-v2.md`
