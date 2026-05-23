# Colored Units Plan

Plan for adding configurable unit color to metric widgets and the gradient widget, with the constraint that value-unit layout must remain pixel-perfect and unchanged.

## Scope

- Metric widgets: `speed`, `temperature`, `heartrate`, `cadence`, `power`
- Gradient widget: only the trailing `%` changes color
- Time widget is not included because it has no unit segment today

## Requirements Mapping

### A. Show a color picker in `UnitsControlRow`

Update [app/src/features/widget-editor/components/widgetEditorSections.jsx](../app/src/features/widget-editor/components/widgetEditorSections.jsx) so `UnitsControlRow` can render:

- the existing units toggle
- the existing optional unit select
- a new unit color picker

Then thread that through:

- [app/src/features/widget-editor/components/MetricWidgetEditor.jsx](../app/src/features/widget-editor/components/MetricWidgetEditor.jsx)
- [app/src/features/widget-editor/components/TemperatureWidgetEditor.jsx](../app/src/features/widget-editor/components/TemperatureWidgetEditor.jsx)
- [app/src/features/widget-editor/components/GradientWidgetEditor.jsx](../app/src/features/widget-editor/components/GradientWidgetEditor.jsx)

Notes:

- Metric widgets already use `UnitsControlRow`, so this is mostly a prop expansion.
- Gradient does not currently use `UnitsControlRow`, so its editor needs to be refactored to include it.
- For gradient, the picker should represent the `%` color only.

### B. Add a new config field for metric widgets with default white

Add `unit_color` with default `#ffffff` in the frontend config/default pipeline:

- [app/src/features/widget-editor/data/widgetDefaults.js](../app/src/features/widget-editor/data/widgetDefaults.js)
- [app/src/features/widget-editor/utils/widgetUtils.js](../app/src/features/widget-editor/utils/widgetUtils.js)
- [app/src/features/template-manager/data/templateConstants.js](../app/src/features/template-manager/data/templateConstants.js)
- [app/src/features/template-manager/utils/templateSnapshot.js](../app/src/features/template-manager/utils/templateSnapshot.js)

Notes:

- `unit_color` should be snake_case to match existing widget config fields.
- No special color normalization code is needed because [app/src/lib/color-utils.js](../app/src/lib/color-utils.js) already normalizes any field ending in `_color`.

### C. Add `unit_color` to existing templates with green values

Update the shipped templates in [templates](../templates):

- [templates/recent-template.json](../templates/recent-template.json)
- [templates/recent-template-1080p.json](../templates/recent-template-1080p.json)

Use a clearly visible green debug value for `unit_color`.

Also update the Rust render baseline fixture so backend tests exercise the new field:

- [src-tauri/ovrley_core/tests/fixtures/config/render-baseline-rich.json](../src-tauri/ovrley_core/tests/fixtures/config/render-baseline-rich.json)

Notes:

- Backend tests load `templates/recent-template.json` in some places, so changing the shipped template also affects test coverage automatically.

### D. Backend render must read and apply unit color

Add `unit_color: Option<String>` to Rust config in:

- [src-tauri/ovrley_core/src/config/mod.rs](../src-tauri/ovrley_core/src/config/mod.rs)

For regular metric widgets, update draw-time color only and keep the current layout math unchanged in:

- [src-tauri/ovrley_core/src/render/widgets/value/layout.rs](../src-tauri/ovrley_core/src/render/widgets/value/layout.rs)

Likely approach:

- clone the existing unit text style
- override only its resolved color from `value.unit_color`
- keep font size, line height, x, y, and measurement flow unchanged

For gradient, update:

- [src-tauri/ovrley_core/src/render/widgets/value/gradient.rs](../src-tauri/ovrley_core/src/render/widgets/value/gradient.rs)
- [src-tauri/ovrley_core/src/render/format.rs](../src-tauri/ovrley_core/src/render/format.rs) if needed

Important backend detail:

- Gradient currently formats and draws one string like `+7.2%`.
- To color only `%`, the renderer must split draw runs while preserving the same combined-string measurement and anchor point.
- The safe approach is to measure/layout against the full string, then draw numeric/sign text in the main color and `%` in `unit_color`.

### E. Preview must read and apply unit color

Regular metric preview already has separate unit rendering, so this is mostly a color-threading change in:

- [app/src/features/widget-preview/components/MetricRenderer.jsx](../app/src/features/widget-preview/components/MetricRenderer.jsx)

Supporting files to review while implementing:

- [app/src/features/widget-preview/utils/metricWidgetPreviewModel.js](../app/src/features/widget-preview/utils/metricWidgetPreviewModel.js)
- [app/src/features/widget-preview/utils/textMeasurement.js](../app/src/features/widget-preview/utils/textMeasurement.js)

