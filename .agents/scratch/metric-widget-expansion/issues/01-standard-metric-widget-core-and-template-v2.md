Status: ready-for-agent

# Standard Metric Widget Core and Template V2

## Parent

`.agents/scratch/metric-widget-expansion/PRD.md`

## What to build

Create the shared foundation for OVRLEY's expanded standard metric widget family. Introduce a metadata-driven standard metric widget layer that covers the existing standard metric value widgets and becomes the path for the new standard metric widgets. This slice should centralize widget labels, icon bindings, placeholder behavior, default display units, supported display units, and formatting capabilities behind a stable contract used by the editor, preview renderer, Rust renderer, and template serialization.

Make `display_unit` the canonical unit field for standard metric value widgets. Remove the legacy standard-metric unit field behavior from the standard metric widget path rather than keeping compatibility aliases. Bump the template file version and explicitly reject older template versions on load so the schema break fails fast and predictably.

Move the shared standard metric widget icon catalog to the canonical shared SVG asset location so both the React preview renderer and the Rust export renderer consume the same source-of-truth assets.

## Acceptance criteria

- [ ] Existing standard metric value widgets run through a shared metadata-driven standard metric widget contract instead of scattered per-type configuration
- [ ] The metadata-driven layer covers the current standard metric widgets and is ready to host new standard metric widgets without introducing a second system
- [ ] `display_unit` is the canonical unit-selection field for standard metric value widgets
- [ ] The old standard-metric unit field behavior is removed from the standard metric widget path
- [ ] Template serialization and normalization for standard metric value widgets use the new schema consistently
- [ ] The template file version is bumped
- [ ] Loading an older template version fails fast with an explicit rejection path rather than silently loading a mismatched schema
- [ ] Shared standard metric widget SVG assets are sourced from one canonical shared location used by both preview and export
- [ ] Frontend automated tests cover the metadata-driven standard metric widget contract at the behavior level
- [ ] Backend automated tests cover template version rejection, standard metric config behavior, and the shared formatter contract at the behavior level
- [ ] No lint errors (`pnpm lint`)
- [ ] Relevant frontend tests pass (`cd app && pnpm test`)
- [ ] Relevant Rust tests pass for the touched standard metric/config areas

## Blocked by

None - can start immediately
