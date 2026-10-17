---
name: add-widget
description: Add OVRLEY widgets, metric display types, or derived metrics from a supplied widget template and reference images.
---

# Add a widget

Implement the supplied widget across the editor, WASM preview, and native export. Rust crate
paths below start under `src-tauri/`; frontend paths under `app/src/`. Confirm owners against
current code before editing.

## 1. Establish the specification

The user supplies a widget template: TRIAGE (`new_metric`, `display_type`, `independent`),
GENERAL INFORMATION, METRIC CONTRACT, CONFIGURATION FIELDS, BEHAVIOR, GESTURES, and CACHING.
Read it in full; extra sections, formulas, notes, and attachments are equally binding.

Classify extraction, derivation, and presentation separately. A widget may need new formulas
using existing data/metrics even when `new_metric` is off and extraction is unchanged. Trace
formulas to inputs, source units, time alignment, interpolation, and missing-data behavior.
Resolve material ambiguities before implementing the affected behavior.

Treat supplied images as **blueprints to recreate as closely as possible**. Inspect geometry,
proportions, spacing, typography, colors, strokes, labels, indicators, layering, and effects.
Apply explicit template changes where they intentionally differ from the image.

**Complete when:** every setting, behavior, formula, gesture, and visual element is accounted
for, and the metric/display/independent-family implementation path is selected.

## 2. Declare manifests and telemetry

Copy the closest current entry; inherit shared defaults:

- **Metric** (`engine_load`): `assets/standard-metrics.json` → `definitions`. Declare snake_case
  `type`, `current: true`, name keys, icon, category, formatter, interpolation, units mode,
  supported units, and `defaults.display_unit`. Per-entry `displayTypes` overrides shared
  `widgetDefaults.displayTypes`; `defaults.display_type` selects the initial display.
- **Display** (`heading_tape`, `lean_angle`): `displayTypes.definitions.<type>` with label/icon,
  `layoutMode`, shared `defaults`, boxed `variantDefaults`, geometry bindings/constraints,
  and modes as needed. Opt affected metrics into it through their `displayTypes` lists.
- **Independent** (`course`, `backdrop`): `assets/standard-widgets.json` → `definitions` with
  identity, labels, icon, category, collection, layout, defaults, and optional fields.
  Declare global bindings and `timeDependent: true` where needed. New collections also need
  document/template and Rust config/render support.

Telemetry widgets declare `activity.sources` (numeric priority) and `activity.requires`
(supporting columns); empty arrays are valid. Priority selects the first observed column.
Reuse existing columns. New widget-owned columns use `activity.column`, named by `type`,
with source `unit` and definition-level `interpolation`. Numeric policies inherit
`activitySeries.numericDefaults`; override idle, smoothing, bounds, and input/finalization
policies as needed. Use `rawSample: false` without raw row input. Supporting columns without
widgets belong in `activitySeries.auxiliary`.

Add availability names to `activitySeries.attributes.core`/`.extended`; use
`activity.coverageSource` for another column's coverage. Modes declare `activity` dependencies.
Add SVGs referenced by `icon.assetFile` to `assets/widget-icons/` using the supported subset
in `ovrley_widgets/src/render/svg.rs`. Current metric icons are compile-required. Add all name,
display/mode, and control translations to `app/src/i18n/locales/*-translation.json`.

**Complete when:** manifests declare every configuration field and telemetry dependency;
referenced icons and translations exist.

## 3. Supply values

For new extraction, update the relevant parser: frontend `lib/activity/{fit,gpx}-parser.js`,
core `activity/{csv,vbo}/`, or `media/mp4_telemetry/`. Emit canonical source units. Implement
new derivation formulas/source precedence in `ovrley_core/src/activity/finalize/metrics.rs`.

Cargo generates `MetricKind`, `ActivitySeries`, numeric fields, and input operations from
the manifests. Generic finalization, ranges, trimming, interpolation, requirements, and preview
sampling consume them. Extend these declarations rather than handwritten registration tables.
Specialized nonnumeric columns require explicit Rust storage/preparation/sampling.

**Complete when:** requested direct/derived values reach sampling with correct units and gap
behavior. A scalar metric using existing formatting/rendering proceeds directly to step 6.

## 4. Implement rendering and dispatch (new displays/families)

Create/extend `ovrley_widgets/src/widgets/<family>/`; register in `widgets/mod.rs`. Follow the
closest family's config, validation, layout/frame, preview cache, and interaction structure.
Share native/WASM drawing; prepare static work once. Validate required config at ingress.

- **Metric display:** extend `value/config.rs` and `value/validation.rs` (`ValidatedValue`,
  `metric_kind()`). Update handwritten `DisplayType` in `model/types.rs`: variant, serde name,
  `as_str`, deserializer. New formatters need shared formatting/validation and editor controls.
- **Preview:** `preview_widget.rs` — request dispatch, `PreviewWidget` construction, pixels,
  geometry, revision, `ResizeUpdate`; `preview.rs` — render/resize and cache invalidation.
  Reuse the existing WASM ABI.
- **Export:** core `normalize/widgets.rs` validation, `render/assets.rs` preparation,
  `render/frame.rs` drawing. Extend config collections/`ValidatedRenderConfig` for new families
  where needed. Consume family/mode dependencies in `normalize/mod.rs` and preview sampling.

**Complete when:** both hosts use shared drawing with complete validation, sampling, resource,
cache, geometry, and resize dispatch for every requested display.

## 5. Wire the editor (new displays/families)

Register display sections in `DISPLAY_SECTION` in `MetricWidgetEditor.jsx` under
`features/widget-editor/components/metricWidget/`; independent editors in `WIDGET_EDITOR_MAP`
in `components/SidebarWidgetsTab.jsx`. Keep components presentational. Persist boxed settings
in `display_variants.<type>` and use `lib/widget/widget-resolver.js` for edits/projection.

Check behavior-specific seams: `previewSession.js` (`widgetFonts`), `overlayEditorUtils.js`
(handles/aspect/rotation), `useWasmWidgetInteractions.js`/`widgetInteraction.js` (gestures),
and `WasmWidgetCanvas.jsx`/`widgetPresentation.js` (layering/pixel placement).

**Complete when:** all controls and specified gestures update the canonical config, survive
display switching/save/reload, and present the correct resources and geometry.

## 6. Verify behavior and fidelity

Test derivations with known inputs/outputs, including gaps and boundaries. Extend relevant
parser, family, and editor tests. `manifest_activity_tests.rs` covers generated numeric columns;
`widget-catalog.test.jsx` covers manifest creation, icons, and persistence. Run from repo root:

```sh
pnpm check
pnpm test
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml
```

Follow repository build authorization rules for `pnpm wasm:build` and `pnpm dev`; bindings
reuse `vendor/skia-wasm`. Verify playback, editing, gestures, save/reload, and native export.
Compare actual output with the blueprint at matching size/state, correct visible differences,
and check preview/export agreement.

**Complete when:** behavior and derivations satisfy the template, available visual comparisons
support close fidelity, and remaining differences/unverified requirements are explicitly reported.
