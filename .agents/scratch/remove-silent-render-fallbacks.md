# Plan: Remove All Silent Fallbacks from Backend Rendering & Encoding

## Status: Draft / Ready for review

---

## 1. Problem Summary

The Rust rendering and encoding engine currently contains **~413 instances** of silent fallbacks (`unwrap_or`, `unwrap_or_else`, `unwrap_or_default`) combined with **~228 `#[serde(default)]`** annotations across the config schema. When a config field is missing, the backend silently substitutes a hardcoded Rust-side value instead of rejecting the config. This creates three problems:

1. **Editor/render mismatch.** The frontend live canvas and the final rendered output can diverge because different defaults are active in TypeScript vs. Rust.
2. **Invisible bugs.** Missing or null-stripped JSON fields produce wrong pixels without any error.
3. **Unmaintainable inheritance chains.** Style resolution crawls through 3–4 tiers of fallback (e.g., `explicit_color → inherited_line_color → plot_base_color → DEFAULT_COLOR = "#ffffff"`), making it impossible to reason about what will actually render.

The architecture change underway will add a **config validation seam** between frontend and backend that rejects incorrect configs before rendering. For this to work, the backend renderer must make **zero assumptions** about missing styling — every field it consumes must be guaranteed present by the seam.

---

## 2. Infected Files (Complete Inventory)

### 2.1 Render directory (149 `unwrap_or*` instances)

| File | Est. count | Key categories |
|---|---|---|
| `render/mod.rs` | 9 | Scene dimensions 1920×1080, timing bucket fallbacks |
| `render/text.rs` | 27 | Font, color, size, shadow, border, opacity, hex parsing |
| `render/format.rs` | 25 | `"--"` placeholders, unit defaults, format chains, decimal precision |
| `render/widgets/common.rs` | 24 | 6 hardcoded `DEFAULT_*` constants, opacity, marker synthesis, shadow, color inheritance |
| `render/widgets/route/normalize.rs` | 15 | Marker size 18.0, simplification 1.0, density 1.0, opacity cascades |
| `render/widgets/elevation/normalize.rs` | 22 | Marker 16.0, margin 0.0, y_scale 1.0, area opacity 0.12/0.24, label offsets -28/6, font 12.5 |
| `render/widgets/heading/prepare.rs` | 10 | 4 color fields → `"#ffffff"`, label_font_size 12.0, label_offset 4.0, indicator color/size |
| `render/widgets/heading/draw.rs` | 2 | Unrecognized indicator_style/placement → silent no-op |
| `render/widgets/heading/geometry.rs` | 1 | `partial_cmp` fallback |
| `render/widgets/marker.rs` | 6 | Default filled circle for empty layers, point weight 80.0, color `"#ffffff"` |
| `render/widgets/value/layout.rs` | 12 | icon_size 28.0, icon_color `"#40e0d0"`, unit_color `"#ffffff"`, offset 0.0 |
| `render/widgets/value/gradient.rs` | 8 | triangle_width 72.0, triangle colors `"#c65102"`/`"#40e0d0"`, show_triangle true |
| `render/widgets/value/svg.rs` | 1 | SVG stroke-width → 2.0 |
| `render/widgets/metric_presentation.rs` | 1 | Heading → 0.0 |
| `render/widgets/route/prepare.rs` | 3 | show_full_activity false, progress fallback |
| `render/widgets/route/frame_state.rs` | 3 | route_position_at_progress → (0,0,0), elevation_m → 0.0 |
| `render/widgets/elevation/prepare.rs` | 3 | show_full_activity false, progress fallback |
| `render/widgets/elevation/frame_state.rs` | 5 | elevation 0.0, interpolation → vec![0.0] |
| Various draw files | 7 | scene.scale, shadow/border defaults inside draw closures |

### 2.2 Encoding directory (~42 `unwrap_or*` instances)

