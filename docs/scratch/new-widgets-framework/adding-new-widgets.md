# ADDING A NEW WIDGET

Convert a filled `new-widget-template.md` into a widget. The template is the source of truth;
its TRIAGE keys (`new_metric` / `display_type` / `independent`) branch each step below. Skip branches
that don't apply.

| new_metric | display_type | independent | Path |
|---|---|---|---|
| yes | — | no | **A** standard metric (generic editor, value renderer) |
| no | yes | no | **B** display type for metrics |
| yes | yes | no | **A+B** metric with dedicated display type (lean_angle pattern) |
| no | no | yes | **C** independent family |

Shared facts: manifests (`assets/standard-metrics.json`, `assets/standard-widgets.json`) are the
canonical type registry (frontend `widget-catalog.js`, Rust `include_str!`); every widget renders
via WASM (`WasmWidgetCanvas`), ABI untouched; icons are compile-required (`build.rs`); unknown
preview types fail loudly (`preview_widget.rs`).

## Workflow

1. **Manifest entry** (always) — `name` → `type` + `nameKey`/`shortNameKey`, `icon`.
   - new_metric: METRIC CONTRACT → definition fields in `standard-metrics.json`, verbatim
     (`default unit` → `defaultDisplayUnit`, `other units` → `supportedDisplayUnits`,
     `number format` → `formatter`, `between recorded samples` → `interpolation`,
     `unit picker` → `unitsMode`, `category` → `category`).
   - display_type: CONFIGURATION FIELDS → `displayTypes.definitions.<name>.defaults`
     (snake_case keys); register in `displayTypes.overrides` for affected metrics.
2. **Icon + i18n** (always) — `assets/widget-icons/widget-<name>.svg`;
   `widgets.types.<name>.*` (+ `widgets.displayTypes.<name>.*` if display_type) in all 15
   `app/src/i18n/locales/*-translation.json`.
3. **Rust model** —
   - new_metric: `MetricKind` variant in `model/types.rs` (`#[serde(rename)]` = name) +
     kind↔string maps in `model/standard_metrics.rs`.
   - display_type: `DisplayType` variant in `model/types.rs`.
   - independent: nothing (type string matched in `Deserialize`).
4. **Activity data** (new_metric only) — sampling column in `activity_columns!`
   (`ovrley_widgets/src/activity.rs`); `ovrley_core` spread mirroring `engine_load`:
   `activity/schema.rs`, `finalize.rs`/`finalize/metrics.rs`, `metric_ranges.rs`, `trim.rs`,
   `interpolate.rs`, `normalize/mod.rs`, plus parsers as applicable (`fit-parser.js`,
   `gpx-parser.js`, `csv/columns.rs`, `vbo/channels.rs`, `mp4_telemetry/activity.rs`).
   New formatter only if existing kinds don't fit (`standard_metrics.rs` + `widgets/value/format.rs`).
5. **Widget module** (display_type or independent) — `ovrley_widgets/src/widgets/<name>/`:
   `config.rs`, `validation.rs`, `layout.rs`, `frame.rs`, `preview.rs`, `interaction.rs`.
   BEHAVIOR → frame/layout; GESTURES → interaction; CACHING → static/dynamic split.
   Register in `widgets/mod.rs`.
6. **Preview dispatch** (display_type or independent) — `preview_widget.rs`: `WidgetRequest` +
   Deserialize arm, `PreviewWidget` + new/pixels/geometry/revision/inherit_revision arms,
   `ResizeUpdate` variant; resize dispatch in `preview.rs`. display_type routes via the metric
   fallthrough on `display_type`. new_metric alone rides the `value` family — nothing to do.
7. **Export path** (display_type or independent) — `ovrley_core`: `normalize/mod.rs` validation +
   `ValidatedValue`; `render/assets.rs` `PreparedWidget` + static cache; `render/frame.rs` draw
   arm; optional `activity/widget_samples.rs` accessors.
8. **Editor** — display_type: `metricWidget/<Name>DisplaySection.jsx` → `DISPLAY_SECTION`
   (`MetricWidgetEditor.jsx:27`). independent: `<Name>WidgetEditor.jsx` → `WIDGET_EDITOR_MAP`
   (`SidebarWidgetsTab.jsx:34`). new_metric alone: none — generic `MetricWidgetEditor` via `metric: true`.
9. **Frontend seams** (display_type or independent, from GESTURES) — resize handles
   (`useWasmWidgetInteractions.js`), layering/separate raster (`WasmWidgetCanvas.jsx`), fonts
   (`previewSession.js`), rotation allow-list.
10. **Verify** — tests: `metric_kind_serde_tests.rs`, `value_widget_tests.rs` (new_metric);
    `wasm_widget_preview_tests.rs` + family tests (display_type/independent);
    `widget-catalog.test.jsx`; editor dispatch tests. Then `pnpm wasm:build` (user permission),
    `pnpm lint`, `cargo test` (ovrley_core), `npx vitest run` (in `app/`).

Gotchas: `isWasmWidget` in the migrate skill is obsolete; `app/src/components/widgets/icons/` is
unused; never build without user permission.
