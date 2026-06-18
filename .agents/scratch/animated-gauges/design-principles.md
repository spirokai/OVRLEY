# Gauge Implementation Playbook

Step-by-step instructions for adding a new boxed gauge display type (bars, arc, corner, etc.). Follow these in order. Reference the linear gauge implementation as the canonical example.

---

## 0. Pre-flight checklist

Before writing any gauge code, define:

- The `display_type` string (e.g. `"bars"`, `"arc"`)
- Its geometry model: what shape? what fills how? any sub-elements (needle, center widget)?
- Which fields are **shared** across all gauges (already on `ValueConfig`: `track_corner_radius`, `track_border_thickness`, `track_border_color`, `track_empty_color`, `track_empty_opacity`, `track_filled_color`, `track_filled_opacity`, `track_fill_flat`, `show_min_max_labels`, `min_max_label_*`)
- Which fields are **gauge-specific** (e.g. `orientation` for linear/bars, `arc_angle` for arc, `bar_count` for bars)

---

## 1. Backend: DisplayType enum — `src-tauri/ovrley_core/src/types.rs`

Add the variant to `DisplayType` with `serde(rename)`:

```rust
#[serde(rename = "bars")]
Bars,   // or Arc, Corner — lowercase, matches frontend string
```

Update `as_str()` to return the same string. The custom `Deserialize` impl already falls back to `Text` for unknown values — no change needed there.

---

## 2. Backend: Raw config fields — `src-tauri/ovrley_core/src/normalize/raw/mod.rs`

Add gauge-specific fields to `ValueConfig`. **Every field must be `Option<T>` with `#[serde(default)]`.** Do not add required fields here — validation makes them required downstream.

Example (bars):
```rust
#[serde(default)]
pub bar_count: Option<u32>,
#[serde(default)]
pub bar_gap: Option<f32>,
```

The `orientation` field already exists on `ValueConfig` (shared by linear and bars).

---

## 3. Backend: Validator — `src-tauri/ovrley_core/src/normalize/<gauge>.rs`

Create a new validator module. It must:

1. **Check `display_type` matches** — reject with a clear error if not.
2. **Check the metric is a standard metric** — use `is_standard_metric()`.
3. **Require all fields** — use `require_string`, `require_f32`, `require_bool`, `require_u32` helpers. Every field that affects rendering must be explicit.
4. **Validate ranges** — opacity 0–1, enum values from a known set, dimensions > 0.
5. **Return a validated struct** with concrete (non-Option) types.

```rust
pub fn validate_bars_gauge(value: ValueConfig, index: usize) -> CoreResult<ValidatedBarsGaugeWidget> {
    let p = |f: &str| format!("values[{index}].{f}");
    // ... check display_type, metric, dimensions, require all fields ...
    Ok(ValidatedBarsGaugeWidget { /* concrete fields */ })
}
```

Export the validated struct and validator from `normalize/mod.rs`.

### Fallback rule

The Rust render path must never use fallbacks that serve as **styling substitutions** (e.g. defaulting a missing color to `"#ffffff"`, defaulting a missing opacity to `1.0`). The validator is the single gate — if a field affects visual output and the frontend didn't send it, reject with a clear error. The only fallbacks permitted are **runtime-safety guards**: clamping a fill fraction to `[0.0, 1.0]`, using `max(0.0)` to prevent negative rect dimensions, returning 0 for an invalid range (`max <= min`), and similar crash-prevention measures.

---

## 4. Backend: Validation dispatch — `src-tauri/ovrley_core/src/normalize/mod.rs`

In `validate_render_config`, add a dispatch arm **before** the default text validation:

```rust
if value.display_type == DisplayType::Bars {
    return validate_bars_gauge(value, idx).map(PreparedValue::BarsGauge);
}
```

If the JS normalizer stores variant-specific fields under `display_variants.<key>`, call `value.with_promoted_display_variant("bars")?` first. This promotes nested keys to the top level so the validator sees flat fields.

---

## 5. Backend: Cache types — `src-tauri/ovrley_core/src/render/widgets/types.rs`

### 5a. Add variant to `PreparedValue`