| File | Count | Key fallbacks |
|---|---|---|
| `encode/video_pipeline.rs` | 4 | width→1920, height→1080, scale→1.0, pixel format→"rgba" |
| `encode/video_composite_pipeline.rs` | 15 | width→1920×2, height→1080×2, scale→1.0, sync_offset→0.0, trim_start→0.0, update_rate→1, codec→"libx264" |
| `encode/video_debug.rs` | 3 | width→1920, height→1080, codec→"unknown" |
| `encode/video_segmented.rs` | 3 | codec→"libx264", duration→0 |
| `encode/video.rs` | 5 | duration, trim_start→0.0, update_rate→1, codec→"libx264" |
| `encode/video_parallel.rs` | 1 | thread_count→4 |
| `encode/ffmpeg_settings.rs` | 3 | codec→"prores_ks", log_level→"info", container→"mov" |
| `encode/ffmpeg_composite.rs` | 3 | render_duration, profile fallback |
| `encode/ffmpeg_composite_profiles.rs` | 1 | codec name fallback |
| `encode/ffmpeg_transparent_profiles.rs` | 1 | codec name fallback |
| `encode/codec_detect.rs` | 1 | qsv init args → default |
| `encode/progress.rs` | 1 | progress → default |

### 2.3 Config & schema files (~10 `unwrap_or*` + ~200 `#[serde(default)]`)

| File | `unwrap_or*` | `#[serde(default)]` |
|---|---|---|
| `config/mod.rs` | 4 | 160 |
| `activity/schema.rs` | 0 | 35 |
| `standard_metrics.rs` | 7 | 5 |

### 2.4 Other files (~10 `unwrap_or*`)

| File | Count | Fallbacks |
|---|---|---|
| `commands/mod.rs` | 4 | filename→"template.json", resolution→(0,0) |
| `paths.rs` | 2 | env var → "." |
| `activity/trim.rs` | 3 | interpolation → 0.0 |
| `activity/interpolate.rs` | 2 | timestamp → UNIX_EPOCH, duration → default |

---

## 3. All Assumptions the Backend Makes About Frontend Config Data

### 3.1 Rendering — Scene

| What the backend assumes | Default value | Where |
|---|---|---|
| Output width | 1920 px | `render/mod.rs:112,222,374` |
| Output height | 1080 px | `render/mod.rs:113,223,375` |
| Render scale | 1.0 | `render/mod.rs:114,224` |
| Custom export range active | false | `widgets/common.rs:101` |
| Show full activity | false | `route/prepare.rs:33`, `elevation/prepare.rs:34` |

### 3.2 Rendering — Text Style

| What the backend assumes | Default value | Where |
|---|---|---|
| Text opacity | 1.0 | `text.rs:80,117` |
| Font size | 32.0 px | `text.rs:90,127` |
| Line height factor | 0.92 | `text.rs:91,128` |
| Text color | `"#ffffff"` | `text.rs:97,134` |
| Shadow strength | 0.0 | `text.rs:102,139` |
| Shadow distance | 0.0 | `text.rs:103,140` |
| Border thickness | 0.0 | `text.rs:105,142` |
| Border distance | 0.0 | `text.rs:106,143` |
| Value offset Y | 0.0 | `text.rs:115` |
| Invalid hex color → white | `(255,255,255,255)` | `text.rs:282` |
| Missing font → Arial → system default | chain | `text.rs:315-318` |

### 3.3 Rendering — Metric Values & Formatting

| What the backend assumes | Default value | Where |
|---|---|---|
| Missing metric value text | `"--"` | `format.rs:95,347,361,371,379,387,392` |
| Missing time | `"--:--"` | `format.rs:438` |
| Missing gradient | `"--%"` | `format.rs:413` |
| Speed unit | `"kmh"` | `format.rs:339` |
| Air pressure unit | `"hpa"` | `format.rs:585` |
| Stride length unit | `"m"` | `format.rs:591` |
| Vertical speed unit | `"mps"` | `format.rs:598` |
| Gradient show sign | `true` | `format.rs:424` |
| Hours offset | `0` | `format.rs:444` |
| Time format cascade | `"time-24"` → 24h | `format.rs:456-458,500` |
| Decimal precision | `0` | `format.rs:528` |
| Balance format | `"plain"` | `format.rs:620` |

