Status: ready-for-agent

# Standard Metric Widget Core and Template V2

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Create the shared foundation for OVRLEY's expanded standard metric widget family. Introduce a metadata-driven standard metric widget layer that covers the existing standard metric value widgets and becomes the path for the new standard metric widgets. This slice should centralize widget labels, icon bindings, placeholder behavior, default display units, supported display units, and formatting capabilities behind a stable contract used by the editor, preview renderer, Rust renderer, and template serialization.

Make `display_unit` the canonical unit field for standard metric value widgets. Remove the legacy standard-metric unit field behavior from the standard metric widget path rather than keeping compatibility aliases. Bump the template file version and explicitly reject older template versions on load so the schema break fails fast and predictably.

Move the shared standard metric widget icon catalog to the canonical shared SVG asset location so both the React preview renderer and the Rust export renderer consume the same source-of-truth assets. The canonical location is `assets/widget-icons/`.

This core slice should establish the explicit planned icon mapping contract:

- extracted Lucide SVG assets:
  `pace` -> `Footprints`
  `air_pressure` -> `Wind`
  `left_right_balance` -> `Scale`
  `stride_length` -> `Ruler`
  `stroke_rate` -> `Waves`
  `vertical_speed` -> `TrendingUp`
  `vertical_ratio` -> `Percent`
  `core_temperature` -> `Thermometer`
- custom shared SVG assets:
  `g_force`
  `ground_contact_time`
  `torque`
  `gear_position`

## Acceptance criteria

- [ ] Existing standard metric value widgets run through a shared metadata-driven standard metric widget contract instead of scattered per-type configuration
- [ ] The shared metadata-driven standard metric widget contract covers the existing standard metric widgets `speed`, `heartrate`, `cadence`, `power`, and `temperature`
- [ ] The metadata-driven layer covers the current standard metric widgets and is ready to host new standard metric widgets without introducing a second system
- [ ] `time` and `gradient` are allowed to remain specialized paths if they do not fit the shared standard metric widget model cleanly
- [ ] `display_unit` is the canonical unit-selection field for standard metric value widgets
- [ ] The old standard-metric unit field behavior is removed from the standard metric widget path
- [ ] Template serialization and normalization for standard metric value widgets use the new schema consistently
- [ ] The template file version is bumped
- [ ] The template files and extracted configs within this repo are patched to the new version and to match the new schema.
- [ ] Loading an older template version fails fast with an explicit rejection path rather than silently loading a mismatched schema
- [ ] Shared standard metric widget SVG assets are sourced from `assets/widget-icons/` and used by both preview and export
- [ ] The shared icon catalog records which planned widgets use extracted Lucide SVG assets and which require custom SVG assets
- [ ] Frontend automated tests cover the metadata-driven standard metric widget contract at the behavior level
- [ ] Backend automated tests cover template version rejection, standard metric config behavior, and the shared formatter contract at the behavior level
- [ ] No lint errors (`pnpm lint`)
- [ ] Relevant frontend tests pass (`cd app && pnpm test`)
- [ ] Relevant Rust tests pass for the touched standard metric/config areas

## Blocked by

None - can start immediately