```rust
pub enum PreparedValue {
    // ...existing variants...
    BarsGauge(ValidatedBarsGaugeWidget),
}
```

Update `metric_kind()`, `display_type()`, `x()`, `y()` match arms.

### 5b. Add variant to `PresentationCache`

```rust
pub enum PresentationCache {
    // ...existing variants...
    BarsGauge(BarsGaugeCache),
}
```

### 5c. Add a cache struct

```rust
pub struct BarsGaugeCache {
    pub static_image: Image,
    pub x: f32, pub y: f32,
    pub width: u32, pub height: u32,
    pub rotation: f32,
    pub display_type: DisplayType,
    // style fields needed during per-frame draw
    pub track_corner_radius: f32,
    pub track_border_thickness: f32,
    pub track_filled_color: String,
    pub track_filled_opacity: f32,
    pub track_fill_flat: bool,
    // precomputed per-frame states
    pub min_value: f64, pub max_value: f64,
    pub frame_states: Vec<BarsGaugeFrameState>,
}

pub struct BarsGaugeFrameState {
    pub value: f64,
    pub fill01: f32,
}
```

The cache holds **everything needed for per-frame draw** — the static image, all style params that affect dynamic rendering, and precomputed frame states.

---

## 6. Backend: Prepare function — `src-tauri/ovrley_core/src/render/widgets/<gauge>.rs`

```rust
pub fn prepare_bars_gauge_cache(
    gauge: &ValidatedBarsGaugeWidget,
    dense_activity: &DenseActivityReport,
    scene: &ValidatedSceneConfig,
    scale: f32,
    font_dirs: &[PathBuf],
    prepare_profiler: &mut RenderProfiler,
) -> CoreResult<BarsGaugeCache> {
```

**Steps:**
1. Compute scaled dimensions: `(gauge.width * scale).round() as u32`
2. Derive metric range from activity data (fallback 0–100 if no valid data)
3. Precompute `frame_states`: for each frame, get interpolated value + `fill01`
4. Create a Skia surface at scaled dimensions, clear to transparent
5. Draw static layer: empty track, border, labels — everything that doesn't change per frame
6. Snapshot the surface → `surface.image_snapshot()`
7. Return the cache struct

**Static layer drawing rules:**

### Border (when `border_thickness > 0`):
```
1. Draw outer RRect (full widget rect) filled with border_color
   - Apply shadow filter to this paint if shadows are active
2. Draw inner RRect (inset by border, radius - border) with BlendMode::Clear
   - This punches a hole
3. Draw inner RRect filled with track_empty_color (with track_empty_opacity)
```

### No border:
```
1. Draw outer RRect filled with track_empty_color (with track_empty_opacity)
   - Apply shadow filter if shadows are active
```

### Min/max labels (when `show_min_max_labels`):
- Resolve font via `resolve_font(font_dirs, font_name, font_size * scale)`
- Draw min label at left/bottom edge, max label at right/top edge
- Y baseline: `(h + font_size) / 2.0 - font_size * 0.15`

**GOTCHA:** Shadow filter goes on the **outermost** rect only. If there's a border, shadow on border; if no border, shadow on empty track. Never apply shadow to inner/cleared rects.

---

## 7. Backend: Draw function — `src-tauri/ovrley_core/src/render/widgets/<gauge>.rs`

```rust
pub fn draw_bars_gauge_widget(
    canvas: &Canvas,
    cache: &BarsGaugeCache,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
```

**Steps:**
1. Guard: return `None` if `cache.display_type` doesn't match
2. `canvas.draw_image(&cache.static_image, (cache.x, cache.y), None)` — blit the pre-rendered static layer
3. Look up the frame's `fill01` from `cache.frame_states[frame_index]`
4. Compute fill geometry (rects, paths, arcs) based on orientation and `fill01`
5. Draw fill with appropriate paint (color + opacity from cache)
6. Return a `WidgetRenderReport` with geometry and frame diagnostics

### Fill drawing: rounded corners

**Non-flat fill** (`track_fill_flat = false`):
- Compute fill rect (inset by border_thickness)
- Radius = `track_corner_radius - border_thickness` (max 0)
- Draw `RRect::new_rect_xy(fill_rect, radius, radius)`

