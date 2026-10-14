/**
 * Installs raster placement and tight interaction bounds together. Both rectangles
 * are supplied by Rust in scene pixels; canvas overflow preserves shadows.
 * @param {HTMLCanvasElement} canvas Widget raster.
 * @param {object} geometry Rust drawing and content bounds.
 * @param {HTMLElement} target Selection wrapper; a sibling raster avoids transformed overflow for routes.
 */
export function presentWidgetGeometry(canvas, geometry, target = canvas.parentElement) {
  const { content } = geometry
  target.style.left = `${content.left}px`
  target.style.top = `${content.top}px`
  target.style.width = `${content.width}px`
  target.style.height = `${content.height}px`
  target.style.visibility = 'visible'
  target.dataset.widgetContentWidth = geometry.content_width
  const nested = canvas.parentElement === target
  canvas.style.left = `${geometry.left - (nested ? content.left : 0)}px`
  canvas.style.top = `${geometry.top - (nested ? content.top : 0)}px`
  // Rotated families expose an oriented selection frame. Counter-rotate the
  // canvas so its already-rotated Rust pixels remain in scene coordinates.
  const rotation = geometry.rotation ?? 0
  const origin = geometry.rotation_origin ?? [0, 0]
  target.style.transformOrigin = `${origin[0]}px ${origin[1]}px`
  target.style.transform = `rotate(${rotation}deg)`
  canvas.style.transformOrigin = `${content.left - geometry.left + origin[0]}px ${content.top - geometry.top + origin[1]}px`
  canvas.style.transform = nested ? `rotate(${-rotation}deg)` : 'none'
  target.dispatchEvent(new Event('wasm-widget-render', { bubbles: true }))
}
