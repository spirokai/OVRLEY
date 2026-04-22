# Route And Elevation Rendering Refactor Strategy

## Objective

Refactor the current video rendering pipeline so that route map and elevation profile rendering no longer depend on Matplotlib image export inside the per-frame loop.

The target outcome is to reduce render time substantially for templates that include route and elevation widgets, while preserving a close visual match to the current output.

This strategy is implementation-ready and is intended to be sufficient to execute the full refactor end to end.

## Locked Constraints

These implementation choices are fixed for this refactor:

1. Close visual match is sufficient. Exact pixel-perfect parity with current Matplotlib output is not required.
2. Slight progress quantization is acceptable for route completed-state rendering if it materially improves performance.
3. Route/elevation rotation should be supported, but it is not a first-pass hot path to optimize aggressively.
4. The refactor should include secondary text/font-path optimization in the same pass.
5. The refactor should focus on route and elevation widgets first, but may also optimize shared frame rendering utilities where they clearly support the same performance goal.

## Current Bottleneck Summary

The current expensive path is:

1. [backend/frame.py](h:/tools/cyclemetry/backend/frame.py#L84) calls `draw_figure()` per frame.
2. `draw_figure()` calls [backend/plot.py](h:/tools/cyclemetry/backend/plot.py#L127) `build_image()` per frame.
3. `build_image()` uses Matplotlib and `plt.savefig(...)` into a `BytesIO` buffer per frame.
4. The buffer is reopened as a PIL image and composited back into the full overlay frame.

This is fundamentally too expensive, especially for route and elevation widgets.

## Refactor Goals

The refactor must achieve all of the following:

1. Remove Matplotlib from the per-frame route/elevation rendering path completely.
2. Precompute route and elevation geometry once per render job.
3. Build static raster layers once per render job.
4. Precompute per-frame progress, marker coordinates, and optional label text once per render job.
5. Composite cached layers and marker sprites during the frame loop instead of regenerating figures.
6. Add a font cache so text rendering no longer reloads fonts repeatedly.
7. Preserve route/elevation support for large widgets, including near-full-screen overlays.

## Non-Goals

This refactor does not attempt to:

1. Replace ffmpeg export.
2. Replace the full Python renderer.
3. Move rendering to GPU.
4. Redesign widget styling.
5. Rewrite unrelated numeric gauge logic beyond shared text/font improvements.

## High-Level Design

Replace the current figure-driven route/elevation rendering model with an asset-driven compositing model.

The new model is:

1. Precompute widget-local geometry.
2. Simplify that geometry according to actual widget dimensions.
3. Render static background and completed-state layers once.
4. Build per-frame state tables.
5. In the frame loop, perform only:
   - base image copy
   - dynamic numeric text drawing
   - route compositing from cached assets
   - elevation compositing from cached assets
   - ffmpeg write

## Rendering Modes

The renderer should support two route rendering modes internally, selected by widget size.

### Route Mode A: Bucket Mask / Bucket Overlay

This is the default and safe mode for medium and large route widgets.

Characteristics:

1. Uses cached route background and completed layers.
2. Uses quantized progress buckets such as 64, 128, or 256 steps.
3. Performs only cached compositing per frame.
4. Is robust for large route overlays.

### Route Mode B: Dynamic Prefix Draw

This is optional and only allowed for small route widgets.

Characteristics:

1. Draws only the completed polyline prefix each frame on a small widget-local transparent canvas.
2. Should only be used behind a size threshold.
3. Should not be the first implemented mode.

### Elevation Mode

Elevation always uses cached layers plus a progress-based reveal.

Characteristics:

1. Uses cached background and completed layers.
2. Uses horizontal crop or mask reveal according to `progress01`.
3. Uses a cached marker sprite.
4. Uses optional preformatted label text.

## New Data Structures

Implement these structures as the architectural backbone of the refactor.

### RenderAssets

```python
@dataclass
class RenderAssets:
    base_image: Image.Image
    font_cache: FontCache
    route_cache: RouteWidgetCache | None
    elevation_cache: ElevationWidgetCache | None
```

Purpose:

1. Replaces the current partial `plot_backgrounds` model.
2. Carries all cached render-time assets into the frame loop.

### FontCache

```python
@dataclass
class FontCache:
    by_key: dict[tuple[str, int], object]
```

Key:

1. `(font_path_or_name, font_size)`

Purpose:

1. Eliminate repeated `ImageFont.truetype(...)` calls.
2. Speed up numeric values and optional elevation labels.

### RouteWidgetCache

```python
@dataclass
class RouteWidgetCache:
    widget_x: int
    widget_y: int
    widget_width: int
    widget_height: int
    rotation_deg: float
    render_mode: str
    bucket_count: int

    background_layer: Image.Image
    completed_layer: Image.Image | None
    rotated_background_layer: Image.Image | None
    rotated_completed_layer: Image.Image | None

    marker_sprite: Image.Image
    marker_anchor: tuple[int, int]

    simplified_points: list[tuple[float, float]]
    cumulative_progress: list[float]

    bucket_masks: list[Image.Image] | None
    bucket_overlays: list[Image.Image] | None

    frame_states: list[RouteFrameState]
```

### RouteFrameState

```python
@dataclass
class RouteFrameState:
    progress01: float
    marker_x: float
    marker_y: float
    bucket_index: int
```

### ElevationWidgetCache

```python
@dataclass
class ElevationWidgetCache:
    widget_x: int
    widget_y: int
    widget_width: int
    widget_height: int
    rotation_deg: float

    background_layer: Image.Image
    completed_layer: Image.Image
    rotated_background_layer: Image.Image | None
    rotated_completed_layer: Image.Image | None

    marker_sprite: Image.Image
    marker_anchor: tuple[int, int]

    simplified_points: list[tuple[float, float]]
    frame_states: list[ElevationFrameState]
    label_style: ElevationLabelStyle | None
```

### ElevationFrameState

```python
@dataclass
class ElevationFrameState:
    progress01: float
    marker_x: float
    marker_y: float
    elevation_m: float
    label_text: str | None
```

### ElevationLabelStyle

```python
@dataclass
class ElevationLabelStyle:
    font_path: str
    font_size: int
    color: str
    x_offset: int
    y_offset: int
    units: list[str]
    decimal_rounding: int | None
```

### WidgetGeometry

```python
@dataclass
class WidgetGeometry:
    points: list[tuple[float, float]]
    bbox: tuple[float, float, float, float]
    cumulative_progress: list[float]
```

Purpose:

1. Shared geometry representation before rasterization.
2. Supports route and elevation preparation.

## What Must Be Cached

These assets must be generated once per render job and reused:

1. `base_image`
2. Static labels
3. Route background layer
4. Route completed layer
5. Elevation background layer
6. Elevation completed layer
7. Marker sprites
8. Optional rotated versions of static layers
9. Font objects
10. Route bucket masks or overlays
11. Per-frame route progress table
12. Per-frame elevation progress table
13. Optional per-frame label strings for elevation text

## What Must Be Precomputed

These values must be computed before the frame loop begins:

1. Widget-local route geometry at final display size
2. Widget-local elevation geometry at final display size
3. Resolution-aware simplified route polyline
4. Resolution-aware simplified elevation profile
5. Route cumulative progress table
6. Elevation cumulative progress table
7. Per-frame route marker coordinates
8. Per-frame elevation marker coordinates
9. Per-frame route bucket indices
10. Per-frame elevation label text

## Debug Artifact Strategy

The refactor must generate debug artifacts during development so route geometry, elevation geometry, cached rasters, and sample frame composites can be visually inspected without rendering an entire video.

### Required Debug Output Directory

Use a dedicated per-run debug output directory such as:

1. `WRITE_DIR()/debug_render/`
2. Optional subdirectories per phase or per render job

Suggested structure:

1. `debug_render/phase_1/`
2. `debug_render/phase_4_geometry/`
3. `debug_render/phase_5_layers/`
4. `debug_render/phase_6_elevation/`
5. `debug_render/phase_7_route/`

### Required Artifact Types

The refactor should be able to save these artifact categories:

1. Simplified route geometry preview images
2. Simplified elevation geometry preview images
3. Route background layer rasters
4. Route completed layer rasters
5. Elevation background layer rasters
6. Elevation completed layer rasters
7. Marker sprites
8. Sample completed-state route bucket outputs
9. Sample completed-state elevation reveals
10. Sample composited full-frame images for selected frames such as start, quarter, half, three-quarters, and end

### Required Sample Frames

For any phase that produces visual artifacts, save at least these frame positions when available:

1. First frame
2. Approximately 25 percent progress
3. Approximately 50 percent progress
4. Approximately 75 percent progress
5. Final frame

These should be sufficient to spot geometry, reveal, compositing, and marker-position bugs without exporting a full video.

### Debug Mode Requirement

Add a debug-artifact mode that can be turned on during development so the renderer writes these assets without requiring normal full-export behavior.

This debug mode should:

1. Save intermediate route and elevation assets
2. Save sample composited frames
3. Avoid forcing a full video export when only visual inspection is needed

### Debug Artifact Naming

Use predictable names so comparisons across refactor steps are easy.

Suggested examples:

1. `route_geometry.png`
2. `route_background.png`
3. `route_completed.png`
4. `route_bucket_032.png`
5. `elevation_geometry.png`
6. `elevation_background.png`
7. `elevation_completed.png`
8. `sample_frame_0000.png`
9. `sample_frame_0450.png`
10. `sample_frame_0900.png`

## What Must Not Happen In The Frame Loop

The following must be completely absent from per-frame rendering:

1. `plot.build_image(...)`
2. `plt.savefig(...)`
3. `Image.open(buffer)` for plot rasters
4. Matplotlib scatter artist creation
5. Matplotlib text artist creation
6. Full route redraw
7. Full elevation redraw
8. Font loading
9. Static widget rotation

## Size-Aware Simplification Rules

Simplification must depend on final widget dimensions.

### Route

1. Project raw route coordinates into widget-local space.
2. Simplify in widget space, not geographic space.
3. Use a tolerance derived from rendered pixel size.
4. Start with tolerance between `0.5 px` and `1.5 px` depending on widget size.
5. Preserve start, end, and significant turns.

### Elevation

1. Normalize profile to widget width and height.
2. Downsample based on rendered width.
3. Prefer min/max bucket downsampling or LTTB.
4. Preserve visible peaks and valleys.
5. Start with about `2 x widget_width_px` samples or fewer.

## Route Reveal Strategy

### First Implementation

Implement route rendering using bucket masks or bucket overlays.

Recommended default:

1. `bucket_count = 128`
2. Increase to `256` only if stepping is visibly objectionable.

Two viable variants:

1. Bucket masks applied to a single cached completed layer
2. Fully precomputed bucket overlays

Preferred order:

1. Try bucket overlays first if memory is acceptable and implementation is simpler.
2. Fall back to bucket masks if memory is too high.

Per-frame route composite:

1. Paste background layer.
2. Paste completed-state bucket asset or masked completed layer.
3. Paste marker sprite.

### Memory Tradeoff

Bucket overlays are simpler at runtime but heavier in memory.

Estimate conservatively before implementation for large widgets:

1. widget area in pixels
2. bytes per RGBA image
3. `bucket_count * bytes_per_widget_image`

If the memory cost is too high for large overlays, switch to bucket masks.

## Elevation Reveal Strategy

Implement elevation rendering using cached layers and horizontal reveal.

Per-frame elevation composite:

1. Paste background layer.
2. Reveal completed layer according to `progress01` using crop or mask.
3. Paste marker sprite.
4. Draw optional altitude label.

This strategy should be used for both small and large elevation widgets.

## Text And Font Optimization Strategy

Text is secondary, but should be optimized in the same refactor.

### Font Cache

Add a font cache and route all text rendering through it.

Targets:

1. Numeric gauge values
2. Static labels if they are rendered through PIL text
3. Elevation label text

### Label Preformatting

For elevation labels:

1. Precompute per-frame label text strings.
2. Only draw the final string in the frame loop.

### Out Of Scope For This Pass

Do not redesign text layout or typography behavior. Only remove repeated font loading and repetitive string formatting where clearly helpful.

## Module-Level Refactor Plan

### backend/scene.py

Add these functions:

1. `prepare_render_assets()`
2. `build_font_cache()`
3. `build_route_cache()`
4. `build_elevation_cache()`
5. `build_route_frame_states()`
6. `build_elevation_frame_states()`
7. `build_route_geometry()`
8. `build_elevation_geometry()`
9. `build_route_bucket_assets()`
10. `build_elevation_layers()`

Change these functions:

1. `export_video()`
   - Prepare `RenderAssets` before entering ffmpeg loop.
   - Pass `RenderAssets` into frame drawing.
2. `build_figures()`
   - Reduce role or deprecate for route/elevation widgets.
3. `render_demo()`
   - Ensure demo rendering uses the new compositor path eventually.

### backend/frame.py

Add these functions:

1. `get_cached_font()`
2. `composite_route_widget()`
3. `composite_elevation_widget()`
4. `draw_cached_text()` or equivalent helper if needed

Change these functions:

1. `draw_value()`
   - Use `FontCache`
2. `draw()`
   - Accept `RenderAssets`
   - Composite route/elevation from caches
   - Stop routing route/elevation through `draw_figure()`

Deprecate or narrow usage:

1. `draw_figure()`
   - Keep temporarily only if needed during migration
   - Remove route/elevation usage first

### backend/plot.py

Change role of this module:

1. Stop using it as a per-frame route/elevation renderer.
2. Keep only for one-time static asset generation if useful during migration.
3. Optionally keep helper routines for geometry preparation or one-time raster creation.

Targets for deprecation from the frame loop:

1. `build_image()`
2. `draw_points()`
3. `draw_labels()`

## Phased Execution Plan

### Phase 1: Instrumentation

Goal:

1. Establish exact baseline timings before refactor.

Tasks:

1. Add timing around route rendering.
2. Add timing around elevation rendering.
3. Add timing around numeric/text rendering.
4. Add timing around ffmpeg write.
5. Capture baseline render durations for:
   - full template
   - no route/elevation
   - route only
   - elevation only
6. Save baseline sample full-frame images for a small set of frame indices so later phases can be visually compared.

Exit criteria:

1. Baseline timings are recorded and reproducible.
2. Baseline sample frames are available for comparison.

### Phase 2: Data Structures And Seams

Goal:

1. Introduce cache structures and a render-assets seam without changing visuals yet.

Tasks:

1. Add `RenderAssets`, widget caches, frame-state classes, and `FontCache`.
2. Add placeholder preparation functions.
3. Update `Frame.draw()` signature to accept render assets if needed.
4. Add the debug output plumbing so later phases can save intermediate artifacts cleanly.

Exit criteria:

1. Render pipeline can pass prepared assets through the frame loop.
2. Debug artifacts can be emitted from the new pipeline seam.

### Phase 3: Font Cache And Shared Text Path

Goal:

1. Remove repeated font loading.

Tasks:

1. Implement font cache.
2. Update `draw_value()` to use cached fonts.
3. Ensure no repeated `ImageFont.truetype(...)` in the frame loop for repeated fonts.
4. Save sample frames containing dynamic text overlays to confirm there is no visual regression in text rendering.

Exit criteria:

1. Font loading is no longer an inner-loop hotspot.
2. Sample frame text output remains visually acceptable.

### Phase 4: Geometry Preparation

Goal:

1. Precompute route and elevation geometry outside the frame loop.

Tasks:

1. Build widget-local route geometry.
2. Simplify route geometry by actual widget size.
3. Build widget-local elevation geometry.
4. Downsample elevation by actual widget width.
5. Compute cumulative progress arrays.
6. Compute per-frame route marker coordinates.
7. Compute per-frame elevation marker coordinates.
8. Compute optional elevation label strings.
9. Save route geometry preview artifacts.
10. Save elevation geometry preview artifacts.

Exit criteria:

1. Every frame has route/elevation state available without dynamic geometric lookup.
2. Geometry preview artifacts look correct at representative widget sizes.

### Phase 5: Static Layer Generation

Goal:

1. Generate all large static widget layers once.

Tasks:

1. Create route background layer.
2. Create route completed layer.
3. Create elevation background layer.
4. Create elevation completed layer.
5. Create marker sprites.
6. Pre-rotate static layers when rotation is non-zero.
7. Save route and elevation layer rasters for visual inspection.
8. Save a small set of representative route bucket assets if bucket rendering is enabled.

Exit criteria:

1. Route/elevation visuals can be assembled from layers without full redraw.
2. Static layer artifacts look visually correct when inspected directly.

### Phase 6: Elevation Compositor

Goal:

1. Replace per-frame elevation figure rendering first.

Tasks:

1. Implement elevation compositor using background layer, completed layer, marker sprite, and label text.
2. Remove elevation widget from `draw_figure()` path.
3. Validate against current visual output.
4. Save sample elevation-only composited frames at multiple progress points.

Exit criteria:

1. Elevation rendering no longer depends on Matplotlib inside the frame loop.
2. Saved elevation sample frames are visually acceptable.

### Phase 7: Route Compositor

Goal:

1. Replace per-frame route figure rendering.

Tasks:

1. Implement route rendering mode selection.
2. Implement default bucketed route overlay path.
3. Build bucket assets during preparation.
4. Remove route widget from `draw_figure()` path.
5. Validate acceptable visual smoothness and memory cost.
6. Save sample route-only composited frames and representative bucket outputs.

Exit criteria:

1. Route rendering no longer depends on Matplotlib inside the frame loop.
2. Saved route sample frames and bucket artifacts are visually acceptable.

### Phase 8: Remove Old Plotting From Frame Loop

Goal:

1. Complete the architectural transition.

Tasks:

1. Confirm no frame path calls `plot.build_image()`.
2. Remove or isolate obsolete frame-loop plot logic.
3. Keep any remaining `plot.py` usage one-time only.
4. Save sample full-frame composites from the new path to confirm parity against baseline examples.

Exit criteria:

1. All route/elevation rendering uses cached compositing only.
2. Full-frame sample artifacts from the new path are acceptable compared to baseline.

### Phase 9: Measure, Tune, And Stabilize

Goal:

1. Validate performance gain and tune memory/quality tradeoffs.

Tasks:

1. Re-run Phase 1 scenarios.
2. Compare timings.
3. Tune route bucket counts.
4. Tune simplification tolerances.
5. Validate large-widget templates.
6. Validate templates with no route/elevation regressions.
7. Save final representative sample frames and cached route/elevation artifacts for sign-off.

Exit criteria:

1. Route/elevation rendering is substantially faster.
2. Visual output remains acceptable.
3. Final artifact set is available for quick regression comparison later.

## Validation Checklist

Validate each of these explicitly after implementation:

1. 4K render with route and elevation enabled
2. 4K render with route only
3. 4K render with elevation only
4. Template with no route/elevation widgets
5. Route widget with rotation
6. Elevation widget with altitude labels enabled
7. Large route widget spanning most of frame height
8. Large elevation profile spanning most of frame width
9. Small route/elevation widgets
10. Saved debug route geometry artifacts
11. Saved debug elevation geometry artifacts
12. Saved route/elevation layer rasters
13. Saved sample composited frames at representative progress points

## Success Criteria

The refactor is complete when:

1. No Matplotlib image export remains in the per-frame route/elevation path.
2. Route/elevation are rendered from cached assets and frame-state lookup only.
3. Font loading is cached.
4. Large route and elevation widgets render correctly.
5. Total render time with route/elevation enabled is materially closer to the no-route/no-elevation baseline than to the current slow path.

## Fallback And Risk Handling

If a step introduces risk, use these fallback rules:

1. If route bucket overlays are too memory-heavy, switch to bucket masks.
2. If route bucket stepping is too visible at 128 buckets, try 256 before abandoning the approach.
3. If rotation handling adds complexity, pre-rotate static layers and leave further optimization for later.
4. If elevation label drawing remains noticeable, keep preformatted strings and revisit only after route/elevation rasterization is fixed.

## Final Notes For Implementation

1. Start with elevation compositor first because it is structurally simpler.
2. Do route second because the reveal logic is harder.
3. Keep the refactor incremental and measurable.
4. Do not mix this work with unrelated architecture changes.
5. Do not ship intermediate states where route/elevation still call Matplotlib in the frame loop.
6. At the end of each phase, save inspectable artifacts so visual issues can be caught without a full video render.
