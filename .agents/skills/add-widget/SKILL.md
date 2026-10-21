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
  Style constants the template keeps out of the editor (palettes, gaps, label weights) also
  live in `variantDefaults`, so the renderer and the preview's font requests read one source.
  A variant key is promoted only when the root has no key of that name; avoid root-key clashes.
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
Specialized nonnumeric columns require explicit Rust storage/preparation/sampling. A new display
of an existing metric samples through an `ActivitySampler` accessor in
`ovrley_widgets/src/activity.rs` (e.g. `heartrate_at`); add one, with range and coverage checks,
when the metric has none.

**Complete when:** requested direct/derived values reach sampling with correct units and gap
behavior. A scalar metric using existing formatting/rendering proceeds directly to step 6.

## 4. Implement rendering and dispatch (new displays/families)

One `ovrley_widgets` family serves both hosts. The editor preview renders it into a tile;
native export (`ovrley_core/src/render/engine.rs` + `atlas.rs`) builds the same family through
`PreviewWidget::from_validated` and draws it onto the frame. There is no separate export drawing
path. Mirror the newest comparable family (`heading_compass`, `heart_zones`).

Create `ovrley_widgets/src/widgets/<family>/`, registered in `widgets/mod.rs`:

- `validation.rs` — `validate_<family>(ValueConfig, index)` checks metric/display, requires every
  field, parses enums, and resolves derived inputs once (e.g. zones from thresholds).
- `layout.rs` — `prepare_<family>_cache(validated, scene, fonts)` applies `scene.scale`, rasterizes
  static parts once (per-state images, shadow sprites), and fixes stable `bounds` and
  `content_bounds`. Keep pure geometry in testable functions. Paths are not shared across export
  workers; store shape parameters and rebuild trivial paths per frame.
- `frame.rs` — `prepare_frame(sample)` → frame state; `draw(canvas, cache)` in scene coordinates.
- `preview.rs` — `Preview<Family>` with `new`, `from_validated`, `frame_key` (equal keys draw
  identical pixels), `layout_bounds`, `draw`, and `render` (skip unchanged samples, `render_tile`,
  serialize `WidgetGeometry`, bump `revision`). Copy the struct from a neighbour.
- `interaction.rs` — `resize_<family>` returning `ResizeUpdate { x, y, display_variants: { <type> } }`.
  Framed displays take a scene-pixel `size` (divide by `scene.scale`); uniform ones a `factor`.
  Keep the gesture's `fixed` corner on `point`.
- `tests.rs` — unit tests for formulas and geometry.

Dispatch for a metric display:

- `model/types.rs` — `DisplayType` variant, serde name, `as_str`, deserializer arm.
- `value/config.rs` — new `ValueConfig` fields; `value/validation.rs` — `ValidatedValue` variant,
  `metric_kind()`, `display_type()`.
- `preview_widget.rs` — `WidgetRequest` variant and deserializer routing by metric + display;
  `PreviewWidget` variant; arms in `new`, `from_validated`, `frame_key` (`FrameKey`), `pixels`,
  `geometry`, `layout_bounds`, `draw`, `bounds`, `revision`; `ResizeUpdate` variant.
- `preview.rs` — `resize_widget` routing (size-based families return before the `factor` match
  and join its `unreachable!` arm) and the playhead `render` loop.
- `preview_instance.rs` — the placement and `pixel_config` arms.
- `ovrley_core/src/normalize/widgets.rs` — the `validate_render_value` arm. Export needs nothing else.

Independent families also extend config collections/`ValidatedRenderConfig`, the
`ValidatedWidget` enum, and the layer list in `render/engine.rs`. Reuse the existing WASM ABI.

**Complete when:** both hosts build the family from one validated config, with complete
validation, sampling, caching, geometry, and resize dispatch for every requested display.

## 5. Wire the editor (new displays/families)

Register display sections in `DISPLAY_SECTION` in `MetricWidgetEditor.jsx` under
`features/widget-editor/components/metricWidget/`; independent editors in `WIDGET_EDITOR_MAP`
in `components/SidebarWidgetsTab.jsx`. Keep components presentational. Persist boxed settings
in `display_variants.<type>` and use `lib/widget/widget-resolver.js` for edits/projection.

Check behavior-specific seams:

- `features/wasm-widgets/utils/previewSession.js` `widgetFonts` — request exactly the fonts and
  weights the family resolves (none when its text is hidden); a missing request fails the preview.
- `features/wasm-widgets/utils/widgetGestureGeometry.js` `widgetResizeGeometry` — list framed
  displays that resize by `size`; the rest send a uniform `factor`.
- `features/overlay-editor/utils/overlayEditorUtils.js` — `maintainAspectRatio` (boxed displays
  are unlocked unless listed), rotation, and edge handles.
- `features/widget-editor/utils/widgetSize.js` — inspector size sliders for uniform displays.
- `useWasmWidgetInteractions.js`/`widgetInteraction.js` (gestures) and
  `WasmWidgetCanvas.jsx`/`widgetPresentation.js` (layering/pixel placement).

Put editor-side transformations (e.g. clamped list edits) in `features/widget-editor/utils/`.

**Complete when:** all controls and specified gestures update the canonical config, survive
display switching/save/reload, and present the correct resources and geometry.

## 6. Verify behavior and fidelity

Test derivations with known inputs/outputs, including gaps and boundaries. Extend relevant
parser, family, and editor tests. `manifest_activity_tests.rs` covers generated numeric columns;
`widget-catalog.test.jsx` covers manifest creation, icons, and persistence. For a new
display or family also extend:

- `ovrley_core/tests/display_type_tests.rs` — serde round trip and layout mode.
- `ovrley_core/tests/wasm_widget_preview_tests.rs` — a `<family>_preview()` fixture (its scene
  scale is 1.7, so content is scaled); a preview-vs-`render_full_frames` pixel test across samples
  with unchanged-sample revision skips and malformed-field ingress errors; a resize test that
  checks the fixed corner. Add the fixture to `every_family_preview()`, give
  `every_family_parsed_activity()` the metric's column, and raise the atlas test's value count
  and layer range.
- `benchmarks/widgets/browser.js` — one case per visual variant.

Run from repo root:

```sh
pnpm check
pnpm test
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml
```

Follow repository build authorization rules for `pnpm wasm:build` and `pnpm dev`; bindings
reuse `vendor/skia-wasm`. Verify playback, editing, gestures, save/reload, and native export.
Compare actual output with the blueprint at matching size/state, correct visible differences,
and check preview/export agreement. Without the app, a throwaway test can write
`widget.pixels()` (premultiplied RGBA) and `geometry()` to the scratchpad for inspection;
delete it afterwards.

With approval, `pnpm benchmark:widgets:build` then `pnpm benchmark:widgets --run --case <case>`
(one case per run) measures preview, WASM, and export cost; results land in
`debug/benchmarks/widgets/`.

**Complete when:** behavior and derivations satisfy the template, available visual comparisons
support close fidelity, and remaining differences/unverified requirements are explicitly reported.
