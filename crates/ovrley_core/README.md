# ovrley_core

Headless Rust library for OVRLEY's overlay rendering and video encoding. Owns the entire production data path — activity processing, config validation, Skia frame rendering, and FFmpeg encoding — with no dependency on a GUI framework.

## Data flow

```
ovrley_app (the editor-effective document)
  │  RenderConfig  +  ParsedActivity
  ▼
render_jobs::execution::RenderExecutionService::submit_single_config
  │                                 (submit_batch for videos)
  │
  └─► normalize::validate_render_config  ◄── the seam
        │
        │  Rejects: missing fields, zero widths, negative scales, out-of-range values.
        │  Provides: NO render-affecting defaults. Every value must be explicit.
        │
        ▼
   ValidatedRenderConfig  ──────────────────────┐
        │                                        │
        ▼                                        │
   activity:: trim → densify                     │
        │                                        │
        ▼                                        │
   render:: (Skia)                               │
        │  fonts, widgets, labels, plots         │  scene.width, scene.height,
        │  surface allocation, frame loop        │  scene.fps, scene.ffmpeg,
        │                                        │  codec settings, timing
        ▼                                        ▼
   RGBA frames ───────────────────►  encode:: (FFmpeg subprocess)
                                            │
                                            ▼
                                      final video
```

## No fallbacks by design

The normalization seam (`validate_render_config`) intentionally rejects incomplete configs rather than filling in guessed defaults. If a template is missing `scene.scale`, `shadow_color`, or any widget font/color — the render **fails** with a descriptive error.

This is not an oversight. The renderer cannot infer design intent — only the editor knows what the user actually wants. Silently applying a hardcoded fallback would produce a video that doesn't match what the user saw in the editor. Failing early and verbosely forces the editor to materialize every value before rendering, keeping the WYSIWYG contract honest.

`ovrley_app` merges the global defaults into the scene and every widget before it submits (`lib/template.rs`, `editor_effective_config`). This crate's job is to reject anything that is still missing.