### 3.4 Rendering — Value Widget (Icons & Gradient)

| What the backend assumes | Default value | Where |
|---|---|---|
| Icon size | 28.0 px | `value/layout.rs:49,157` |
| Icon color | `"#40e0d0"` | `value/layout.rs:82,170` |
| Unit color | `"#ffffff"` | `value/layout.rs:109` |
| Icon offset X | 0.0 | `value/layout.rs:87,175` |
| Icon offset Y | 0.0 | `value/layout.rs:93,181` |
| Show icon (non-gradient) | `true` | `value/layout.rs:127,152` |
| Show icon (gradient) | `false` | implicit via `!= Gradient` |
| Triangle width | 72.0 px | `value/gradient.rs:56` |
| Show triangle | `true` | `value/gradient.rs:59` |
| Triangle positive color | `"#40e0d0"` | `value/gradient.rs:115` |
| Triangle negative color | `"#c65102"` | `value/gradient.rs:110` |
| SVG stroke-width (if missing from file) | 2.0 | `value/svg.rs:24` |
| Icon viewbox size | 24.0 | `value/icons.rs:13` (constant, not config-related) |

### 3.5 Rendering — Route & Elevation Widget

| What the backend assumes | Default value | Where |
|---|---|---|
| Route plot color | `"#ffffff"` | `common.rs:257` |
| Line width multiplier (route) | 1.75 × 2.5 | `common.rs:18,24` |
| Line width multiplier (elevation) | 1.75 × 2.5 | `common.rs:18,22` |
| Route marker size | 18.0 px | `route/normalize.rs:31` |
| Elevation marker size | 16.0 px | `elevation/normalize.rs:29` |
| Marker variant (unrecognized) | `"single"` | `route/normalize.rs:107-111` |
| Marker variant diameter | `marker_size × 2 + 8` | `route/normalize.rs:41` |
| Marker point weight | 80.0 | `common.rs:19`, `marker.rs:77` |
| Empty marker layers → default circle | synthesized | `marker.rs:34-43` |
| Simplification tolerance (route) | 1.0 px | `route/normalize.rs:54` |
| Target density (route) | 1.0 | `route/normalize.rs:57` |
| Target density (elevation) | 0.75 | `elevation/normalize.rs:58` |
| Remaining line opacity (route) | 0.75 | `route/normalize.rs:68` |
| Completed line opacity (route) | 1.0 | `route/normalize.rs:89` |
| Remaining line opacity (elevation) | 1.0 | `elevation/normalize.rs:69` |
| Completed line opacity (elevation) | 1.0 | `elevation/normalize.rs:90` |
| Area remaining opacity | 0.12 | `elevation/normalize.rs:104` |
| Area completed opacity | 0.24 | `elevation/normalize.rs:114` |
| Elevation margin | 0.0 | `elevation/normalize.rs:55` |
| Elevation Y scale | 1.0 | `elevation/normalize.rs:56` |
| Show elevation metric label | `false` | `elevation/normalize.rs:128` |
| Show elevation imperial label | `false` | `elevation/normalize.rs:129` |
| Metric label offset Y | −28.0 px | `elevation/normalize.rs:138` |
| Imperial label offset Y | 6.0 px | `elevation/normalize.rs:141` |
| Elevation label font size | 12.5 px | `elevation/normalize.rs:150` |
| Elevation label color | plot base color | `elevation/normalize.rs:152` |
| Point label sub-object empty | `Default::default()` | `elevation/normalize.rs:33` |
| Route/elevation shadow | scene shadow | inherited, not explicit |