**Flat fill** (`track_fill_flat = true`):
- Compute inner rect (full track inset by border)
- Compute fill rect (the advancing portion)
- `canvas.save()` → `canvas.clip_rect(fill_rect, Intersect, true)` → draw full inner `RRect` → `canvas.restore()`
- This gives rounded corners everywhere except the advancing edge, which is flat.

### Fill draw: arc gauges (different from linear)

For arc gauges, **do NOT** use rounded corners on the fill. Use `stroke-linecap: round` semantics instead. Draw the filled arc as a stroked partial arc:
- Use Skia `Cap::Round` on the stroke paint
- The `track_corner_radius` field controls stroke cap behavior conceptually, but the actual rendering uses round caps

---

## 8. Backend: Wire up — `mod.rs` and `metric_presentation.rs`

### `render/widgets/mod.rs` — Prepare dispatch

In `prepare_render_assets`, add a match arm for the new `PreparedValue` variant:

```rust
PreparedValue::BarsGauge(validated) => {
    let cache = bars_gauge::prepare_bars_gauge_cache(
        validated, dense_activity, &assets.scene, assets.scene.scale,
        &paths.font_dirs, prepare_profiler,
    )?;
    assets.presentation_caches.insert(idx, PresentationCache::BarsGauge(cache));
}
```

### `render/widgets/metric_presentation.rs` — Draw dispatch

Add a match arm in `draw_metric_presentation`:

```rust
DisplayType::Bars => draw_bars_presentation(
    canvas,
    presentation_caches.get(&value_idx),
    frame_index,
    frame_profiler,
),
```

And a forwarding function:
```rust
fn draw_bars_presentation(
    canvas: &Canvas,
    cache: Option<&PresentationCache>,
    frame_index: usize,
    frame_profiler: &mut RenderProfiler,
) -> Option<WidgetRenderReport> {
    let PresentationCache::BarsGauge(cache) = cache? else { return None };
    draw_bars_gauge_widget(canvas, cache, frame_index, frame_profiler)
}
```

---

## 9. Frontend: Manifest — `assets/standard-metrics.json`

Add entry in `displayTypes.definitions`:

```json
"bars": {
  "label": "Bars",
  "layoutMode": "boxed",
  "defaultFrameWidth": 200,
  "defaultFrameHeight": 40,
  "defaults": {
    "orientation": "horizontal",
    "bar_count": 5,
    "bar_gap": 2,
    "track_corner_radius": 4,
    "track_border_thickness": 2,
    "track_border_color": "#ffffff",
    "track_empty_color": "#222222",
    "track_empty_opacity": 0.5,
    "track_filled_color": "#40e0d0",
    "track_filled_opacity": 1.0,
    "track_fill_flat": true,
    "show_min_max_labels": false,
    "min_max_label_font": "Arial.ttf",
    "min_max_label_font_size": 12,
    "min_max_label_color": "#ffffff"
  }
}
```

Key rules:
- `layoutMode: "boxed"` enables the boxed rendering path in `WidgetPreview.jsx`
- `defaultFrameWidth`/`defaultFrameHeight` are used for the unsupported fallback placeholder
- `defaults` provides all widget-default values — the editor reads these via `getDisplayVariantNonGeometryDefaults()`

---

## 10. Frontend: Geometry utils — `app/src/features/widget-preview/utils/<gauge>Geometry.js`

**MUST mirror the Rust geometry functions exactly.** Same inputs, same formulas, same outputs.

Export pure functions only (no React, no DOM, no side effects):
- `getFillPercentage(value, min, max)` → number 0–1
- `get<Gauge>Range(values)` → `{ min, max }` with fallback 0–100
- `get<Gauge>Layout({ value, values, width, height, ...params })` → `{ min, max, fill, trackRect, fillRects[], ... }`
- `format<Gauge>Label(value)` → string

**Rules:**
- No default parameters that diverge from Rust behavior
- Empty/null values → fallback to placeholder (0.5 fill, 0–100 range)
- Clamp fill to [0,1]; guard against `max <= min`

---

