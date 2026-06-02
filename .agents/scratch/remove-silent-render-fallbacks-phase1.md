# Phase 1 Inventory: Output-Drift Critical Backend Fallbacks

This is the Phase 1 deliverable for `remove-silent-render-fallbacks-v2`.

Scope:
- Includes only category 1 fallback sites: backend-side defaults that can change pixels, geometry, fps, resolution, codec/container, or render/composite timing.
- Excludes category 2 data-absence presentation, category 3 runtime/environment heuristics, and category 4 final-resort platform fallbacks unless they directly affect the validated render contract.

## Highest-risk entry-point defaults

### 1. Scene resolution and scale defaulted at render entry

- `src-tauri/ovrley_core/src/render/mod.rs:112`
- `src-tauri/ovrley_core/src/render/mod.rs:113`
- `src-tauri/ovrley_core/src/render/mod.rs:114`
- `src-tauri/ovrley_core/src/render/mod.rs:222`
- `src-tauri/ovrley_core/src/render/mod.rs:223`
- `src-tauri/ovrley_core/src/render/mod.rs:224`
- `src-tauri/ovrley_core/src/render/mod.rs:374`
- `src-tauri/ovrley_core/src/render/mod.rs:375`

Current behavior:
- Missing `scene.width` becomes `1920`
- Missing `scene.height` becomes `1080`
- Missing `scene.scale` becomes `1.0`

Why category 1:
- Directly changes render target size, text/widget scaling, and output pixels.

### 2. Scene resolution and scale defaulted again in encode pipelines

- `src-tauri/ovrley_core/src/encode/video_pipeline.rs:119`
- `src-tauri/ovrley_core/src/encode/video_pipeline.rs:120`
- `src-tauri/ovrley_core/src/encode/video_pipeline.rs:208`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:269`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:270`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:272`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:562`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:563`

Current behavior:
- Encode path independently defaults missing dimensions and scale instead of consuming an explicit validated scene contract.

Why category 1:
- Changes encoded frame geometry and overlay sizing.

## Styling defaults that invent visible output

### 3. Scene/label/value text style fallback chain in render text

- `src-tauri/ovrley_core/src/render/text.rs:80`
- `src-tauri/ovrley_core/src/render/text.rs:85`
- `src-tauri/ovrley_core/src/render/text.rs:90`
- `src-tauri/ovrley_core/src/render/text.rs:97`
- `src-tauri/ovrley_core/src/render/text.rs:102`
- `src-tauri/ovrley_core/src/render/text.rs:105`
- `src-tauri/ovrley_core/src/render/text.rs:115`
- `src-tauri/ovrley_core/src/render/text.rs:117`
- `src-tauri/ovrley_core/src/render/text.rs:122`
- `src-tauri/ovrley_core/src/render/text.rs:127`
- `src-tauri/ovrley_core/src/render/text.rs:134`
- `src-tauri/ovrley_core/src/render/text.rs:139`
- `src-tauri/ovrley_core/src/render/text.rs:142`

Current behavior:
- Missing opacity defaults to `1.0`
- Missing font size defaults to `32.0`
- Missing color defaults to `#ffffff`
- Missing shadow/border numeric fields default to `0.0`
- Value baseline offset defaults to `0.0`
- Font can fall back from widget to scene font at draw-time

Why category 1:
- Changes visible styling, placement, and glyph appearance inside backend draw code.

### 4. Value widget icon/unit/gradient styling defaults

- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:48`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:82`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:87`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:109`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:157`
- `src-tauri/ovrley_core/src/render/widgets/value/layout.rs:170`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:55`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:56`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:59`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:89`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:106`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:110`
- `src-tauri/ovrley_core/src/render/widgets/value/gradient.rs:115`

Current behavior:
- Missing icon size defaults to `28.0`
- Missing icon color defaults to `#40e0d0`
- Missing unit color defaults to `#ffffff`
- Missing icon offsets default to `0.0`
- Missing gradient triangle width defaults to `72.0`
- Missing gradient triangle visibility defaults to `true`
- Missing positive/negative triangle colors default to hardcoded colors