Important preview detail:

- Metric layout in `textMeasurement.js` is geometry-only and should not be changed.
- Only the rendered color of the units text should change for regular metric widgets.

For gradient preview:

- `MetricRenderer.jsx` currently renders `%` as part of the same value string.
- [app/src/features/widget-preview/utils/formatUtils.js](../app/src/features/widget-preview/utils/formatUtils.js) currently returns the numeric/sign portion and `MetricRenderer.jsx` appends `%`.
- To color only `%`, preview should keep the current full-string centering behavior but render the number/sign and `%` as separate text runs.

## Pixel-Perfect Constraint

Acceptance requires that value-unit position, size, and layout do not change.

To preserve parity:

- do not change unit font size rules
- do not change unit gap rules
- do not change baseline calculations
- do not change icon placement logic
- do not change visual bounds calculations unless required for split text rendering
- for gradient, always measure the combined displayed string before splitting draw runs

This means the change should be draw-time styling only for regular metric widgets, and split-draw with unchanged measurement for gradient.

## Codebase Review Findings

### 1. Regular metric widgets are already structurally ready

The following path already models units separately:

- preview layout and bounds in [app/src/features/widget-preview/utils/textMeasurement.js](../app/src/features/widget-preview/utils/textMeasurement.js)
- preview draw in [app/src/features/widget-preview/components/MetricRenderer.jsx](../app/src/features/widget-preview/components/MetricRenderer.jsx)
- backend formatting in [src-tauri/ovrley_core/src/render/format.rs](../src-tauri/ovrley_core/src/render/format.rs)
- backend draw/layout in [src-tauri/ovrley_core/src/render/widgets/value/layout.rs](../src-tauri/ovrley_core/src/render/widgets/value/layout.rs)

That means regular metric widgets should only need style propagation, not geometry changes.

### 2. Gradient is the real implementation risk

Gradient does not currently have a separate unit segment in either layer:

- preview builds one visible string by appending `%` in [app/src/features/widget-preview/components/MetricRenderer.jsx](../app/src/features/widget-preview/components/MetricRenderer.jsx)
- backend formats one string with `%` in [src-tauri/ovrley_core/src/render/format.rs](../src-tauri/ovrley_core/src/render/format.rs)
- backend draws that string in one call in [src-tauri/ovrley_core/src/render/widgets/value/gradient.rs](../src-tauri/ovrley_core/src/render/widgets/value/gradient.rs)

So gradient needs a special split-render implementation, not just a new color prop.

### 3. Template serialization needs explicit key additions

If `unit_color` is not added to `VALUE_TYPE_KEYS`, it will be lost during normalization/export even if the editor writes it in memory.

Relevant files:

- [app/src/features/template-manager/data/templateConstants.js](../app/src/features/template-manager/data/templateConstants.js)
- [app/src/features/template-manager/utils/templateSnapshot.js](../app/src/features/template-manager/utils/templateSnapshot.js)

### 4. Default creation path must include the new field

If `unit_color` is only added to templates but not widget defaults, newly created widgets will behave inconsistently.

Relevant files:

- [app/src/features/widget-editor/data/widgetDefaults.js](../app/src/features/widget-editor/data/widgetDefaults.js)
- [app/src/features/widget-editor/utils/widgetUtils.js](../app/src/features/widget-editor/utils/widgetUtils.js)

### 5. Rust tests will need constructor updates

`ValueConfig` is manually instantiated in tests, so adding a field will require updating those test structs.

Known places:

- [src-tauri/ovrley_core/tests/format_tests.rs](../src-tauri/ovrley_core/tests/format_tests.rs)

## Implementation Checklist

1. Add `unit_color` defaults and serialization keys on the frontend.
2. Extend `UnitsControlRow` to show a unit color picker.
3. Wire `unit_color` into metric, temperature, and gradient widget editors.
4. Update shipped templates and Rust baseline fixture with green `unit_color`.
5. Apply `unit_color` in preview for regular metric widgets.
6. Implement gradient preview split-render so only `%` changes color.
7. Add `unit_color` to Rust `ValueConfig`.
8. Apply `unit_color` in backend metric widget rendering.
9. Implement backend gradient split-render so only `%` changes color.
10. Update or add tests to verify config parsing and rendering behavior.

## Final Assumptions

- `unit_color` applies to all metric widgets that display units.
- For gradient, `unit_color` applies only to `%`.
- `time` remains unchanged.
- The acceptance bar is parity of geometry, not parity of shared-color behavior.
