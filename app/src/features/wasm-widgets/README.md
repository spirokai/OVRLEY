# WASM widget interactions

The document keeps the canonical widget configuration. No persisted scale field or
CSS bitmap scaling is involved in dragging/resizing the speed/text widget.

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

Mouseup flushes pending intent before committing one configuration patch. The
commit preserves the preview's fractional coordinates and sizes. Escape, window
blur, selection changes, document replacement and unmount cancel the gesture.

Single WASM widgets support dragging and corner resizing. Groups containing WASM
widgets support translation through shared data drafts; group scaling is disabled
until a shared multi-widget resize policy is implemented. Legacy-only selections
continue using the existing interaction handlers.

Future widget implementations should supply their resize policy and content bounds
in Rust and reuse the frontend lifecycle. A widget that reflows when resized should
resolve requested dimensions through its layout, rather than imitate bitmap scaling.