Why category 1:
- Changes visible metric widget composition and coloring.

### 5. Heading tape styling and geometry defaults during prepare

- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:29`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:53`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:54`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:55`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:56`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:74`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:75`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:84`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:239`
- `src-tauri/ovrley_core/src/render/widgets/heading/prepare.rs:243`

Current behavior:
- Missing scale defaults to `1.0`
- Missing tick and label colors default to `#ffffff`
- Missing cardinal colors inherit from non-cardinal colors
- Missing label font size defaults to `12.0`
- Missing label font can fall back to first value font or scene font
- Missing label offset defaults to `4.0`
- Missing indicator color defaults to `#ffffff`
- Missing indicator size defaults to `10.0`

Why category 1:
- Changes heading-tape pixels and backend-side inheritance decides final styling.

## Route and elevation normalization still owns semantic defaults

### 6. Shared plot helper defaults used by route/elevation normalization

- `src-tauri/ovrley_core/src/render/widgets/common.rs:255`
- `src-tauri/ovrley_core/src/render/widgets/common.rs:261`
- `src-tauri/ovrley_core/src/render/widgets/common.rs:279`
- `src-tauri/ovrley_core/src/render/widgets/common.rs:320`

Current behavior:
- Base plot color defaults to white
- Legacy line width defaults through a shared constant
- Empty marker-point arrays synthesize marker layers from flat marker fields
- Color precedence is resolved inside backend helpers

Why category 1:
- These helpers are the backend-owned inheritance/default engine for multiple widgets.

### 7. Route plot normalization defaults

- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:23`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:24`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:29`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:33`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:39`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:54`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:57`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:58`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:64`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:79`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:85`
- `src-tauri/ovrley_core/src/render/widgets/route/normalize.rs:97`

Current behavior:
- Missing scale defaults to `1.0`
- Missing base color defaults to white
- Missing marker size derived from point weights or a hardcoded default
- Missing marker color inherits from base color
- Missing marker variant diameter derived from marker size
- Missing simplify tolerance and target density default in backend
- Missing line widths inherit from legacy line width shims
- Missing line colors/opacities resolved from nested legacy fields and hardcoded defaults
- Missing marker point layers synthesized when only legacy flat fields exist

Why category 1:
- Changes route widget geometry, density, stroke appearance, and marker rendering.

### 8. Elevation plot normalization defaults

- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:22`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:23`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:28`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:33`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:34`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:40`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:55`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:56`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:57`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:58`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:59`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:65`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:80`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:86`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:92`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:97`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:106`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:111`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:122`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:130`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:135`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:140`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:142`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:147`
- `src-tauri/ovrley_core/src/render/widgets/elevation/normalize.rs:152`

Current behavior:
- Missing scale defaults to `1.0`
- Missing base color defaults to white
- Missing marker size derived in backend
- Missing marker color inherits from base color
- Missing marker variant diameter derived in backend
- Missing margin, y-scale, simplify tolerance, and target density default in backend
- Missing completed/remaining line widths inherit from legacy width shims
- Missing line/fill colors and opacities are resolved through inheritance chains and hardcoded defaults
- Missing marker points are synthesized from legacy flat fields
- Missing point-label offsets, font, font size, and label color default in backend

Why category 1:
- Changes plotted profile shape density, marker treatment, filled areas, and labels.

### 9. Marker layer defaults after normalization

- `src-tauri/ovrley_core/src/render/widgets/marker.rs:75`
- `src-tauri/ovrley_core/src/render/widgets/marker.rs:81`
- `src-tauri/ovrley_core/src/render/widgets/marker.rs:85`

Current behavior:
- Missing marker point weight defaults to a shared constant
- Missing marker point color defaults to white
- Missing marker point opacity defaults through `normalize_opacity(..., 1.0)`

Why category 1:
- Still invents visible marker layer styling after earlier normalization.

## Metric formatting defaults that alter displayed semantics

### 10. Backend default display units and formatting choices

- `src-tauri/ovrley_core/src/render/format.rs:339`
- `src-tauri/ovrley_core/src/render/format.rs:424`
- `src-tauri/ovrley_core/src/render/format.rs:444`
- `src-tauri/ovrley_core/src/render/format.rs:452`
- `src-tauri/ovrley_core/src/render/format.rs:524`
- `src-tauri/ovrley_core/src/render/format.rs:559`
- `src-tauri/ovrley_core/src/render/format.rs:585`
- `src-tauri/ovrley_core/src/render/format.rs:591`
- `src-tauri/ovrley_core/src/render/format.rs:598`
- `src-tauri/ovrley_core/src/render/format.rs:620`
- `src-tauri/ovrley_core/src/standard_metrics.rs:267`
- `src-tauri/ovrley_core/src/standard_metrics.rs:312`

Current behavior:
- Missing speed display unit defaults to `kmh`
- Missing air pressure display unit defaults to `hpa`
- Missing stride length display unit defaults to `m`
- Missing vertical speed display unit defaults to `mps`
- Missing sign display defaults to `true`
- Missing time format can fall back to scene time format and finally a backend hardcoded preset
- Missing decimal rounding can fall back to scene/global defaults at format time
- Missing balance format defaults to `plain`
- Standard metric display unit can still be resolved from manifest defaults inside backend format code

Why category 1:
- Changes user-visible text values and units, not just presentation chrome.

## Encode/composite defaults that alter output semantics

### 11. Transparent export codec/container defaults

- `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:39`
- `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:44`
- `src-tauri/ovrley_core/src/encode/ffmpeg_settings.rs:80`

Current behavior:
- Missing `scene.ffmpeg.codec` defaults to `prores_ks`
- Missing `scene.ffmpeg.container` defaults to `mov`
- Missing `scene.ffmpeg.loglevel` defaults to `info`

Why category 1:
- Codec and container change encoded output semantics.
- `loglevel` is not category 1 by itself, but it is coupled to the same resolution site and should be excluded in Phase 2 work.

### 12. Composite render timing/update-rate/codec defaults

- `src-tauri/ovrley_core/src/encode/video.rs:70`
- `src-tauri/ovrley_core/src/encode/video.rs:71`
- `src-tauri/ovrley_core/src/encode/video.rs:73`
- `src-tauri/ovrley_core/src/encode/video.rs:74`
- `src-tauri/ovrley_core/src/encode/video.rs:76`
- `src-tauri/ovrley_core/src/encode/video_segmented.rs:291`
- `src-tauri/ovrley_core/src/encode/video_segmented.rs:298`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:107`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:113`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:125`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:131`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:557`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:559`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:560`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:752`
- `src-tauri/ovrley_core/src/encode/video_composite_pipeline.rs:759`

Current behavior:
- Missing composite trim start defaults to `0.0`
- Missing composite widget update rate defaults to `1`
- Missing composite render duration defaults to `video_duration - trim_start`
- Missing composite codec defaults to `libx264`
- Composite pipeline derives timing and codec defaults independently in multiple places

Why category 1:
- Changes frame cadence, output duration, sync window, and selected codec path.

## Explicit non-goals for Phase 2 from this pass

These were found but are not part of the category 1 inventory:

- Data-absence placeholders like `"--"`, `"--:--"`, and empty-series interpolation fallbacks
- Runtime/environment fallbacks like ffmpeg binary discovery, hardware probe defaults, temp/debug paths, worker-count heuristics, and input pixel format env vars
- Final-resort font resolver fallback to bundled/system fonts
- Debug-only summaries that default width/height or `"unknown"` labels

## Suggested Phase 2 slice order

1. Scene geometry and text/value widget contract
2. Route/elevation normalized plot contract
3. Heading normalized contract
4. Composite/ffmpeg validated encode contract

