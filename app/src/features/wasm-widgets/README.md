# WASM widget interactions

The document keeps the canonical widget configuration. No persisted scale field or
CSS bitmap scaling is involved in dragging/resizing widgets.

Every standard metric supporting `display_type: text` uses this path, including
gear labels and GPS coordinate rows. Shared Rust formatting handles units,
precision, affixes, totals, and altitude correction. The sampler prepares source
columns once and applies each metric's bridge, preserve, or hold policy at the
current `previewSecond`. Outside the activity's time range, preview shows missing
value placeholders. Time text also uses this path, including timezone-aware
daytime and activity/export-relative elapsed time. The scene supplies the current
export start second, and native export shares the timestamp sampler and formatter.
WASM also owns gauges, gradient, lean-angle, G-force, lap timers, heading tapes,
labels, backdrops, raster images, routes, and elevation profiles.

WASM module loading starts at canvas mount and does not gate app startup. Module
initialization errors are displayed by the canvas. The preview session lives for
the canvas lifetime, including when there are no
WASM widgets. Activity replacement starts its raw-byte transfer immediately, and
the default value font is prepared independently in parallel. Adding the first
widget therefore reuses these resources instead of starting their loading chain.
Deleting the last widget releases its renderer but retains session resources until
the canvas unmounts. Activity replacement and clearing still replace the WASM data.

- `useWasmWidgetInteractions` translates Moveable gestures into scene-space intent.
- `widgetInteraction` captures the original configuration and gesture anchor. It
  replaces pending intent instead of accumulating deltas from previous drafts.
  Its per-widget `geometry()` returns the session's last presented geometry without
  flushing or scheduling work. Selection badges reuse it to avoid DOM rectangle
  reads and a second geometry cache; callers must treat it as read-only.
- `previewSession` owns the animation-frame scheduler. It resolves the latest
  intent, updates WASM and presents pixels/geometry in that frame. Inspector draft
  publication is downstream; it does not gate gesture rendering.
- Rust `widgets/value/interaction.rs` owns intrinsic size limits and field updates.
  It measures the new layout at the current playhead and adjusts canonical x/y so
  the opposite content corner stays fixed. Scene scale is already included in the
  returned geometry; editor viewport zoom is handled by Moveable.
- Rust returns separate content and raster rectangles. The wrapper uses content
  bounds (glyph/icon strokes, excluding shadows); its overflowing canvas uses
  raster bounds. `widgetPresentation` installs both and notifies the editor to
  refresh Moveable, including during an active gesture.

Every widget family uses the existing `pixels_revision` contract. The overall
revision can advance for placement alone; `previewSession` copies WASM pixels and
uploads the canvas only when the pixel revision changes. At a fixed playhead,
translation preserves the Rust instance, layout, surface, and rendered values.
Content and raster bounds move together, preserving shadow offsets. Geometry-only
frames still update placement; pixel-only playback frames skip DOM placement and
Moveable refreshes.

Active drags set `transient` so fractional movement can reuse cached pixels. On
mouseup, clearing that flag and flushing restores native/export pixel alignment,
with one rerasterization when the final fractional position requires it. Raster
images retain their existing placement behavior. The paused 15-widget drag
benchmark measured zero steady-state pixel copies and canvas uploads after this
change; per-widget config and geometry updates still occur. See the
[overlay editor notes](../overlay-editor/README.md) for selection measurement costs.

Mouseup flushes pending intent before committing one configuration patch. The
commit preserves the preview's fractional coordinates and sizes. Escape, window
blur, selection changes, document replacement and unmount cancel the gesture.

Single widgets support dragging and resizing, with rotation for routes and raster
images. Groups share the same interaction controller and frame scheduler for
translation and proportional resizing; Rust applies each widget's resize policy.

The overlay editor keeps one data-patch draft model for the canvas and inspector,
and one node registry with stable callbacks across appearance and display edits.
Inspector size controls retain their editable-content policies in
`features/widget-editor/utils/widgetSize.js`; canvas resize geometry belongs to Rust.

Future widget implementations should supply their resize policy and content bounds
in Rust and reuse the frontend lifecycle. A widget that reflows when resized should
resolve requested dimensions through its layout, rather than imitate bitmap scaling.
