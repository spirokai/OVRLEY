Status: done

# 02 — Tape Rendering: Ticks and Labels

## Parent

[Heading Tape Widget PRD](../PRD.md)

## What to build

Implement the prepare-and-draw pipeline for the heading tape in the Skia backend, rendering configurable ticks and labels onto a cached tape image.

During preparation, render the full 360° tape (major/minor ticks + numeric/cardinal labels + shadows) into a `SkiaImage`. The tape surface is cached so it is rendered once, not per frame. Per-frame, draw the cached tape with `TileMode::Repeat` in X (offset by `heading × pixels_per_degree`) inside a clip rect matching the widget bounds. This handles the 0°/360° wrap seamlessly.

Tick and label rendering must honor all config parameters:

- **Ticks**: major at 15° intervals, minor at 5° (3 per major). Major/minor independently toggleable with `show_major_ticks` / `show_minor_ticks`. Tick lengths as percentage of widget height. Thickness in pixels. Color for regular ticks, `cardinal_tick_color` for ticks at 45° multiples. `tick_alignment` (`"below"` or `"centered"`).
- **Labels**: one shared row below ticks. Cardinal labels (N/NE/E/SE/S/SW/W/NW) take priority over numeric labels at the 8 cardinal positions. Each type has its own on/off toggle and color. Font size and offset from tick bottoms configurable.
- **Shadows**: ticks and labels inherit shadow distance/strength/color from widget config (overriding scene defaults). Shadows are baked into the cached image — correct because shadows are relative to the widget viewport, not the scrolling tape.

Integrate heading into `prepare_render_assets` and `render_frame_to_surface`. The widget must be skippable when no heading config is present.

## Acceptance criteria

- [x] `prepare_render_assets` generates a heading widget cache (`HeadingWidgetCache`) when heading config is present, containing the cached tape `SkiaImage` and precomputed per-frame heading offsets
- [x] Per-frame rendering draws the tiled tape image with correct `heading × pixels_per_degree` offset, clipped to widget bounds
- [x] All tick config parameters are honored (show/hide, length, thickness, color, cardinal color, alignment)
- [x] All label config parameters are honored (show/hide, color, font size, offset, cardinal priority override)
- [x] Shadows appear on ticks and labels matching the widget's shadow override
- [x] The 0°/360° wrap boundary renders seamlessly (no gap, no duplicate/overlapping ticks)
- [x] Heading widget is skipped when no heading config is present in the template
- [x] Backend tests pass: at minimum a baseline render test producing a non-empty PNG and tick/label position calculation tests

## Blocked by

- [01 — Heading Data Plumbing + Config Struct](./01-heading-data-plumbing-and-config.md)