### 3.6 Rendering — Heading Widget

| What the backend assumes | Default value | Where |
|---|---|---|
| Pixels per degree | 5.0 | serde default `config/mod.rs:659` |
| Major tick interval | 15° | serde default `config/mod.rs:662` |
| Minor ticks per major | 3 | serde default `config/mod.rs:665` |
| Show major/minor ticks | `true` | serde default `config/mod.rs:668` |
| Major tick length | 40% of height | serde default `config/mod.rs:671` |
| Minor tick length | 20% of height | serde default `config/mod.rs:674` |
| Major tick thickness | 2.0 px | serde default `config/mod.rs:677` |
| Minor tick thickness | 2.0 px | serde default `config/mod.rs:680` |
| Tick alignment | `"below"` | serde default `config/mod.rs:683` |
| Tick color | `"#ffffff"` | `heading/prepare.rs:53` |
| Cardinal tick color | falls back to tick_color | `heading/prepare.rs:54` |
| Label color | `"#ffffff"` | `heading/prepare.rs:55` |
| Cardinal label color | falls back to label_color | `heading/prepare.rs:56` |
| Label font size | 12.0 px | `heading/prepare.rs:74` |
| Label offset | 4.0 px | `heading/prepare.rs:84` |
| Label font | cascades through value font → scene font | `heading/prepare.rs:76-80` |
| Indicator style | `"chevron"` | serde default `config/mod.rs:686` |
| Indicator placement | `"top"` | serde default `config/mod.rs:688` |
| Indicator color | `"#ffffff"` | `heading/prepare.rs:242` |
| Indicator size | 10.0 px | `heading/prepare.rs:243` |
| Show indicator | `true` | serde default `config/mod.rs:668` |
| Unrecognized indicator_style | nothing drawn | `heading/draw.rs:108-114` |
| Unrecognized indicator_placement | nothing drawn | `heading/draw.rs:157-187` |
| Missing heading value | 0.0° | `metric_presentation.rs:100` |

### 3.7 Encoding — Resolution & Scale

| What the backend assumes | Default value | Where |
|---|---|---|
| Output width | 1920 px | `encode/video_pipeline.rs:119`, `video_composite_pipeline.rs:269,562` |
| Output height | 1080 px | `encode/video_pipeline.rs:120`, `video_composite_pipeline.rs:270,563` |
| Scale | 1.0 | `encode/video_pipeline.rs:208`, `video_composite_pipeline.rs:272` |

### 3.8 Encoding — Video Composite

| What the backend assumes | Default value | Where |
|---|---|---|
| Composite sync offset | 0.0 sec | `video_composite_pipeline.rs:107` |
| Composite trim start | 0.0 sec | `video_composite_pipeline.rs:113`, `video.rs:73` |
| Composite render duration | video_duration − trim_start | `video_composite_pipeline.rs:134`, `video.rs:70-71` |
| Widget update rate | 1 | `video_composite_pipeline.rs:557`, `video.rs:74` |

### 3.9 Encoding — Codec & Container

| What the backend assumes | Default value | Where |
|---|---|---|
| Video codec | `"libx264"` | `video_segmented.rs:298`, `video_composite_pipeline.rs:759`, `video.rs:83` |
| ProRes default | `"prores_ks"` | `ffmpeg_settings.rs:42` |
| Container format | `"mov"` | `ffmpeg_settings.rs:83` |
| FFmpeg log level | `"info"` | `ffmpeg_settings.rs:47` |
| Codec profile (composite) | built-in template | `ffmpeg_composite.rs:306` |
| Codec name resolution | identity fallback | `ffmpeg_composite_profiles.rs:246`, `ffmpeg_transparent_profiles.rs:95` |

### 3.10 Encoding — Other

