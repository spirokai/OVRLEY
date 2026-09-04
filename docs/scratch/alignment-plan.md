# Metric/value widget alignment implementation plan

## Goal

Add configurable horizontal alignment to intrinsic metric/value widgets so users can choose `left`, `center`, or `right` for the complete icon/value/unit row.

The implementation must preserve these invariants:

- Widget `x` is the horizontal alignment anchor: left edge for left alignment, center for center alignment, and right edge for right alignment.
- Widget `y` retains the existing vertical origin semantics.
- Changing alignment does not rewrite `x`; content moves around the existing anchor.
- JSX preview and Rust rendering use the same layout inputs, constants, and alignment formulas.
- Existing templates render as they do today by defaulting to left alignment.
- Left-aligned icons remain eligible for the static layer.
- Center-aligned rows contribute no static metric parts.
- Right-aligned units remain eligible for the static layer because their right edge is fixed.
- Required configuration is validated once at ingress and consumed without downstream fallbacks.

This plan applies only to intrinsic `text` metric presentations, including standard metrics, time, and GPS coordinates. It does not change gradient, lap-timer, or boxed display types such as linear, arc, corner, heading tape, lean angle, or G-force.

## Chosen layout contract

### Alignment uses a point anchor

Intrinsic metric widgets remain auto-width. Alignment changes which horizontal point of the current row is attached to `x`; it does not introduce a fixed or maximum-width frame.

Persist only the alignment choice:

```json
{
  "content_alignment": "left"
}
```

- `content_alignment` is the canonical alignment field and accepts exactly `left`, `center`, or `right`.
- `content_alignment` is required after frontend normalization.
- No alignment width is persisted or prepared.

## Canonical horizontal geometry

Both frontend and Rust must implement the following model.

Let:

```text
W = natural width of the complete row
p = an element's natural x position within the row
```

Resolve the row origin and element position as:

```text
left:   row_origin_x = x
center: row_origin_x = x - W / 2
right:  row_origin_x = x - W

absolute_element_x = row_origin_x + p
```

For a standard icon/value/unit row:

```text
I = icon slot width, including the icon-to-value gaps; zero without an icon
V = measured value advance
G = value/unit gap; zero without a visible unit
U = measured unit advance; zero without a visible unit

W = I + V + G + U

natural icon x = 0
natural value x = I
natural unit x = I + V + G
```

For right alignment, substitution gives:

```text
unit x = x - U
```

The unit position is therefore independent of the changing value width and icon slot, which makes it static-layer eligible.

Alignment changes only horizontal geometry. Existing line heights, baselines, icon vertical centering, `icon_offset_y`, colors, shadows, borders, and opacity remain unchanged.

### Measurement behavior

The browser Canvas and Skia independently measure value and unit advances. Icon size, icon offsets, and all gaps are deterministic. Both renderers must use the same formulas and constants, but their measured advances may differ by a small subpixel amount.

## Phase 1: Configuration and normalization

### Frontend manifest and defaults

Update `assets/standard-metrics.json` under the `text` display definition:

- Add `content_alignment: "left"` to text defaults.
- Add alignment metadata/options only if the existing manifest is already the canonical owner for editor option metadata; otherwise keep the small option list beside the editor control.

Verify the derived flow through:

- `app/src/lib/widget/standard-widgets.js`
- `app/src/lib/template/template-constants.js`
- `app/src/lib/template/template-normalization.js`
- `app/src/features/widget-editor/utils/widgetUtils.js`
- `app/src/lib/widget/widget-resolver.js`

Specific normalization requirements:

- Old templates receive explicit `content_alignment: "left"`.
- Invalid alignment strings are rejected, not coerced.
- No derived alignment width is serialized into templates or display variants.

### Rust raw and validated contracts

Update `src-tauri/ovrley_core/src/normalize/raw/mod.rs`:

- Add raw `content_alignment` to `ValueConfig`.
- Keep the raw field optional so validation can distinguish missing required input from a valid value.

Define the typed Rust enum in `src-tauri/ovrley_core/src/normalize/value.rs` beside `ValidatedValueWidget`, then re-export it from `src-tauri/ovrley_core/src/normalize/mod.rs`:

```rust
pub enum ContentAlignment {
    Left,
    Center,
    Right,
}
```

Use canonical serde spellings `left`, `center`, and `right`. Do not carry strings into the renderer.

Update `src-tauri/ovrley_core/src/normalize/value.rs`:

- Require and validate `content_alignment` for text metric widgets.
- Store the typed alignment in `ValidatedValueWidget`.
- Do not default missing alignment in Rust; the frontend owns materialized defaults.

Update the separate time constructor in `src-tauri/ovrley_core/src/normalize/time.rs` to apply the same alignment validation and populate the shared `ValidatedValueWidget` fields. Prefer extracting one small alignment validator rather than duplicating the contract.

Arc inner values reuse `ValidatedValueWidget`, but alignment is not part of their boxed inner-value layout. Ensure `validate_arc_inner_value_widget()` either receives a deliberate boxed-only alignment value internally or moves alignment into a text-only validated substructure. Do not require irrelevant text-frame fields from arc configuration merely because the current struct is shared.

## Phase 2: Shared frontend layout

### Standard metric/time rows

Update `getMetricWidgetLayout()` in `app/src/features/widget-preview/shared/textMeasurement.js`:

- Accept `contentAlignment`.
- Continue computing the existing natural icon, value, and unit layout first.
- Compute the current natural row width `W`.
- Resolve the row origin relative to the horizontal anchor as `0`, `-W / 2`, or `-W`.
- Add that origin to the icon, value, and unit horizontal positions.
- Keep `width` as the current natural row width.

Do not apply alignment later in JSX. The layout utility must own it so preview rendering, visual bounds, selection geometry, and tests all consume one result.

### GPS coordinate rows

Update `buildCoordinateLayout()` and its positioning helpers in `app/src/features/widget-preview/widgets/metric/model.js`:

- Compute the existing natural coordinate-row width first.
- Apply the same point-anchor origin to the icon and both coordinate lines.
- Preserve the existing right alignment inside the coordinate numeric column; this is separate from alignment of the complete widget row.
- GPS coordinates have no normal unit run, so center/right coordinate widgets have no static text part.

### Visual and interaction bounds

Update `getMetricWidgetVisualBounds()` and `getCoordinateVisualBounds()`:

- Return current visual bounds relative to the alignment anchor, including negative center/right origins and `icon_offset_x` overflow.
- Position the widget DOM node at `x + minX`; translate its internal SVG content by `-minX`, as the existing bounds-offset model already does for icon overflow.
- Allow selection bounds to follow the current content width. The stored anchor remains stable even though the visible left edge and selection box move as the value changes.
- Explicitly accept that scrubbing a selected center/right widget may move or resize its selection box. This is intentional point-text behavior; do not stabilize the box with a maximum-width frame.

Review and adjust:

- `app/src/features/overlay-editor/utils/overlayEditorHelpers.js`
- `app/src/features/overlay-editor/utils/widgetRenderGeometry.js`
- `app/src/features/overlay-editor/utils/widgetInteractionGeometry.js`
- `app/src/features/overlay-editor/utils/widgetResizeScaling.js`
- `app/src/features/overlay-editor/hooks/useScaleHandlers.js`
- drag handlers that distinguish stored intrinsic coordinates from visual DOM bounds

Dragging must update the stored anchor by the drag delta rather than committing the DOM element's visual left.

Scaling requires an explicit anchor-aware path:

- `buildScaleDraft()` continues scaling font size, icon size, and icon offsets only; it must not derive position from visual bounds.
- Audit the `renderedMinX * (1 - scale)` corrections in `useScaleHandlers.js`; for center/right point anchors they would move the stored anchor as content width changes.
- With no drag translation, live and committed `x` must remain `origin.x`.
- With drag translation, live and committed `x` must be `origin.x + anchorTranslationX`.
- Ensure the transient CSS transform/Moveable translation uses the anchor's local position even though the widget wrapper remains located at the current visual top-left.

### Presentation layer

`app/src/features/widget-preview/widgets/metric/useMetricPreview.js` and `MetricPreview.jsx` should remain presentational:

- Consume the already-aligned positions from the preview model.
- Do not branch on alignment in JSX.
- Render the current visual bounds with SVG overflow visible.

## Phase 3: Text editor UX

Update `app/src/features/widget-editor/components/metricWidget/TextDisplaySection.jsx`:

- Install the shadcn `toggle-group` component and add its generated UI files/dependency changes to the repository.
- Add a controlled, single-selection `ToggleGroup` rendered from a three-item option array.
- Use Lucide `AlignLeft`, `AlignCenter`, and `AlignRight` icons in the three `ToggleGroupItem`s.
- Provide an accessible label for the group and an `aria-label`/tooltip for each icon-only item.
- Prevent the single-selection control from committing an empty value when the active item is clicked; one of the three canonical alignments must always remain selected.
- Commit only `{ content_alignment: value }`.
- Do not compensate `x` when alignment changes. The row moves around the existing horizontal anchor.
- Present the metric widget's x coordinate as “Horizontal Anchor” or add equivalent help text so it is not described as the visual left edge for center/right alignment.

No alignment-specific change is needed in `resetCurrentDisplayConfig()`. Its existing `...TEXT_DEFAULTS` merge restores `content_alignment: "left"` once that default is added to the manifest.

## Phase 4: Rust layout refactor and mirrored formulas

Refactor `src-tauri/ovrley_core/src/render/widgets/value/layout.rs` so static and dynamic rendering share one horizontal point-anchor calculation.

Introduce internal layout structures along these lines:

```rust
struct MetricHorizontalLayout {
    content_width: f32,
    row_origin_x: f32,
    icon_x: Option<f32>,
    value_x: f32,
    unit_x: Option<f32>,
}

struct StaticMetricParts {
    icon: bool,
    unit: bool,
}
```

The concrete names can follow local conventions, but responsibilities should remain explicit.

For standard rows:

- Measure value and unit text once.
- Calculate the icon slot and natural content width using existing constants.
- Apply the same point-anchor origin formulas as the frontend.
- Use the resulting positions for icon, value, and unit drawing.
- Apply `icon_offset_x` after alignment to the icon only.

For coordinate rows:

- Reuse the same point-anchor helper after calculating coordinate content width.
- Apply the row origin to the icon and the coordinate text group.
- Preserve existing direction/value column alignment.

Avoid duplicating alignment formulas between standard rows, coordinates, and static-layer drawing. A pure helper taking `ContentAlignment`, anchor x, and current content width should be the single Rust owner.

## Phase 5: Static-layer generalization

Rename the icon-specific static-layer concepts in:

- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs`
- `src-tauri/ovrley_core/src/render/static_layer.rs`
- `src-tauri/ovrley_core/src/render/mod.rs`

Rename/generalize these functions:

- `has_static_metric_icon_validated`
- `draw_static_metric_icon_for_value_validated`
- `config_has_static_metric_icons`

Separately replace the `static_icon_rendered: bool` request/layout parameter with per-widget static-part state that can represent icon and unit independently.

Use this eligibility matrix:

| Alignment | Icon                              | Unit                              |
| --------- | --------------------------------- | --------------------------------- |
| Left      | Static when visible and supported | Dynamic                           |
| Center    | Dynamic                           | Dynamic                           |
| Right     | Dynamic                           | Static when visible and non-empty |

Additional rules:

- GPS coordinates never cache a unit because they use coordinate direction/value runs rather than the standard unit run.
- A left-aligned time widget still caches its icon when the icon is visible and supported; right/center time widgets cache nothing because time has no unit.
- A value-only widget with no icon and no unit caches nothing in every alignment. A fixed right edge does not make changing value pixels static.
- Widgets without a supported icon do not claim a static icon.
- A zero-size icon does not contribute static pixels.
- Static eligibility must be computed per widget. A global “some metric part was cached” flag may decide whether a base layer exists, but it must not cause dynamic parts in other widgets to be skipped.

### Static unit rendering

Use the same canonical unit-label resolver as dynamic formatting (`standard_metric_unit_label`) and the same unit font sizing, color, opacity, baseline, shadow, and border behavior.

For a right-aligned standard row:

```text
unit_x = widget.x - measured_unit_width
```

The unit baseline remains independent of the changing value because value vertical metrics already use a stable reference and unit font/line height are configuration-derived.

The dynamic pass must receive the per-widget `StaticMetricParts` and skip only the cached unit or icon. It must still measure the unit when needed to position dynamic value/icon content, even if the unit pixels themselves were restored from the base layer.

Update static-layer early-return checks and comments from “labels/icons” to “labels/static metric parts.” Once `ValidatedValueWidget` carries alignment, the existing prepared-value `Debug` representation used by the cache key will include it automatically; alignment is not part of the current key before this change.

## Phase 6: Render-config and persistence audit

Confirm `app/src/features/render-video/utils/renderConfig.js` forwards `content_alignment` unchanged for text values while continuing to strip only display variants and lean-angle-only dimensions.

Audit all bundled templates:

- Keep load-time normalization so older external templates receive `content_alignment: "left"`.
- Update all 12 bundled templates on disk with explicit `content_alignment: "left"` to preserve their materialized-default convention.
- Ensure saving a template writes the canonical fields and does not introduce aliases such as `align`, `text_align`, or `horizontal_alignment`.

Audit any Rust JSON fixtures that bypass frontend normalization. Since the backend contract is strict, those fixtures must include `content_alignment`.

## Phase 7: Tests

Keep the automated coverage focused on contracts and geometry that are easy to regress.

### Frontend tests

Add one table-driven test for the pure alignment helper using fixed widths:

```text
x = 300, W = 120
left origin   = 300
center origin = 240
right origin  = 180
```

Add only these focused regressions beyond the helper:

- In `metricWidgetPreviewModel.test.js`, verify that a right-aligned unit keeps the same x position for two different value widths, while center alignment applies the expected row origin.
- Add one coordinate-layout assertion because GPS coordinates use a separate layout path.
- Verify an old widget normalizes to left.
- Verify a time widget normalizes to left and applies center/right point-anchor layout through the standard intrinsic path.
- Verify the single-selection Toggle Group commits only the selected canonical alignment and cannot clear the selection.
- Verify changing alignment keeps `x` unchanged and moves the current row around it.
- Verify dragging updates the stored anchor by the drag delta rather than committing the current visual left edge.
- Verify scaling a center/right widget leaves `x` unchanged without drag translation and applies only the anchor translation when dragged.

### Rust validation tests

Use a compact table-driven validation test covering:

- Valid left, center, and right alignment.
- Missing or unknown alignment.
- Time receiving the same text-alignment contract without imposing it on boxed inner values.

Update direct `ValidatedValueWidget` constructors in unit tests to include the new typed fields or replace repetitive literals with a test helper that produces a complete valid contract.

### Rust layout and caching tests

Add one table-driven pure-layout test for left/center/right using synthetic measured widths, one cache-eligibility test covering the three-row matrix plus left-aligned time and value-only widgets, and one mixed-widget regression ensuring a cached part on one widget does not suppress another widget's dynamic parts.

## Phase 8: Verification sequence

Run focused frontend tests first from `app/`, then the complete frontend suite:

```text
npx vitest run src/tests/features/widget-preview/metricWidgetPreviewModel.test.js
npx vitest run src/tests/features/widget-editor
npx vitest run
```

Run frontend lint from the repository root:

```text
pnpm lint
```

Run focused Rust tests and then the complete core suite:

```text
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml value_widget
cargo test --manifest-path src-tauri/ovrley_core/Cargo.toml
```

Do not run a production or Tauri build without explicit user permission.

Perform a manual editor/render check with one speed widget containing an icon and unit:

1. Place the widget at a known `x`, `y`.
2. Use activity data containing one-, two-, and three-digit values.
3. Scrub between those values.
4. Confirm left keeps the icon fixed.
5. Confirm center keeps the current row centered on `x`.
6. Confirm right keeps the unit's right edge fixed.
7. Switch alignment and confirm `x` remains unchanged while the content moves around it.
8. Drag the widget and confirm the horizontal anchor moves by the drag delta.

## Recommended implementation order

1. Add frontend defaults, canonical fields, and strict Rust validation.
2. Add pure alignment helpers and their exact unit tests in both languages.
3. Update standard frontend layout, coordinate layout, visual bounds, and anchor-aware interactions.
4. Install and add the single-selection alignment Toggle Group.
5. Refactor Rust dynamic layout to consume the shared horizontal geometry.
6. Generalize the Rust static layer from icons to static metric parts.
7. Update fixtures, run the focused regressions and full suites, and complete the manual editor behavior check.

## Completion criteria

The feature is complete when:

- Every intrinsic text metric accepts and persists one canonical alignment value.
- Left, center, and right alignment attach the corresponding horizontal point of the current row to `x`.
- Changing alignment leaves `x` unchanged and moves content around that anchor.
- Dragging and scaling preserve the anchor semantics in editor state.
- Selection bounds intentionally follow current content while the stored anchor remains stable.
- JSX and Rust use mirrored formulas for standard, time, and coordinate layouts.
- Left icons, center rows, and right units follow the declared cache matrix without missing or duplicated pixels.
- Existing templates retain left-aligned rendering.
- Malformed present configuration fails at ingress.
- Frontend lint and the focused frontend and Rust tests pass.
  q