## 11. Frontend: Renderer component — `app/src/features/widget-preview/components/<Gauge>Renderer.jsx`

```jsx
export function OverlayBarsGaugeWidget({ widget, activity, previewSecond, globalOpacity = 1, globalScale = 1, sceneStyle }) {
  const data = widget.data
  if (data.display_type !== 'bars') return null   // GUARD FIRST
  // ...
}
```

**Component rules:**
1. **Guard on display_type** — return `null` if the widget isn't your type. This is the safety net for dispatch mismatches.
2. **Compute layout** from geometry utils, passing `data.orientation`, `data.<specific_fields>`, and border thickness.
3. **Render an `<svg>`** with `viewBox="0 0 {width} {height}"` and scaled `width`/`height` attributes: `width={width * scale}`, `height={height * scale}`.
4. **Add a `data-testid`** for test targeting.
5. **Apply global opacity** as a multiplier: `opacity={data.opacity * globalOpacity}`.
6. **Resolve fonts** via `getPreviewFontFamily(data.<font_field>)`.
7. **Handle shadows** via `getTextShadowParts(sceneStyle)` and `<PreviewSvgShadowOnlyFilter>`.

### SVG structure for border + empty track:

```jsx
<defs>
  {borderThickness > 0 && (
    <mask id={maskId}>
      <rect fullTrack rx={cornerRadius} fill="white" />
      <rect innerTrack rx={fillCornerRadius} fill="black" />
    </mask>
  )}
  {flatFill && fillCornerRadius > 0 && (
    <clipPath id={clipId}>
      <rect x={fillRect.x} y={fillRect.y} width={fillRect.width} height={fillRect.height} />
    </clipPath>
  )}
</defs>

{/* Border (with mask) OR empty track (no mask) */}
{borderThickness > 0 ? (
  <rect fullTrack rx={cornerRadius} mask={`url(#${maskId})`} fill={borderColor} filter={shadowFilter} />
) : (
  <rect fullTrack rx={cornerRadius} fill={emptyColor} fillOpacity={emptyOpacity} filter={shadowFilter} />
)}

{/* Empty track when border exists */}
{borderThickness > 0 && (
  <rect innerTrack rx={fillCornerRadius} fill={emptyColor} fillOpacity={emptyOpacity} />
)}