| What the backend assumes | Default value | Where |
|---|---|---|
| Thread count | 4 | `video_parallel.rs:135` |
| Input pixel format | `"rgba"` (env var override) | `video_pipeline.rs:491` |
| QSV init args | empty | `codec_detect.rs:579` |

---

## 4. Concrete Removal Plan

### Phase 1: Define the Validated Contract (Config Schema Audit)

**Goal:** Establish which fields must be required vs. can remain optional.

**Tasks:**

1. **Audit every `#[serde(default)]` field** in `config/mod.rs` and classify:
   - **Category A — Must become required**: Fields where the renderer has no sensible general-purpose default (colors, sizes, positions, units, formats, opacities, marker variants, indicator styles, codecs).
   - **Category B — Can stay optional with frontend aware**: Fields where a default is genuinely universal (e.g., `ffmpeg: Value`, `extra: BTreeMap`, `labels: Vec<LabelConfig>` — empty vecs are fine).
   - **Category C — Remove entirely**: Unused or reserved fields that no code path reads (`border_strength`, `border_distance` on LabelConfig).

2. **Audit every `unwrap_or` in the render and encode directories** and tag each with the field(s) it corresponds to. Create a spreadsheet mapping `unwrap_or` location → config field → proposed resolution (required field / keep fallback / remove).

3. **Document the contract** in a new file (e.g., `docs/config-schema-contract.md`) that lists every field the backend consumes, whether it's required, and what it controls.

**Deliverable:** Spreadsheet of all ~413 fallback sites with classification.

### Phase 2: Add Seam Validation

**Goal:** Add a validation layer that rejects configs before they reach the renderer.

**Tasks:**

1. **Create a `config::validate` module** with a function:
   ```rust
   pub fn validate_render_config(config: &RenderConfig) -> CoreResult<()>
   ```
   This function checks every required field and returns a structured error with the field path and reason.

2. **Integrate into `parse_config_json`**: call `validate_render_config` after deserialization but before returning. Invalid configs never reach the renderer.

3. **Error format**: Use structured errors with JSON paths (the frontend displays these to the user if the backend rejects a config):
   ```json
   {
     "errors": [
       {
         "path": "values[0].icon_color",
         "message": "missing required field: icon_color"
       },
       {
         "path": "plots.course.completed_line_color",
         "message": "missing required field: completed_line_color"
       }
     ]
   }
   ```

**Deliverable:** `config::validate` module, integrated into parse pipeline.

### Phase 3: Remove Silent Fallbacks — Render (by module, bottom-up)

**Principle:** After the seam guarantees fields are present, the renderer should use `expect("field X guaranteed by config validation")` or receive non-`Option` types. Every `unwrap_or` becomes either a removal (the field is now always present) or a legitimate data-absence case (sensor data missing → `"--"`).

**Order:** Leaf modules first, normalize modules second, orchestrator last.

| Priority | Module | Action |
|---|---|---|
| P0 | `text.rs` | Remove ~18 style fallbacks. `ResolvedTextStyle` fields become non-`Option`. Hex parsing error → `CoreError`. |
| P0 | `format.rs` | Remove ~15 unit/format fallbacks. Keep only `"--"` for missing telemetry data (not config). |
| P0 | `widgets/common.rs` | Remove 6 `DEFAULT_*` constants. Remove `resolve_style_color` inherit function. Remove `fallback_marker_points`. Remove `legacy_line_width`. Every field explicit. |
| P0 | `widgets/route/normalize.rs` | Remove ~15 `unwrap_or` calls. All fields become non-`Option` in `NormalizedRoutePlot`. |
| P0 | `widgets/elevation/normalize.rs` | Remove ~22 `unwrap_or` calls. All fields become non-`Option` in `NormalizedElevationPlot`. |
| P0 | `widgets/heading/prepare.rs` | Remove ~10 `unwrap_or` calls. All color/size/offset fields non-`Option`. |
| P0 | `widgets/marker.rs` | Remove default circle fallback for empty layers. Empty layers → draw nothing or error. |
| P0 | `widgets/value/layout.rs` | Remove ~8 icon defaults. `icon_color`, `icon_size`, `unit_color` become required. |
| P0 | `widgets/value/gradient.rs` | Remove ~6 gradient defaults. Triangle colors/size become required. |
| P1 | `widgets/value/svg.rs` | Remove stroke-width 2.0 fallback. Bad SVG → `CoreError`. |
| P1 | `widgets/heading/draw.rs` | Unknown indicator style/placement → `CoreError` instead of silent no-op. |
| P1 | `widgets/metric_presentation.rs` | Missing heading → already handled by seam validation (heading series required). |
| P1 | `widgets/elevation/draw.rs` | Remove 3 shadow/border scene fallbacks. Inherit from validated normalized plot. |
| P2 | `render/mod.rs` | Remove scene dimension fallbacks (3×3 sites). Width/height/scale now required. |
| P2 | `render/text.rs` | Font becomes required field. Typeface resolver keeps Arial/system chain as documented last resort (logs warning if hit). |

### Phase 4: Remove Silent Fallbacks — Encode (by module)

| Priority | Module | Action |
|---|---|---|
| P0 | `encode/video_pipeline.rs` | Remove width/height/scale fallbacks. Resolution now required in scene config. |
| P0 | `encode/video_composite_pipeline.rs` | Remove 15 fallbacks. Composite fields become required or removed if unused. |
| P0 | `encode/video.rs` | Remove duration/trim/update_rate fallbacks. |
| P1 | `encode/video_segmented.rs` | Remove codec fallback. Codec must be explicit. |
| P1 | `encode/video_debug.rs` | Remove resolution fallbacks. |
| P1 | `encode/ffmpeg_settings.rs` | Remove codec/container/log_level fallbacks. Must be explicit. |
| P1 | `encode/ffmpeg_composite.rs` | Remove profile fallback. |
| P2 | `encode/video_parallel.rs` | Thread count → keep as system reasonable default (not config-controlled). |
| P2 | `encode/codec_detect.rs` | QSV detection → keep (hardware detection, not config). |

### Phase 5: Remove Silent Fallbacks — Config & Activity

| Priority | Module | Action |
|---|---|---|
| P0 | `config/mod.rs` | `width`/`height` become required fields (not `Option<u32>`). Frontend always writes explicit resolution. Remove all 11 render+encode fallback sites. |
| P0 | `config/mod.rs` | Remove `#[serde(default)]` from all Category A fields. Replace with required fields. Remove `strip_json_nulls` hack. |
| P0 | `config/mod.rs` | Remove `resolve_style_color` usage from plot normalization — plots read their own fields directly. |
| P0 | `config/mod.rs` | Heading widget: remove all 13 `default = "..."` serde functions. Every heading field becomes required (frontend always writes explicit values). |
| P2 | `activity/schema.rs` | Leave as-is — 35 `#[serde(default)]` annotations are for sparse telemetry input, not config. No changes. |
| P2 | `activity/trim.rs`, `interpolate.rs` | Interpolation fallbacks → `0.0` is reasonable for missing data. Keep as explicit signal. |

### Phase 6: Remove Inheritance Chains

**Goal:** The `resolve_style_color` pattern and similar cascades must be replaced with direct field reads.

**Affected:** `common.rs:320-329` (`resolve_style_color`), `common.rs:279-294` (`fallback_marker_points`), `common.rs:261-263` (`legacy_line_width`), `route/normalize.rs` and `elevation/normalize.rs` (all the `.or_else(...)` opacity cascades).

**Action:** After Phase 1 validates that every field is present, these functions become either:
- Removed entirely (just read `plot.completed_line_color` directly)
- Kept as explicit two-field merge where both are required (e.g., if the schema genuinely needs both)