{/* Filled portion */}
{flatFill && fillCornerRadius > 0 ? (
  <rect innerTrack rx={fillCornerRadius} clipPath={`url(#${clipId})`} fill={filledColor} fillOpacity={filledOpacity} />
) : (
  <rect x={fillRect.x} y={fillRect.y} width={fillRect.width} height={fillRect.height} rx={fillCornerRadius} fill={filledColor} fillOpacity={filledOpacity} />
)}
```

**GOTCHAs:**
- When `borderThickness > 0`, draw the empty track as a **separate inner rect** (not relying on the border mask for empty fill). This avoids antialiasing artifacts where the mask edge creates a faint gap between border and empty fill.
- When `borderThickness === 0`, the empty track doubles as the outer rect and carries the shadow.
- The `fillCornerRadius` = `Math.max(0, cornerRadius - borderThickness)`.
- `innerTrackRect` = `{ x: border, y: border, width: w - border*2, height: h - border*2 }`.

---

## 12. Frontend: Wire up preview — `WidgetPreview.jsx`

Register the renderer in `BOXED_PREVIEW_COMPONENTS`:

```jsx
const BOXED_PREVIEW_COMPONENTS = {
  heading_tape: OverlayHeadingWidget,
  linear: OverlayLinearGaugeWidget,
  bars: OverlayBarsGaugeWidget,     // ADD HERE
}
```

The dispatch is automatic: `isBoxedDisplayType(displayType)` checks `layoutMode === 'boxed'` in the manifest, then `BOXED_PREVIEW_COMPONENTS[displayType]` picks the renderer. If the display type is in the manifest but has no renderer yet, the fallback `UnsupportedBoxedPreview` shows a placeholder.

**Do NOT add conditional logic to `WidgetPreview.jsx`** — just register in the map.

---

## 13. State handling: drag, resize, rotate

All boxed gauge widgets reuse the existing editor interaction system. **No gauge-specific changes needed.** The system works automatically for any `PreparedValue` variant because:

- **Drag:** Reads/writes `x`, `y` from `widget.data` — all boxed widgets have these.
- **Resize:** Reads/writes `width`, `height` from `widget.data`. Boxed widgets are detected by `isBoxedMetricWidget()` which checks the manifest for `layoutMode === 'boxed'`. Resize dimensions are divided by `globalScale` on boxed widgets so stored values remain in widget-local coordinates.
- **Rotate:** Reads/writes `rotation` from `widget.data`. Normalized to 0–360 with 1 decimal on commit.

The draft pattern:
1. `on*Start`: capture origin state in `interactionStartRef`, init empty draft
2. `on*`: compute delta, store in both `draftWidgetsRef` (sync) and React state (render), apply live CSS transform to DOM
3. `on*End`: read draft, round values, commit via `commitWidgetUpdate`, clear draft

Values are **never persisted mid-interaction**. Draft is transient; only the rounded final state is saved.

---

## 14. Frontend editable-fields config

The widget editor needs to know which gauge-specific fields to show. If using a manifest-driven editor, add your fields to the display type definition's `defaults` object (step 9). The editor reads these keys and renders appropriate controls.

If the editor has hardcoded field lists, add the gauge-specific fields there (e.g., `bar_count` slider, `bar_gap` slider for bars gauge).

---

## 15. Testing

### Backend (Rust) — `tests/<gauge>_tests.rs`

1. **Deserialization**: verify all gauge fields parse from JSON
2. **Fill percentage**: test `fill_percentage` with in-range, min, max, out-of-range, degenerate range
3. **Geometry functions**: test fill rects, bar buckets, arc angles with exact expected tuples
4. **Label formatting**: test integer vs decimal formatting
5. **Cache preparation**: build cache from validated config + dense activity, assert cache fields
6. **Preview render**: call `render_preview_with_report`, verify `MetricPresentationReport` has correct metric_kind, display_type, geometry, and progress01

### Frontend (JS) — `tests/features/widget-preview/`

1. **Geometry unit tests** (`<gauge>Geometry.test.js`): test all geometry functions with same inputs as Rust tests
2. **Renderer tests** (`<gauge>Renderer.test.jsx`): render with Testing Library, assert SVG attributes, fill rect dimensions, font resolution, flat fill mode
3. Ensure the unsupported fallback renders when the type is in the manifest but no renderer is registered

---

## Summary checklist for a new gauge type

- [ ] 0. Define display_type string and gauge-specific fields
- [ ] 1. `types.rs` — add `DisplayType` variant
- [ ] 2. `normalize/raw/mod.rs` — add gauge-specific `Option<>` fields to `ValueConfig`
- [ ] 3. `normalize/<gauge>.rs` — validator with all fields required
- [ ] 4. `normalize/mod.rs` — dispatch arm in `validate_render_config`
- [ ] 5. `render/widgets/types.rs` — `PreparedValue` variant, `PresentationCache` variant, cache struct, frame state struct
- [ ] 6. `render/widgets/<gauge>.rs` — prepare function (static layer → Skia Image)
- [ ] 7. `render/widgets/<gauge>.rs` — draw function (blit static + composite fill)
- [ ] 8. `render/widgets/mod.rs` + `metric_presentation.rs` — wire up prepare and draw dispatch
- [ ] 9. `assets/standard-metrics.json` — display type definition with defaults
- [ ] 10. `utils/<gauge>Geometry.js` — pure geometry functions mirroring Rust
- [ ] 11. `components/<gauge>Renderer.jsx` — SVG renderer with mask/clip/round logic
- [ ] 12. `WidgetPreview.jsx` — register in `BOXED_PREVIEW_COMPONENTS`
- [ ] 13. Editor field config — add gauge-specific fields to editor UI
- [ ] 14. Rust tests: deserialization, geometry, cache, preview render
- [ ] 15. JS tests: geometry unit tests, renderer tests

No single step should take more than ~50 lines of new code. If it does, you're probably adding logic that belongs in a shared utility.