**Deliverable:** No `resolve_style_color`, no `legacy_line_width`, no `fallback_marker_points`. Every widget normalization reads its fields directly from the config.

---

## 5. What Should Stay

Not everything needs to be removed. These are appropriate:

| Item | Reason |
|---|---|
| `"--"` placeholder for missing telemetry values | Sensor dropout is runtime data absence, not config error |
| `vec![0.0; n]` for empty interpolation input | No data = reasonable zero output |
| `0.0` for missing progress/elevation in sparse data | Data absence, not config absence |
| `thread_count → 4` | System heuristic, not config-driven |
| `QSV detection → default` | Hardware probe, not config-driven |
| `paths.rs` env var → `"."` | Runtime environment, not config |
| `update_rate → 1` (optional, means "no decimation") | Genuine optional feature with clear semantics |
| `extra: BTreeMap` (flatten) | Forward-compat container, correctly empty |
| `labels: Vec<LabelConfig>` → `vec![]` | No static labels is valid |
| `values: Vec<ValueConfig>` → `vec![]` | No dynamic values is valid (though unusual) |
| Font → Arial (typeface resolver) | Final safety net, logs warning. Should never trigger if font field is required. |

---

## 6. Migration Strategy

### 6.1 Template Version Bump

The config schema changes will break existing templates. Increment `TEMPLATE_FILE_VERSION` from 2 → 3. The frontend editor must:
- Migrate templates to the new schema on load
- Populate previously-optional fields with explicit values (sourced from the same defaults that were previously in Rust)

### 6.2 Default Value Extraction

The frontend must adopt the same defaults the Rust backend currently has, but as explicit values in the config JSON. To find these, run:
```bash
# Extract every hardcoded value from unwrap_or calls
rg -n 'unwrap_or\(([^)]+)\)' src-tauri/ovrley_core/src/
```
The extracted values become the migration defaults in the frontend template loader.

### 6.3 Backward Compatibility (Optional)

If backward compat with v2 templates is needed, add a `migrate_v2_to_v3` function that fills in the v3 required fields using the v2 Rust defaults. This lives in the seam validator, not the renderer.

### 6.4 Test Strategy

- **Before removal:** Capture "golden" PNG outputs for a set of templates with missing optional fields
- **After removal:** Run templates through the migrator + new validator → render → compare PNGs pixel-identical with golden outputs
- **Failure case tests:** Templates with intentionally missing required fields → assert structured error with correct field path

---

## 7. Estimated Scope

| Phase | Files touched | Est. lines changed | Risk |
|---|---|---|---|
| P1: Contract audit | 0 (docs only) | ~200 lines doc | None |
| P2: Seam validation | 2 new + 1 existing | ~400 lines | Low |
| P3: Render removal | ~18 files | ~600 lines changed | Medium (visual regression) |
| P4: Encode removal | ~12 files | ~300 lines changed | Medium (encoding regression) |
| P5: Config/schema | ~3 files | ~400 lines changed | High (breaks template compat) |
| P6: Inheritance removal | ~6 files | ~200 lines removed | Medium |
| **Total** | **~42 files** | **~2100 lines** | |

---

## 8. Resolved Decisions

1. **`width`/`height` → required fields.** The frontend editor always writes explicit resolution values (defaulting to 1920×1080 in the editor itself). The 11 fallback sites in render + encode are removed.

2. **Font resolution → required field, Arial as documented last resort.** `font` becomes required and the frontend always writes it. The typeface resolver's Arial/system-default chain stays as a final safety net, but logs a warning when hit — it should never trigger in normal operation.

3. **Heading widget serde defaults → required fields.** All 13 `default = "..."` functions are removed. Every heading field (`pixels_per_degree`, `major_tick_interval`, `indicator_style`, etc.) is written explicitly by the frontend.

4. **`activity/schema.rs` → leave as-is.** The 35 `#[serde(default)]` annotations are for sparse telemetry input, not config. No changes needed.
